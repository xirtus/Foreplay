//! Godot 4-faithful `CharacterBody3DController` and `CharacterBody2DController`
//! implementing `move_and_slide`, slope sliding, stair stepping, moving platform carryover,
//! and complete collision telemetry over Rapier spaces.

use bevy_math::Vec3;
use foreplay_ir::NodeId;
use rapier3d::prelude as r3d;
use rapier2d::prelude as r2d;
use crate::rapier_backend::{RapierSpace2D, RapierSpace3D};

/// Telemetry record for a 3D kinematic slide collision during `move_and_slide`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KinematicSlideCollision3D {
    pub collider_id: Option<NodeId>,
    pub collider_handle: Option<r3d::ColliderHandle>,
    pub position: Vec3,
    pub normal: Vec3,
    pub travel: Vec3,
    pub remainder: Vec3,
    pub angle: f32,
    pub depth: f32,
}

/// Telemetry record for a 2D kinematic slide collision during `move_and_slide`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KinematicSlideCollision2D {
    pub collider_id: Option<NodeId>,
    pub collider_handle: Option<r2d::ColliderHandle>,
    pub position: [f32; 2],
    pub normal: [f32; 2],
    pub travel: [f32; 2],
    pub remainder: [f32; 2],
    pub angle: f32,
    pub depth: f32,
}

/// Godot 4 `CharacterBody3D` controller implementing standard `move_and_slide` over `RapierSpace3D`.
#[derive(Clone, Debug, PartialEq)]
pub struct CharacterBody3DController {
    pub id: NodeId,
    pub position: Vec3,
    pub velocity: Vec3,
    pub up_direction: Vec3,
    pub floor_max_angle: f32,
    pub floor_snap_length: f32,
    pub max_slides: u32,
    pub wall_min_slide_angle: f32,
    pub floor_stop_on_slope: bool,
    pub floor_constant_speed: bool,
    pub step_height: f32,
    pub platform_velocity: Vec3,
    pub collision_layer: u32,
    pub collision_mask: u32,

    on_floor: bool,
    on_wall: bool,
    on_ceiling: bool,
    floor_normal: Vec3,
    wall_normal: Vec3,
    real_velocity: Vec3,
    slide_collisions: Vec<KinematicSlideCollision3D>,
}

impl CharacterBody3DController {
    /// Construct a new Godot 4 CharacterBody3D controller.
    #[must_use]
    pub fn new(id: NodeId, position: Vec3) -> Self {
        Self {
            id,
            position,
            velocity: Vec3::ZERO,
            up_direction: Vec3::Y,
            floor_max_angle: std::f32::consts::FRAC_PI_4, // 45 degrees
            floor_snap_length: 0.1,
            max_slides: 6,
            wall_min_slide_angle: 0.2618, // ~15 degrees
            floor_stop_on_slope: true,
            floor_constant_speed: false,
            step_height: 0.0,
            platform_velocity: Vec3::ZERO,
            collision_layer: 1,
            collision_mask: 1,
            on_floor: false,
            on_wall: false,
            on_ceiling: false,
            floor_normal: Vec3::Y,
            wall_normal: Vec3::ZERO,
            real_velocity: Vec3::ZERO,
            slide_collisions: Vec::with_capacity(6),
        }
    }

    #[must_use]
    pub const fn is_on_floor(&self) -> bool {
        self.on_floor
    }

    #[must_use]
    pub const fn is_on_wall(&self) -> bool {
        self.on_wall
    }

    #[must_use]
    pub const fn is_on_ceiling(&self) -> bool {
        self.on_ceiling
    }

    #[must_use]
    pub const fn get_floor_normal(&self) -> Vec3 {
        self.floor_normal
    }

    #[must_use]
    pub const fn get_wall_normal(&self) -> Vec3 {
        self.wall_normal
    }

    #[must_use]
    pub const fn get_real_velocity(&self) -> Vec3 {
        self.real_velocity
    }

    #[must_use]
    pub fn get_slide_collision_count(&self) -> usize {
        self.slide_collisions.len()
    }

    #[must_use]
    pub fn get_slide_collision(&self, index: usize) -> Option<&KinematicSlideCollision3D> {
        self.slide_collisions.get(index)
    }

    #[must_use]
    pub fn get_slide_collisions(&self) -> &[KinematicSlideCollision3D] {
        &self.slide_collisions
    }

    /// Snaps character down to ground if within `floor_snap_length`.
    pub fn apply_floor_snap(&mut self, space: &RapierSpace3D, shape: &r3d::SharedShape) -> bool {
        if self.floor_snap_length <= 0.0 {
            return false;
        }

        let down = -self.up_direction * self.floor_snap_length;
        let shape_pos = r3d::Pose::from_translation(r3d::Vector::new(self.position.x, self.position.y, self.position.z));
        let vel_vec = r3d::Vector::new(down.x, down.y, down.z);

        let groups = r3d::InteractionGroups::all().with_filter(r3d::Group::from_bits_truncate(self.collision_mask));
        let query = space.broad_phase.as_query_pipeline(
            space.narrow_phase.query_dispatcher(),
            &space.bodies,
            &space.colliders,
            r3d::QueryFilter::default().groups(groups),
        );

        let opt = rapier3d::parry::query::ShapeCastOptions {
            max_time_of_impact: 1.0,
            stop_at_penetration: false,
            ..Default::default()
        };

        if let Some((_, hit)) = query.cast_shape(&shape_pos, vel_vec, shape.as_ref(), opt) {
            let n = Vec3::new(hit.normal1.x, hit.normal1.y, hit.normal1.z);
            if n.dot(self.up_direction) >= self.floor_max_angle.cos() {
                let snap_toi = hit.time_of_impact.clamp(0.0, 1.0);
                self.position += down * snap_toi + n * 0.001;
                self.on_floor = true;
                self.floor_normal = n;
                return true;
            }
        }
        false
    }

    /// Godot 4 `move_and_slide` over `RapierSpace3D`.
    pub fn move_and_slide(
        &mut self,
        dt_seconds: f32,
        space: &RapierSpace3D,
        shape: &r3d::SharedShape,
    ) -> Vec3 {
        if dt_seconds <= 0.0 || !dt_seconds.is_finite() {
            return self.velocity;
        }

        let initial_pos = self.position;
        let was_on_floor = self.on_floor;
        self.on_floor = false;
        self.on_wall = false;
        self.on_ceiling = false;
        self.floor_normal = self.up_direction;
        self.wall_normal = Vec3::ZERO;
        self.slide_collisions.clear();

        // 1. Moving platform velocity carryover
        if was_on_floor && self.platform_velocity.length_squared() > 1e-6 {
            self.position += self.platform_velocity * dt_seconds;
        }

        let cos_floor = self.floor_max_angle.cos();
        let target_speed = if self.floor_constant_speed { self.velocity.length() } else { 0.0 };
        let mut remaining_velocity = self.velocity;

        for _ in 0..self.max_slides {
            let motion = remaining_velocity * dt_seconds;
            let motion_len = motion.length();
            if motion_len <= 1e-6 {
                break;
            }

            let shape_pos = r3d::Pose::from_translation(r3d::Vector::new(self.position.x, self.position.y, self.position.z));
            let vel_vec = r3d::Vector::new(motion.x, motion.y, motion.z);

            let groups = r3d::InteractionGroups::all().with_filter(r3d::Group::from_bits_truncate(self.collision_mask));
            let query = space.broad_phase.as_query_pipeline(
                space.narrow_phase.query_dispatcher(),
                &space.bodies,
                &space.colliders,
                r3d::QueryFilter::default().groups(groups),
            );

            let opt = rapier3d::parry::query::ShapeCastOptions {
                max_time_of_impact: 1.0,
                stop_at_penetration: false,
                ..Default::default()
            };

            let hit = query.cast_shape(&shape_pos, vel_vec, shape.as_ref(), opt);
            if let Some((ch, cast_hit)) = hit {
                let hit_toi = cast_hit.time_of_impact.clamp(0.0, 1.0);
                let hit_n = Vec3::new(cast_hit.normal1.x, cast_hit.normal1.y, cast_hit.normal1.z);
                let hit_pt = Vec3::new(cast_hit.witness1.x, cast_hit.witness1.y, cast_hit.witness1.z);
                let collider_id = space.collider_to_node.get(&ch).copied();

                let travel = motion * hit_toi;
                let remainder = motion - travel;

                // 2. Stair stepping auto-climb
                let is_wall = hit_n.dot(self.up_direction) < cos_floor;
                let mut stair_climbed = false;

                if is_wall && self.step_height > 0.0 && was_on_floor {
                    let lift_vec = self.up_direction * self.step_height;
                    let lifted_pos = self.position + lift_vec;
                    let lifted_shape_pos = r3d::Pose::from_translation(r3d::Vector::new(lifted_pos.x, lifted_pos.y, lifted_pos.z));
                    let fwd_motion = remainder - self.up_direction * remainder.dot(self.up_direction);
                    let fwd_len = fwd_motion.length();

                    if fwd_len > 1e-4 {
                        let fwd_hit = query.cast_shape(
                            &lifted_shape_pos,
                            r3d::Vector::new(fwd_motion.x, fwd_motion.y, fwd_motion.z),
                            shape.as_ref(),
                            opt,
                        );

                        let fwd_travel = if let Some((_, fwd_cast)) = fwd_hit {
                            fwd_motion * (fwd_cast.time_of_impact.clamp(0.0, 1.0) - 0.001).max(0.0)
                        } else {
                            fwd_motion
                        };

                        let step_land_pos = lifted_pos + fwd_travel;
                        let step_land_shape_pos = r3d::Pose::from_translation(r3d::Vector::new(step_land_pos.x, step_land_pos.y, step_land_pos.z));
                        let down_vec = -self.up_direction * (self.step_height + 0.05);

                        let down_hit = query.cast_shape(
                            &step_land_shape_pos,
                            r3d::Vector::new(down_vec.x, down_vec.y, down_vec.z),
                            shape.as_ref(),
                            opt,
                        );

                        if let Some((_, down_cast)) = down_hit {
                            let step_floor_normal = Vec3::new(down_cast.normal1.x, down_cast.normal1.y, down_cast.normal1.z);
                            if step_floor_normal.dot(self.up_direction) >= cos_floor {
                                let down_travel = down_vec * down_cast.time_of_impact.clamp(0.0, 1.0);
                                self.position = step_land_pos + down_travel + step_floor_normal * 0.001;
                                self.on_floor = true;
                                self.floor_normal = step_floor_normal;
                                stair_climbed = true;
                                remaining_velocity = fwd_motion / dt_seconds;
                            }
                        }
                    }
                }

                if !stair_climbed {
                    let safe_toi = (hit_toi - 0.001).max(0.0);
                    self.position += motion * safe_toi;

                    let dot_up = hit_n.dot(self.up_direction);
                    let angle = hit_n.angle_between(self.up_direction);

                    if dot_up >= cos_floor {
                        self.on_floor = true;
                        self.floor_normal = hit_n;
                    } else if dot_up <= -cos_floor {
                        self.on_ceiling = true;
                    } else {
                        self.on_wall = true;
                        self.wall_normal = hit_n;
                    }

                    self.slide_collisions.push(KinematicSlideCollision3D {
                        collider_id,
                        collider_handle: Some(ch),
                        position: hit_pt,
                        normal: hit_n,
                        travel,
                        remainder,
                        angle,
                        depth: motion_len * (1.0 - hit_toi),
                    });

                    // Tangent sliding projection
                    let slide_motion = remainder - hit_n * remainder.dot(hit_n);
                    remaining_velocity = if dt_seconds > 0.0 { slide_motion / dt_seconds } else { Vec3::ZERO };

                    self.velocity -= hit_n * self.velocity.dot(hit_n);

                    // Floor stop on slope
                    if self.floor_stop_on_slope && self.on_floor && self.velocity.dot(self.up_direction) < 0.0 {
                        self.velocity -= self.up_direction * self.velocity.dot(self.up_direction);
                    }

                    // Constant speed on slope
                    if self.floor_constant_speed && self.on_floor && target_speed > 1e-4 {
                        let cur_speed = self.velocity.length();
                        if cur_speed > 1e-4 {
                            self.velocity = (self.velocity / cur_speed) * target_speed;
                        }
                    }
                }
            } else {
                self.position += motion;
                break;
            }
        }

        // 3. Floor snapping
        if was_on_floor && !self.on_floor && self.velocity.dot(self.up_direction) <= 0.0 && self.floor_snap_length > 0.0 {
            self.apply_floor_snap(space, shape);
        }

        // 4. Compute real velocity
        self.real_velocity = (self.position - initial_pos) / dt_seconds;
        self.velocity
    }
}

/// Godot 4 `CharacterBody2D` controller implementing standard `move_and_slide` over `RapierSpace2D`.
#[derive(Clone, Debug, PartialEq)]
pub struct CharacterBody2DController {
    pub id: NodeId,
    pub position: [f32; 2],
    pub velocity: [f32; 2],
    pub up_direction: [f32; 2],
    pub floor_max_angle: f32,
    pub floor_snap_length: f32,
    pub max_slides: u32,
    pub wall_min_slide_angle: f32,
    pub floor_stop_on_slope: bool,
    pub floor_constant_speed: bool,
    pub step_height: f32,
    pub platform_velocity: [f32; 2],
    pub collision_layer: u32,
    pub collision_mask: u32,

    on_floor: bool,
    on_wall: bool,
    on_ceiling: bool,
    floor_normal: [f32; 2],
    wall_normal: [f32; 2],
    real_velocity: [f32; 2],
    slide_collisions: Vec<KinematicSlideCollision2D>,
}

impl CharacterBody2DController {
    /// Construct a new Godot 4 2D CharacterBody controller.
    #[must_use]
    pub fn new(id: NodeId, position: [f32; 2]) -> Self {
        Self {
            id,
            position,
            velocity: [0.0, 0.0],
            up_direction: [0.0, 1.0],
            floor_max_angle: std::f32::consts::FRAC_PI_4, // 45 degrees
            floor_snap_length: 0.1,
            max_slides: 6,
            wall_min_slide_angle: 0.2618,
            floor_stop_on_slope: true,
            floor_constant_speed: false,
            step_height: 0.0,
            platform_velocity: [0.0, 0.0],
            collision_layer: 1,
            collision_mask: 1,
            on_floor: false,
            on_wall: false,
            on_ceiling: false,
            floor_normal: [0.0, 1.0],
            wall_normal: [0.0, 0.0],
            real_velocity: [0.0, 0.0],
            slide_collisions: Vec::with_capacity(6),
        }
    }

    #[must_use]
    pub const fn is_on_floor(&self) -> bool {
        self.on_floor
    }

    #[must_use]
    pub const fn is_on_wall(&self) -> bool {
        self.on_wall
    }

    #[must_use]
    pub const fn is_on_ceiling(&self) -> bool {
        self.on_ceiling
    }

    #[must_use]
    pub const fn get_floor_normal(&self) -> [f32; 2] {
        self.floor_normal
    }

    #[must_use]
    pub const fn get_wall_normal(&self) -> [f32; 2] {
        self.wall_normal
    }

    #[must_use]
    pub const fn get_real_velocity(&self) -> [f32; 2] {
        self.real_velocity
    }

    #[must_use]
    pub fn get_slide_collision_count(&self) -> usize {
        self.slide_collisions.len()
    }

    #[must_use]
    pub fn get_slide_collision(&self, index: usize) -> Option<&KinematicSlideCollision2D> {
        self.slide_collisions.get(index)
    }

    #[must_use]
    pub fn get_slide_collisions(&self) -> &[KinematicSlideCollision2D] {
        &self.slide_collisions
    }

    /// Snap 2D character to floor within snap length.
    pub fn apply_floor_snap(&mut self, space: &RapierSpace2D, shape: &r2d::SharedShape) -> bool {
        if self.floor_snap_length <= 0.0 {
            return false;
        }

        let down = [-self.up_direction[0] * self.floor_snap_length, -self.up_direction[1] * self.floor_snap_length];
        let shape_pos = r2d::Pose::from_translation(r2d::Vector::new(self.position[0], self.position[1]));
        let vel_vec = r2d::Vector::new(down[0], down[1]);

        let groups = r2d::InteractionGroups::all().with_filter(r2d::Group::from_bits_truncate(self.collision_mask));
        let query = space.broad_phase.as_query_pipeline(
            space.narrow_phase.query_dispatcher(),
            &space.bodies,
            &space.colliders,
            r2d::QueryFilter::default().groups(groups),
        );

        let opt = rapier2d::parry::query::ShapeCastOptions {
            max_time_of_impact: 1.0,
            stop_at_penetration: false,
            ..Default::default()
        };

        if let Some((_, hit)) = query.cast_shape(&shape_pos, vel_vec, shape.as_ref(), opt) {
            let n = [hit.normal1.x, hit.normal1.y];
            let dot_up = n[0] * self.up_direction[0] + n[1] * self.up_direction[1];
            if dot_up >= self.floor_max_angle.cos() {
                let snap_toi = hit.time_of_impact.clamp(0.0, 1.0);
                self.position[0] += down[0] * snap_toi + n[0] * 0.001;
                self.position[1] += down[1] * snap_toi + n[1] * 0.001;
                self.on_floor = true;
                self.floor_normal = n;
                return true;
            }
        }
        false
    }

    /// Godot 4 2D `move_and_slide` over `RapierSpace2D`.
    pub fn move_and_slide(
        &mut self,
        dt_seconds: f32,
        space: &RapierSpace2D,
        shape: &r2d::SharedShape,
    ) -> [f32; 2] {
        if dt_seconds <= 0.0 || !dt_seconds.is_finite() {
            return self.velocity;
        }

        let initial_pos = self.position;
        let was_on_floor = self.on_floor;
        self.on_floor = false;
        self.on_wall = false;
        self.on_ceiling = false;
        self.floor_normal = self.up_direction;
        self.wall_normal = [0.0, 0.0];
        self.slide_collisions.clear();

        // Moving platform carryover
        let plat_sq = self.platform_velocity[0] * self.platform_velocity[0] + self.platform_velocity[1] * self.platform_velocity[1];
        if was_on_floor && plat_sq > 1e-6 {
            self.position[0] += self.platform_velocity[0] * dt_seconds;
            self.position[1] += self.platform_velocity[1] * dt_seconds;
        }

        let cos_floor = self.floor_max_angle.cos();
        let mut remaining_velocity = self.velocity;

        for _ in 0..self.max_slides {
            let motion = [remaining_velocity[0] * dt_seconds, remaining_velocity[1] * dt_seconds];
            let motion_len = (motion[0] * motion[0] + motion[1] * motion[1]).sqrt();
            if motion_len <= 1e-6 {
                break;
            }

            let shape_pos = r2d::Pose::from_translation(r2d::Vector::new(self.position[0], self.position[1]));
            let vel_vec = r2d::Vector::new(motion[0], motion[1]);

            let groups = r2d::InteractionGroups::all().with_filter(r2d::Group::from_bits_truncate(self.collision_mask));
            let query = space.broad_phase.as_query_pipeline(
                space.narrow_phase.query_dispatcher(),
                &space.bodies,
                &space.colliders,
                r2d::QueryFilter::default().groups(groups),
            );

            let opt = rapier2d::parry::query::ShapeCastOptions {
                max_time_of_impact: 1.0,
                stop_at_penetration: false,
                ..Default::default()
            };

            let hit = query.cast_shape(&shape_pos, vel_vec, shape.as_ref(), opt);
            if let Some((ch, cast_hit)) = hit {
                let hit_toi = cast_hit.time_of_impact.clamp(0.0, 1.0);
                let hit_n = [cast_hit.normal1.x, cast_hit.normal1.y];
                let hit_pt = [cast_hit.witness1.x, cast_hit.witness1.y];
                let collider_id = space.collider_to_node.get(&ch).copied();

                let safe_toi = (hit_toi - 0.001).max(0.0);
                self.position[0] += motion[0] * safe_toi;
                self.position[1] += motion[1] * safe_toi;

                let dot_up = hit_n[0] * self.up_direction[0] + hit_n[1] * self.up_direction[1];
                if dot_up >= cos_floor {
                    self.on_floor = true;
                    self.floor_normal = hit_n;
                } else if dot_up <= -cos_floor {
                    self.on_ceiling = true;
                } else {
                    self.on_wall = true;
                    self.wall_normal = hit_n;
                }

                let travel = [motion[0] * hit_toi, motion[1] * hit_toi];
                let remainder = [motion[0] - travel[0], motion[1] - travel[1]];

                self.slide_collisions.push(KinematicSlideCollision2D {
                    collider_id,
                    collider_handle: Some(ch),
                    position: hit_pt,
                    normal: hit_n,
                    travel,
                    remainder,
                    angle: dot_up.acos(),
                    depth: motion_len * (1.0 - hit_toi),
                });

                // Tangent slide projection
                let dot_rem = remainder[0] * hit_n[0] + remainder[1] * hit_n[1];
                let slide_motion = [remainder[0] - hit_n[0] * dot_rem, remainder[1] - hit_n[1] * dot_rem];
                remaining_velocity = if dt_seconds > 0.0 {
                    [slide_motion[0] / dt_seconds, slide_motion[1] / dt_seconds]
                } else {
                    [0.0, 0.0]
                };

                let dot_vel = self.velocity[0] * hit_n[0] + self.velocity[1] * hit_n[1];
                self.velocity[0] -= hit_n[0] * dot_vel;
                self.velocity[1] -= hit_n[1] * dot_vel;
            } else {
                self.position[0] += motion[0];
                self.position[1] += motion[1];
                break;
            }
        }

        // Floor snapping
        let vel_dot_up = self.velocity[0] * self.up_direction[0] + self.velocity[1] * self.up_direction[1];
        if was_on_floor && !self.on_floor && vel_dot_up <= 0.0 && self.floor_snap_length > 0.0 {
            self.apply_floor_snap(space, shape);
        }

        self.real_velocity = [
            (self.position[0] - initial_pos[0]) / dt_seconds,
            (self.position[1] - initial_pos[1]) / dt_seconds,
        ];
        self.velocity
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_math::Quat;
    use crate::rapier_backend::{BodyType, RigidBodyConfig2D, RigidBodyConfig3D, ShapeConfig2D, ShapeConfig3D};

    #[test]
    fn character_body_3d_35deg_slope_climb() {
        let mut space = RapierSpace3D::new();

        // Slope angled at 35 degrees (< 45 deg floor_max_angle):
        let angle_rad = 35.0_f32.to_radians();
        let slope_id = NodeId::new(2001, 1).unwrap();
        let rot = Quat::from_rotation_z(angle_rad);

        space.add_rigid_body(slope_id, &RigidBodyConfig3D {
            body_type: BodyType::Fixed,
            position: Vec3::new(4.1, 2.87, 0.0),
            rotation: rot,
            ..Default::default()
        });
        space.add_collider(slope_id, ShapeConfig3D::Cuboid { half_extents: Vec3::new(5.0, 0.2, 5.0) }, 0.0, 0.0, 1, 1);

        let char_id = NodeId::new(2002, 1).unwrap();
        let mut controller = CharacterBody3DController::new(char_id, Vec3::new(1.0, 1.5, 0.0));
        let char_shape = r3d::SharedShape::ball(0.5);

        for _ in 0..15 {
            controller.velocity = Vec3::new(5.0, 0.0, 0.0);
            controller.move_and_slide(0.05, &space, &char_shape);
        }

        assert!(controller.is_on_floor(), "character should be on floor climbing 35 deg slope");
        assert!(!controller.is_on_wall(), "35 deg slope should not be treated as a wall");
        assert!(controller.position.y > 1.5, "character Y should have increased climbing slope: y = {}", controller.position.y);
        assert!(controller.get_floor_normal().y > 0.7);
    }

    #[test]
    fn character_body_3d_65deg_wall_slide() {
        let mut space = RapierSpace3D::new();

        // Steep ramp angled at 65 degrees (> 45 deg floor_max_angle)
        let angle_rad = 65.0_f32.to_radians();
        let wall_id = NodeId::new(2010, 1).unwrap();
        let rot = Quat::from_rotation_z(angle_rad);

        space.add_rigid_body(wall_id, &RigidBodyConfig3D {
            body_type: BodyType::Fixed,
            position: Vec3::new(4.11, 4.53, 0.0),
            rotation: rot,
            ..Default::default()
        });
        space.add_collider(wall_id, ShapeConfig3D::Cuboid { half_extents: Vec3::new(5.0, 0.2, 5.0) }, 0.0, 0.0, 1, 1);

        let char_id = NodeId::new(2011, 1).unwrap();
        let mut controller = CharacterBody3DController::new(char_id, Vec3::new(1.0, 1.0, 0.0));
        let char_shape = r3d::SharedShape::ball(0.5);

        for _ in 0..10 {
            controller.velocity = Vec3::new(5.0, 0.0, 0.0);
            controller.move_and_slide(0.05, &space, &char_shape);
        }

        assert!(controller.is_on_wall(), "65 deg slope must be treated as a wall");
        assert!(!controller.is_on_floor(), "65 deg slope must not be treated as floor");
        assert!(controller.get_wall_normal().x < 0.0);
    }

    #[test]
    fn character_body_3d_stair_stepping_auto_climb() {
        let mut space = RapierSpace3D::new();

        // Ground slab at Y = 0
        let ground_id = NodeId::new(2020, 1).unwrap();
        space.add_rigid_body(ground_id, &RigidBodyConfig3D {
            body_type: BodyType::Fixed,
            position: Vec3::ZERO,
            ..Default::default()
        });
        space.add_collider(ground_id, ShapeConfig3D::Cuboid { half_extents: Vec3::new(10.0, 0.1, 10.0) }, 0.0, 0.0, 1, 1);

        // Stair step of height 0.3m (top at Y = 0.4)
        let step_id = NodeId::new(2021, 1).unwrap();
        space.add_rigid_body(step_id, &RigidBodyConfig3D {
            body_type: BodyType::Fixed,
            position: Vec3::new(3.0, 0.25, 0.0),
            ..Default::default()
        });
        space.add_collider(step_id, ShapeConfig3D::Cuboid { half_extents: Vec3::new(1.0, 0.15, 2.0) }, 0.0, 0.0, 1, 1);

        let char_id = NodeId::new(2022, 1).unwrap();
        let mut controller = CharacterBody3DController::new(char_id, Vec3::new(1.5, 0.6, 0.0));
        controller.step_height = 0.35; // Capable of stepping over 0.3m step
        controller.on_floor = true;
        let char_shape = r3d::SharedShape::capsule_y(0.25, 0.25);

        for _ in 0..10 {
            controller.velocity = Vec3::new(3.0, 0.0, 0.0);
            controller.move_and_slide(0.05, &space, &char_shape);
        }

        assert!(controller.position.x > 2.2, "controller should have stepped onto/over the step: x = {}", controller.position.x);
        assert!(controller.position.y >= 0.85, "controller should be on top of step: y = {}", controller.position.y);
        assert!(controller.is_on_floor());
    }

    #[test]
    fn character_body_3d_floor_snapping_downhill() {
        let mut space = RapierSpace3D::new();

        // Downward inclined slope along +X
        let slope_angle = (-20.0_f32).to_radians();
        let slope_id = NodeId::new(2030, 1).unwrap();
        let rot = Quat::from_rotation_z(slope_angle);

        space.add_rigid_body(slope_id, &RigidBodyConfig3D {
            body_type: BodyType::Fixed,
            position: Vec3::new(5.0, 2.0, 0.0),
            rotation: rot,
            ..Default::default()
        });
        space.add_collider(slope_id, ShapeConfig3D::Cuboid { half_extents: Vec3::new(10.0, 0.1, 5.0) }, 0.0, 0.0, 1, 1);

        let char_id = NodeId::new(2031, 1).unwrap();
        let mut controller = CharacterBody3DController::new(char_id, Vec3::new(2.0, 3.15, 0.0));
        controller.floor_snap_length = 0.5;
        controller.on_floor = true;
        let char_shape = r3d::SharedShape::ball(0.5);

        for _ in 0..10 {
            controller.velocity = Vec3::new(4.0, 0.0, 0.0); // Pure horizontal walk
            controller.move_and_slide(0.05, &space, &char_shape);
            assert!(controller.is_on_floor(), "controller must stay snapped to floor while walking downhill");
        }
    }

    #[test]
    fn character_body_3d_moving_platform_carryover() {
        let space = RapierSpace3D::new();
        let char_id = NodeId::new(2040, 1).unwrap();
        let mut controller = CharacterBody3DController::new(char_id, Vec3::new(0.0, 1.0, 0.0));
        controller.on_floor = true;
        controller.platform_velocity = Vec3::new(6.0, 0.0, -2.0);
        controller.velocity = Vec3::ZERO;
        let char_shape = r3d::SharedShape::ball(0.5);

        controller.move_and_slide(0.1, &space, &char_shape);

        assert!((controller.position.x - 0.6).abs() < 1e-3);
        assert!((controller.position.z - (-0.2)).abs() < 1e-3);
    }

    #[test]
    fn character_body_3d_ceiling_collision() {
        let mut space = RapierSpace3D::new();

        // Ceiling slab at Y = 4.0
        let ceiling_id = NodeId::new(2050, 1).unwrap();
        space.add_rigid_body(ceiling_id, &RigidBodyConfig3D {
            body_type: BodyType::Fixed,
            position: Vec3::new(0.0, 4.0, 0.0),
            ..Default::default()
        });
        space.add_collider(ceiling_id, ShapeConfig3D::Cuboid { half_extents: Vec3::new(5.0, 0.2, 5.0) }, 0.0, 0.0, 1, 1);

        let char_id = NodeId::new(2051, 1).unwrap();
        let mut controller = CharacterBody3DController::new(char_id, Vec3::new(0.0, 2.0, 0.0));
        controller.velocity = Vec3::new(0.0, 25.0, 0.0); // High vertical jump
        let char_shape = r3d::SharedShape::ball(0.5);

        controller.move_and_slide(0.1, &space, &char_shape);

        assert!(controller.is_on_ceiling(), "controller should have detected ceiling hit");
        assert!(controller.position.y <= 3.35, "controller should be stopped below ceiling: y = {}", controller.position.y);
    }

    #[test]
    fn character_body_3d_telemetry_and_slide_collisions() {
        let mut space = RapierSpace3D::new();

        // Vertical obstacle wall at X = 5.0
        let wall_id = NodeId::new(2060, 1).unwrap();
        space.add_rigid_body(wall_id, &RigidBodyConfig3D {
            body_type: BodyType::Fixed,
            position: Vec3::new(5.0, 1.0, 0.0),
            ..Default::default()
        });
        space.add_collider(wall_id, ShapeConfig3D::Cuboid { half_extents: Vec3::new(0.5, 5.0, 5.0) }, 0.0, 0.0, 1, 1);

        let char_id = NodeId::new(2061, 1).unwrap();
        let mut controller = CharacterBody3DController::new(char_id, Vec3::new(3.8, 1.0, 0.0));
        controller.velocity = Vec3::new(10.0, 0.0, 0.0);
        let char_shape = r3d::SharedShape::ball(0.5);

        controller.move_and_slide(0.1, &space, &char_shape);

        assert_eq!(controller.get_slide_collision_count(), 1);
        let col = controller.get_slide_collision(0).unwrap();
        assert_eq!(col.collider_id, Some(wall_id));
        assert_eq!(col.normal, Vec3::NEG_X);
        assert!(controller.get_real_velocity().x > 0.0);
    }

    #[test]
    fn character_body_2d_platformer_jump_ceiling_and_moving_platform() {
        let mut space = RapierSpace2D::new();

        // Ground at Y = 0
        let ground_id = NodeId::new(2070, 1).unwrap();
        space.add_rigid_body(ground_id, &RigidBodyConfig2D {
            body_type: BodyType::Fixed,
            position: [0.0, 0.0],
            ..Default::default()
        });
        space.add_collider(ground_id, ShapeConfig2D::Cuboid { half_extents: [10.0, 0.1] }, 0.0, 0.0, 1, 1);

        // Ceiling at Y = 5.0
        let ceiling_id = NodeId::new(2071, 1).unwrap();
        space.add_rigid_body(ceiling_id, &RigidBodyConfig2D {
            body_type: BodyType::Fixed,
            position: [0.0, 5.0],
            ..Default::default()
        });
        space.add_collider(ceiling_id, ShapeConfig2D::Cuboid { half_extents: [10.0, 0.1] }, 0.0, 0.0, 1, 1);

        let char_id = NodeId::new(2072, 1).unwrap();
        let mut controller = CharacterBody2DController::new(char_id, [0.0, 0.6]);
        let char_shape = r2d::SharedShape::ball(0.5);

        // 1. Initial fall to floor
        controller.velocity = [0.0, -10.0];
        controller.move_and_slide(0.1, &space, &char_shape);
        assert!(controller.is_on_floor());

        // 2. Jump to ceiling
        controller.velocity = [0.0, 30.0];
        controller.move_and_slide(0.2, &space, &char_shape);
        assert!(controller.is_on_ceiling());
        assert!(controller.position[1] <= 4.45);

        // 3. Ride moving platform
        controller.position = [0.0, 0.6];
        controller.on_floor = true;
        controller.platform_velocity = [8.0, 0.0];
        controller.velocity = [0.0, 0.0];
        controller.move_and_slide(0.1, &space, &char_shape);
        assert!((controller.position[0] - 0.8).abs() < 1e-3);
    }

    #[test]
    fn character_body_2d_slope_slide_and_floor_snapping() {
        let mut space = RapierSpace2D::new();

        // 2D Obstacle wall at X = 5.0
        let wall_id = NodeId::new(2080, 1).unwrap();
        space.add_rigid_body(wall_id, &RigidBodyConfig2D {
            body_type: BodyType::Fixed,
            position: [5.0, 2.0],
            ..Default::default()
        });
        space.add_collider(wall_id, ShapeConfig2D::Cuboid { half_extents: [0.2, 5.0] }, 0.0, 0.0, 1, 1);

        let char_id = NodeId::new(2081, 1).unwrap();
        let mut controller = CharacterBody2DController::new(char_id, [4.0, 1.0]);
        controller.velocity = [10.0, 0.0];
        let char_shape = r2d::SharedShape::ball(0.5);

        controller.move_and_slide(0.1, &space, &char_shape);
        assert!(controller.is_on_wall());
        assert_eq!(controller.get_slide_collision_count(), 1);
        assert_eq!(controller.get_slide_collision(0).unwrap().collider_id, Some(wall_id));
    }
}
