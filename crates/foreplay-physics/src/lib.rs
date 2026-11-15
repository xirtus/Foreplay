//! Hotswappable `PhysicsServer3D` supporting live A/B switching
//! (e.g. Standard rigid dynamics vs. Skate 2/3 arcade board physics).

use std::sync::{Arc, RwLock};
use bevy_math::Vec3;
use foreplay_ir::{NodeId, PhysicsBackend};

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
}

/// Grind rail geometry for skate arcade physics.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct GrindRail {
    pub start: Vec3,
    pub end: Vec3,
    pub radius: f32,
}

/// Comprehensive physical world snapshot used for serialization,
/// state extraction/injection, and mid-frame live backend hotswapping.
#[derive(Clone, PartialEq, Debug)]
pub struct PhysicsWorldState {
    pub bodies: Vec<PhysicsBody3D>,
    pub static_aabbs: Vec<StaticAabb>,
    pub grind_rails: Vec<GrindRail>,
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
        self.static_aabbs.push(StaticAabb { min, max });
    }

    /// Add a grind rail segment.
    pub fn add_grind_rail(&mut self, start: Vec3, end: Vec3, radius: f32) {
        self.grind_rails.push(GrindRail { start, end, radius });
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

/// Standard semi-implicit Euler 3D rigid body simulator.
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
                resolve_static_aabb_collision(body, aabb);
            }
        }
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
        let body = PhysicsBody3D {
            id: skater_id,
            position: Vec3::new(0.0, 5.0, 0.0),
            velocity: Vec3::ZERO,
            mass: 1.0,
            restitution: 0.5,
            grounded: false,
            collider: Collider3D::Sphere { radius: 0.5 },
            angular_velocity: Vec3::ZERO,
        };
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
        let body = PhysicsBody3D {
            id: skater_id,
            position: Vec3::new(0.0, 1.2, 0.2), // Within 0.65m snap distance
            velocity: Vec3::new(8.0, 0.0, 1.0), // Has lateral drift
            mass: 1.0,
            restitution: 0.0,
            grounded: false,
            collider: Collider3D::Aabb { half_extents: Vec3::new(0.2, 0.4, 0.2) },
            angular_velocity: Vec3::ZERO,
        };
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
        let body = PhysicsBody3D {
            id: skater_id,
            position: Vec3::new(0.0, 0.5, 0.0),
            velocity: initial_vel,
            mass: 1.0,
            restitution: 0.1,
            grounded: true,
            collider: Collider3D::Sphere { radius: 0.5 },
            angular_velocity: Vec3::ZERO,
        };
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
        // Add a wall from X = 5 to 6, Y = 0 to 5, Z = -5 to 5
        server.world_state.write().unwrap().add_static_aabb(
            Vec3::new(5.0, 0.0, -5.0),
            Vec3::new(6.0, 5.0, 5.0),
        );

        let body_id = NodeId::new(10, 1).unwrap();
        // Moving fast towards wall at (+X)
        let body = PhysicsBody3D {
            id: body_id,
            position: Vec3::new(4.5, 0.5, 0.0),
            velocity: Vec3::new(10.0, 0.0, 2.0), // Has forward + lateral velocity
            mass: 1.0,
            restitution: 0.0,
            grounded: true,
            collider: Collider3D::Sphere { radius: 0.5 },
            angular_velocity: Vec3::ZERO,
        };
        server.add_body(body);

        // Step simulation
        for _ in 0..10 {
            server.step(1.0 / 60.0);
        }

        let body_after = server.get_body(body_id).unwrap();
        // Body cannot penetrate wall past X = 5.0 - radius (4.5)
        assert!(body_after.position.x <= 4.5 + 1e-4);
        // X velocity stopped by wall
        assert!(body_after.velocity.x.abs() < 1e-3);
    }

    #[test]
    fn skate_pop_impulse_and_wobble_damping() {
        let mut backend = SkatePortArcadeBackend::new();
        let body_id = NodeId::new(20, 1).unwrap();
        let body = PhysicsBody3D {
            id: body_id,
            position: Vec3::new(0.0, 0.5, 0.0),
            velocity: Vec3::new(10.0, 0.0, 0.0), // Fast speed > 6.0
            mass: 1.0,
            restitution: 0.0,
            grounded: true,
            collider: Collider3D::Sphere { radius: 0.5 },
            angular_velocity: Vec3::new(5.0, 0.0, 5.0), // High wobble
        };
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
}
