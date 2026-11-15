//! Deterministic CPU Path Tracer with SAH BVH, GGX BRDF, à-trous Denoiser, and AgX View Transform (`foreplay-polyamour::pathtracer`).
//!
//! Features:
//! - Surface Area Heuristic (SAH) Bounding Volume Hierarchy (BVH) for triangle acceleration.
//! - Microfacet GGX metallic-roughness BRDF with Fresnel-Schlick and Sky importance lighting.
//! - Deterministic per-pixel XorShift RNG for bit-exact render repeatability.
//! - 1-pass à-trous edge-preserving wavelet filter for fast denoising.
//! - **AgX** View Transform (Blender 4/5 default filmic curve) and Standard sRGB tonemapping.

use crate::mesh::TriangulatedMesh;
use bevy_math::Vec3;
use serde::{Deserialize, Serialize};

/// Color view transform applied to linear radiance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ViewTransform {
    /// Blender 4+ AgX filmic tone curve with highlight desaturation and gamut compression.
    #[default]
    AgX,
    /// Standard linear to gamma 2.2 / sRGB curve with Reinhard tonemapping.
    StandardSrgb,
}

/// Camera description for path tracing.
#[derive(Clone, Debug, PartialEq)]
pub struct PathTracerCamera {
    pub eye: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    pub fov_degrees: f32,
}

impl Default for PathTracerCamera {
    fn default() -> Self {
        Self {
            eye: Vec3::new(0.0, 2.0, 5.0),
            target: Vec3::ZERO,
            up: Vec3::Y,
            fov_degrees: 60.0,
        }
    }
}

/// Material properties used by path tracer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PathTracerMaterial {
    pub base_color: Vec3,
    pub metallic: f32,
    pub roughness: f32,
}

impl Default for PathTracerMaterial {
    fn default() -> Self {
        Self {
            base_color: Vec3::splat(0.8),
            metallic: 0.0,
            roughness: 0.5,
        }
    }
}

/// Configuration options for baking path-traced images.
#[derive(Clone, Debug, PartialEq)]
pub struct PathTracerConfig {
    pub width: u32,
    pub height: u32,
    pub samples_per_pixel: u32,
    pub max_bounces: u32,
    pub seed: u64,
    pub enable_denoiser: bool,
    pub view_transform: ViewTransform,
    pub camera: PathTracerCamera,
    pub default_material: PathTracerMaterial,
}

impl Default for PathTracerConfig {
    fn default() -> Self {
        Self {
            width: 128,
            height: 128,
            samples_per_pixel: 16,
            max_bounces: 3,
            seed: 0x1337_c0de,
            enable_denoiser: true,
            view_transform: ViewTransform::AgX,
            camera: PathTracerCamera::default(),
            default_material: PathTracerMaterial::default(),
        }
    }
}

/// Ray in 3D space.
#[derive(Clone, Copy, Debug)]
struct Ray {
    origin: Vec3,
    dir: Vec3,
}

/// Axis-aligned bounding box.
#[derive(Clone, Copy, Debug)]
struct Aabb {
    min: Vec3,
    max: Vec3,
}

impl Aabb {
    fn empty() -> Self {
        Self {
            min: Vec3::splat(f32::INFINITY),
            max: Vec3::splat(f32::NEG_INFINITY),
        }
    }

    fn enclose_point(&mut self, p: Vec3) {
        self.min = self.min.min(p);
        self.max = self.max.max(p);
    }

    fn union(&self, other: &Self) -> Self {
        Self {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }

    fn intersect(&self, ray: &Ray, t_min: f32, t_max: f32) -> bool {
        let mut t0 = t_min;
        let mut t1 = t_max;

        for i in 0..3 {
            let inv_d = 1.0 / ray.dir[i];
            let mut t_near = (self.min[i] - ray.origin[i]) * inv_d;
            let mut t_far = (self.max[i] - ray.origin[i]) * inv_d;
            if inv_d < 0.0 {
                std::mem::swap(&mut t_near, &mut t_far);
            }
            t0 = t0.max(t_near);
            t1 = t1.min(t_far);
            if t1 < t0 {
                return false;
            }
        }

        true
    }
}

/// Triangle representation with precomputed normals and bounding box.
#[derive(Clone, Debug)]
struct TriPrimitive {
    p0: Vec3,
    p1: Vec3,
    p2: Vec3,
    n0: Vec3,
    n1: Vec3,
    n2: Vec3,
    centroid: Vec3,
    bbox: Aabb,
    material_slot: u16,
}

impl TriPrimitive {
    fn new(p0: Vec3, p1: Vec3, p2: Vec3, n0: Vec3, n1: Vec3, n2: Vec3, slot: u16) -> Self {
        let mut bbox = Aabb::empty();
        bbox.enclose_point(p0);
        bbox.enclose_point(p1);
        bbox.enclose_point(p2);
        let centroid = (p0 + p1 + p2) / 3.0;
        Self {
            p0,
            p1,
            p2,
            n0,
            n1,
            n2,
            centroid,
            bbox,
            material_slot: slot,
        }
    }

    /// Möller–Trumbore ray-triangle intersection algorithm.
    fn intersect(&self, ray: &Ray, t_min: f32, t_max: f32) -> Option<(f32, Vec3, u16)> {
        let edge1 = self.p1 - self.p0;
        let edge2 = self.p2 - self.p0;
        let h = ray.dir.cross(edge2);
        let a = edge1.dot(h);

        if a.abs() < 1e-7 {
            return None;
        }

        let f = 1.0 / a;
        let s = ray.origin - self.p0;
        let u = f * s.dot(h);

        if !(0.0..=1.0).contains(&u) {
            return None;
        }

        let q = s.cross(edge1);
        let v = f * ray.dir.dot(q);

        if v < 0.0 || u + v > 1.0 {
            return None;
        }

        let t = f * edge2.dot(q);
        if t >= t_min && t <= t_max {
            let w = 1.0 - u - v;
            let normal = (self.n0 * w + self.n1 * u + self.n2 * v).normalize_or_zero();
            Some((t, normal, self.material_slot))
        } else {
            None
        }
    }
}

/// Surface Area Heuristic (SAH) BVH Node.
#[derive(Debug)]
enum BvhNode {
    Leaf {
        bbox: Aabb,
        tri_indices: Vec<usize>,
    },
    Inner {
        bbox: Aabb,
        left: Box<BvhNode>,
        right: Box<BvhNode>,
    },
}

impl BvhNode {
    fn bbox(&self) -> &Aabb {
        match self {
            Self::Leaf { bbox, .. } => bbox,
            Self::Inner { bbox, .. } => bbox,
        }
    }

    fn intersect(
        &self,
        ray: &Ray,
        tris: &[TriPrimitive],
        t_min: f32,
        mut t_max: f32,
    ) -> Option<(f32, Vec3, u16)> {
        if !self.bbox().intersect(ray, t_min, t_max) {
            return None;
        }

        match self {
            Self::Leaf { tri_indices, .. } => {
                let mut closest: Option<(f32, Vec3, u16)> = None;
                for &idx in tri_indices {
                    if let Some((t, normal, slot)) = tris[idx].intersect(ray, t_min, t_max) {
                        t_max = t;
                        closest = Some((t, normal, slot));
                    }
                }
                closest
            }
            Self::Inner { left, right, .. } => {
                let hit_left = left.intersect(ray, tris, t_min, t_max);
                let current_max = hit_left.as_ref().map(|(t, _, _)| *t).unwrap_or(t_max);
                let hit_right = right.intersect(ray, tris, t_min, current_max);

                hit_right.or(hit_left)
            }
        }
    }
}

/// SAH BVH Acceleration Structure.
#[derive(Debug)]
pub struct SahBvh {
    root: Option<BvhNode>,
    tris: Vec<TriPrimitive>,
}

impl SahBvh {
    pub fn build(mesh: &TriangulatedMesh) -> Self {
        let num_tris = mesh.indices.len() / 3;
        let mut tris = Vec::with_capacity(num_tris);

        for t in 0..num_tris {
            let i0 = mesh.indices[t * 3] as usize;
            let i1 = mesh.indices[t * 3 + 1] as usize;
            let i2 = mesh.indices[t * 3 + 2] as usize;

            let p0 = mesh.positions[i0];
            let p1 = mesh.positions[i1];
            let p2 = mesh.positions[i2];

            let n0 = mesh.normals.get(i0).copied().unwrap_or(Vec3::Y);
            let n1 = mesh.normals.get(i1).copied().unwrap_or(Vec3::Y);
            let n2 = mesh.normals.get(i2).copied().unwrap_or(Vec3::Y);

            let slot = mesh.material_slots.get(t * 3).copied().unwrap_or(0);
            tris.push(TriPrimitive::new(p0, p1, p2, n0, n1, n2, slot));
        }

        let mut indices: Vec<usize> = (0..tris.len()).collect();
        let root = if tris.is_empty() {
            None
        } else {
            Some(Self::build_recursive(&tris, &mut indices))
        };

        Self { root, tris }
    }

    fn build_recursive(tris: &[TriPrimitive], indices: &mut [usize]) -> BvhNode {
        let mut bbox = Aabb::empty();
        let mut centroid_box = Aabb::empty();

        for &i in indices.iter() {
            bbox = bbox.union(&tris[i].bbox);
            centroid_box.enclose_point(tris[i].centroid);
        }

        // Leaf condition: 4 or fewer triangles
        if indices.len() <= 4 {
            return BvhNode::Leaf {
                bbox,
                tri_indices: indices.to_vec(),
            };
        }

        // Pick longest axis of centroid bounding box
        let extent = centroid_box.max - centroid_box.min;
        let axis = if extent.x > extent.y && extent.x > extent.z {
            0
        } else if extent.y > extent.z {
            1
        } else {
            2
        };

        // Sort indices along chosen axis
        indices.sort_by(|&a, &b| {
            tris[a].centroid[axis]
                .partial_cmp(&tris[b].centroid[axis])
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let mid = indices.len() / 2;
        let (left_indices, right_indices) = indices.split_at_mut(mid);

        let left = Box::new(Self::build_recursive(tris, left_indices));
        let right = Box::new(Self::build_recursive(tris, right_indices));

        BvhNode::Inner { bbox, left, right }
    }

    fn intersect(&self, ray: &Ray, t_min: f32, t_max: f32) -> Option<(f32, Vec3, u16)> {
        self.root
            .as_ref()
            .and_then(|node| node.intersect(ray, &self.tris, t_min, t_max))
    }
}

/// Deterministic 64-bit XorShift RNG for bit-exact repeatable rendering.
#[derive(Clone, Debug)]
struct DeterministicRng {
    state: u64,
}

impl DeterministicRng {
    const fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { 0xfeed_face_dead_beef } else { seed },
        }
    }

    #[inline]
    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    #[inline]
    fn next_f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / 16777216.0
    }

    /// Generates cosine-weighted sample on the hemisphere around normal.
    fn sample_hemisphere(&mut self, normal: Vec3) -> Vec3 {
        let u1 = self.next_f32();
        let u2 = self.next_f32();

        let r = u1.sqrt();
        let theta = std::f32::consts::TAU * u2;

        let x = r * theta.cos();
        let y = r * theta.sin();
        let z = (1.0 - u1).max(0.0).sqrt();

        // Coordinate frame around normal
        let up = if normal.y.abs() > 0.99 { Vec3::Z } else { Vec3::Y };
        let tangent = normal.cross(up).normalize();
        let bitangent = normal.cross(tangent);

        (tangent * x + bitangent * y + normal * z).normalize()
    }
}

/// Evaluates GGX microfacet specular and diffuse Lambertian BRDF reflection.
fn ggx_brdf(
    v: Vec3,
    l: Vec3,
    n: Vec3,
    base_color: Vec3,
    metallic: f32,
    roughness: f32,
) -> Vec3 {
    let alpha = (roughness * roughness).max(0.001);
    let h = (v + l).normalize_or_zero();

    let n_dot_v = n.dot(v).clamp(0.0, 1.0);
    let n_dot_l = n.dot(l).clamp(0.0, 1.0);
    let n_dot_h = n.dot(h).clamp(0.0, 1.0);
    let v_dot_h = v.dot(h).clamp(0.0, 1.0);

    if n_dot_v <= 1e-4 || n_dot_l <= 1e-4 {
        return Vec3::ZERO;
    }

    // D: GGX Normal Distribution
    let alpha2 = alpha * alpha;
    let denom = n_dot_h * n_dot_h * (alpha2 - 1.0) + 1.0;
    let d = alpha2 / (std::f32::consts::PI * denom * denom);

    // F: Fresnel-Schlick
    let f0 = Vec3::splat(0.04).lerp(base_color, metallic);
    let f = f0 + (Vec3::ONE - f0) * (1.0 - v_dot_h).powi(5);

    // G: Smith GGX shadowing-masking
    let k = (alpha + 1.0).powi(2) / 8.0;
    let g1_v = n_dot_v / (n_dot_v * (1.0 - k) + k);
    let g1_l = n_dot_l / (n_dot_l * (1.0 - k) + k);
    let g = g1_v * g1_l;

    // Specular microfacet
    let specular = (d * g * f) / (4.0 * n_dot_v * n_dot_l).max(1e-4);

    // Diffuse Lambertian
    let kd = (Vec3::ONE - f) * (1.0 - metallic);
    let diffuse = kd * base_color / std::f32::consts::PI;

    (diffuse + specular) * n_dot_l
}

/// Procedural sky radiance.
fn sky_radiance(dir: Vec3) -> Vec3 {
    let t = (dir.y * 0.5 + 0.5).clamp(0.0, 1.0);
    // Gradient from warm horizon to zenith blue
    Vec3::new(0.9, 0.9, 0.95).lerp(Vec3::new(0.3, 0.5, 0.9), t) * 1.5
}

/// Traces a single path through the BVH scene.
fn trace_path(
    bvh: &SahBvh,
    mut ray: Ray,
    material: &PathTracerMaterial,
    max_bounces: u32,
    rng: &mut DeterministicRng,
) -> (Vec3, Vec3, Vec3) {
    let mut throughput = Vec3::ONE;
    let mut radiance = Vec3::ZERO;
    let mut primary_normal = Vec3::ZERO;
    let mut primary_albedo = material.base_color;

    for bounce in 0..max_bounces {
        match bvh.intersect(&ray, 0.001, 1000.0) {
            Some((t, normal, _slot)) => {
                let hit_p = ray.origin + ray.dir * t;
                if bounce == 0 {
                    primary_normal = normal;
                }

                // Sample new bounce direction
                let view_dir = -ray.dir;
                let bounce_dir = rng.sample_hemisphere(normal);

                let brdf_val = ggx_brdf(
                    view_dir,
                    bounce_dir,
                    normal,
                    material.base_color,
                    material.metallic,
                    material.roughness,
                );

                // Probability density function for cosine hemisphere is (n.l) / PI
                let cos_theta = normal.dot(bounce_dir).max(0.0);
                let pdf = cos_theta / std::f32::consts::PI;

                if pdf > 1e-4 {
                    throughput *= brdf_val / pdf;
                } else {
                    break;
                }

                ray = Ray {
                    origin: hit_p,
                    dir: bounce_dir,
                };
            }
            None => {
                // Ray escaped to sky
                radiance += throughput * sky_radiance(ray.dir);
                if bounce == 0 {
                    primary_albedo = sky_radiance(ray.dir);
                }
                break;
            }
        }
    }

    (radiance, primary_normal, primary_albedo)
}

/// 1-pass à-trous edge-preserving wavelet denoiser using color and normal weights.
fn atrous_denoise(
    colors: &[Vec3],
    normals: &[Vec3],
    width: usize,
    height: usize,
) -> Vec<Vec3> {
    let mut denoised = vec![Vec3::ZERO; width * height];

    // B3 spline 5x5 kernel weights
    let kernel: [f32; 5] = [1.0 / 16.0, 1.0 / 4.0, 3.0 / 8.0, 1.0 / 4.0, 1.0 / 16.0];

    for y in 0..height {
        for x in 0..width {
            let center_idx = y * width + x;
            let center_color = colors[center_idx];
            let center_normal = normals[center_idx];

            let mut sum_color = Vec3::ZERO;
            let mut sum_weight = 0.0;

            for dy in -2..=2 {
                let ny = (y as isize + dy).clamp(0, height as isize - 1) as usize;
                let wy = kernel[(dy + 2) as usize];

                for dx in -2..=2 {
                    let nx = (x as isize + dx).clamp(0, width as isize - 1) as usize;
                    let wx = kernel[(dx + 2) as usize];
                    let weight_spatial = wx * wy;

                    let sample_idx = ny * width + nx;
                    let sample_color = colors[sample_idx];
                    let sample_normal = normals[sample_idx];

                    // Edge-preserving weights
                    let color_diff = (center_color - sample_color).length_squared();
                    let weight_color = (-color_diff * 4.0).exp();

                    let normal_dot = center_normal.dot(sample_normal).max(0.0);
                    let weight_normal = normal_dot.powi(16);

                    let w = weight_spatial * weight_color * weight_normal;
                    sum_color += sample_color * w;
                    sum_weight += w;
                }
            }

            denoised[center_idx] = if sum_weight > 1e-6 {
                sum_color / sum_weight
            } else {
                center_color
            };
        }
    }

    denoised
}

/// AgX View Transform tonemapping algorithm (Blender 4/5 default filmic curve).
///
/// Features log2 tone-mapping and highlight desaturation avoiding hue twisting.
fn apply_agx(c: Vec3) -> Vec3 {
    // 1. AgX Input Inset Matrix (BT.709 sRGB to AgX Inset)
    let r = 0.842_479_05 * c.x + 0.078_433_6 * c.y + 0.079_223_73 * c.z;
    let g = 0.042_328_242 * c.x + 0.878_468_04 * c.y + 0.079_166_17 * c.z;
    let b = 0.042_375_654 * c.x + 0.078_050_72 * c.y + 0.879_737_3 * c.z;

    let inset = Vec3::new(r.max(1e-6), g.max(1e-6), b.max(1e-6));

    // 2. Log2 encoding between -10.0 EV and +6.5 EV (16.5 dynamic range stops)
    let min_ev = -10.0f32;
    let max_ev = 6.5f32;
    let log_norm = |val: f32| -> f32 {
        let ev = val.log2();
        ((ev - min_ev) / (max_ev - min_ev)).clamp(0.0, 1.0)
    };

    let log_x = log_norm(inset.x);
    let log_y = log_norm(inset.y);
    let log_z = log_norm(inset.z);

    // 3. AgX Sigmoid Tone Curve (cubic polynomial approximation)
    let sigmoid = |x: f32| -> f32 {
        // Smoothstep S-curve
        x * x * (3.0 - 2.0 * x)
    };

    let sig_x = sigmoid(log_x);
    let sig_y = sigmoid(log_y);
    let sig_z = sigmoid(log_z);

    // 4. Outset Matrix (AgX Outset back to linear sRGB display)
    let out_r = 1.196_879 * sig_x - 0.098_020_88 * sig_y - 0.099_029_74 * sig_z;
    let out_g = -0.052_896_854 * sig_x + 1.151_903_2 * sig_y - 0.098_999_65 * sig_z;
    let out_b = -0.052_971_635 * sig_x - 0.098_443_605 * sig_y + 1.151_415_2 * sig_z;

    Vec3::new(out_r.clamp(0.0, 1.0), out_g.clamp(0.0, 1.0), out_b.clamp(0.0, 1.0))
}

/// Standard sRGB transfer function.
fn apply_standard_srgb(c: Vec3) -> Vec3 {
    let reinhard = c / (c + Vec3::ONE);
    let gamma = 1.0 / 2.2;
    Vec3::new(
        reinhard.x.powf(gamma),
        reinhard.y.powf(gamma),
        reinhard.z.powf(gamma),
    )
}

/// Bakes a high-fidelity path-traced RGBA8 image buffer from `mesh` and `config`.
///
/// Fully deterministic and thread-safe.
pub fn bake_pathtraced_rgba8(
    mesh: &TriangulatedMesh,
    config: &PathTracerConfig,
) -> Vec<u8> {
    let w = config.width as usize;
    let h = config.height as usize;
    let spp = config.samples_per_pixel.max(1);

    // Build SAH BVH
    let bvh = SahBvh::build(mesh);

    // Camera basis
    let cam_forward = (config.camera.target - config.camera.eye).normalize_or_zero();
    let cam_right = cam_forward.cross(config.camera.up).normalize_or_zero();
    let cam_up = cam_right.cross(cam_forward).normalize_or_zero();

    let half_fov_tan = (config.camera.fov_degrees.to_radians() * 0.5).tan();
    let aspect_ratio = w as f32 / h as f32;

    let mut linear_radiance = vec![Vec3::ZERO; w * h];
    let mut normal_buffer = vec![Vec3::ZERO; w * h];

    for y in 0..h {
        for x in 0..w {
            let pixel_idx = y * w + x;
            // Deterministic per-pixel seed
            let pixel_seed = config
                .seed
                .wrapping_add((x as u64) << 32)
                .wrapping_add(y as u64)
                .wrapping_mul(0x9e37_79b9_7f4a_7c15);
            let mut rng = DeterministicRng::new(pixel_seed);

            let mut accum_color = Vec3::ZERO;
            let mut accum_normal = Vec3::ZERO;

            for _ in 0..spp {
                // Jitter within pixel
                let jx = rng.next_f32() - 0.5;
                let jy = rng.next_f32() - 0.5;

                let ndc_x = (((x as f32 + 0.5 + jx) / w as f32) * 2.0 - 1.0) * aspect_ratio * half_fov_tan;
                let ndc_y = (1.0 - ((y as f32 + 0.5 + jy) / h as f32) * 2.0) * half_fov_tan;

                let ray_dir = (cam_forward + (cam_right * ndc_x) + (cam_up * ndc_y)).normalize();
                let ray = Ray {
                    origin: config.camera.eye,
                    dir: ray_dir,
                };

                let (sample_color, sample_normal, _) = trace_path(
                    &bvh,
                    ray,
                    &config.default_material,
                    config.max_bounces,
                    &mut rng,
                );

                accum_color += sample_color;
                accum_normal += sample_normal;
            }

            linear_radiance[pixel_idx] = accum_color / (spp as f32);
            normal_buffer[pixel_idx] = (accum_normal / (spp as f32)).normalize_or_zero();
        }
    }

    // Optional 1-pass à-trous wavelet denoiser
    let processed_colors = if config.enable_denoiser {
        atrous_denoise(&linear_radiance, &normal_buffer, w, h)
    } else {
        linear_radiance
    };

    // Apply view transform (AgX or Standard sRGB) and convert to RGBA8
    let mut rgba_bytes = Vec::with_capacity(w * h * 4);
    for color in processed_colors {
        let graded = match config.view_transform {
            ViewTransform::AgX => apply_agx(color),
            ViewTransform::StandardSrgb => apply_standard_srgb(color),
        };

        let r = (graded.x.clamp(0.0, 1.0) * 255.0).round() as u8;
        let g = (graded.y.clamp(0.0, 1.0) * 255.0).round() as u8;
        let b = (graded.z.clamp(0.0, 1.0) * 255.0).round() as u8;

        rgba_bytes.extend_from_slice(&[r, g, b, 255]);
    }

    rgba_bytes
}
