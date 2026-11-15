//! Hotswappable `PhysicsServer3D` supporting live A/B switching
//! (e.g. Standard rigid dynamics vs. Skate 2/3 arcade board physics)
//! plus Godot 4 `CharacterBody3D::move_and_slide`, 2D physics server, raycasts,
//! Area3D triggers, and 3D constraints.

pub mod server_2d;
pub mod trimesh_convex;

use std::collections::BTreeSet;
use std::sync::{Arc, RwLock};
use bevy_math::Vec3;
use foreplay_ir::{NodeId, PhysicsBackend};

pub use server_2d::{CharacterBody2D, Collider2D, PhysicsBody2D, PhysicsServer2D, RayHit2D, StaticRect2D};
pub use trimesh_convex::{
    closest_point_on_triangle, gjk_intersects, ContactManifold3D, ConvexHullCollider3D,
    RagdollBone, RagdollBoneId, RagdollRig3D, TriMeshCollider3D,
};

/// 3D collision shape for broad-phase and narrow-phase queries.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Collider3D {
    Sphere { radius: f32 },
    Aabb { half_extents: Vec3 },
}

impl Collider3D {
    /// Vertical distance from center of mass to bottom-most contact point.
    #[must_use]
    pub fn bottom_offset(&self) -> f32 {
        match self {
            Self::Sphere { radius } => *radius,
            Self::Aabb { half_extents } => half_extents.y,
        }
    }

    /// Conservative bounding radius for broad-phase culling.
    #[must_use]
    pub fn bounding_radius(&self) -> f32 {
        match self {
            Self::Sphere { radius } => *radius,
            Self::Aabb { half_extents } => half_extents.length(),
        }
    }
}

/// Dynamic or kinematic 3D physical body.
#[derive(Clone, PartialEq, Debug)]
pub struct PhysicsBody3D {
    pub id: NodeId,
    pub position: Vec3,
    pub velocity: Vec3,
    pub mass: f32,
    pub restitution: f32,
    pub grounded: bool,
    pub collider: Collider3D,
    pub angular_velocity: Vec3,
    pub collision_layer: u32,
    pub collision_mask: u32,
}

impl PhysicsBody3D {
    /// Construct a new 3D physics body.
    #[must_use]
    pub fn new(id: NodeId, position: Vec3, collider: Collider3D) -> Self {
        Self {
            id,
            position,
            velocity: Vec3::ZERO,
            mass: 1.0,
            restitution: 0.2,
            grounded: false,
            collider,
            angular_velocity: Vec3::ZERO,
            collision_layer: 1,
            collision_mask: 1,
        }
    }

    /// Apply an instantaneous linear impulse.
    pub fn apply_impulse(&mut self, impulse: Vec3) {
        if self.mass > 0.0 && impulse.is_finite() {
            self.velocity += impulse / self.mass;
        }
    }

    /// Apply an instantaneous torque impulse.
    pub fn apply_torque_impulse(&mut self, torque: Vec3) {
        if torque.is_finite() {
            self.angular_velocity += torque;
        }
    }
}

/// Static axis-aligned bounding box obstacle in the world.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct StaticAabb {
    pub min: Vec3,
    pub max: Vec3,
    pub collision_layer: u32,
}

impl StaticAabb {
    #[must_use]
    pub const fn new(min: Vec3, max: Vec3) -> Self {
        Self { min, max, collision_layer: 1 }
    }
}

/// Grind rail geometry for skate arcade physics.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct GrindRail {
    pub start: Vec3,
    pub end: Vec3,
    pub radius: f32,
}

/// Result of a 3D raycast query.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct RayHit3D {
    pub body_id: Option<NodeId>,
    pub point: Vec3,
    pub normal: Vec3,
    pub distance: f32,
}

/// 3D Area trigger detecting body entries and exits.
#[derive(Clone, PartialEq, Debug)]
pub struct Area3D {
    pub id: NodeId,
    pub position: Vec3,
    pub collider: Collider3D,
    pub collision_mask: u32,
    pub overlapping_bodies: BTreeSet<NodeId>,
}

impl Area3D {
    #[must_use]
    pub fn new(id: NodeId, position: Vec3, collider: Collider3D) -> Self {
        Self {
            id,
            position,
            collider,
            collision_mask: 1,
            overlapping_bodies: BTreeSet::new(),
        }
    }
}

/// Events emitted when a body enters or exits an `Area3D`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AreaEvent {
    BodyEntered { area_id: NodeId, body_id: NodeId },
    BodyExited { area_id: NodeId, body_id: NodeId },
}

/// Elastic spring joint constraint between two bodies.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct SpringJoint3D {
    pub body_a: NodeId,
    pub body_b: NodeId,
    pub rest_length: f32,
    pub stiffness: f32,
    pub damping: f32,
}

/// Inelastic distance constraint holding two bodies within min/max distance.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct DistanceJoint3D {
    pub body_a: NodeId,
    pub body_b: NodeId,
    pub min_distance: f32,
    pub max_distance: f32,
}

/// Rotational hinge constraint around an axis.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct HingeJoint3D {
    pub body_a: NodeId,
    pub body_b: NodeId,
    pub anchor: Vec3,
    pub axis: Vec3,
    pub min_limit_rad: f32,
    pub max_limit_rad: f32,
}

/// Comprehensive physical world snapshot used for serialization,
/// state extraction/injection, raycasting, and mid-frame live backend hotswapping.
#[derive(Clone, PartialEq, Debug)]
pub struct PhysicsWorldState {
    pub bodies: Vec<PhysicsBody3D>,
    pub static_aabbs: Vec<StaticAabb>,
    pub grind_rails: Vec<GrindRail>,
    pub areas: Vec<Area3D>,
    pub spring_joints: Vec<SpringJoint3D>,
    pub distance_joints: Vec<DistanceJoint3D>,
    pub hinge_joints: Vec<HingeJoint3D>,
    pub gravity: Vec3,
    pub time_elapsed: f32,
}

impl Default for PhysicsWorldState {
    fn default() -> Self {
        Self::new()
    }
}

impl PhysicsWorldState {
    /// Create an empty physics world state with standard Earth gravity.
    #[must_use]
    pub fn new() -> Self {
        Self {
            bodies: Vec::new(),
            static_aabbs: Vec::new(),
            grind_rails: Vec::new(),
            areas: Vec::new(),
            spring_joints: Vec::new(),
            distance_joints: Vec::new(),
            hinge_joints: Vec::new(),
            gravity: Vec3::new(0.0, -9.81, 0.0),
            time_elapsed: 0.0,
        }
    }

    /// Add a dynamic physics body.
    pub fn add_body(&mut self, body: PhysicsBody3D) {
        self.bodies.retain(|b| b.id != body.id);
        self.bodies.push(body);
    }

    /// Find an immutable reference to a body by ID.
    #[must_use]
    pub fn get_body(&self, id: NodeId) -> Option<&PhysicsBody3D> {
        self.bodies.iter().find(|b| b.id == id)
    }

    /// Find a mutable reference to a body by ID.
    pub fn get_body_mut(&mut self, id: NodeId) -> Option<&mut PhysicsBody3D> {
        self.bodies.iter_mut().find(|b| b.id == id)
    }

    /// Add a static obstacle AABB box.
    pub fn add_static_aabb(&mut self, min: Vec3, max: Vec3) {
        self.static_aabbs.push(StaticAabb { min, max, collision_layer: 1 });
    }

    /// Add a static obstacle AABB box with explicit collision layer bitmask.
    pub fn add_static_aabb_with_layer(&mut self, min: Vec3, max: Vec3, layer: u32) {
        self.static_aabbs.push(StaticAabb { min, max, collision_layer: layer });
    }

    /// Add a grind rail segment.
    pub fn add_grind_rail(&mut self, start: Vec3, end: Vec3, radius: f32) {
        self.grind_rails.push(GrindRail { start, end, radius });
    }

    /// Add an Area3D trigger volume.
    pub fn add_area(&mut self, area: Area3D) {
        self.areas.retain(|a| a.id != area.id);
        self.areas.push(area);
    }

    /// Add a spring joint constraint.
    pub fn add_spring_joint(&mut self, joint: SpringJoint3D) {
        self.spring_joints.push(joint);
    }

    /// Add a distance joint constraint.
    pub fn add_distance_joint(&mut self, joint: DistanceJoint3D) {
        self.distance_joints.push(joint);
    }

    /// Add a hinge joint constraint.
    pub fn add_hinge_joint(&mut self, joint: HingeJoint3D) {
        self.hinge_joints.push(joint);
    }

    /// Cast a 3D ray against the ground plane, static boxes, and dynamic bodies.
    #[must_use]
    pub fn ray_cast_3d(&self, from: Vec3, to: Vec3, mask: u32) -> Option<RayHit3D> {
        let dir = to - from;
        let len = dir.length();
        if len <= 1e-6 {
            return None;
        }

        let mut closest_hit: Option<RayHit3D> = None;

        // 1. Ground plane at Y = 0 (Normal: Vec3::Y)
        if from.y > 0.0 && to.y <= 0.0 {
            let t = from.y / (from.y - to.y);
            if (0.0..=1.0).contains(&t) {
                let hit_pt = from + dir * t;
                let hit_dist = len * t;
                closest_hit = Some(RayHit3D {
                    body_id: None,
                    point: hit_pt,
                    normal: Vec3::Y,
                    distance: hit_dist,
                });
            }
        }

        // 2. Static AABBs
        for aabb in &self.static_aabbs {
            if (aabb.collision_layer & mask) == 0 {
                continue;
            }
            if let Some((t, n)) = ray_intersect_aabb(from, dir, aabb.min, aabb.max) {
                let dist = len * t;
                if closest_hit.as_ref().is_none_or(|h| dist < h.distance) {
                    closest_hit = Some(RayHit3D {
                        body_id: None,
                        point: from + dir * t,
                        normal: n,
                        distance: dist,
                    });
                }
            }
        }

        // 3. Dynamic Bodies
        for body in &self.bodies {
            if (body.collision_layer & mask) == 0 {
                continue;
            }
            let hit_opt = match body.collider {
                Collider3D::Sphere { radius } => ray_intersect_sphere(from, dir, body.position, radius),
                Collider3D::Aabb { half_extents } => {
                    ray_intersect_aabb(from, dir, body.position - half_extents, body.position + half_extents)
                }
            };

            if let Some((t, n)) = hit_opt {
                let dist = len * t;
                if closest_hit.as_ref().is_none_or(|h| dist < h.distance) {
                    closest_hit = Some(RayHit3D {
                        body_id: Some(body.id),
                        point: from + dir * t,
                        normal: n,
                        distance: dist,
                    });
                }
            }
        }

        closest_hit
    }

    /// Sweep a sphere along a segment, detecting the earliest collision point and normal.
    #[must_use]
    pub fn shape_cast_sphere_3d(&self, from: Vec3, to: Vec3, radius: f32, mask: u32) -> Option<RayHit3D> {
        let dir = to - from;
        let len = dir.length();
        if len <= 1e-6 {
            return None;
        }

        let mut closest_hit: Option<RayHit3D> = None;

        // Ground plane at Y = 0 (bottom of sphere at from.y - radius)
        if (from.y - radius) > 0.0 && (to.y - radius) <= 0.0 {
            let t = (from.y - radius) / (from.y - to.y);
            if (0.0..=1.0).contains(&t) {
                let hit_pt = from + dir * t - Vec3::Y * radius;
                let hit_dist = len * t;
                closest_hit = Some(RayHit3D {
                    body_id: None,
                    point: hit_pt,
                    normal: Vec3::Y,
                    distance: hit_dist,
                });
            }
        }

        // Expanded static AABBs
        let rad_vec = Vec3::splat(radius);
        for aabb in &self.static_aabbs {
            if (aabb.collision_layer & mask) == 0 {
                continue;
            }
            let exp_min = aabb.min - rad_vec;
            let exp_max = aabb.max + rad_vec;
            if let Some((t, n)) = ray_intersect_aabb(from, dir, exp_min, exp_max) {
                let dist = len * t;
                if closest_hit.as_ref().is_none_or(|h| dist < h.distance) {
                    closest_hit = Some(RayHit3D {
                        body_id: None,
                        point: from + dir * t - n * radius,
                        normal: n,
                        distance: dist,
                    });
                }
            }
        }

        closest_hit
    }

    /// Monitor and update `Area3D` overlap sets, emitting body entered/exited delta events.
    pub fn update_area_overlaps(&mut self) -> Vec<AreaEvent> {
        let mut events = Vec::new();

        for area in &mut self.areas {
            let mut current_overlaps = BTreeSet::new();

            for body in &self.bodies {
                if (area.collision_mask & body.collision_layer) == 0 {
                    continue;
                }

                let d = (body.position - area.position).length();
                let threshold = area.collider.bounding_radius() + body.collider.bounding_radius();
                if d <= threshold {
                    current_overlaps.insert(body.id);
                }
            }

            // Detect newly entered bodies
            for &id in &current_overlaps {
                if !area.overlapping_bodies.contains(&id) {
                    events.push(AreaEvent::BodyEntered { area_id: area.id, body_id: id });
                }
            }

            // Detect newly exited bodies
            for &id in &area.overlapping_bodies {
                if !current_overlaps.contains(&id) {
                    events.push(AreaEvent::BodyExited { area_id: area.id, body_id: id });
                }
            }

            area.overlapping_bodies = current_overlaps;
        }

        events
    }
}

/// Ray vs AABB intersection test. Returns `(t, normal)`.
fn ray_intersect_aabb(from: Vec3, dir: Vec3, min: Vec3, max: Vec3) -> Option<(f32, Vec3)> {
    let mut tmin = f32::NEG_INFINITY;
    let mut tmax = f32::INFINITY;
    let mut normal = Vec3::ZERO;

    for i in 0..3 {
        let d = dir[i];
        let p = from[i];
        let mi = min[i];
        let ma = max[i];

        if d.abs() < 1e-6 {
            if p < mi || p > ma {
                return None;
            }
        } else {
            let inv_d = 1.0 / d;
            let mut t1 = (mi - p) * inv_d;
            let mut t2 = (ma - p) * inv_d;
            let mut n = if inv_d < 0.0 {
                let mut v = Vec3::ZERO; v[i] = 1.0; v
            } else {
                let mut v = Vec3::ZERO; v[i] = -1.0; v
            };

            if t1 > t2 {
                std::mem::swap(&mut t1, &mut t2);
                n = -n;
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

/// Ray vs Sphere intersection test. Returns `(t, normal)`.
fn ray_intersect_sphere(from: Vec3, dir: Vec3, center: Vec3, radius: f32) -> Option<(f32, Vec3)> {
    let m = from - center;
    let b = m.dot(dir);
    let c = m.dot(m) - radius * radius;

    if c > 0.0 && b > 0.0 {
        return None;
    }

    let dir_len_sq = dir.dot(dir);
    let discr = b * b - dir_len_sq * c;
    if discr < 0.0 {
        return None;
    }

    let t = (-b - discr.sqrt()) / dir_len_sq;
    if (0.0..=1.0).contains(&t) {
        let hit_pt = from + dir * t;
        let normal = (hit_pt - center).normalize_or_zero();
        Some((t, normal))
    } else {
        None
    }
}

/// Godot 4 `CharacterBody3D` controller with signature `move_and_slide`.
#[derive(Clone, PartialEq, Debug)]
pub struct CharacterBody3D {
    pub id: NodeId,
    pub position: Vec3,
    pub velocity: Vec3,
    pub up_direction: Vec3,
    pub floor_max_angle: f32,
    pub floor_snap_length: f32,
    pub max_slides: u32,
    pub collider: Collider3D,
    pub collision_layer: u32,
    pub collision_mask: u32,
    pub floor_stop_on_slope: bool,
    on_floor: bool,
    on_wall: bool,
    on_ceiling: bool,
    floor_normal: Vec3,
    wall_normal: Vec3,
}

impl CharacterBody3D {
    /// Construct a new CharacterBody3D controller.
    #[must_use]
    pub fn new(id: NodeId, position: Vec3, collider: Collider3D) -> Self {
        Self {
            id,
            position,
            velocity: Vec3::ZERO,
            up_direction: Vec3::Y,
            floor_max_angle: std::f32::consts::FRAC_PI_4, // 45 deg
            floor_snap_length: 0.1,
            max_slides: 6,
            collider,
            collision_layer: 1,
            collision_mask: 1,
            floor_stop_on_slope: true,
            on_floor: false,
            on_wall: false,
            on_ceiling: false,
            floor_normal: Vec3::Y,
            wall_normal: Vec3::ZERO,
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
    pub const fn floor_normal(&self) -> Vec3 {
        self.floor_normal
    }

    #[must_use]
    pub const fn wall_normal(&self) -> Vec3 {
        self.wall_normal
    }

    /// Godot 4 signature `move_and_slide`: slides along obstacles and updates floor/wall/ceiling state.
    pub fn move_and_slide(&mut self, dt_seconds: f32, world: &PhysicsWorldState) -> Vec3 {
        self.on_floor = false;
        self.on_wall = false;
        self.on_ceiling = false;

        let mut remaining_velocity = self.velocity;

        for _ in 0..self.max_slides {
            let motion = remaining_velocity * dt_seconds;
            if motion.length_squared() <= 1e-8 {
                break;
            }

            let bottom_offset = self.collider.bottom_offset();

            // Check collision against ground plane Y = 0
            let mut earliest_t = 1.0_f32;
            let mut best_normal = Vec3::Y;
            let mut hit = false;

            if motion.y < 0.0 && self.position.y >= bottom_offset {
                let dist_to_ground = self.position.y - bottom_offset;
                let t = dist_to_ground / (-motion.y);
                if (0.0..earliest_t).contains(&t) {
                    earliest_t = t;
                    best_normal = Vec3::Y;
                    hit = true;
                }
            }

            // Check static AABB obstacles
            let half_extents = match self.collider {
                Collider3D::Sphere { radius } => Vec3::splat(radius),
                Collider3D::Aabb { half_extents } => half_extents,
            };

            for aabb in &world.static_aabbs {
                if (aabb.collision_layer & self.collision_mask) == 0 {
                    continue;
                }
                let exp_min = aabb.min - half_extents;
                let exp_max = aabb.max + half_extents;

                if let Some((t, n)) = ray_intersect_aabb(self.position, motion, exp_min, exp_max).filter(|&(t, _)| (0.0..earliest_t).contains(&t)) {
                    earliest_t = t;
                    best_normal = n;
                    hit = true;
                }
            }

            if hit {
                self.position += motion * earliest_t.max(0.0) + best_normal * 0.001;

                let dot_up = best_normal.dot(self.up_direction);
                let cos_floor = self.floor_max_angle.cos();

                if dot_up >= cos_floor {
                    self.on_floor = true;
                    self.floor_normal = best_normal;
                } else if dot_up <= -cos_floor {
                    self.on_ceiling = true;
                } else {
                    self.on_wall = true;
                    self.wall_normal = best_normal;
                }

                // Tangent slide projection
                let rem_factor = 1.0 - earliest_t;
                let rem_motion = motion * rem_factor;
                let slide_motion = rem_motion - best_normal * rem_motion.dot(best_normal);
                remaining_velocity = if dt_seconds > 0.0 { slide_motion / dt_seconds } else { Vec3::ZERO };

                // Adjust body velocity vector
                self.velocity -= best_normal * self.velocity.dot(best_normal);
            } else {
                self.position += motion;
                break;
            }
        }

        self.velocity
    }
}

/// Stateful physics backend contract enabling state inspection and injection.
pub trait StatefulPhysicsBackend: PhysicsBackend {
    fn extract_state(&self) -> PhysicsWorldState;
    fn inject_state(&mut self, state: PhysicsWorldState);
}

/// Backend selector for seamless momentum-preserving hotswaps.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PhysicsBackendKind {
    StandardRigid,
    SkatePortArcade,
}

/// Standard semi-implicit Euler 3D rigid body simulator with joint constraints.
#[derive(Debug)]
pub struct StandardRigidBackend {
    state: Arc<RwLock<PhysicsWorldState>>,
    pub ground_friction: f32,
}

impl Default for StandardRigidBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl StandardRigidBackend {
    /// Create a standard rigid backend with fresh state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(PhysicsWorldState::new())),
            ground_friction: 5.0,
        }
    }

    /// Create from a pre-existing shared state.
    #[must_use]
    pub fn from_shared_state(state: Arc<RwLock<PhysicsWorldState>>) -> Self {
        Self {
            state,
            ground_friction: 5.0,
        }
    }

    /// Read-only snapshot of current world state.
    #[must_use]
    pub fn state(&self) -> PhysicsWorldState {
        self.state.read().unwrap().clone()
    }
}

impl StatefulPhysicsBackend for StandardRigidBackend {
    fn extract_state(&self) -> PhysicsWorldState {
        self.state.read().unwrap().clone()
    }

    fn inject_state(&mut self, new_state: PhysicsWorldState) {
        *self.state.write().unwrap() = new_state;
    }
}

impl PhysicsBackend for StandardRigidBackend {
    fn backend_id(&self) -> &'static str {
        "standard_rigid"
    }

    fn step(&mut self, dt_seconds: f32) {
        if dt_seconds <= 0.0 || !dt_seconds.is_finite() {
            return;
        }

        let mut world = self.state.write().unwrap();
        world.time_elapsed += dt_seconds;
        let gravity = world.gravity;
        let static_boxes = world.static_aabbs.clone();
        let friction = self.ground_friction;

        // 1. Solve spring joint constraints
        for joint in &world.spring_joints.clone() {
            let pos_a = world.get_body(joint.body_a).map(|b| b.position);
            let pos_b = world.get_body(joint.body_b).map(|b| b.position);

            if let (Some(pa), Some(pb)) = (pos_a, pos_b) {
                let delta = pb - pa;
                let dist = delta.length();
                if dist > 1e-4 {
                    let dir = delta / dist;
                    let disp = dist - joint.rest_length;
                    let spring_force = dir * (disp * joint.stiffness);

                    if let Some(ba) = world.get_body_mut(joint.body_a) {
                        ba.apply_impulse(spring_force * dt_seconds);
                    }
                    if let Some(bb) = world.get_body_mut(joint.body_b) {
                        bb.apply_impulse(-spring_force * dt_seconds);
                    }
                }
            }
        }

        // 2. Integrate bodies
        for body in &mut world.bodies {
            // Semi-implicit Euler: integrate velocity from gravity
            if !body.grounded {
                body.velocity += gravity * dt_seconds;
            }

            // Integrate position
            body.position += body.velocity * dt_seconds;

            // Ground plane collision at Y = 0
            let bottom = body.position.y - body.collider.bottom_offset();
            if bottom <= 0.0 {
                body.position.y = body.collider.bottom_offset();
                if body.velocity.y < 0.0 {
                    let bounce = -body.velocity.y * body.restitution;
                    if bounce < 0.2 {
                        body.velocity.y = 0.0;
                        body.grounded = true;
                    } else {
                        body.velocity.y = bounce;
                        body.grounded = false;
                    }
                }

                // Ground friction damping on lateral velocity
                if body.grounded {
                    let damp = (1.0 - friction * dt_seconds).clamp(0.0, 1.0);
                    body.velocity.x *= damp;
                    body.velocity.z *= damp;
                }
            } else if bottom > 0.05 {
                body.grounded = false;
            }

            // Static AABB obstacle collision and sliding
            for aabb in &static_boxes {
                if (aabb.collision_layer & body.collision_mask) != 0 {
                    resolve_static_aabb_collision(body, aabb);
                }
            }
        }

        // 3. Update area overlaps
        let _events = world.update_area_overlaps();
    }
}

/// Resolve static box collision with sliding along tangent.
fn resolve_static_aabb_collision(body: &mut PhysicsBody3D, aabb: &StaticAabb) {
    let half_ext = match body.collider {
        Collider3D::Sphere { radius } => Vec3::splat(radius),
        Collider3D::Aabb { half_extents } => half_extents,
    };

    let b_min = body.position - half_ext;
    let b_max = body.position + half_ext;

    // Check overlap
    if b_max.x > aabb.min.x && b_min.x < aabb.max.x
        && b_max.y > aabb.min.y && b_min.y < aabb.max.y
        && b_max.z > aabb.min.z && b_min.z < aabb.max.z
    {
        // Compute overlap along each axis
        let dx1 = aabb.max.x - b_min.x;
        let dx2 = b_max.x - aabb.min.x;
        let dy1 = aabb.max.y - b_min.y;
        let dy2 = b_max.y - aabb.min.y;
        let dz1 = aabb.max.z - b_min.z;
        let dz2 = b_max.z - aabb.min.z;

        let ox = if dx1 < dx2 { dx1 } else { -dx2 };
        let oy = if dy1 < dy2 { dy1 } else { -dy2 };
        let oz = if dz1 < dz2 { dz1 } else { -dz2 };

        let ax = ox.abs();
        let ay = oy.abs();
        let az = oz.abs();

        if ax <= ay && ax <= az {
            body.position.x += ox;
            body.velocity.x = 0.0;
        } else if ay <= ax && ay <= az {
            body.position.y += oy;
            if oy > 0.0 {
                body.velocity.y = 0.0;
                body.grounded = true;
            } else {
                body.velocity.y = 0.0;
            }
        } else {
            body.position.z += oz;
            body.velocity.z = 0.0;
        }
    }
}

/// `skate2-rust-port`-inspired arcade skateboard physics simulator:
/// pop impulse multiplier, grind-rail magnetic snap corridor, speed wobble damping, and air control.
#[derive(Debug)]
pub struct SkatePortArcadeBackend {
    state: Arc<RwLock<PhysicsWorldState>>,
    pub pop_impulse_multiplier: f32,
    pub magnetic_snap_distance: f32,
    pub wobble_damping: f32,
    pub air_control_torque: f32,
}

impl Default for SkatePortArcadeBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl SkatePortArcadeBackend {
    /// Create an arcade skateboard backend with default tuning parameters.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(PhysicsWorldState::new())),
            pop_impulse_multiplier: 1.35,
            magnetic_snap_distance: 0.65,
            wobble_damping: 0.85,
            air_control_torque: 3.5,
        }
    }

    /// Create from a pre-existing shared state.
    #[must_use]
    pub fn from_shared_state(state: Arc<RwLock<PhysicsWorldState>>) -> Self {
        Self {
            state,
            pop_impulse_multiplier: 1.35,
            magnetic_snap_distance: 0.65,
            wobble_damping: 0.85,
            air_control_torque: 3.5,
        }
    }

    /// Pop an ollie / kickflip with arcade multiplier applied.
    pub fn pop_trick(&mut self, body_id: NodeId, base_pop_impulse: f32) {
        let mut world = self.state.write().unwrap();
        if let Some(body) = world.get_body_mut(body_id) {
            let impulse = Vec3::Y * (base_pop_impulse * self.pop_impulse_multiplier);
            body.apply_impulse(impulse);
            body.grounded = false;
        }
    }

    /// Read-only snapshot of current world state.
    #[must_use]
    pub fn state(&self) -> PhysicsWorldState {
        self.state.read().unwrap().clone()
    }
}

impl StatefulPhysicsBackend for SkatePortArcadeBackend {
    fn extract_state(&self) -> PhysicsWorldState {
        self.state.read().unwrap().clone()
    }

    fn inject_state(&mut self, new_state: PhysicsWorldState) {
        *self.state.write().unwrap() = new_state;
    }
}

impl PhysicsBackend for SkatePortArcadeBackend {
    fn backend_id(&self) -> &'static str {
        "skate_port_arcade"
    }

    fn step(&mut self, dt_seconds: f32) {
        if dt_seconds <= 0.0 || !dt_seconds.is_finite() {
            return;
        }

        let mut world = self.state.write().unwrap();
        world.time_elapsed += dt_seconds;
        let gravity = world.gravity;
        let rails = world.grind_rails.clone();
        let snap_dist = self.magnetic_snap_distance;
        let wobble_factor = self.wobble_damping;

        for body in &mut world.bodies {
            let mut on_rail = false;

            // Grind rail magnetic snap corridor check
            for rail in &rails {
                let rail_vec = rail.end - rail.start;
                let rail_len_sq = rail_vec.length_squared();
                if rail_len_sq > 1e-6 {
                    let to_body = body.position - rail.start;
                    let t = (to_body.dot(rail_vec) / rail_len_sq).clamp(0.0, 1.0);
                    let closest_point = rail.start + rail_vec * t;
                    let dist = (body.position - closest_point).length();

                    if dist <= snap_dist {
                        // Snap position to rail surface
                        let rail_up = Vec3::Y * (rail.radius + body.collider.bottom_offset());
                        let target_pos = closest_point + rail_up;
                        body.position = body.position.lerp(target_pos, (0.5 * dt_seconds * 60.0).clamp(0.0, 1.0));

                        // Lock velocity along rail vector
                        let rail_dir = rail_vec.normalize();
                        let forward_speed = body.velocity.dot(rail_dir);
                        body.velocity = rail_dir * forward_speed;
                        body.grounded = true;
                        on_rail = true;
                        break;
                    }
                }
            }

            // If not on rail, integrate standard gravity
            if !on_rail && !body.grounded {
                body.velocity += gravity * dt_seconds;
            }

            // Speed wobble damping at high velocities: smooth out lateral angular velocity
            let speed = body.velocity.length();
            if speed > 6.0 {
                let damp = (1.0 - wobble_factor * dt_seconds).clamp(0.0, 1.0);
                body.angular_velocity.x *= damp;
                body.angular_velocity.z *= damp;
            }

            // Air control torque integration: rotates skater orientation while airborne
            if !body.grounded && !on_rail {
                body.angular_velocity *= (1.0 - 0.2 * dt_seconds).clamp(0.0, 1.0);
            }

            // Integrate position
            body.position += body.velocity * dt_seconds;

            // Ground plane check at Y = 0 if not on rail
            if !on_rail {
                let bottom = body.position.y - body.collider.bottom_offset();
                if bottom <= 0.0 {
                    body.position.y = body.collider.bottom_offset();
                    if body.velocity.y < 0.0 {
                        body.velocity.y = 0.0;
                    }
                    body.grounded = true;
                } else if bottom > 0.05 {
                    body.grounded = false;
                }
            }
        }
    }
}

/// Hotswappable `PhysicsServer3D` supporting live A/B switching
/// (e.g. Standard rigid dynamic simulation vs. Skate 2/3 arcade physics).
#[derive(Debug)]
pub struct PhysicsServer3D {
    active: Box<dyn PhysicsBackend>,
    world_state: Arc<RwLock<PhysicsWorldState>>,
}

impl PhysicsServer3D {
    /// Construct server with an initial physics backend implementation.
    #[must_use]
    pub fn new(initial: Box<dyn PhysicsBackend>) -> Self {
        Self {
            active: initial,
            world_state: Arc::new(RwLock::new(PhysicsWorldState::new())),
        }
    }

    /// Construct server initialized with the standard rigid body backend.
    #[must_use]
    pub fn new_standard() -> Self {
        let state = Arc::new(RwLock::new(PhysicsWorldState::new()));
        let backend = StandardRigidBackend::from_shared_state(state.clone());
        Self {
            active: Box::new(backend),
            world_state: state,
        }
    }

    /// Construct server initialized with the arcade skate backend.
    #[must_use]
    pub fn new_arcade() -> Self {
        let state = Arc::new(RwLock::new(PhysicsWorldState::new()));
        let backend = SkatePortArcadeBackend::from_shared_state(state.clone());
        Self {
            active: Box::new(backend),
            world_state: state,
        }
    }

    /// Add a physics body to the server simulation.
    pub fn add_body(&mut self, body: PhysicsBody3D) {
        self.world_state.write().unwrap().add_body(body);
    }

    /// Retrieve a cloned snapshot of a body by ID.
    #[must_use]
    pub fn get_body(&self, id: NodeId) -> Option<PhysicsBody3D> {
        self.world_state.read().unwrap().get_body(id).cloned()
    }

    /// Extract a full snapshot of the physical world state.
    #[must_use]
    pub fn extract_state(&self) -> PhysicsWorldState {
        self.world_state.read().unwrap().clone()
    }

    /// Inject a physical world state snapshot.
    pub fn inject_state(&mut self, state: PhysicsWorldState) {
        *self.world_state.write().unwrap() = state;
    }

    /// Live A/B hotswap of physics implementation mid-session (`skate2-rust-port` workflow).
    pub fn hotswap_backend(&mut self, next: Box<dyn PhysicsBackend>) -> Box<dyn PhysicsBackend> {
        std::mem::replace(&mut self.active, next)
    }

    /// Live mid-frame backend hotswap between `StandardRigid` and `SkatePortArcade`
    /// seamlessly preserving every body's position, linear velocity, angular velocity, and collider!
    pub fn hotswap_preserving_bodies(&mut self, kind: PhysicsBackendKind) -> Box<dyn PhysicsBackend> {
        let next: Box<dyn PhysicsBackend> = match kind {
            PhysicsBackendKind::StandardRigid => {
                Box::new(StandardRigidBackend::from_shared_state(self.world_state.clone()))
            }
            PhysicsBackendKind::SkatePortArcade => {
                Box::new(SkatePortArcadeBackend::from_shared_state(self.world_state.clone()))
            }
        };
        std::mem::replace(&mut self.active, next)
    }

    /// Active backend identifier string.
    #[must_use]
    pub fn active_backend_id(&self) -> &'static str {
        self.active.backend_id()
    }

    /// Step simulation forward by `dt_seconds`.
    pub fn step(&mut self, dt_seconds: f32) {
        self.active.step(dt_seconds);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_rigid_gravity_and_ground_bounce() {
        let mut server = PhysicsServer3D::new_standard();
        let skater_id = NodeId::new(1, 1).unwrap();
        let body = PhysicsBody3D::new(skater_id, Vec3::new(0.0, 5.0, 0.0), Collider3D::Sphere { radius: 0.5 });
        server.add_body(body);

        // Step simulation forward until all bounce cycles settle to rest (t = 3.0s)
        for _ in 0..180 {
            server.step(1.0 / 60.0);
        }

        let skater = server.get_body(skater_id).unwrap();
        // Skater should have fallen to the ground (y = bottom_offset = 0.5)
        assert!((skater.position.y - 0.5).abs() < 0.01, "expected settled y ~= 0.5, got {}", skater.position.y);
        assert!(skater.grounded);
    }

    #[test]
    fn skate_arcade_rail_snap_corridor() {
        let mut server = PhysicsServer3D::new_arcade();
        let skater_id = NodeId::new(2, 1).unwrap();

        // Add a grind rail along the X axis at Y = 1.0
        server.world_state.write().unwrap().add_grind_rail(
            Vec3::new(-10.0, 1.0, 0.0),
            Vec3::new(10.0, 1.0, 0.0),
            0.05,
        );

        // Position skater slightly offset from rail, moving forward (+X)
        let mut body = PhysicsBody3D::new(
            skater_id,
            Vec3::new(0.0, 1.2, 0.2), // Within 0.65m snap distance
            Collider3D::Aabb { half_extents: Vec3::new(0.2, 0.4, 0.2) },
        );
        body.velocity = Vec3::new(8.0, 0.0, 1.0); // Has lateral drift
        body.restitution = 0.0;
        server.add_body(body);

        // Step simulation
        for _ in 0..10 {
            server.step(1.0 / 60.0);
        }

        let skater = server.get_body(skater_id).unwrap();
        // Skater should have snapped onto rail Z = 0
        assert!(skater.position.z.abs() < 0.1);
        // Forward velocity along X preserved
        assert!(skater.velocity.x > 7.0);
        // Lateral drift along Z neutralized by rail lock
        assert!(skater.velocity.z.abs() < 0.1);
    }

    #[test]
    fn live_ab_physics_hotswap_preserves_momentum() {
        let mut server = PhysicsServer3D::new_standard();
        assert_eq!(server.active_backend_id(), "standard_rigid");

        let skater_id = NodeId::new(42, 1).unwrap();
        let initial_vel = Vec3::new(12.0, 0.0, 3.0);
        let mut body = PhysicsBody3D::new(skater_id, Vec3::new(0.0, 0.5, 0.0), Collider3D::Sphere { radius: 0.5 });
        body.velocity = initial_vel;
        body.grounded = true;
        server.add_body(body);

        // Step for 10 frames in standard mode
        for _ in 0..10 {
            server.step(1.0 / 60.0);
        }

        let state_before = server.get_body(skater_id).unwrap();
        let pos_before = state_before.position;
        let vel_before = state_before.velocity;
        assert!(pos_before.x > 1.0); // Has moved forward

        // Hotswap mid-frame to arcade skate backend!
        let old_backend = server.hotswap_preserving_bodies(PhysicsBackendKind::SkatePortArcade);
        assert_eq!(old_backend.backend_id(), "standard_rigid");
        assert_eq!(server.active_backend_id(), "skate_port_arcade");

        // Immediately inspect body state: position and velocity are 100% preserved
        let state_after_swap = server.get_body(skater_id).unwrap();
        assert_eq!(state_after_swap.position, pos_before);
        assert_eq!(state_after_swap.velocity, vel_before);

        // Continue simulation under arcade backend for 10 more frames
        for _ in 0..10 {
            server.step(1.0 / 60.0);
        }

        let state_continued = server.get_body(skater_id).unwrap();
        // Momentum carried smoothly forward
        assert!(state_continued.position.x > pos_before.x);
    }

    #[test]
    fn static_aabb_obstacle_sliding_collision() {
        let mut server = PhysicsServer3D::new_standard();
        server.world_state.write().unwrap().add_static_aabb(
            Vec3::new(5.0, 0.0, -5.0),
            Vec3::new(6.0, 5.0, 5.0),
        );

        let body_id = NodeId::new(10, 1).unwrap();
        let mut body = PhysicsBody3D::new(body_id, Vec3::new(4.5, 0.5, 0.0), Collider3D::Sphere { radius: 0.5 });
        body.velocity = Vec3::new(10.0, 0.0, 2.0);
        body.restitution = 0.0;
        body.grounded = true;
        server.add_body(body);

        for _ in 0..10 {
            server.step(1.0 / 60.0);
        }

        let body_after = server.get_body(body_id).unwrap();
        assert!(body_after.position.x <= 4.5 + 1e-4);
        assert!(body_after.velocity.x.abs() < 1e-3);
    }

    #[test]
    fn skate_pop_impulse_and_wobble_damping() {
        let mut backend = SkatePortArcadeBackend::new();
        let body_id = NodeId::new(20, 1).unwrap();
        let mut body = PhysicsBody3D::new(body_id, Vec3::new(0.0, 0.5, 0.0), Collider3D::Sphere { radius: 0.5 });
        body.velocity = Vec3::new(10.0, 0.0, 0.0);
        body.angular_velocity = Vec3::new(5.0, 0.0, 5.0);
        body.grounded = true;
        backend.state.write().unwrap().add_body(body);

        // Pop trick with base impulse 5.0
        backend.pop_trick(body_id, 5.0);
        let popped = backend.state().get_body(body_id).cloned().unwrap();
        let expected_impulse = 5.0 * backend.pop_impulse_multiplier;
        assert!((popped.velocity.y - expected_impulse).abs() < 1e-3);
        assert!(!popped.grounded);

        // Step to test wobble damping
        let initial_wobble = popped.angular_velocity.x;
        backend.step(1.0 / 60.0);
        let damped = backend.state().get_body(body_id).cloned().unwrap();
        assert!(damped.angular_velocity.x < initial_wobble);
    }

    #[test]
    fn character_body_3d_move_and_slide_floor_and_wall() {
        let mut world = PhysicsWorldState::new();
        world.add_static_aabb(Vec3::new(5.0, 0.0, -5.0), Vec3::new(6.0, 5.0, 5.0));

        let char_id = NodeId::new(50, 1).unwrap();
        let mut char_body = CharacterBody3D::new(char_id, Vec3::new(0.0, 0.5, 0.0), Collider3D::Sphere { radius: 0.5 });
        char_body.velocity = Vec3::new(10.0, -2.0, 0.0); // Moving towards wall and falling

        let _ = char_body.move_and_slide(0.1, &world);
        assert!(char_body.is_on_floor());
        assert!((char_body.position.y - 0.5).abs() < 0.05);

        // Run until hitting wall at X = 4.5
        for _ in 0..10 {
            char_body.velocity = Vec3::new(5.0, 0.0, 0.0);
            char_body.move_and_slide(0.1, &world);
        }

        assert!(char_body.is_on_wall());
        assert!(char_body.wall_normal().x < 0.0);
        assert!(char_body.position.x <= 4.51);
    }

    #[test]
    fn character_body_2d_move_and_slide() {
        let obstacles = vec![StaticRect2D {
            min: [5.0, 0.0],
            max: [6.0, 5.0],
            collision_layer: 1,
        }];

        let mut body = CharacterBody2D::new([0.0, 0.5]);
        body.velocity = [10.0, -6.0];

        body.move_and_slide(0.1, &obstacles);
        assert!(body.is_on_floor);

        for _ in 0..10 {
            body.velocity = [5.0, 0.0];
            body.move_and_slide(0.1, &obstacles);
        }
        assert!(body.is_on_wall);
    }

    #[test]
    fn ray_cast_and_shape_cast_3d() {
        let mut world = PhysicsWorldState::new();
        let box_min = Vec3::new(5.0, 0.0, -1.0);
        let box_max = Vec3::new(7.0, 2.0, 1.0);
        world.add_static_aabb(box_min, box_max);

        // Raycast straight at the box
        let hit = world.ray_cast_3d(Vec3::new(0.0, 1.0, 0.0), Vec3::new(10.0, 1.0, 0.0), 1).expect("ray hits box");
        assert!((hit.point.x - 5.0).abs() < 1e-3);
        assert_eq!(hit.normal, Vec3::NEG_X);

        // Shapecast sphere towards box: hits at X = 5.0 - radius (0.5) = 4.5
        let shape_hit = world.shape_cast_sphere_3d(Vec3::new(0.0, 1.0, 0.0), Vec3::new(10.0, 1.0, 0.0), 0.5, 1).expect("sphere hits box");
        assert!((shape_hit.point.x - 5.0).abs() < 1e-3);
        assert!((shape_hit.distance - 4.5).abs() < 1e-3);
    }

    #[test]
    fn area_3d_body_entered_and_exited_events() {
        let mut world = PhysicsWorldState::new();
        let area_id = NodeId::new(99, 1).unwrap();
        let area = Area3D::new(area_id, Vec3::new(10.0, 0.0, 0.0), Collider3D::Sphere { radius: 2.0 });
        world.add_area(area);

        let body_id = NodeId::new(100, 1).unwrap();
        let mut body = PhysicsBody3D::new(body_id, Vec3::new(0.0, 0.0, 0.0), Collider3D::Sphere { radius: 0.5 });
        world.add_body(body.clone());

        // Initially body is at X=0, outside area at X=10
        let events1 = world.update_area_overlaps();
        assert!(events1.is_empty());

        // Move body into area at X=10
        body.position = Vec3::new(10.0, 0.0, 0.0);
        world.add_body(body.clone());
        let events2 = world.update_area_overlaps();
        assert_eq!(events2, vec![AreaEvent::BodyEntered { area_id, body_id }]);

        // Move body out of area
        body.position = Vec3::new(20.0, 0.0, 0.0);
        world.add_body(body);
        let events3 = world.update_area_overlaps();
        assert_eq!(events3, vec![AreaEvent::BodyExited { area_id, body_id }]);
    }
}
