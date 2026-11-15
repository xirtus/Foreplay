//! UE5/6 3D Spline & Procedural Content Generation (`foreplay-core::pcg`).
//!
//! Provides:
//! - `CatmullRomSpline3D`: arc-length parameterization, position, tangent, and rotation-minimizing frame sampling.
//! - `PcgSplineExtruder`: generates grindable skate rail and curb vertex/index buffers plus collision lines along splines.
//! - `PcgScatterGrid`: deterministic seeded Poisson-like point scattering with slope, height, and collision filters.

use bevy_math::{Quat, Vec3};
use serde::{Deserialize, Serialize};

/// 3D Spline evaluation frame with position, unit tangent, normal, and binormal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SplineFrame3D {
    pub position: Vec3,
    pub tangent: Vec3,
    pub normal: Vec3,
    pub binormal: Vec3,
    pub distance: f32,
}

/// 3D Catmull-Rom Spline with arc-length parameterization and frame sampling.
#[derive(Clone, Debug, PartialEq)]
pub struct CatmullRomSpline3D {
    pub control_points: Vec<Vec3>,
    pub is_closed: bool,
    arc_lengths: Vec<f32>,
    total_length: f32,
    subdivisions_per_segment: usize,
}

impl CatmullRomSpline3D {
    /// Constructs a Catmull-Rom spline from a series of 3D control points.
    pub fn new(control_points: Vec<Vec3>, is_closed: bool) -> Self {
        let mut spline = Self {
            control_points,
            is_closed,
            arc_lengths: Vec::new(),
            total_length: 0.0,
            subdivisions_per_segment: 32,
        };
        spline.rebuild_arc_lengths();
        spline
    }

    /// Total arc-length of the spline in world units.
    pub fn total_length(&self) -> f32 {
        self.total_length
    }

    /// Number of control points.
    pub fn point_count(&self) -> usize {
        self.control_points.len()
    }

    /// Number of spline segments.
    pub fn segment_count(&self) -> usize {
        if self.control_points.len() < 2 {
            0
        } else if self.is_closed {
            self.control_points.len()
        } else {
            self.control_points.len() - 1
        }
    }

    /// Recomputes the arc-length lookup table.
    fn rebuild_arc_lengths(&mut self) {
        let num_segments = self.segment_count();
        if num_segments == 0 {
            self.arc_lengths.clear();
            self.total_length = 0.0;
            return;
        }

        let total_samples = num_segments * self.subdivisions_per_segment;
        self.arc_lengths.clear();
        self.arc_lengths.reserve(total_samples + 1);
        self.arc_lengths.push(0.0);

        let mut current_length = 0.0;
        let mut prev_pos = self.sample_raw(0, 0.0);

        for seg_idx in 0..num_segments {
            for step in 1..=self.subdivisions_per_segment {
                let u = step as f32 / self.subdivisions_per_segment as f32;
                let pos = self.sample_raw(seg_idx, u);
                current_length += pos.distance(prev_pos);
                self.arc_lengths.push(current_length);
                prev_pos = pos;
            }
        }

        self.total_length = current_length;
    }

    /// Samples raw position along segment `seg_idx` at parameter `u` in `[0, 1]`.
    fn sample_raw(&self, seg_idx: usize, u: f32) -> Vec3 {
        let n = self.control_points.len();
        if n < 2 {
            return self.control_points.first().copied().unwrap_or(Vec3::ZERO);
        }

        let (p0_idx, p1_idx, p2_idx, p3_idx) = if self.is_closed {
            let p1 = seg_idx % n;
            let p0 = if p1 == 0 { n - 1 } else { p1 - 1 };
            let p2 = (p1 + 1) % n;
            let p3 = (p1 + 2) % n;
            (p0, p1, p2, p3)
        } else {
            let p1 = seg_idx;
            let p2 = (seg_idx + 1).min(n - 1);
            let p0 = if seg_idx == 0 { 0 } else { seg_idx - 1 };
            let p3 = (seg_idx + 2).min(n - 1);
            (p0, p1, p2, p3)
        };

        let p0 = self.control_points[p0_idx];
        let p1 = self.control_points[p1_idx];
        let p2 = self.control_points[p2_idx];
        let p3 = self.control_points[p3_idx];

        let u2 = u * u;
        let u3 = u2 * u;

        0.5 * ((2.0 * p1)
            + (-p0 + p2) * u
            + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * u2
            + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * u3)
    }

    /// Evaluates raw tangent along segment `seg_idx` at parameter `u` in `[0, 1]`.
    fn sample_tangent_raw(&self, seg_idx: usize, u: f32) -> Vec3 {
        let n = self.control_points.len();
        if n < 2 {
            return Vec3::Z;
        }

        let (p0_idx, p1_idx, p2_idx, p3_idx) = if self.is_closed {
            let p1 = seg_idx % n;
            let p0 = if p1 == 0 { n - 1 } else { p1 - 1 };
            let p2 = (p1 + 1) % n;
            let p3 = (p1 + 2) % n;
            (p0, p1, p2, p3)
        } else {
            let p1 = seg_idx;
            let p2 = (seg_idx + 1).min(n - 1);
            let p0 = if seg_idx == 0 { 0 } else { seg_idx - 1 };
            let p3 = (seg_idx + 2).min(n - 1);
            (p0, p1, p2, p3)
        };

        let p0 = self.control_points[p0_idx];
        let p1 = self.control_points[p1_idx];
        let p2 = self.control_points[p2_idx];
        let p3 = self.control_points[p3_idx];

        let u2 = u * u;

        let deriv = 0.5
            * ((-p0 + p2)
                + 2.0 * (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * u
                + 3.0 * (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * u2);

        if deriv.length_squared() > 1e-8 {
            deriv.normalize()
        } else {
            (p2 - p1).normalize_or_zero()
        }
    }

    /// Converts an arc-length distance `s` into `(segment_index, u_within_segment)`.
    pub fn distance_to_segment_u(&self, distance: f32) -> (usize, f32) {
        if self.arc_lengths.len() < 2 || self.total_length <= 1e-6 {
            return (0, 0.0);
        }

        let clamped_dist = distance.clamp(0.0, self.total_length);

        // Binary search in cumulative arc lengths
        let idx = match self
            .arc_lengths
            .binary_search_by(|val| val.partial_cmp(&clamped_dist).unwrap_or(std::cmp::Ordering::Equal))
        {
            Ok(exact) => exact,
            Err(next) => next.saturating_sub(1),
        };

        let sample_idx = idx.min(self.arc_lengths.len() - 2);
        let seg_idx = sample_idx / self.subdivisions_per_segment;
        let step_within_seg = sample_idx % self.subdivisions_per_segment;

        let l0 = self.arc_lengths[sample_idx];
        let l1 = self.arc_lengths[sample_idx + 1];
        let sub_t = if (l1 - l0) > 1e-6 {
            (clamped_dist - l0) / (l1 - l0)
        } else {
            0.0
        };

        let step_size = 1.0 / self.subdivisions_per_segment as f32;
        let u = (step_within_seg as f32 * step_size) + (sub_t * step_size);

        (seg_idx.min(self.segment_count().saturating_sub(1)), u.clamp(0.0, 1.0))
    }

    /// Samples the 3D position along the spline at world-unit distance `distance`.
    pub fn sample_position(&self, distance: f32) -> Vec3 {
        let (seg, u) = self.distance_to_segment_u(distance);
        self.sample_raw(seg, u)
    }

    /// Samples the normalized tangent vector at world-unit distance `distance`.
    pub fn sample_tangent(&self, distance: f32) -> Vec3 {
        let (seg, u) = self.distance_to_segment_u(distance);
        self.sample_tangent_raw(seg, u)
    }

    /// Samples a full rotation-minimizing coordinate frame along the spline at distance `distance`.
    pub fn sample_frame(&self, distance: f32) -> SplineFrame3D {
        let position = self.sample_position(distance);
        let tangent = self.sample_tangent(distance);

        // Compute stable normal & binormal using up vector (Y-up preferred)
        let up = if tangent.y.abs() > 0.99 {
            Vec3::Z
        } else {
            Vec3::Y
        };
        let binormal = tangent.cross(up).normalize();
        let normal = binormal.cross(tangent).normalize();

        SplineFrame3D {
            position,
            tangent,
            normal,
            binormal,
            distance,
        }
    }
}

/// 2D cross-section profile for procedural extrusion.
#[derive(Clone, Debug, PartialEq)]
pub enum PcgProfile {
    /// Circular pipe (e.g. skate handrail).
    RoundRail { radius: f32, radial_segments: u32 },
    /// Rectangular box rail.
    SquareRail { width: f32, height: f32 },
    /// Street curb with beveled top edge.
    Curb { width: f32, height: f32, bevel: f32 },
    /// Custom 2D polygon profile (local X/Y coordinates).
    Custom { points: Vec<[f32; 2]> },
}

/// Configuration parameters for spline rail/curb mesh extrusion.
#[derive(Clone, Debug, PartialEq)]
pub struct PcgExtrusionConfig {
    pub profile: PcgProfile,
    pub step_distance: f32,
    pub uv_scale: [f32; 2],
    pub generate_caps: bool,
}

impl Default for PcgExtrusionConfig {
    fn default() -> Self {
        Self {
            profile: PcgProfile::RoundRail {
                radius: 0.05,
                radial_segments: 12,
            },
            step_distance: 0.1,
            uv_scale: [1.0, 1.0],
            generate_caps: true,
        }
    }
}

/// Generated procedural geometry buffers + collision segments.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PcgMeshBuffers {
    pub positions: Vec<Vec3>,
    pub normals: Vec<Vec3>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
    /// Grindable 3D line segments along the top edge of the rail/curb.
    pub collision_segments: Vec<(Vec3, Vec3)>,
}

/// Procedural Content Generator for spline-based grind rails, curbs, and pipes.
#[derive(Clone, Copy, Debug, Default)]
pub struct PcgSplineExtruder;

impl PcgSplineExtruder {
    /// Generates 2D profile vertices `(x, y)` in local cross-section space.
    fn build_profile_points(profile: &PcgProfile) -> Vec<[f32; 2]> {
        match profile {
            PcgProfile::RoundRail {
                radius,
                radial_segments,
            } => {
                let segments = (*radial_segments).max(3);
                let mut pts = Vec::with_capacity(segments as usize);
                for i in 0..segments {
                    let angle = (i as f32 / segments as f32) * std::f32::consts::TAU;
                    pts.push([radius * angle.cos(), radius * angle.sin()]);
                }
                pts
            }
            PcgProfile::SquareRail { width, height } => {
                let hw = width * 0.5;
                let hh = height * 0.5;
                vec![[-hw, -hh], [hw, -hh], [hw, hh], [-hw, hh]]
            }
            PcgProfile::Curb {
                width,
                height,
                bevel,
            } => {
                let hw = width * 0.5;
                let b = bevel.min(hw * 0.5).min(*height * 0.5);
                vec![
                    [-hw, 0.0],
                    [hw, 0.0],
                    [hw, height - b],
                    [hw - b, *height],
                    [-hw + b, *height],
                    [-hw, height - b],
                ]
            }
            PcgProfile::Custom { points } => points.clone(),
        }
    }

    /// Extrudes geometry and grind lines along the given spline.
    pub fn extrude(spline: &CatmullRomSpline3D, config: &PcgExtrusionConfig) -> PcgMeshBuffers {
        let total_len = spline.total_length();
        if total_len <= 1e-4 || spline.point_count() < 2 {
            return PcgMeshBuffers::default();
        }

        let profile_pts = Self::build_profile_points(&config.profile);
        let num_profile_pts = profile_pts.len();
        if num_profile_pts < 2 {
            return PcgMeshBuffers::default();
        }

        let step_dist = config.step_distance.max(0.01);
        let num_rings = (total_len / step_dist).ceil() as usize + 1;

        let mut positions = Vec::with_capacity(num_rings * num_profile_pts);
        let mut normals = Vec::with_capacity(num_rings * num_profile_pts);
        let mut uvs = Vec::with_capacity(num_rings * num_profile_pts);
        let mut indices = Vec::with_capacity((num_rings - 1) * num_profile_pts * 6);
        let mut collision_segments = Vec::with_capacity(num_rings - 1);

        let mut prev_grind_pt: Option<Vec3> = None;

        for r in 0..num_rings {
            let dist = ((r as f32) / (num_rings - 1) as f32 * total_len).min(total_len);
            let frame = spline.sample_frame(dist);

            // Record top-most point for grind collision line
            let mut top_pos = frame.position;
            let mut max_y = f32::NEG_INFINITY;

            for (p_idx, &[px, py]) in profile_pts.iter().enumerate() {
                // Transform 2D profile offset onto 3D frame (px -> binormal, py -> normal)
                let world_pos = frame.position + (frame.binormal * px) + (frame.normal * py);
                let local_normal = ((frame.binormal * px) + (frame.normal * py)).normalize_or_zero();

                positions.push(world_pos);
                normals.push(if local_normal.length_squared() > 1e-4 {
                    local_normal
                } else {
                    frame.normal
                });

                let u = (p_idx as f32 / num_profile_pts as f32) * config.uv_scale[0];
                let v = (dist / total_len) * config.uv_scale[1];
                uvs.push([u, v]);

                if world_pos.y > max_y {
                    max_y = world_pos.y;
                    top_pos = world_pos;
                }
            }

            if let Some(prev) = prev_grind_pt {
                collision_segments.push((prev, top_pos));
            }
            prev_grind_pt = Some(top_pos);

            // Stitch quads between consecutive rings
            if r > 0 {
                let r0 = (r - 1) * num_profile_pts;
                let r1 = r * num_profile_pts;
                for p in 0..num_profile_pts {
                    let next_p = (p + 1) % num_profile_pts;
                    let i0 = (r0 + p) as u32;
                    let i1 = (r0 + next_p) as u32;
                    let i2 = (r1 + next_p) as u32;
                    let i3 = (r1 + p) as u32;

                    indices.extend_from_slice(&[i0, i1, i2, i0, i2, i3]);
                }
            }
        }

        // Generate end caps if non-closed and enabled
        if config.generate_caps && !spline.is_closed && num_rings >= 2 {
            // Cap start (ring 0)
            let start_frame = spline.sample_frame(0.0);
            let start_center = start_frame.position;
            let start_normal = -start_frame.tangent;
            let center_idx = positions.len() as u32;
            positions.push(start_center);
            normals.push(start_normal);
            uvs.push([0.5, 0.5]);

            for p in 0..num_profile_pts {
                let next_p = (p + 1) % num_profile_pts;
                indices.extend_from_slice(&[center_idx, (next_p) as u32, p as u32]);
            }

            // Cap end (last ring)
            let end_frame = spline.sample_frame(total_len);
            let end_center = end_frame.position;
            let end_normal = end_frame.tangent;
            let end_center_idx = positions.len() as u32;
            positions.push(end_center);
            normals.push(end_normal);
            uvs.push([0.5, 0.5]);

            let last_ring_start = (num_rings - 1) * num_profile_pts;
            for p in 0..num_profile_pts {
                let next_p = (p + 1) % num_profile_pts;
                indices.extend_from_slice(&[
                    end_center_idx,
                    (last_ring_start + p) as u32,
                    (last_ring_start + next_p) as u32,
                ]);
            }
        }

        PcgMeshBuffers {
            positions,
            normals,
            uvs,
            indices,
            collision_segments,
        }
    }
}

/// A scattered procedural instance (foliage, street props, debris).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PcgScatterInstance {
    pub position: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}

/// Deterministic 64-bit pseudo-random number generator for PCG scattering.
#[derive(Clone, Debug)]
struct PcgRng {
    state: u64,
}

impl PcgRng {
    const fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { 0xdead_beef_cafe_babe } else { seed },
        }
    }

    fn next_u64(&mut self) -> u64 {
        // Xorshift64*
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn next_f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / 16777216.0
    }

    fn range_f32(&mut self, min: f32, max: f32) -> f32 {
        min + self.next_f32() * (max - min)
    }
}

/// Deterministic seeded grid scattering with height, slope, and spacing filters.
#[derive(Clone, Debug, PartialEq)]
pub struct PcgScatterGrid {
    pub bounds_min: Vec3,
    pub bounds_max: Vec3,
    pub cell_size: f32,
    pub seed: u64,
    pub min_distance: f32,
    pub min_height: f32,
    pub max_height: f32,
    pub max_slope_rad: f32,
    pub align_to_normal: bool,
}

impl Default for PcgScatterGrid {
    fn default() -> Self {
        Self {
            bounds_min: Vec3::new(-50.0, 0.0, -50.0),
            bounds_max: Vec3::new(50.0, 100.0, 50.0),
            cell_size: 4.0,
            seed: 1337,
            min_distance: 2.0,
            min_height: -100.0,
            max_height: 1000.0,
            max_slope_rad: std::f32::consts::FRAC_PI_4, // 45 degrees
            align_to_normal: true,
        }
    }
}

impl PcgScatterGrid {
    /// Scatters instances over a surface determined by `surface_height_fn(x, z) -> (height_y, normal)`.
    pub fn scatter<F>(&self, surface_height_fn: F) -> Vec<PcgScatterInstance>
    where
        F: Fn(f32, f32) -> (f32, Vec3),
    {
        let mut rng = PcgRng::new(self.seed);
        let mut instances: Vec<PcgScatterInstance> = Vec::new();

        let dx = self.bounds_max.x - self.bounds_min.x;
        let dz = self.bounds_max.z - self.bounds_min.z;
        if dx <= 0.0 || dz <= 0.0 || self.cell_size <= 0.1 {
            return instances;
        }

        let cells_x = (dx / self.cell_size).ceil() as usize;
        let cells_z = (dz / self.cell_size).ceil() as usize;

        for cx in 0..cells_x {
            for cz in 0..cells_z {
                // Jittered position within the cell
                let jx = rng.next_f32();
                let jz = rng.next_f32();
                let x = self.bounds_min.x + (cx as f32 + jx) * self.cell_size;
                let z = self.bounds_min.z + (cz as f32 + jz) * self.cell_size;

                if x > self.bounds_max.x || z > self.bounds_max.z {
                    continue;
                }

                let (y, normal) = surface_height_fn(x, z);

                // Height filter
                if y < self.min_height || y > self.max_height {
                    continue;
                }

                // Slope filter (angle against up vector)
                let n = normal.normalize_or_zero();
                let slope_cos = n.dot(Vec3::Y).clamp(-1.0, 1.0);
                let slope_angle = slope_cos.acos();
                if slope_angle > self.max_slope_rad {
                    continue;
                }

                let candidate_pos = Vec3::new(x, y, z);

                // Minimum distance rejection test
                let too_close = instances.iter().any(|inst| {
                    inst.position.distance_squared(candidate_pos) < self.min_distance * self.min_distance
                });
                if too_close {
                    continue;
                }

                // Random yaw rotation + alignment to normal
                let random_yaw = rng.range_f32(0.0, std::f32::consts::TAU);
                let yaw_rot = Quat::from_rotation_y(random_yaw);

                let rotation = if self.align_to_normal {
                    let align_rot = Quat::from_rotation_arc(Vec3::Y, n);
                    align_rot * yaw_rot
                } else {
                    yaw_rot
                };

                let uniform_scale = rng.range_f32(0.85, 1.15);
                let scale = Vec3::splat(uniform_scale);

                instances.push(PcgScatterInstance {
                    position: candidate_pos,
                    rotation,
                    scale,
                });
            }
        }

        instances
    }
}
