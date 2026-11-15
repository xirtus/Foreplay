//! Production Rapier 2D and 3D Physics Server backends, Direct Space State Queries,
//! and bidirectional live state hotswapping bridge.

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::sync::{Arc, RwLock};
use bevy_math::{Quat, Vec3};
use foreplay_ir::{NodeId, PhysicsBackend};
use rapier3d::prelude as r3d;
use rapier2d::prelude as r2d;
use crate::{Collider3D, PhysicsBody3D, PhysicsWorldState, RayHit3D, StatefulPhysicsBackend};

/// Dynamic, Kinematic, or Fixed body type configuration.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum BodyType {
    #[default]
    Dynamic,
    KinematicPositionBased,
    KinematicVelocityBased,
    Fixed,
}

impl From<BodyType> for r3d::RigidBodyType {
    fn from(bt: BodyType) -> Self {
        match bt {
            BodyType::Dynamic => r3d::RigidBodyType::Dynamic,
            BodyType::KinematicPositionBased => r3d::RigidBodyType::KinematicPositionBased,
            BodyType::KinematicVelocityBased => r3d::RigidBodyType::KinematicVelocityBased,
            BodyType::Fixed => r3d::RigidBodyType::Fixed,
        }
    }
}

impl From<BodyType> for r2d::RigidBodyType {
    fn from(bt: BodyType) -> Self {
        match bt {
            BodyType::Dynamic => r2d::RigidBodyType::Dynamic,
            BodyType::KinematicPositionBased => r2d::RigidBodyType::KinematicPositionBased,
            BodyType::KinematicVelocityBased => r2d::RigidBodyType::KinematicVelocityBased,
            BodyType::Fixed => r2d::RigidBodyType::Fixed,
        }
    }
}

/// 3D Collider shape configuration.
#[derive(Clone, Debug, PartialEq)]
pub enum ShapeConfig3D {
    Cuboid { half_extents: Vec3 },
    Ball { radius: f32 },
    CapsuleY { half_height: f32, radius: f32 },
    Cylinder { half_height: f32, radius: f32 },
    ConvexHull { points: Vec<Vec3> },
    HeightField { nrows: usize, ncols: usize, heights: Vec<f32>, scale: Vec3 },
    TriMesh { vertices: Vec<Vec3>, indices: Vec<[u32; 3]> },
}

impl ShapeConfig3D {
    /// Convert configuration into Rapier SharedShape.
    #[must_use]
    pub fn to_shared_shape(&self) -> r3d::SharedShape {
        match self {
            Self::Cuboid { half_extents } => {
                r3d::SharedShape::cuboid(half_extents.x, half_extents.y, half_extents.z)
            }
            Self::Ball { radius } => r3d::SharedShape::ball(*radius),
            Self::CapsuleY { half_height, radius } => {
                r3d::SharedShape::capsule_y(*half_height, *radius)
            }
            Self::Cylinder { half_height, radius } => {
                r3d::SharedShape::cylinder(*half_height, *radius)
            }
            Self::ConvexHull { points } => {
                let pts: Vec<r3d::Vector> = points.iter().map(|p| r3d::Vector::new(p.x, p.y, p.z)).collect();
                r3d::SharedShape::convex_hull(&pts).unwrap_or_else(|| r3d::SharedShape::ball(0.5))
            }
            Self::HeightField { nrows, ncols, heights, scale } => {
                let arr = r3d::Array2::new(*nrows, *ncols, heights.clone());
                r3d::SharedShape::heightfield(arr, r3d::Vector::new(scale.x, scale.y, scale.z))
            }
            Self::TriMesh { vertices, indices } => {
                let verts: Vec<r3d::Vector> = vertices.iter().map(|v| r3d::Vector::new(v.x, v.y, v.z)).collect();
                r3d::SharedShape::trimesh(verts, indices.clone()).unwrap_or_else(|_| r3d::SharedShape::ball(0.5))
            }
        }
    }
}

/// 2D Collider shape configuration.
#[derive(Clone, Debug, PartialEq)]
pub enum ShapeConfig2D {
    Cuboid { half_extents: [f32; 2] },
    Ball { radius: f32 },
    CapsuleY { half_height: f32, radius: f32 },
    ConvexHull { points: Vec<[f32; 2]> },
    HeightField { heights: Vec<f32>, scale: [f32; 2] },
    Polyline { vertices: Vec<[f32; 2]>, indices: Option<Vec<[u32; 2]>> },
}

impl ShapeConfig2D {
    /// Convert configuration into Rapier2D SharedShape.
    #[must_use]
    pub fn to_shared_shape(&self) -> r2d::SharedShape {
        match self {
            Self::Cuboid { half_extents } => {
                r2d::SharedShape::cuboid(half_extents[0], half_extents[1])
            }
            Self::Ball { radius } => r2d::SharedShape::ball(*radius),
            Self::CapsuleY { half_height, radius } => {
                r2d::SharedShape::capsule_y(*half_height, *radius)
            }
            Self::ConvexHull { points } => {
                let pts: Vec<r2d::Vector> = points.iter().map(|p| r2d::Vector::new(p[0], p[1])).collect();
                r2d::SharedShape::convex_hull(&pts).unwrap_or_else(|| r2d::SharedShape::ball(0.5))
            }
            Self::HeightField { heights, scale } => {
                r2d::SharedShape::heightfield(heights.clone(), r2d::Vector::new(scale[0], scale[1]))
            }
            Self::Polyline { vertices, indices } => {
                let verts: Vec<r2d::Vector> = vertices.iter().map(|v| r2d::Vector::new(v[0], v[1])).collect();
                r2d::SharedShape::polyline(verts, indices.clone())
            }
        }
    }
}

/// Motor and limits configuration for joints.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JointMotorConfig {
    pub target_vel: f32,
    pub target_pos: f32,
    pub stiffness: f32,
    pub damping: f32,
    pub max_force: f32,
}

impl Default for JointMotorConfig {
    fn default() -> Self {
        Self {
            target_vel: 0.0,
            target_pos: 0.0,
            stiffness: 1000.0,
            damping: 100.0,
            max_force: f32::MAX,
        }
    }
}

/// 3D Impulse joint configuration.
#[derive(Clone, Debug, PartialEq)]
pub enum JointConfig3D {
    Fixed {
        local_anchor1: Vec3,
        local_anchor2: Vec3,
    },
    Spherical {
        local_anchor1: Vec3,
        local_anchor2: Vec3,
    },
    Revolute {
        axis: Vec3,
        local_anchor1: Vec3,
        local_anchor2: Vec3,
        limits: Option<[f32; 2]>,
        motor: Option<JointMotorConfig>,
    },
    Prismatic {
        axis: Vec3,
        local_anchor1: Vec3,
        local_anchor2: Vec3,
        limits: Option<[f32; 2]>,
        motor: Option<JointMotorConfig>,
    },
    Generic6Dof {
        local_anchor1: Vec3,
        local_anchor2: Vec3,
        locked_axes: Vec<r3d::JointAxis>,
        limits: Vec<(r3d::JointAxis, [f32; 2])>,
        motors: Vec<(r3d::JointAxis, JointMotorConfig)>,
    },
}

/// 2D Impulse joint configuration.
#[derive(Clone, Debug, PartialEq)]
pub enum JointConfig2D {
    Fixed {
        local_anchor1: [f32; 2],
        local_anchor2: [f32; 2],
    },
    Revolute {
        local_anchor1: [f32; 2],
        local_anchor2: [f32; 2],
        limits: Option<[f32; 2]>,
        motor: Option<JointMotorConfig>,
    },
    Prismatic {
        axis: [f32; 2],
        local_anchor1: [f32; 2],
        local_anchor2: [f32; 2],
        limits: Option<[f32; 2]>,
        motor: Option<JointMotorConfig>,
    },
}

/// Rigid body construction configuration in 3D.
#[derive(Clone, Debug, PartialEq)]
pub struct RigidBodyConfig3D {
    pub body_type: BodyType,
    pub position: Vec3,
    pub rotation: Quat,
    pub linear_velocity: Vec3,
    pub angular_velocity: Vec3,
    pub gravity_scale: f32,
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub ccd_enabled: bool,
    pub can_sleep: bool,
    pub sleeping: bool,
    pub collision_layer: u32,
    pub collision_mask: u32,
    pub mass: f32,
}

impl Default for RigidBodyConfig3D {
    fn default() -> Self {
        Self {
            body_type: BodyType::Dynamic,
            position: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            linear_velocity: Vec3::ZERO,
            angular_velocity: Vec3::ZERO,
            gravity_scale: 1.0,
            linear_damping: 0.0,
            angular_damping: 0.0,
            ccd_enabled: false,
            can_sleep: true,
            sleeping: false,
            collision_layer: 1,
            collision_mask: 1,
            mass: 1.0,
        }
    }
}

/// Rigid body construction configuration in 2D.
#[derive(Clone, Debug, PartialEq)]
pub struct RigidBodyConfig2D {
    pub body_type: BodyType,
    pub position: [f32; 2],
    pub rotation: f32,
    pub linear_velocity: [f32; 2],
    pub angular_velocity: f32,
    pub gravity_scale: f32,
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub ccd_enabled: bool,
    pub can_sleep: bool,
    pub sleeping: bool,
    pub collision_layer: u32,
    pub collision_mask: u32,
    pub mass: f32,
}

impl Default for RigidBodyConfig2D {
    fn default() -> Self {
        Self {
            body_type: BodyType::Dynamic,
            position: [0.0, 0.0],
            rotation: 0.0,
            linear_velocity: [0.0, 0.0],
            angular_velocity: 0.0,
            gravity_scale: 1.0,
            linear_damping: 0.0,
            angular_damping: 0.0,
            ccd_enabled: false,
            can_sleep: true,
            sleeping: false,
            collision_layer: 1,
            collision_mask: 1,
            mass: 1.0,
        }
    }
}

/// Detailed result of a 3D shape sweep / motion cast.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ShapeHit3D {
    pub body_id: Option<NodeId>,
    pub collider_handle: r3d::ColliderHandle,
    pub point: Vec3,
    pub normal: Vec3,
    pub distance: f32,
    pub time_of_impact: f32,
}

/// Detailed result of a 3D point query.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct PointHit3D {
    pub body_id: Option<NodeId>,
    pub collider_handle: r3d::ColliderHandle,
    pub point: Vec3,
}

/// Detailed result of a 2D shape sweep / motion cast.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ShapeHit2D {
    pub body_id: Option<NodeId>,
    pub collider_handle: r2d::ColliderHandle,
    pub point: [f32; 2],
    pub normal: [f32; 2],
    pub distance: f32,
    pub time_of_impact: f32,
}

/// Detailed result of a 2D point query.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct PointHit2D {
    pub body_id: Option<NodeId>,
    pub collider_handle: r2d::ColliderHandle,
    pub point: [f32; 2],
}

/// Production 3D physics world space backed by `rapier3d 0.36.1`.
pub struct RapierSpace3D {
    pub gravity: Vec3,
    pub integration_parameters: r3d::IntegrationParameters,
    pub physics_pipeline: r3d::PhysicsPipeline,
    pub island_manager: r3d::IslandManager,
    pub broad_phase: r3d::BroadPhaseBvh,
    pub narrow_phase: r3d::NarrowPhase,
    pub bodies: r3d::RigidBodySet,
    pub colliders: r3d::ColliderSet,
    pub impulse_joints: r3d::ImpulseJointSet,
    pub multibody_joints: r3d::MultibodyJointSet,
    pub soft_bodies: r3d::SoftBodySet,
    pub ccd_solver: r3d::CCDSolver,
    pub node_to_body: BTreeMap<NodeId, r3d::RigidBodyHandle>,
    pub body_to_node: HashMap<r3d::RigidBodyHandle, NodeId>,
    pub node_to_collider: BTreeMap<NodeId, r3d::ColliderHandle>,
    pub collider_to_node: HashMap<r3d::ColliderHandle, NodeId>,
    pub body_masses: HashMap<NodeId, f32>,
    pub time_elapsed: f32,
}

impl fmt::Debug for RapierSpace3D {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RapierSpace3D")
            .field("gravity", &self.gravity)
            .field("bodies_count", &self.bodies.len())
            .field("colliders_count", &self.colliders.len())
            .field("time_elapsed", &self.time_elapsed)
            .finish()
    }
}

impl Default for RapierSpace3D {
    fn default() -> Self {
        Self::new()
    }
}

impl RapierSpace3D {
    /// Construct a new empty 3D space with default Earth gravity (-9.81 on Y).
    #[must_use]
    pub fn new() -> Self {
        Self {
            gravity: Vec3::new(0.0, -9.81, 0.0),
            integration_parameters: r3d::IntegrationParameters::default(),
            physics_pipeline: r3d::PhysicsPipeline::new(),
            island_manager: r3d::IslandManager::new(),
            broad_phase: r3d::BroadPhaseBvh::new(),
            narrow_phase: r3d::NarrowPhase::new(),
            bodies: r3d::RigidBodySet::new(),
            colliders: r3d::ColliderSet::new(),
            impulse_joints: r3d::ImpulseJointSet::new(),
            multibody_joints: r3d::MultibodyJointSet::new(),
            soft_bodies: r3d::SoftBodySet::new(),
            ccd_solver: r3d::CCDSolver::new(),
            node_to_body: BTreeMap::new(),
            body_to_node: HashMap::new(),
            node_to_collider: BTreeMap::new(),
            collider_to_node: HashMap::new(),
            body_masses: HashMap::new(),
            time_elapsed: 0.0,
        }
    }

    /// Construct a space with customized gravity.
    #[must_use]
    pub fn with_gravity(gravity: Vec3) -> Self {
        let mut s = Self::new();
        s.gravity = gravity;
        s
    }

    /// Step simulation forward by `dt_seconds`.
    pub fn step(&mut self, dt_seconds: f32) {
        if dt_seconds <= 0.0 || !dt_seconds.is_finite() {
            return;
        }

        self.integration_parameters.dt = dt_seconds;
        self.time_elapsed += dt_seconds;

        let grav = r3d::Vector::new(self.gravity.x, self.gravity.y, self.gravity.z);
        let physics_hooks = ();
        let event_handler = ();

        self.physics_pipeline.step(
            grav,
            &self.integration_parameters,
            &mut self.island_manager,
            &mut self.broad_phase,
            &mut self.narrow_phase,
            &mut self.bodies,
            &mut self.colliders,
            &mut self.impulse_joints,
            &mut self.multibody_joints,
            &mut self.soft_bodies,
            &mut self.ccd_solver,
            &physics_hooks,
            &event_handler,
        );
    }

    /// Add a rigid body to the simulation space.
    pub fn add_rigid_body(&mut self, id: NodeId, config: &RigidBodyConfig3D) -> r3d::RigidBodyHandle {
        if self.node_to_body.contains_key(&id) {
            self.remove_rigid_body(id);
        }

        let mut builder = match config.body_type {
            BodyType::Dynamic => r3d::RigidBodyBuilder::dynamic(),
            BodyType::KinematicPositionBased => r3d::RigidBodyBuilder::kinematic_position_based(),
            BodyType::KinematicVelocityBased => r3d::RigidBodyBuilder::kinematic_velocity_based(),
            BodyType::Fixed => r3d::RigidBodyBuilder::fixed(),
        };

        let trans = r3d::Vector::new(config.position.x, config.position.y, config.position.z);
        let rot = r3d::Rot3::from_xyzw(
            config.rotation.x,
            config.rotation.y,
            config.rotation.z,
            config.rotation.w,
        );

        builder = builder
            .pose(r3d::Pose::from_parts(trans, rot))
            .linvel(r3d::Vector::new(config.linear_velocity.x, config.linear_velocity.y, config.linear_velocity.z))
            .angvel(r3d::Vector::new(config.angular_velocity.x, config.angular_velocity.y, config.angular_velocity.z))
            .gravity_scale(config.gravity_scale)
            .linear_damping(config.linear_damping)
            .angular_damping(config.angular_damping)
            .ccd_enabled(config.ccd_enabled)
            .can_sleep(config.can_sleep);

        if config.mass > 0.0 && config.body_type == BodyType::Dynamic {
            builder = builder.additional_mass(config.mass);
        }

        if config.sleeping {
            builder = builder.sleeping(true);
        }

        let handle = self.bodies.insert(builder);
        self.node_to_body.insert(id, handle);
        self.body_to_node.insert(handle, id);
        self.body_masses.insert(id, config.mass);
        handle
    }

    /// Remove a rigid body and its attached colliders by ID.
    pub fn remove_rigid_body(&mut self, id: NodeId) -> Option<r3d::RigidBodyHandle> {
        let handle = self.node_to_body.remove(&id)?;
        self.body_to_node.remove(&handle);
        self.body_masses.remove(&id);

        let to_remove: Vec<r3d::ColliderHandle> = self
            .colliders
            .iter()
            .filter_map(|(ch, c)| if c.parent() == Some(handle) { Some(ch) } else { None })
            .collect();

        for ch in to_remove {
            if let Some(nid) = self.collider_to_node.remove(&ch) {
                self.node_to_collider.remove(&nid);
            }
            self.colliders.remove(ch, &mut self.island_manager, &mut self.bodies, &mut self.soft_bodies, true);
        }

        self.bodies.remove(
            handle,
            &mut self.island_manager,
            &mut self.colliders,
            &mut self.impulse_joints,
            &mut self.multibody_joints,
            &mut self.soft_bodies,
            true,
        );

        Some(handle)
    }

    /// Attach a collider to a body.
    pub fn add_collider(
        &mut self,
        body_id: NodeId,
        shape: ShapeConfig3D,
        restitution: f32,
        friction: f32,
        collision_layer: u32,
        collision_mask: u32,
    ) -> Option<r3d::ColliderHandle> {
        let body_handle = *self.node_to_body.get(&body_id)?;
        let shared_shape = shape.to_shared_shape();

        let groups = r3d::InteractionGroups::none()
            .with_memberships(r3d::Group::from_bits_truncate(collision_layer))
            .with_filter(r3d::Group::from_bits_truncate(collision_mask));

        let collider = r3d::ColliderBuilder::new(shared_shape)
            .restitution(restitution)
            .friction(friction)
            .collision_groups(groups)
            .build();

        let collider_handle = self.colliders.insert_with_parent(
            collider,
            body_handle,
            &mut self.bodies,
        );

        let aabb = self.colliders[collider_handle].compute_aabb();
        self.broad_phase.set_aabb(&self.integration_parameters, collider_handle, aabb);

        self.node_to_collider.insert(body_id, collider_handle);
        self.collider_to_node.insert(collider_handle, body_id);
        Some(collider_handle)
    }

    /// Add an impulse joint between two bodies.
    pub fn add_impulse_joint(
        &mut self,
        body_a: NodeId,
        body_b: NodeId,
        config: JointConfig3D,
    ) -> Option<r3d::ImpulseJointHandle> {
        let ha = *self.node_to_body.get(&body_a)?;
        let hb = *self.node_to_body.get(&body_b)?;

        let joint_data: r3d::GenericJoint = match config {
            JointConfig3D::Fixed { local_anchor1, local_anchor2 } => {
                r3d::FixedJointBuilder::new()
                    .local_anchor1(r3d::Vector::new(local_anchor1.x, local_anchor1.y, local_anchor1.z))
                    .local_anchor2(r3d::Vector::new(local_anchor2.x, local_anchor2.y, local_anchor2.z))
                    .into()
            }
            JointConfig3D::Spherical { local_anchor1, local_anchor2 } => {
                r3d::SphericalJointBuilder::new()
                    .local_anchor1(r3d::Vector::new(local_anchor1.x, local_anchor1.y, local_anchor1.z))
                    .local_anchor2(r3d::Vector::new(local_anchor2.x, local_anchor2.y, local_anchor2.z))
                    .into()
            }
            JointConfig3D::Revolute { axis, local_anchor1, local_anchor2, limits, motor } => {
                let mut b = r3d::RevoluteJointBuilder::new(r3d::Vector::new(axis.x, axis.y, axis.z))
                    .local_anchor1(r3d::Vector::new(local_anchor1.x, local_anchor1.y, local_anchor1.z))
                    .local_anchor2(r3d::Vector::new(local_anchor2.x, local_anchor2.y, local_anchor2.z));
                if let Some(lim) = limits {
                    b = b.limits(lim);
                }
                if let Some(m) = motor {
                    b = b.motor_velocity(m.target_vel, m.damping).motor_max_force(m.max_force);
                }
                b.into()
            }
            JointConfig3D::Prismatic { axis, local_anchor1, local_anchor2, limits, motor } => {
                let mut b = r3d::PrismaticJointBuilder::new(r3d::Vector::new(axis.x, axis.y, axis.z))
                    .local_anchor1(r3d::Vector::new(local_anchor1.x, local_anchor1.y, local_anchor1.z))
                    .local_anchor2(r3d::Vector::new(local_anchor2.x, local_anchor2.y, local_anchor2.z));
                if let Some(lim) = limits {
                    b = b.limits(lim);
                }
                if let Some(m) = motor {
                    b = b.motor_velocity(m.target_vel, m.damping).motor_max_force(m.max_force);
                }
                b.into()
            }
            JointConfig3D::Generic6Dof { local_anchor1, local_anchor2, locked_axes, limits, motors } => {
                let mut mask = r3d::JointAxesMask::empty();
                for ax in locked_axes {
                    mask |= ax.into();
                }
                let mut b = r3d::GenericJointBuilder::new(mask)
                    .local_anchor1(r3d::Vector::new(local_anchor1.x, local_anchor1.y, local_anchor1.z))
                    .local_anchor2(r3d::Vector::new(local_anchor2.x, local_anchor2.y, local_anchor2.z));
                for (ax, lim) in limits {
                    b = b.limits(ax, lim);
                }
                for (ax, m) in motors {
                    b = b.motor_velocity(ax, m.target_vel, m.damping)
                        .motor_position(ax, m.target_pos, m.stiffness, m.damping)
                        .motor_max_force(ax, m.max_force);
                }
                b.into()
            }
        };

        Some(self.impulse_joints.insert(ha, hb, joint_data, true))
    }

    /// Retrieve position of body by ID.
    #[must_use]
    pub fn get_body_position(&self, id: NodeId) -> Option<Vec3> {
        let handle = *self.node_to_body.get(&id)?;
        let rb = self.bodies.get(handle)?;
        let t = rb.translation();
        Some(Vec3::new(t.x, t.y, t.z))
    }

    /// Set position of body by ID.
    pub fn set_body_position(&mut self, id: NodeId, pos: Vec3) {
        if let Some(&handle) = self.node_to_body.get(&id) {
            if let Some(rb) = self.bodies.get_mut(handle) {
                let rot = *rb.rotation();
                rb.set_position(
                    r3d::Pose::from_parts(r3d::Vector::new(pos.x, pos.y, pos.z), rot),
                    true,
                );
            }
            let to_update: Vec<(r3d::ColliderHandle, r3d::Aabb)> = self
                .colliders
                .iter()
                .filter_map(|(ch, c)| if c.parent() == Some(handle) { Some((ch, c.compute_aabb())) } else { None })
                .collect();
            for (ch, aabb) in to_update {
                self.broad_phase.set_aabb(&self.integration_parameters, ch, aabb);
            }
        }
    }

    /// Retrieve linear velocity of body by ID.
    #[must_use]
    pub fn get_body_velocity(&self, id: NodeId) -> Option<Vec3> {
        let handle = *self.node_to_body.get(&id)?;
        let rb = self.bodies.get(handle)?;
        let v = rb.linvel();
        Some(Vec3::new(v.x, v.y, v.z))
    }

    /// Set linear velocity of body by ID.
    pub fn set_body_velocity(&mut self, id: NodeId, vel: Vec3) {
        if let Some(rb) = self.node_to_body.get(&id).and_then(|&h| self.bodies.get_mut(h)) {
            rb.set_linvel(r3d::Vector::new(vel.x, vel.y, vel.z), true);
        }
    }

    /// Retrieve angular velocity of body by ID.
    #[must_use]
    pub fn get_body_angular_velocity(&self, id: NodeId) -> Option<Vec3> {
        let handle = *self.node_to_body.get(&id)?;
        let rb = self.bodies.get(handle)?;
        let v = rb.angvel();
        Some(Vec3::new(v.x, v.y, v.z))
    }

    /// Set angular velocity of body by ID.
    pub fn set_body_angular_velocity(&mut self, id: NodeId, ang_vel: Vec3) {
        if let Some(rb) = self.node_to_body.get(&id).and_then(|&h| self.bodies.get_mut(h)) {
            rb.set_angvel(r3d::Vector::new(ang_vel.x, ang_vel.y, ang_vel.z), true);
        }
    }

    /// Apply linear impulse to body by ID.
    pub fn apply_impulse(&mut self, id: NodeId, impulse: Vec3) {
        if let Some(rb) = self.node_to_body.get(&id).and_then(|&h| self.bodies.get_mut(h)) {
            rb.apply_impulse(r3d::Vector::new(impulse.x, impulse.y, impulse.z), true);
        }
    }

    /// Apply torque impulse to body by ID.
    pub fn apply_torque_impulse(&mut self, id: NodeId, torque: Vec3) {
        if let Some(rb) = self.node_to_body.get(&id).and_then(|&h| self.bodies.get_mut(h)) {
            rb.apply_torque_impulse(r3d::Vector::new(torque.x, torque.y, torque.z), true);
        }
    }

    /// Wake up a body from island sleep.
    pub fn wake_up(&mut self, id: NodeId) {
        if let Some(rb) = self.node_to_body.get(&id).and_then(|&h| self.bodies.get_mut(h)) {
            rb.wake_up(true);
        }
    }

    /// Force a body into island sleep.
    pub fn sleep(&mut self, id: NodeId) {
        if let Some(rb) = self.node_to_body.get(&id).and_then(|&h| self.bodies.get_mut(h)) {
            rb.sleep();
        }
    }

    /// Check if body is currently sleeping.
    #[must_use]
    pub fn is_sleeping(&self, id: NodeId) -> bool {
        self.node_to_body.get(&id).and_then(|&h| self.bodies.get(h)).is_some_and(|rb| rb.is_sleeping())
    }

    /// Access direct space state query interface.
    #[must_use]
    pub fn direct_state(&self) -> PhysicsDirectSpaceState3D<'_> {
        PhysicsDirectSpaceState3D { space: self }
    }
}

/// 3D Direct space state spatial query interface (Godot 4 `PhysicsDirectSpaceState3D`).
#[derive(Copy, Clone, Debug)]
pub struct PhysicsDirectSpaceState3D<'a> {
    space: &'a RapierSpace3D,
}

impl PhysicsDirectSpaceState3D<'_> {
    /// Cast a ray from `from` to `to`, filtering by `collision_mask` and excluding bodies in `exclude`.
    #[must_use]
    pub fn intersect_ray(
        &self,
        from: Vec3,
        to: Vec3,
        collision_mask: u32,
        exclude: &[NodeId],
    ) -> Option<RayHit3D> {
        let dir = to - from;
        let max_toi = dir.length();
        if max_toi <= 1e-6 {
            return None;
        }

        let ray = r3d::Ray::new(
            r3d::Vector::new(from.x, from.y, from.z),
            r3d::Vector::new(dir.x / max_toi, dir.y / max_toi, dir.z / max_toi),
        );

        let groups = r3d::InteractionGroups::all().with_filter(r3d::Group::from_bits_truncate(collision_mask));

        let query = self.space.broad_phase.as_query_pipeline(
            self.space.narrow_phase.query_dispatcher(),
            &self.space.bodies,
            &self.space.colliders,
            r3d::QueryFilter::default().groups(groups),
        );

        for (ch, _co, inter) in query.intersect_ray(ray, max_toi, true) {
            let body_id = self.space.collider_to_node.get(&ch).copied();
            if body_id.is_some_and(|bid| exclude.contains(&bid)) {
                continue;
            }
            let hit_pt = from + (dir / max_toi) * inter.time_of_impact;
            return Some(RayHit3D {
                body_id,
                point: hit_pt,
                normal: Vec3::new(inter.normal.x, inter.normal.y, inter.normal.z),
                distance: inter.time_of_impact,
            });
        }

        None
    }

    /// Check all colliders intersecting a point.
    #[must_use]
    pub fn intersect_point(
        &self,
        point: Vec3,
        collision_mask: u32,
        exclude: &[NodeId],
    ) -> Vec<PointHit3D> {
        let pt = r3d::Vector::new(point.x, point.y, point.z);
        let groups = r3d::InteractionGroups::all().with_filter(r3d::Group::from_bits_truncate(collision_mask));

        let query = self.space.broad_phase.as_query_pipeline(
            self.space.narrow_phase.query_dispatcher(),
            &self.space.bodies,
            &self.space.colliders,
            r3d::QueryFilter::default().groups(groups),
        );

        let mut hits = Vec::new();
        for (ch, _co) in query.intersect_point(pt) {
            let body_id = self.space.collider_to_node.get(&ch).copied();
            if body_id.is_some_and(|bid| exclude.contains(&bid)) {
                continue;
            }
            hits.push(PointHit3D {
                body_id,
                collider_handle: ch,
                point,
            });
        }

        hits
    }

    /// Sweep / shape-cast a 3D shape along a motion vector.
    #[must_use]
    pub fn cast_shape(
        &self,
        shape: &r3d::SharedShape,
        from: Vec3,
        to: Vec3,
        collision_mask: u32,
        exclude: &[NodeId],
    ) -> Option<ShapeHit3D> {
        let dir = to - from;
        let max_toi = dir.length();
        if max_toi <= 1e-6 {
            return None;
        }

        let shape_pos = r3d::Pose::from_translation(r3d::Vector::new(from.x, from.y, from.z));
        let velocity = r3d::Vector::new(dir.x, dir.y, dir.z);

        let groups = r3d::InteractionGroups::all().with_filter(r3d::Group::from_bits_truncate(collision_mask));

        let query = self.space.broad_phase.as_query_pipeline(
            self.space.narrow_phase.query_dispatcher(),
            &self.space.bodies,
            &self.space.colliders,
            r3d::QueryFilter::default().groups(groups),
        );

        let opt = rapier3d::parry::query::ShapeCastOptions {
            max_time_of_impact: 1.0,
            stop_at_penetration: false,
            ..Default::default()
        };

        let (ch, hit) = query.cast_shape(&shape_pos, velocity, shape.as_ref(), opt)?;
        let body_id = self.space.collider_to_node.get(&ch).copied();
        if body_id.is_some_and(|bid| exclude.contains(&bid)) {
            return None;
        }

        let hit_pt = Vec3::new(hit.witness1.x, hit.witness1.y, hit.witness1.z);
        let hit_n = Vec3::new(hit.normal1.x, hit.normal1.y, hit.normal1.z);

        Some(ShapeHit3D {
            body_id,
            collider_handle: ch,
            point: hit_pt,
            normal: hit_n,
            distance: max_toi * hit.time_of_impact,
            time_of_impact: hit.time_of_impact,
        })
    }

    /// Godot 4 `cast_motion`: returns safe and unsafe motion fractions [0.0..1.0].
    #[must_use]
    pub fn cast_motion(
        &self,
        shape: &r3d::SharedShape,
        from: Vec3,
        motion: Vec3,
        collision_mask: u32,
        exclude: &[NodeId],
    ) -> (f32, f32) {
        if motion.length_squared() <= 1e-8 {
            return (1.0, 1.0);
        }

        if let Some(hit) = self.cast_shape(shape, from, from + motion, collision_mask, exclude) {
            let unsafe_frac = hit.time_of_impact.clamp(0.0, 1.0);
            let safe_frac = (unsafe_frac - 0.001).max(0.0);
            (safe_frac, unsafe_frac)
        } else {
            (1.0, 1.0)
        }
    }
}

/// Production 2D physics world space backed by `rapier2d 0.36.1`.
pub struct RapierSpace2D {
    pub gravity: [f32; 2],
    pub integration_parameters: r2d::IntegrationParameters,
    pub physics_pipeline: r2d::PhysicsPipeline,
    pub island_manager: r2d::IslandManager,
    pub broad_phase: r2d::BroadPhaseBvh,
    pub narrow_phase: r2d::NarrowPhase,
    pub bodies: r2d::RigidBodySet,
    pub colliders: r2d::ColliderSet,
    pub impulse_joints: r2d::ImpulseJointSet,
    pub multibody_joints: r2d::MultibodyJointSet,
    pub soft_bodies: r2d::SoftBodySet,
    pub ccd_solver: r2d::CCDSolver,
    pub node_to_body: BTreeMap<NodeId, r2d::RigidBodyHandle>,
    pub body_to_node: HashMap<r2d::RigidBodyHandle, NodeId>,
    pub node_to_collider: BTreeMap<NodeId, r2d::ColliderHandle>,
    pub collider_to_node: HashMap<r2d::ColliderHandle, NodeId>,
    pub body_masses: HashMap<NodeId, f32>,
    pub time_elapsed: f32,
}

impl fmt::Debug for RapierSpace2D {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RapierSpace2D")
            .field("gravity", &self.gravity)
            .field("bodies_count", &self.bodies.len())
            .field("colliders_count", &self.colliders.len())
            .field("time_elapsed", &self.time_elapsed)
            .finish()
    }
}

impl Default for RapierSpace2D {
    fn default() -> Self {
        Self::new()
    }
}

impl RapierSpace2D {
    /// Construct a new empty 2D space with default 2D gravity (-9.81 on Y).
    #[must_use]
    pub fn new() -> Self {
        Self {
            gravity: [0.0, -9.81],
            integration_parameters: r2d::IntegrationParameters::default(),
            physics_pipeline: r2d::PhysicsPipeline::new(),
            island_manager: r2d::IslandManager::new(),
            broad_phase: r2d::BroadPhaseBvh::new(),
            narrow_phase: r2d::NarrowPhase::new(),
            bodies: r2d::RigidBodySet::new(),
            colliders: r2d::ColliderSet::new(),
            impulse_joints: r2d::ImpulseJointSet::new(),
            multibody_joints: r2d::MultibodyJointSet::new(),
            soft_bodies: r2d::SoftBodySet::new(),
            ccd_solver: r2d::CCDSolver::new(),
            node_to_body: BTreeMap::new(),
            body_to_node: HashMap::new(),
            node_to_collider: BTreeMap::new(),
            collider_to_node: HashMap::new(),
            body_masses: HashMap::new(),
            time_elapsed: 0.0,
        }
    }

    /// Construct a 2D space with custom gravity.
    #[must_use]
    pub fn with_gravity(gravity: [f32; 2]) -> Self {
        let mut s = Self::new();
        s.gravity = gravity;
        s
    }

    /// Step 2D simulation forward by `dt_seconds`.
    pub fn step(&mut self, dt_seconds: f32) {
        if dt_seconds <= 0.0 || !dt_seconds.is_finite() {
            return;
        }

        self.integration_parameters.dt = dt_seconds;
        self.time_elapsed += dt_seconds;

        let grav = r2d::Vector::new(self.gravity[0], self.gravity[1]);
        let physics_hooks = ();
        let event_handler = ();

        self.physics_pipeline.step(
            grav,
            &self.integration_parameters,
            &mut self.island_manager,
            &mut self.broad_phase,
            &mut self.narrow_phase,
            &mut self.bodies,
            &mut self.colliders,
            &mut self.impulse_joints,
            &mut self.multibody_joints,
            &mut self.soft_bodies,
            &mut self.ccd_solver,
            &physics_hooks,
            &event_handler,
        );
    }

    /// Add a 2D rigid body.
    pub fn add_rigid_body(&mut self, id: NodeId, config: &RigidBodyConfig2D) -> r2d::RigidBodyHandle {
        if self.node_to_body.contains_key(&id) {
            self.remove_rigid_body(id);
        }

        let mut builder = match config.body_type {
            BodyType::Dynamic => r2d::RigidBodyBuilder::dynamic(),
            BodyType::KinematicPositionBased => r2d::RigidBodyBuilder::kinematic_position_based(),
            BodyType::KinematicVelocityBased => r2d::RigidBodyBuilder::kinematic_velocity_based(),
            BodyType::Fixed => r2d::RigidBodyBuilder::fixed(),
        };

        let trans = r2d::Vector::new(config.position[0], config.position[1]);
        let rot = r2d::Rot2::from_angle(config.rotation);

        builder = builder
            .pose(r2d::Pose::from_parts(trans, rot))
            .linvel(r2d::Vector::new(config.linear_velocity[0], config.linear_velocity[1]))
            .angvel(config.angular_velocity)
            .gravity_scale(config.gravity_scale)
            .linear_damping(config.linear_damping)
            .angular_damping(config.angular_damping)
            .ccd_enabled(config.ccd_enabled)
            .can_sleep(config.can_sleep);

        if config.mass > 0.0 && config.body_type == BodyType::Dynamic {
            builder = builder.additional_mass(config.mass);
        }

        if config.sleeping {
            builder = builder.sleeping(true);
        }

        let handle = self.bodies.insert(builder);
        self.node_to_body.insert(id, handle);
        self.body_to_node.insert(handle, id);
        self.body_masses.insert(id, config.mass);
        handle
    }

    /// Remove a 2D rigid body.
    pub fn remove_rigid_body(&mut self, id: NodeId) -> Option<r2d::RigidBodyHandle> {
        let handle = self.node_to_body.remove(&id)?;
        self.body_to_node.remove(&handle);
        self.body_masses.remove(&id);

        let to_remove: Vec<r2d::ColliderHandle> = self
            .colliders
            .iter()
            .filter_map(|(ch, c)| if c.parent() == Some(handle) { Some(ch) } else { None })
            .collect();

        for ch in to_remove {
            if let Some(nid) = self.collider_to_node.remove(&ch) {
                self.node_to_collider.remove(&nid);
            }
            self.colliders.remove(ch, &mut self.island_manager, &mut self.bodies, &mut self.soft_bodies, true);
        }

        self.bodies.remove(
            handle,
            &mut self.island_manager,
            &mut self.colliders,
            &mut self.impulse_joints,
            &mut self.multibody_joints,
            &mut self.soft_bodies,
            true,
        );

        Some(handle)
    }

    /// Attach a 2D collider to a body.
    pub fn add_collider(
        &mut self,
        body_id: NodeId,
        shape: ShapeConfig2D,
        restitution: f32,
        friction: f32,
        collision_layer: u32,
        collision_mask: u32,
    ) -> Option<r2d::ColliderHandle> {
        let body_handle = *self.node_to_body.get(&body_id)?;
        let shared_shape = shape.to_shared_shape();

        let groups = r2d::InteractionGroups::none()
            .with_memberships(r2d::Group::from_bits_truncate(collision_layer))
            .with_filter(r2d::Group::from_bits_truncate(collision_mask));

        let collider = r2d::ColliderBuilder::new(shared_shape)
            .restitution(restitution)
            .friction(friction)
            .collision_groups(groups)
            .build();

        let collider_handle = self.colliders.insert_with_parent(
            collider,
            body_handle,
            &mut self.bodies,
        );

        let aabb = self.colliders[collider_handle].compute_aabb();
        self.broad_phase.set_aabb(&self.integration_parameters, collider_handle, aabb);

        self.node_to_collider.insert(body_id, collider_handle);
        self.collider_to_node.insert(collider_handle, body_id);
        Some(collider_handle)
    }

    /// Add a 2D impulse joint.
    pub fn add_impulse_joint(
        &mut self,
        body_a: NodeId,
        body_b: NodeId,
        config: JointConfig2D,
    ) -> Option<r2d::ImpulseJointHandle> {
        let ha = *self.node_to_body.get(&body_a)?;
        let hb = *self.node_to_body.get(&body_b)?;

        let joint_data: r2d::GenericJoint = match config {
            JointConfig2D::Fixed { local_anchor1, local_anchor2 } => {
                r2d::FixedJointBuilder::new()
                    .local_anchor1(r2d::Vector::new(local_anchor1[0], local_anchor1[1]))
                    .local_anchor2(r2d::Vector::new(local_anchor2[0], local_anchor2[1]))
                    .into()
            }
            JointConfig2D::Revolute { local_anchor1, local_anchor2, limits, motor } => {
                let mut b = r2d::RevoluteJointBuilder::new()
                    .local_anchor1(r2d::Vector::new(local_anchor1[0], local_anchor1[1]))
                    .local_anchor2(r2d::Vector::new(local_anchor2[0], local_anchor2[1]));
                if let Some(lim) = limits {
                    b = b.limits(lim);
                }
                if let Some(m) = motor {
                    b = b.motor_velocity(m.target_vel, m.damping).motor_max_force(m.max_force);
                }
                b.into()
            }
            JointConfig2D::Prismatic { axis, local_anchor1, local_anchor2, limits, motor } => {
                let mut b = r2d::PrismaticJointBuilder::new(r2d::Vector::new(axis[0], axis[1]))
                    .local_anchor1(r2d::Vector::new(local_anchor1[0], local_anchor1[1]))
                    .local_anchor2(r2d::Vector::new(local_anchor2[0], local_anchor2[1]));
                if let Some(lim) = limits {
                    b = b.limits(lim);
                }
                if let Some(m) = motor {
                    b = b.motor_velocity(m.target_vel, m.damping).motor_max_force(m.max_force);
                }
                b.into()
            }
        };

        Some(self.impulse_joints.insert(ha, hb, joint_data, true))
    }

    /// Retrieve position of 2D body.
    #[must_use]
    pub fn get_body_position(&self, id: NodeId) -> Option<[f32; 2]> {
        let handle = *self.node_to_body.get(&id)?;
        let rb = self.bodies.get(handle)?;
        let t = rb.translation();
        Some([t.x, t.y])
    }

    /// Set position of 2D body.
    pub fn set_body_position(&mut self, id: NodeId, pos: [f32; 2]) {
        if let Some(&handle) = self.node_to_body.get(&id) {
            if let Some(rb) = self.bodies.get_mut(handle) {
                let rot = *rb.rotation();
                rb.set_position(
                    r2d::Pose::from_parts(r2d::Vector::new(pos[0], pos[1]), rot),
                    true,
                );
            }
            let to_update: Vec<(r2d::ColliderHandle, r2d::Aabb)> = self
                .colliders
                .iter()
                .filter_map(|(ch, c)| if c.parent() == Some(handle) { Some((ch, c.compute_aabb())) } else { None })
                .collect();
            for (ch, aabb) in to_update {
                self.broad_phase.set_aabb(&self.integration_parameters, ch, aabb);
            }
        }
    }

    /// Retrieve velocity of 2D body.
    #[must_use]
    pub fn get_body_velocity(&self, id: NodeId) -> Option<[f32; 2]> {
        let handle = *self.node_to_body.get(&id)?;
        let rb = self.bodies.get(handle)?;
        let v = rb.linvel();
        Some([v.x, v.y])
    }

    /// Set velocity of 2D body.
    pub fn set_body_velocity(&mut self, id: NodeId, vel: [f32; 2]) {
        if let Some(rb) = self.node_to_body.get(&id).and_then(|&h| self.bodies.get_mut(h)) {
            rb.set_linvel(r2d::Vector::new(vel[0], vel[1]), true);
        }
    }

    /// Apply impulse to 2D body.
    pub fn apply_impulse(&mut self, id: NodeId, impulse: [f32; 2]) {
        if let Some(rb) = self.node_to_body.get(&id).and_then(|&h| self.bodies.get_mut(h)) {
            rb.apply_impulse(r2d::Vector::new(impulse[0], impulse[1]), true);
        }
    }

    /// Wake up 2D body.
    pub fn wake_up(&mut self, id: NodeId) {
        if let Some(rb) = self.node_to_body.get(&id).and_then(|&h| self.bodies.get_mut(h)) {
            rb.wake_up(true);
        }
    }

    /// Check if 2D body is sleeping.
    #[must_use]
    pub fn is_sleeping(&self, id: NodeId) -> bool {
        self.node_to_body.get(&id).and_then(|&h| self.bodies.get(h)).is_some_and(|rb| rb.is_sleeping())
    }

    /// Access 2D direct space query state.
    #[must_use]
    pub fn direct_state(&self) -> PhysicsDirectSpaceState2D<'_> {
        PhysicsDirectSpaceState2D { space: self }
    }
}

/// 2D Direct space state spatial query interface (Godot 4 `PhysicsDirectSpaceState2D`).
#[derive(Copy, Clone, Debug)]
pub struct PhysicsDirectSpaceState2D<'a> {
    space: &'a RapierSpace2D,
}

impl PhysicsDirectSpaceState2D<'_> {
    /// Cast a 2D ray from `from` to `to`.
    #[must_use]
    pub fn intersect_ray(
        &self,
        from: [f32; 2],
        to: [f32; 2],
        collision_mask: u32,
        exclude: &[NodeId],
    ) -> Option<RayHit3D> {
        let dx = to[0] - from[0];
        let dy = to[1] - from[1];
        let max_toi = (dx * dx + dy * dy).sqrt();
        if max_toi <= 1e-6 {
            return None;
        }

        let ray = r2d::Ray::new(
            r2d::Vector::new(from[0], from[1]),
            r2d::Vector::new(dx / max_toi, dy / max_toi),
        );

        let groups = r2d::InteractionGroups::all().with_filter(r2d::Group::from_bits_truncate(collision_mask));

        let query = self.space.broad_phase.as_query_pipeline(
            self.space.narrow_phase.query_dispatcher(),
            &self.space.bodies,
            &self.space.colliders,
            r2d::QueryFilter::default().groups(groups),
        );

        for (ch, _co, inter) in query.intersect_ray(ray, max_toi, true) {
            let body_id = self.space.collider_to_node.get(&ch).copied();
            if body_id.is_some_and(|bid| exclude.contains(&bid)) {
                continue;
            }
            let hit_pt = [
                from[0] + (dx / max_toi) * inter.time_of_impact,
                from[1] + (dy / max_toi) * inter.time_of_impact,
            ];
            return Some(RayHit3D {
                body_id,
                point: Vec3::new(hit_pt[0], hit_pt[1], 0.0),
                normal: Vec3::new(inter.normal.x, inter.normal.y, 0.0),
                distance: inter.time_of_impact,
            });
        }

        None
    }

    /// Check all 2D colliders intersecting a point.
    #[must_use]
    pub fn intersect_point(
        &self,
        point: [f32; 2],
        collision_mask: u32,
        exclude: &[NodeId],
    ) -> Vec<PointHit2D> {
        let pt = r2d::Vector::new(point[0], point[1]);
        let groups = r2d::InteractionGroups::all().with_filter(r2d::Group::from_bits_truncate(collision_mask));

        let query = self.space.broad_phase.as_query_pipeline(
            self.space.narrow_phase.query_dispatcher(),
            &self.space.bodies,
            &self.space.colliders,
            r2d::QueryFilter::default().groups(groups),
        );

        let mut hits = Vec::new();
        for (ch, _co) in query.intersect_point(pt) {
            let body_id = self.space.collider_to_node.get(&ch).copied();
            if body_id.is_some_and(|bid| exclude.contains(&bid)) {
                continue;
            }
            hits.push(PointHit2D {
                body_id,
                collider_handle: ch,
                point,
            });
        }

        hits
    }

    /// Sweep a 2D shape along motion vector.
    #[must_use]
    pub fn cast_shape(
        &self,
        shape: &r2d::SharedShape,
        from: [f32; 2],
        to: [f32; 2],
        collision_mask: u32,
        exclude: &[NodeId],
    ) -> Option<ShapeHit2D> {
        let dx = to[0] - from[0];
        let dy = to[1] - from[1];
        let max_toi = (dx * dx + dy * dy).sqrt();
        if max_toi <= 1e-6 {
            return None;
        }

        let shape_pos = r2d::Pose::from_translation(r2d::Vector::new(from[0], from[1]));
        let velocity = r2d::Vector::new(dx, dy);

        let groups = r2d::InteractionGroups::all().with_filter(r2d::Group::from_bits_truncate(collision_mask));

        let query = self.space.broad_phase.as_query_pipeline(
            self.space.narrow_phase.query_dispatcher(),
            &self.space.bodies,
            &self.space.colliders,
            r2d::QueryFilter::default().groups(groups),
        );

        let opt = rapier2d::parry::query::ShapeCastOptions {
            max_time_of_impact: 1.0,
            stop_at_penetration: false,
            ..Default::default()
        };

        let (ch, hit) = query.cast_shape(&shape_pos, velocity, shape.as_ref(), opt)?;
        let body_id = self.space.collider_to_node.get(&ch).copied();
        if body_id.is_some_and(|bid| exclude.contains(&bid)) {
            return None;
        }

        Some(ShapeHit2D {
            body_id,
            collider_handle: ch,
            point: [hit.witness1.x, hit.witness1.y],
            normal: [hit.normal1.x, hit.normal1.y],
            distance: max_toi * hit.time_of_impact,
            time_of_impact: hit.time_of_impact,
        })
    }

    /// Godot 4 2D `cast_motion`: returns safe and unsafe motion fractions [0.0..1.0].
    #[must_use]
    pub fn cast_motion(
        &self,
        shape: &r2d::SharedShape,
        from: [f32; 2],
        motion: [f32; 2],
        collision_mask: u32,
        exclude: &[NodeId],
    ) -> (f32, f32) {
        let m_len_sq = motion[0] * motion[0] + motion[1] * motion[1];
        if m_len_sq <= 1e-8 {
            return (1.0, 1.0);
        }

        let to = [from[0] + motion[0], from[1] + motion[1]];
        if let Some(hit) = self.cast_shape(shape, from, to, collision_mask, exclude) {
            let unsafe_frac = hit.time_of_impact.clamp(0.0, 1.0);
            let safe_frac = (unsafe_frac - 0.001).max(0.0);
            (safe_frac, unsafe_frac)
        } else {
            (1.0, 1.0)
        }
    }
}

/// Snapshot of an individual physical body for lossless cross-backend state migration.
#[derive(Clone, Debug, PartialEq)]
pub struct BodyStateSnapshot3D {
    pub id: NodeId,
    pub position: Vec3,
    pub rotation: Quat,
    pub linear_velocity: Vec3,
    pub angular_velocity: Vec3,
    pub mass: f32,
    pub restitution: f32,
    pub grounded: bool,
    pub active: bool,
    pub collision_layer: u32,
    pub collision_mask: u32,
    pub body_type: BodyType,
}

/// Lossless snapshot of entire 3D world state.
#[derive(Clone, Debug, PartialEq)]
pub struct PhysicsStateSnapshot3D {
    pub bodies: Vec<BodyStateSnapshot3D>,
    pub gravity: Vec3,
    pub time_elapsed: f32,
}

/// Bidirectional lossless state snapshot bridge allowing live mid-simulation hotswapping
/// between `RapierSpace3D` and Foreplay's existing custom `PortFaithful` skate/port physics solvers.
#[derive(Clone, Copy, Debug, Default)]
pub struct HotswapPhysicsBridge3D;

impl HotswapPhysicsBridge3D {
    /// Extract a snapshot of physical state from `RapierSpace3D`.
    #[must_use]
    pub fn snapshot_rapier(space: &RapierSpace3D) -> PhysicsStateSnapshot3D {
        let mut bodies = Vec::with_capacity(space.node_to_body.len());

        for (&node_id, &handle) in &space.node_to_body {
            if let Some(rb) = space.bodies.get(handle) {
                let trans = rb.translation();
                let rot = rb.rotation();
                let linvel = rb.linvel();
                let angvel = rb.angvel();

                let body_type = match rb.body_type() {
                    r3d::RigidBodyType::Dynamic => BodyType::Dynamic,
                    r3d::RigidBodyType::KinematicPositionBased => BodyType::KinematicPositionBased,
                    r3d::RigidBodyType::KinematicVelocityBased => BodyType::KinematicVelocityBased,
                    r3d::RigidBodyType::Fixed => BodyType::Fixed,
                    _ => BodyType::Dynamic,
                };

                let mut col_layer = 1;
                let mut col_mask = 1;
                let mut rest = 0.0;

                if let Some(c) = space.node_to_collider.get(&node_id).and_then(|&ch| space.colliders.get(ch)) {
                    col_layer = c.collision_groups().memberships.bits();
                    col_mask = c.collision_groups().filter.bits();
                    rest = c.restitution();
                }

                let final_mass = if rb.mass() > 0.0 {
                    rb.mass()
                } else {
                    space.body_masses.get(&node_id).copied().unwrap_or(1.0)
                };

                bodies.push(BodyStateSnapshot3D {
                    id: node_id,
                    position: Vec3::new(trans.x, trans.y, trans.z),
                    rotation: Quat::from_xyzw(rot.x, rot.y, rot.z, rot.w),
                    linear_velocity: Vec3::new(linvel.x, linvel.y, linvel.z),
                    angular_velocity: Vec3::new(angvel.x, angvel.y, angvel.z),
                    mass: final_mass,
                    restitution: rest,
                    grounded: rb.is_sleeping(),
                    active: !rb.is_sleeping(),
                    collision_layer: col_layer,
                    collision_mask: col_mask,
                    body_type,
                });
            }
        }

        PhysicsStateSnapshot3D {
            bodies,
            gravity: space.gravity,
            time_elapsed: space.time_elapsed,
        }
    }

    /// Restore or update `RapierSpace3D` from a state snapshot.
    pub fn restore_rapier(snapshot: &PhysicsStateSnapshot3D, space: &mut RapierSpace3D) {
        space.gravity = snapshot.gravity;
        space.time_elapsed = snapshot.time_elapsed;

        for body in &snapshot.bodies {
            if let Some(rb) = space.node_to_body.get(&body.id).and_then(|&h| space.bodies.get_mut(h)) {
                let trans = r3d::Vector::new(body.position.x, body.position.y, body.position.z);
                let rot = r3d::Rot3::from_xyzw(
                    body.rotation.x,
                    body.rotation.y,
                    body.rotation.z,
                    body.rotation.w,
                );
                rb.set_position(r3d::Pose::from_parts(trans, rot), true);
                rb.set_linvel(
                    r3d::Vector::new(body.linear_velocity.x, body.linear_velocity.y, body.linear_velocity.z),
                    true,
                );
                rb.set_angvel(
                    r3d::Vector::new(body.angular_velocity.x, body.angular_velocity.y, body.angular_velocity.z),
                    true,
                );
                if body.active {
                    rb.wake_up(true);
                } else {
                    rb.sleep();
                }
            } else {
                let config = RigidBodyConfig3D {
                    body_type: body.body_type,
                    position: body.position,
                    rotation: body.rotation,
                    linear_velocity: body.linear_velocity,
                    angular_velocity: body.angular_velocity,
                    gravity_scale: 1.0,
                    linear_damping: 0.0,
                    angular_damping: 0.0,
                    ccd_enabled: false,
                    can_sleep: true,
                    sleeping: !body.active,
                    collision_layer: body.collision_layer,
                    collision_mask: body.collision_mask,
                    mass: body.mass,
                };
                space.add_rigid_body(body.id, &config);
            }
        }
    }

    /// Extract a snapshot from `PhysicsWorldState` (custom/port faithful simulator).
    #[must_use]
    pub fn snapshot_world_state(world: &PhysicsWorldState) -> PhysicsStateSnapshot3D {
        let bodies = world
            .bodies
            .iter()
            .map(|b| BodyStateSnapshot3D {
                id: b.id,
                position: b.position,
                rotation: Quat::IDENTITY,
                linear_velocity: b.velocity,
                angular_velocity: b.angular_velocity,
                mass: b.mass,
                restitution: b.restitution,
                grounded: b.grounded,
                active: true,
                collision_layer: b.collision_layer,
                collision_mask: b.collision_mask,
                body_type: BodyType::Dynamic,
            })
            .collect();

        PhysicsStateSnapshot3D {
            bodies,
            gravity: world.gravity,
            time_elapsed: world.time_elapsed,
        }
    }

    /// Restore or synchronize `PhysicsWorldState` from a state snapshot.
    pub fn restore_world_state(snapshot: &PhysicsStateSnapshot3D, world: &mut PhysicsWorldState) {
        world.gravity = snapshot.gravity;
        world.time_elapsed = snapshot.time_elapsed;

        for body in &snapshot.bodies {
            if let Some(b) = world.get_body_mut(body.id) {
                b.position = body.position;
                b.velocity = body.linear_velocity;
                b.angular_velocity = body.angular_velocity;
                b.grounded = body.grounded;
                b.mass = body.mass;
                b.restitution = body.restitution;
                b.collision_layer = body.collision_layer;
                b.collision_mask = body.collision_mask;
            } else {
                let mut new_body = PhysicsBody3D::new(
                    body.id,
                    body.position,
                    Collider3D::Sphere { radius: 0.5 },
                );
                new_body.velocity = body.linear_velocity;
                new_body.angular_velocity = body.angular_velocity;
                new_body.grounded = body.grounded;
                new_body.mass = body.mass;
                new_body.restitution = body.restitution;
                new_body.collision_layer = body.collision_layer;
                new_body.collision_mask = body.collision_mask;
                world.add_body(new_body);
            }
        }
    }

    /// Live lossless hotswap from `RapierSpace3D` to `PhysicsWorldState` (PortFaithful).
    pub fn swap_to_port_faithful(space: &RapierSpace3D, world: &mut PhysicsWorldState) {
        let snapshot = Self::snapshot_rapier(space);
        Self::restore_world_state(&snapshot, world);
    }

    /// Live lossless hotswap from `PhysicsWorldState` (PortFaithful) to `RapierSpace3D`.
    pub fn swap_to_rapier(world: &PhysicsWorldState, space: &mut RapierSpace3D) {
        let snapshot = Self::snapshot_world_state(world);
        Self::restore_rapier(&snapshot, space);
    }
}

/// Stateful physics backend wrapping `RapierSpace3D` for live A/B switching in `PhysicsServer3D`.
#[derive(Debug)]
pub struct RapierBackend3D {
    space: Arc<RwLock<RapierSpace3D>>,
    world_state: Arc<RwLock<PhysicsWorldState>>,
}

impl RapierBackend3D {
    #[must_use]
    pub fn new(space: Arc<RwLock<RapierSpace3D>>, world_state: Arc<RwLock<PhysicsWorldState>>) -> Self {
        Self { space, world_state }
    }

    #[must_use]
    pub fn space(&self) -> Arc<RwLock<RapierSpace3D>> {
        self.space.clone()
    }
}

impl PhysicsBackend for RapierBackend3D {
    fn backend_id(&self) -> &'static str {
        "rapier3d"
    }

    fn step(&mut self, dt_seconds: f32) {
        if dt_seconds <= 0.0 || !dt_seconds.is_finite() {
            return;
        }
        let mut space = self.space.write().unwrap();
        space.step(dt_seconds);
        let mut world = self.world_state.write().unwrap();
        HotswapPhysicsBridge3D::swap_to_port_faithful(&space, &mut world);
    }
}

impl StatefulPhysicsBackend for RapierBackend3D {
    fn extract_state(&self) -> PhysicsWorldState {
        let space = self.space.read().unwrap();
        let mut world = self.world_state.read().unwrap().clone();
        HotswapPhysicsBridge3D::swap_to_port_faithful(&space, &mut world);
        world
    }

    fn inject_state(&mut self, new_state: PhysicsWorldState) {
        let mut space = self.space.write().unwrap();
        HotswapPhysicsBridge3D::swap_to_rapier(&new_state, &mut space);
        *self.world_state.write().unwrap() = new_state;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PhysicsBackendKind, PhysicsServer3D};

    #[test]
    fn rapier3d_ccd_bullet_through_thin_wall_prevention() {
        let mut space = RapierSpace3D::new();
        let wall_id = NodeId::new(1001, 1).unwrap();
        let bullet_id = NodeId::new(1002, 1).unwrap();

        // Thin wall at X = 5.0, half_extents = (0.025, 5.0, 5.0), thickness = 0.05m
        space.add_rigid_body(wall_id, &RigidBodyConfig3D {
            body_type: BodyType::Fixed,
            position: Vec3::new(5.0, 0.0, 0.0),
            ..Default::default()
        });
        space.add_collider(wall_id, ShapeConfig3D::Cuboid { half_extents: Vec3::new(0.025, 5.0, 5.0) }, 0.0, 0.0, 1, 1);

        // Fast bullet: starts at X = 4.0, velocity = (200.0, 0.0, 0.0)
        space.add_rigid_body(bullet_id, &RigidBodyConfig3D {
            body_type: BodyType::Dynamic,
            position: Vec3::new(4.0, 0.0, 0.0),
            linear_velocity: Vec3::new(200.0, 0.0, 0.0),
            gravity_scale: 0.0,
            ccd_enabled: true,
            ..Default::default()
        });
        space.add_collider(bullet_id, ShapeConfig3D::Ball { radius: 0.1 }, 0.0, 0.0, 1, 1);

        // Step simulation 1 frame
        space.step(1.0 / 60.0);

        let pos = space.get_body_position(bullet_id).unwrap();
        // Bullet must NOT tunnel through the thin wall (pos.x <= 5.0)
        assert!(pos.x <= 5.0 + 1e-2, "bullet tunneled through thin wall: x = {}", pos.x);
    }

    #[test]
    fn rapier3d_island_sleeping_and_wake_up() {
        let mut space = RapierSpace3D::new();
        let ground_id = NodeId::new(1010, 1).unwrap();
        let box_id = NodeId::new(1011, 1).unwrap();

        space.add_rigid_body(ground_id, &RigidBodyConfig3D {
            body_type: BodyType::Fixed,
            position: Vec3::ZERO,
            ..Default::default()
        });
        space.add_collider(ground_id, ShapeConfig3D::Cuboid { half_extents: Vec3::new(10.0, 0.1, 10.0) }, 0.0, 0.5, 1, 1);

        space.add_rigid_body(box_id, &RigidBodyConfig3D {
            body_type: BodyType::Dynamic,
            position: Vec3::new(0.0, 0.5, 0.0),
            can_sleep: true,
            ..Default::default()
        });
        space.add_collider(box_id, ShapeConfig3D::Cuboid { half_extents: Vec3::new(0.2, 0.2, 0.2) }, 0.0, 0.5, 1, 1);

        for _ in 0..120 {
            space.step(1.0 / 60.0);
        }

        assert!(space.is_sleeping(box_id), "expected box to enter island sleep after settling");

        space.wake_up(box_id);
        assert!(!space.is_sleeping(box_id), "expected box to wake up");
    }

    #[test]
    fn rapier3d_revolute_hinge_motor_joint() {
        let mut space = RapierSpace3D::new();
        let base_id = NodeId::new(1020, 1).unwrap();
        let rotor_id = NodeId::new(1021, 1).unwrap();

        space.add_rigid_body(base_id, &RigidBodyConfig3D {
            body_type: BodyType::Fixed,
            position: Vec3::ZERO,
            ..Default::default()
        });

        space.add_rigid_body(rotor_id, &RigidBodyConfig3D {
            body_type: BodyType::Dynamic,
            position: Vec3::new(0.0, 1.0, 0.0),
            gravity_scale: 0.0,
            ..Default::default()
        });
        space.add_collider(rotor_id, ShapeConfig3D::Cuboid { half_extents: Vec3::splat(0.2) }, 0.0, 0.0, 1, 1);

        space.add_impulse_joint(
            base_id,
            rotor_id,
            JointConfig3D::Revolute {
                axis: Vec3::Y,
                local_anchor1: Vec3::new(0.0, 1.0, 0.0),
                local_anchor2: Vec3::ZERO,
                limits: None,
                motor: Some(JointMotorConfig {
                    target_vel: 10.0,
                    target_pos: 0.0,
                    stiffness: 0.0,
                    damping: 100.0,
                    max_force: 1000.0,
                }),
            },
        );

        for _ in 0..60 {
            space.step(1.0 / 60.0);
        }

        let ang_vel = space.get_body_angular_velocity(rotor_id).unwrap();
        assert!((ang_vel.y - 10.0).abs() < 1.0, "expected motor angvel.y ~= 10.0, got {}", ang_vel.y);
    }

    #[test]
    fn rapier3d_generic_6dof_joint_limits_and_spring() {
        let mut space = RapierSpace3D::new();
        let base_id = NodeId::new(1030, 1).unwrap();
        let payload_id = NodeId::new(1031, 1).unwrap();

        space.add_rigid_body(base_id, &RigidBodyConfig3D {
            body_type: BodyType::Fixed,
            position: Vec3::ZERO,
            ..Default::default()
        });

        space.add_rigid_body(payload_id, &RigidBodyConfig3D {
            body_type: BodyType::Dynamic,
            position: Vec3::ZERO,
            gravity_scale: 0.0,
            ..Default::default()
        });
        space.add_collider(payload_id, ShapeConfig3D::Ball { radius: 0.2 }, 0.0, 0.0, 1, 1);

        space.add_impulse_joint(
            base_id,
            payload_id,
            JointConfig3D::Generic6Dof {
                local_anchor1: Vec3::ZERO,
                local_anchor2: Vec3::ZERO,
                locked_axes: vec![
                    r3d::JointAxis::LinY,
                    r3d::JointAxis::LinZ,
                    r3d::JointAxis::AngX,
                    r3d::JointAxis::AngY,
                    r3d::JointAxis::AngZ,
                ],
                limits: vec![(r3d::JointAxis::LinX, [-0.5, 0.5])],
                motors: vec![(
                    r3d::JointAxis::LinX,
                    JointMotorConfig {
                        target_vel: 0.0,
                        target_pos: 0.0,
                        stiffness: 50.0,
                        damping: 5.0,
                        max_force: 500.0,
                    },
                )],
            },
        );

        space.apply_impulse(payload_id, Vec3::new(10.0, 0.0, 0.0));

        for _ in 0..60 {
            space.step(1.0 / 60.0);
        }

        let pos = space.get_body_position(payload_id).unwrap();
        assert!(pos.x <= 0.51 && pos.x >= -0.51, "position X escaped limit corridor: x = {}", pos.x);
        assert!(pos.y.abs() < 1e-3);
        assert!(pos.z.abs() < 1e-3);
    }

    #[test]
    fn rapier3d_prismatic_slider_joint_limits() {
        let mut space = RapierSpace3D::new();
        let base_id = NodeId::new(1040, 1).unwrap();
        let slider_id = NodeId::new(1041, 1).unwrap();

        space.add_rigid_body(base_id, &RigidBodyConfig3D {
            body_type: BodyType::Fixed,
            position: Vec3::ZERO,
            ..Default::default()
        });

        space.add_rigid_body(slider_id, &RigidBodyConfig3D {
            body_type: BodyType::Dynamic,
            position: Vec3::ZERO,
            gravity_scale: 0.0,
            ..Default::default()
        });
        space.add_collider(slider_id, ShapeConfig3D::Cuboid { half_extents: Vec3::splat(0.2) }, 0.0, 0.0, 1, 1);

        space.add_impulse_joint(
            base_id,
            slider_id,
            JointConfig3D::Prismatic {
                axis: Vec3::X,
                local_anchor1: Vec3::ZERO,
                local_anchor2: Vec3::ZERO,
                limits: Some([-1.0, 1.0]),
                motor: None,
            },
        );

        space.apply_impulse(slider_id, Vec3::new(100.0, 0.0, 0.0));

        for _ in 0..30 {
            space.step(1.0 / 60.0);
        }

        let pos = space.get_body_position(slider_id).unwrap();
        assert!(pos.x <= 1.02, "slider exceeded prismatic max limit: x = {}", pos.x);
    }

    #[test]
    fn rapier3d_fixed_joint_rigid_attachment() {
        let mut space = RapierSpace3D::new();
        let parent_id = NodeId::new(1050, 1).unwrap();
        let child_id = NodeId::new(1051, 1).unwrap();

        space.add_rigid_body(parent_id, &RigidBodyConfig3D {
            body_type: BodyType::Dynamic,
            position: Vec3::ZERO,
            gravity_scale: 0.0,
            ..Default::default()
        });
        space.add_collider(parent_id, ShapeConfig3D::Ball { radius: 0.5 }, 0.0, 0.0, 1, 1);

        space.add_rigid_body(child_id, &RigidBodyConfig3D {
            body_type: BodyType::Dynamic,
            position: Vec3::new(2.0, 0.0, 0.0),
            gravity_scale: 0.0,
            ..Default::default()
        });
        space.add_collider(child_id, ShapeConfig3D::Ball { radius: 0.5 }, 0.0, 0.0, 1, 1);

        space.add_impulse_joint(
            parent_id,
            child_id,
            JointConfig3D::Fixed {
                local_anchor1: Vec3::new(2.0, 0.0, 0.0),
                local_anchor2: Vec3::ZERO,
            },
        );

        space.apply_impulse(parent_id, Vec3::new(0.0, 10.0, 0.0));

        for _ in 0..10 {
            space.step(1.0 / 60.0);
        }

        let p_pos = space.get_body_position(parent_id).unwrap();
        let c_pos = space.get_body_position(child_id).unwrap();
        let dist = (c_pos - p_pos).length();
        assert!((dist - 2.0).abs() < 0.05, "fixed joint distance deviated: {}", dist);
    }

    #[test]
    fn rapier3d_live_hotswap_to_port_faithful_mid_air() {
        let mut space = RapierSpace3D::new();
        let skater_id = NodeId::new(1060, 1).unwrap();

        let initial_pos = Vec3::new(10.0, 25.0, -5.0);
        let initial_vel = Vec3::new(14.0, 8.0, -3.0);
        let initial_ang = Vec3::new(0.5, 1.2, -0.8);

        space.add_rigid_body(skater_id, &RigidBodyConfig3D {
            body_type: BodyType::Dynamic,
            position: initial_pos,
            linear_velocity: initial_vel,
            angular_velocity: initial_ang,
            mass: 75.0,
            ..Default::default()
        });

        let mut world = PhysicsWorldState::new();
        HotswapPhysicsBridge3D::swap_to_port_faithful(&space, &mut world);

        let migrated = world.get_body(skater_id).expect("body migrated into world state");
        assert_eq!(migrated.position, initial_pos);
        assert_eq!(migrated.velocity, initial_vel);
        assert_eq!(migrated.angular_velocity, initial_ang);
        assert_eq!(migrated.mass, 75.0);
    }

    #[test]
    fn port_faithful_to_rapier_live_hotswap_mid_air() {
        let mut world = PhysicsWorldState::new();
        let skater_id = NodeId::new(1070, 1).unwrap();
        let mut body = PhysicsBody3D::new(skater_id, Vec3::new(3.0, 12.0, 1.0), Collider3D::Sphere { radius: 0.5 });
        body.velocity = Vec3::new(-5.0, 6.0, 2.0);
        body.angular_velocity = Vec3::new(1.0, 0.0, 2.0);
        world.add_body(body);

        let mut space = RapierSpace3D::new();
        HotswapPhysicsBridge3D::swap_to_rapier(&world, &mut space);

        let pos = space.get_body_position(skater_id).unwrap();
        let vel = space.get_body_velocity(skater_id).unwrap();
        let ang = space.get_body_angular_velocity(skater_id).unwrap();

        assert_eq!(pos, Vec3::new(3.0, 12.0, 1.0));
        assert_eq!(vel, Vec3::new(-5.0, 6.0, 2.0));
        assert_eq!(ang, Vec3::new(1.0, 0.0, 2.0));
    }

    #[test]
    fn physics_server_3d_live_hotswap_with_rapier3d() {
        let mut server = PhysicsServer3D::new_standard();
        let skater_id = NodeId::new(1080, 1).unwrap();
        let mut body = PhysicsBody3D::new(skater_id, Vec3::new(0.0, 10.0, 0.0), Collider3D::Sphere { radius: 0.5 });
        body.velocity = Vec3::new(8.0, 0.0, 0.0);
        server.add_body(body);

        for _ in 0..10 {
            server.step(1.0 / 60.0);
        }

        let b_before = server.get_body(skater_id).unwrap();
        let pos_before = b_before.position;
        let vel_before = b_before.velocity;

        let _ = server.hotswap_preserving_bodies(PhysicsBackendKind::Rapier3D);
        assert_eq!(server.active_backend_id(), "rapier3d");

        let b_after = server.get_body(skater_id).unwrap();
        assert_eq!(b_after.position, pos_before);
        assert_eq!(b_after.velocity, vel_before);

        for _ in 0..10 {
            server.step(1.0 / 60.0);
        }

        let b_continued = server.get_body(skater_id).unwrap();
        assert!(b_continued.position.x > pos_before.x);
    }

    #[test]
    fn rapier3d_direct_space_state_raycast() {
        let mut space = RapierSpace3D::new();
        let box_id = NodeId::new(1090, 1).unwrap();

        space.add_rigid_body(box_id, &RigidBodyConfig3D {
            body_type: BodyType::Fixed,
            position: Vec3::new(5.0, 0.0, 0.0),
            ..Default::default()
        });
        space.add_collider(box_id, ShapeConfig3D::Cuboid { half_extents: Vec3::splat(1.0) }, 0.0, 0.0, 1, 1);

        let state = space.direct_state();

        let hit = state.intersect_ray(Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 0.0, 0.0), 1, &[]).expect("hits box");
        assert_eq!(hit.body_id, Some(box_id));
        assert!((hit.distance - 4.0).abs() < 1e-2);
        assert_eq!(hit.normal, Vec3::NEG_X);

        let hit_excluded = state.intersect_ray(Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 0.0, 0.0), 1, &[box_id]);
        assert!(hit_excluded.is_none());
    }

    #[test]
    fn rapier3d_direct_space_state_point_and_shapecast() {
        let mut space = RapierSpace3D::new();
        let ball_id = NodeId::new(1100, 1).unwrap();

        space.add_rigid_body(ball_id, &RigidBodyConfig3D {
            body_type: BodyType::Fixed,
            position: Vec3::new(10.0, 0.0, 0.0),
            ..Default::default()
        });
        space.add_collider(ball_id, ShapeConfig3D::Ball { radius: 2.0 }, 0.0, 0.0, 1, 1);

        let state = space.direct_state();

        let pt_hits = state.intersect_point(Vec3::new(10.0, 0.0, 0.0), 1, &[]);
        assert_eq!(pt_hits.len(), 1);
        assert_eq!(pt_hits[0].body_id, Some(ball_id));

        let sphere_shape = r3d::SharedShape::ball(1.0);
        let shape_hit = state.cast_shape(&sphere_shape, Vec3::new(0.0, 0.0, 0.0), Vec3::new(20.0, 0.0, 0.0), 1, &[]).expect("sweep hits ball");
        assert!((shape_hit.distance - 7.0).abs() < 1e-2);
        assert_eq!(shape_hit.body_id, Some(ball_id));

        let (safe, unsafe_f) = state.cast_motion(&sphere_shape, Vec3::new(0.0, 0.0, 0.0), Vec3::new(20.0, 0.0, 0.0), 1, &[]);
        assert!(safe < unsafe_f);
        assert!((unsafe_f - 0.35).abs() < 1e-2);
    }

    #[test]
    fn rapier2d_direct_space_state_queries() {
        let mut space = RapierSpace2D::new();
        let box_id = NodeId::new(1110, 1).unwrap();

        space.add_rigid_body(box_id, &RigidBodyConfig2D {
            body_type: BodyType::Fixed,
            position: [5.0, 0.0],
            ..Default::default()
        });
        space.add_collider(box_id, ShapeConfig2D::Cuboid { half_extents: [1.0, 1.0] }, 0.0, 0.0, 1, 1);

        let state = space.direct_state();
        let hit = state.intersect_ray([0.0, 0.0], [10.0, 0.0], 1, &[]).expect("2d ray hits box");
        assert_eq!(hit.body_id, Some(box_id));
        assert!((hit.distance - 4.0).abs() < 1e-2);

        let pt_hits = state.intersect_point([5.0, 0.0], 1, &[]);
        assert_eq!(pt_hits.len(), 1);

        let shape = r2d::SharedShape::ball(0.5);
        let shape_hit = state.cast_shape(&shape, [0.0, 0.0], [10.0, 0.0], 1, &[]).expect("2d sweep hits");
        assert!((shape_hit.distance - 3.5).abs() < 1e-2);
    }

    #[test]
    fn rapier3d_collider_shapes_creation_and_simulation() {
        let mut space = RapierSpace3D::new();

        let cuboid_id = NodeId::new(1120, 1).unwrap();
        space.add_rigid_body(cuboid_id, &RigidBodyConfig3D::default());
        space.add_collider(cuboid_id, ShapeConfig3D::Cuboid { half_extents: Vec3::splat(1.0) }, 0.1, 0.5, 1, 1);

        let capsule_id = NodeId::new(1121, 1).unwrap();
        space.add_rigid_body(capsule_id, &RigidBodyConfig3D::default());
        space.add_collider(capsule_id, ShapeConfig3D::CapsuleY { half_height: 1.0, radius: 0.5 }, 0.1, 0.5, 1, 1);

        let cylinder_id = NodeId::new(1122, 1).unwrap();
        space.add_rigid_body(cylinder_id, &RigidBodyConfig3D::default());
        space.add_collider(cylinder_id, ShapeConfig3D::Cylinder { half_height: 1.0, radius: 0.5 }, 0.1, 0.5, 1, 1);

        let hull_id = NodeId::new(1123, 1).unwrap();
        space.add_rigid_body(hull_id, &RigidBodyConfig3D::default());
        space.add_collider(hull_id, ShapeConfig3D::ConvexHull { points: vec![Vec3::ZERO, Vec3::X, Vec3::Y, Vec3::Z] }, 0.1, 0.5, 1, 1);

        let hf_id = NodeId::new(1124, 1).unwrap();
        space.add_rigid_body(hf_id, &RigidBodyConfig3D::default());
        space.add_collider(hf_id, ShapeConfig3D::HeightField { nrows: 2, ncols: 2, heights: vec![0.0, 1.0, 1.0, 0.0], scale: Vec3::ONE }, 0.1, 0.5, 1, 1);

        let trimesh_id = NodeId::new(1125, 1).unwrap();
        space.add_rigid_body(trimesh_id, &RigidBodyConfig3D::default());
        space.add_collider(trimesh_id, ShapeConfig3D::TriMesh { vertices: vec![Vec3::ZERO, Vec3::X, Vec3::Y], indices: vec![[0, 1, 2]] }, 0.1, 0.5, 1, 1);

        space.step(1.0 / 60.0);
        assert_eq!(space.colliders.len(), 6);
    }
}
