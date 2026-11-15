//! Render pipeline utilities: 6-plane Frustum extraction, fast SoA AABB culling,
//! deterministic 3D particle simulation (`GpuParticles3DSim`), and Viewport debug draw modes.

use bevy_math::{Mat4, Vec3};

/// Normalized 3D plane equation: $n \cdot x + d = 0$.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Plane {
    pub normal: Vec3,
    pub distance: f32,
}

impl Plane {
    /// Create plane from normal and distance, ensuring normal is normalized.
    #[must_use]
    pub fn new(normal: Vec3, distance: f32) -> Self {
        let len = normal.length();
        if len > 1e-6 {
            Self {
                normal: normal / len,
                distance: distance / len,
            }
        } else {
            Self {
                normal: Vec3::Y,
                distance: 0.0,
            }
        }
    }

    /// Signed distance from plane to point.
    #[must_use]
    pub fn distance_to(&self, point: Vec3) -> f32 {
        self.normal.dot(point) + self.distance
    }
}

/// 6-plane view frustum for fast hierarchical culling.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Frustum {
    pub planes: [Plane; 6], // Left, Right, Bottom, Top, Near, Far
}

impl Frustum {
    /// Extract the 6 bounding frustum planes from a combined view-projection matrix
    /// (Right-handed coordinates with [0, 1] depth range).
    #[must_use]
    pub fn from_view_projection(vp: &Mat4) -> Self {
        let r0 = vp.row(0);
        let r1 = vp.row(1);
        let r2 = vp.row(2);
        let r3 = vp.row(3);

        // Left plane: row3 + row0
        let p_left = r3 + r0;
        let left = Plane::new(Vec3::new(p_left.x, p_left.y, p_left.z), p_left.w);

        // Right plane: row3 - row0
        let p_right = r3 - r0;
        let right = Plane::new(Vec3::new(p_right.x, p_right.y, p_right.z), p_right.w);

        // Bottom plane: row3 + row1
        let p_bottom = r3 + r1;
        let bottom = Plane::new(Vec3::new(p_bottom.x, p_bottom.y, p_bottom.z), p_bottom.w);

        // Top plane: row3 - row1
        let p_top = r3 - r1;
        let top = Plane::new(Vec3::new(p_top.x, p_top.y, p_top.z), p_top.w);

        // Near plane: row2 (for [0, 1] Vulkan/wgpu depth range)
        let near = Plane::new(Vec3::new(r2.x, r2.y, r2.z), r2.w);

        // Far plane: row3 - row2
        let p_far = r3 - r2;
        let far = Plane::new(Vec3::new(p_far.x, p_far.y, p_far.z), p_far.w);

        Self {
            planes: [left, right, bottom, top, near, far],
        }
    }

    /// Fast conservative AABB vs Frustum intersection test.
    #[must_use]
    pub fn intersects_aabb(&self, min: Vec3, max: Vec3) -> bool {
        for plane in &self.planes {
            // Find positive vertex along plane normal
            let px = if plane.normal.x > 0.0 { max.x } else { min.x };
            let py = if plane.normal.y > 0.0 { max.y } else { min.y };
            let pz = if plane.normal.z > 0.0 { max.z } else { min.z };

            if plane.distance_to(Vec3::new(px, py, pz)) < 0.0 {
                return false;
            }
        }
        true
    }
}

/// Fast Structure-of-Arrays (SoA) frustum culling batch kernel.
/// Appends visible indices into `visible_indices` without reallocating buffer capacity.
pub fn cull_aabbs_frustum(
    frustum: &Frustum,
    aabbs_min: &[Vec3],
    aabbs_max: &[Vec3],
    visible_indices: &mut Vec<usize>,
) {
    let count = aabbs_min.len().min(aabbs_max.len());
    visible_indices.clear();

    for i in 0..count {
        if frustum.intersects_aabb(aabbs_min[i], aabbs_max[i]) {
            visible_indices.push(i);
        }
    }
}

/// Emission shape for 3D particles.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum EmissionShape {
    Point,
    Sphere { radius: f32 },
    Box { half_extents: Vec3 },
}

/// Individual particle state.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Particle3D {
    pub position: Vec3,
    pub velocity: Vec3,
    pub color: [f32; 4],
    pub age: f32,
    pub lifetime: f32,
    pub alive: bool,
}

/// Deterministic CPU/compute-ready 3D particle emitter simulation.
#[derive(Clone, Debug)]
pub struct GpuParticles3DSim {
    pub emission_shape: EmissionShape,
    pub emission_rate: f32, // particles per second
    pub lifetime: f32,
    pub initial_velocity: Vec3,
    pub velocity_spread: f32,
    pub gravity: Vec3,
    pub damping: f32,
    pub color_ramp_start: [f32; 4],
    pub color_ramp_end: [f32; 4],
    particles: Vec<Particle3D>,
    emission_accumulator: f32,
    prng_state: u64,
}

impl GpuParticles3DSim {
    /// Create a particle emitter with a fixed capacity pool.
    #[must_use]
    pub fn new(max_particles: usize) -> Self {
        let count = max_particles.max(16);
        let mut particles = Vec::with_capacity(count);
        for _ in 0..count {
            particles.push(Particle3D {
                position: Vec3::ZERO,
                velocity: Vec3::ZERO,
                color: [1.0, 1.0, 1.0, 1.0],
                age: 0.0,
                lifetime: 1.0,
                alive: false,
            });
        }

        Self {
            emission_shape: EmissionShape::Point,
            emission_rate: 50.0,
            lifetime: 2.0,
            initial_velocity: Vec3::new(0.0, 5.0, 0.0),
            velocity_spread: 1.0,
            gravity: Vec3::new(0.0, -9.81, 0.0),
            damping: 0.1,
            color_ramp_start: [1.0, 0.8, 0.2, 1.0],
            color_ramp_end: [0.8, 0.1, 0.0, 0.0],
            particles,
            emission_accumulator: 0.0,
            prng_state: 0x853c_49e6_748f_ea9b,
        }
    }

    /// Step simulation forward by `dt_seconds`.
    pub fn step(&mut self, dt_seconds: f32) {
        if dt_seconds <= 0.0 || !dt_seconds.is_finite() {
            return;
        }

        // 1. Update existing living particles
        for p in &mut self.particles {
            if !p.alive {
                continue;
            }

            p.age += dt_seconds;
            if p.age >= p.lifetime {
                p.alive = false;
                continue;
            }

            let t = (p.age / p.lifetime).clamp(0.0, 1.0);
            p.color = [
                self.color_ramp_start[0] + (self.color_ramp_end[0] - self.color_ramp_start[0]) * t,
                self.color_ramp_start[1] + (self.color_ramp_end[1] - self.color_ramp_start[1]) * t,
                self.color_ramp_start[2] + (self.color_ramp_end[2] - self.color_ramp_start[2]) * t,
                self.color_ramp_start[3] + (self.color_ramp_end[3] - self.color_ramp_start[3]) * t,
            ];

            p.velocity += self.gravity * dt_seconds;
            p.velocity *= (1.0 - self.damping * dt_seconds).clamp(0.0, 1.0);
            p.position += p.velocity * dt_seconds;
        }

        // 2. Spawn new particles
        self.emission_accumulator += self.emission_rate * dt_seconds;
        let spawn_count = self.emission_accumulator.floor() as usize;
        self.emission_accumulator -= spawn_count as f32;

        let mut prng = self.prng_state;
        let mut next_rnd = || -> f32 {
            prng ^= prng << 13;
            prng ^= prng >> 7;
            prng ^= prng << 17;
            (prng as f32) / (u64::MAX as f32)
        };

        let mut spawned = 0;
        for p in &mut self.particles {
            if spawned >= spawn_count {
                break;
            }
            if !p.alive {
                p.alive = true;
                p.age = 0.0;
                p.lifetime = self.lifetime;
                p.color = self.color_ramp_start;

                // Position based on emission shape
                let r1 = next_rnd() * 2.0 - 1.0;
                let r2 = next_rnd() * 2.0 - 1.0;
                let r3 = next_rnd() * 2.0 - 1.0;

                p.position = match self.emission_shape {
                    EmissionShape::Point => Vec3::ZERO,
                    EmissionShape::Sphere { radius } => {
                        Vec3::new(r1, r2, r3).normalize_or_zero() * (radius * next_rnd())
                    }
                    EmissionShape::Box { half_extents } => {
                        Vec3::new(r1 * half_extents.x, r2 * half_extents.y, r3 * half_extents.z)
                    }
                };

                let spread_vec = Vec3::new(
                    (next_rnd() * 2.0 - 1.0) * self.velocity_spread,
                    (next_rnd() * 2.0 - 1.0) * self.velocity_spread,
                    (next_rnd() * 2.0 - 1.0) * self.velocity_spread,
                );
                p.velocity = self.initial_velocity + spread_vec;

                spawned += 1;
            }
        }
        self.prng_state = prng;
    }

    /// Access active particle slice.
    #[must_use]
    pub fn particles(&self) -> &[Particle3D] {
        &self.particles
    }

    /// Count active living particles.
    #[must_use]
    pub fn active_count(&self) -> usize {
        self.particles.iter().filter(|p| p.alive).count()
    }
}

/// Viewport debug draw modes matching Godot 4 editor viewport visualization options.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ViewportDebugDraw {
    /// Normal shaded PBR lighting.
    #[default]
    LitPbr,
    /// Unshaded diffuse albedo only.
    Unshaded,
    /// Wireframe geometry rasterization.
    Wireframe,
    /// Overdraw complexity heatmap.
    Overdraw,
    /// Physics collision shapes overlaid.
    CollisionShapes,
}
