//! 2D Physics Server, CharacterBody2D move_and_slide, and 2D raycasting.

use foreplay_ir::NodeId;

/// 2D collision shape.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Collider2D {
    Circle { radius: f32 },
    Rect { half_extents: [f32; 2] },
}

/// 2D dynamic or kinematic physics body.
#[derive(Clone, PartialEq, Debug)]
pub struct PhysicsBody2D {
    pub id: NodeId,
    pub position: [f32; 2],
    pub velocity: [f32; 2],
    pub collider: Collider2D,
    pub collision_layer: u32,
    pub collision_mask: u32,
}

impl PhysicsBody2D {
    #[must_use]
    pub fn new(id: NodeId, position: [f32; 2], collider: Collider2D) -> Self {
        Self {
            id,
            position,
            velocity: [0.0, 0.0],
            collider,
            collision_layer: 1,
            collision_mask: 1,
        }
    }
}

/// Static 2D rectangle obstacle.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct StaticRect2D {
    pub min: [f32; 2],
    pub max: [f32; 2],
    pub collision_layer: u32,
}

/// Result of a 2D raycast query.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct RayHit2D {
    pub point: [f32; 2],
    pub normal: [f32; 2],
    pub distance: f32,
}

/// 2D Character controller with Godot 4 `move_and_slide` parity.
#[derive(Clone, PartialEq, Debug)]
pub struct CharacterBody2D {
    pub position: [f32; 2],
    pub velocity: [f32; 2],
    pub up_direction: [f32; 2],
    pub floor_max_angle: f32,
    pub is_on_floor: bool,
    pub is_on_wall: bool,
    pub is_on_ceiling: bool,
    pub floor_normal: [f32; 2],
    pub wall_normal: [f32; 2],
}

impl Default for CharacterBody2D {
    fn default() -> Self {
        Self::new([0.0, 0.0])
    }
}

impl CharacterBody2D {
    #[must_use]
    pub fn new(position: [f32; 2]) -> Self {
        Self {
            position,
            velocity: [0.0, 0.0],
            up_direction: [0.0, 1.0],
            floor_max_angle: std::f32::consts::FRAC_PI_4, // 45 deg
            is_on_floor: false,
            is_on_wall: false,
            is_on_ceiling: false,
            floor_normal: [0.0, 1.0],
            wall_normal: [0.0, 0.0],
        }
    }

    /// Godot 4 2D `move_and_slide` with slope sliding and wall collision.
    pub fn move_and_slide(&mut self, dt_seconds: f32, obstacles: &[StaticRect2D]) -> [f32; 2] {
        self.is_on_floor = false;
        self.is_on_wall = false;
        self.is_on_ceiling = false;

        let mut remaining = [self.velocity[0] * dt_seconds, self.velocity[1] * dt_seconds];

        for _ in 0..4 {
            let motion_len = (remaining[0] * remaining[0] + remaining[1] * remaining[1]).sqrt();
            if motion_len <= 1e-5 {
                break;
            }

            // Test collision against ground plane Y = 0 and static rect obstacles
            let mut earliest_t = 1.0_f32;
            let mut best_normal = [0.0, 1.0];
            let mut hit = false;

            // Ground plane check at Y = 0
            if remaining[1] < 0.0 && self.position[1] >= 0.0 {
                let t = (self.position[1] - 0.0) / (-remaining[1]);
                if (0.0..earliest_t).contains(&t) {
                    earliest_t = t;
                    best_normal = [0.0, 1.0];
                    hit = true;
                }
            }

            // Static rect obstacles
            let char_half = [0.25_f32, 0.5_f32];
            for obs in obstacles {
                let exp_min = [obs.min[0] - char_half[0], obs.min[1] - char_half[1]];
                let exp_max = [obs.max[0] + char_half[0], obs.max[1] + char_half[1]];

                if let Some((t, n)) = swept_point_aabb_2d(self.position, remaining, exp_min, exp_max).filter(|&(t, _)| (0.0..earliest_t).contains(&t)) {
                    earliest_t = t;
                    best_normal = n;
                    hit = true;
                }
            }

            if hit {
                self.position[0] += remaining[0] * earliest_t.max(0.0) + best_normal[0] * 0.001;
                self.position[1] += remaining[1] * earliest_t.max(0.0) + best_normal[1] * 0.001;

                // Normal classification
                let dot_up = best_normal[0] * self.up_direction[0] + best_normal[1] * self.up_direction[1];
                let cos_floor = self.floor_max_angle.cos();

                if dot_up >= cos_floor {
                    self.is_on_floor = true;
                    self.floor_normal = best_normal;
                } else if dot_up <= -cos_floor {
                    self.is_on_ceiling = true;
                } else {
                    self.is_on_wall = true;
                    self.wall_normal = best_normal;
                }

                // Project remaining velocity along tangent
                let rem_factor = 1.0 - earliest_t;
                let rem_vx = remaining[0] * rem_factor;
                let rem_vy = remaining[1] * rem_factor;
                let dot_rem = rem_vx * best_normal[0] + rem_vy * best_normal[1];
                remaining[0] = rem_vx - best_normal[0] * dot_rem;
                remaining[1] = rem_vy - best_normal[1] * dot_rem;

                let dot_vel = self.velocity[0] * best_normal[0] + self.velocity[1] * best_normal[1];
                self.velocity[0] -= best_normal[0] * dot_vel;
                self.velocity[1] -= best_normal[1] * dot_vel;
            } else {
                self.position[0] += remaining[0];
                self.position[1] += remaining[1];
                break;
            }
        }

        self.velocity
    }
}

fn swept_point_aabb_2d(pos: [f32; 2], dir: [f32; 2], min: [f32; 2], max: [f32; 2]) -> Option<(f32, [f32; 2])> {
    let mut tmin = f32::NEG_INFINITY;
    let mut tmax = f32::INFINITY;
    let mut normal = [0.0, 0.0];

    for i in 0..2 {
        if dir[i].abs() < 1e-6 {
            if pos[i] < min[i] || pos[i] > max[i] {
                return None;
            }
        } else {
            let inv_d = 1.0 / dir[i];
            let mut t1 = (min[i] - pos[i]) * inv_d;
            let mut t2 = (max[i] - pos[i]) * inv_d;
            let mut n = [0.0, 0.0];
            if i == 0 {
                n[0] = if inv_d < 0.0 { 1.0 } else { -1.0 };
            } else {
                n[1] = if inv_d < 0.0 { 1.0 } else { -1.0 };
            }

            if t1 > t2 {
                std::mem::swap(&mut t1, &mut t2);
                if i == 0 {
                    n[0] = -n[0];
                } else {
                    n[1] = -n[1];
                }
            }

            if t1 > tmin {
                tmin = t1;
                normal = n;
            }
            tmax = tmax.min(t2);

            if tmin > tmax {
                return None;
            }
        }
    }

    if (0.0..=1.0).contains(&tmin) {
        Some((tmin, normal))
    } else {
        None
    }
}

/// 2D Physics Server for 2D platformers and HUD mini-games.
#[derive(Debug, Default)]
pub struct PhysicsServer2D {
    pub bodies: Vec<PhysicsBody2D>,
    pub static_rects: Vec<StaticRect2D>,
}

impl PhysicsServer2D {
    #[must_use]
    pub fn new() -> Self {
        Self {
            bodies: Vec::new(),
            static_rects: Vec::new(),
        }
    }

    pub fn add_body(&mut self, body: PhysicsBody2D) {
        self.bodies.retain(|b| b.id != body.id);
        self.bodies.push(body);
    }

    pub fn add_static_rect(&mut self, min: [f32; 2], max: [f32; 2], layer: u32) {
        self.static_rects.push(StaticRect2D { min, max, collision_layer: layer });
    }

    /// Cast a 2D ray against static rects and bodies.
    #[must_use]
    pub fn ray_cast_2d(&self, from: [f32; 2], to: [f32; 2]) -> Option<RayHit2D> {
        let dir = [to[0] - from[0], to[1] - from[1]];
        let len = (dir[0] * dir[0] + dir[1] * dir[1]).sqrt();
        if len <= 1e-6 {
            return None;
        }

        let mut closest_t = 1.0_f32;
        let mut closest_norm = [0.0, 1.0];
        let mut hit = false;

        for rect in &self.static_rects {
            if let Some((t, n)) = swept_point_aabb_2d(from, dir, rect.min, rect.max).filter(|&(t, _)| (0.0..closest_t).contains(&t)) {
                closest_t = t;
                closest_norm = n;
                hit = true;
            }
        }

        if hit {
            Some(RayHit2D {
                point: [from[0] + dir[0] * closest_t, from[1] + dir[1] * closest_t],
                normal: closest_norm,
                distance: len * closest_t,
            })
        } else {
            None
        }
    }
}
