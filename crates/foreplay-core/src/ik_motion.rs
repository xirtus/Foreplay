//! Skeletal Inverse Kinematics & Motion Matching (`foreplay-core::ik_motion`).
//!
//! Provides:
//! - Analytical Two-Bone IK (`solve_two_bone_ik`) for planting feet on sloped skateboards, ramps, and stairs.
//! - FABRIK iterative chain IK (`solve_fabrik_chain`) for multi-joint spines, tails, and ropes.
//! - Feature-cost Motion Matching (`PoseSearchDatabase`, `MotionSample`, inertial blending).

use bevy_math::Vec3;
use serde::{Deserialize, Serialize};

/// Solves analytical Two-Bone Inverse Kinematics (Law of Cosines).
///
/// Given three initial joint positions (`root`, `mid`, `tip`), a `target` position,
/// and a `pole` direction vector (e.g. knee or elbow bend direction), returns the
/// updated positions `(new_mid, new_tip)`.
///
/// Preserves exact bone segment lengths `|mid - root|` and `|tip - mid|`.
pub fn solve_two_bone_ik(
    root: Vec3,
    mid: Vec3,
    tip: Vec3,
    target: Vec3,
    pole: Vec3,
) -> (Vec3, Vec3) {
    let l1 = root.distance(mid).max(1e-5);
    let l2 = mid.distance(tip).max(1e-5);

    let to_target = target - root;
    let dist_target = to_target.length();

    // Target direction
    let dir = if dist_target > 1e-5 {
        to_target / dist_target
    } else {
        Vec3::Y
    };

    // Clamp target distance within bone extension limits
    let min_dist = (l1 - l2).abs() + 1e-4;
    let max_dist = (l1 + l2) - 1e-4;
    let d = dist_target.clamp(min_dist, max_dist);

    // Law of cosines: l2^2 = l1^2 + d^2 - 2 * l1 * d * cos(alpha)
    let cos_alpha = ((l1 * l1 + d * d - l2 * l2) / (2.0 * l1 * d)).clamp(-1.0, 1.0);
    let alpha = cos_alpha.acos();

    // Determine bend plane using pole vector
    let to_pole = pole - root;
    let mut plane_normal = dir.cross(to_pole);
    if plane_normal.length_squared() < 1e-6 {
        // Fallback if pole is collinear with target
        let alt = if dir.y.abs() > 0.99 { Vec3::Z } else { Vec3::Y };
        plane_normal = dir.cross(alt);
    }
    let plane_normal = plane_normal.normalize_or_zero();

    // Perpendicular vector pointing towards the pole in the bending plane
    let bend_dir = plane_normal.cross(dir).normalize_or_zero();

    // Position mid joint
    let new_mid = root + (dir * (l1 * alpha.cos())) + (bend_dir * (l1 * alpha.sin()));

    // Position tip joint: preserve length l2 along direction from new_mid towards clamped target
    let clamped_target = root + dir * d;
    let to_clamped = (clamped_target - new_mid).normalize_or_zero();
    let new_tip = new_mid + to_clamped * l2;

    (new_mid, new_tip)
}

/// Solves FABRIK (Forward And Backward Reaching Inverse Kinematics) on an arbitrary chain of joints.
///
/// Modifies `joints` in-place to reach `target` within `tolerance` distance.
/// Preserves distances between adjacent joints in the chain.
///
/// Returns `true` if the chain reached `target` within `tolerance`.
pub fn solve_fabrik_chain(
    joints: &mut [Vec3],
    target: Vec3,
    tolerance: f32,
    max_iters: usize,
) -> bool {
    let n = joints.len();
    if n < 2 {
        if n == 1 {
            joints[0] = target;
        }
        return true;
    }

    // Precompute segment lengths
    let mut lengths = Vec::with_capacity(n - 1);
    let mut total_length = 0.0;
    for i in 0..(n - 1) {
        let len = joints[i].distance(joints[i + 1]);
        lengths.push(len);
        total_length += len;
    }

    let origin = joints[0];
    let dist_to_target = origin.distance(target);

    // If target is beyond maximum reach, stretch straight towards target
    if dist_to_target > total_length {
        let dir = if dist_to_target > 1e-5 {
            (target - origin) / dist_to_target
        } else {
            Vec3::Y
        };

        for i in 0..(n - 1) {
            joints[i + 1] = joints[i] + dir * lengths[i];
        }
        return false;
    }

    // Target is reachable: iterate Forward and Backward passes
    for _ in 0..max_iters {
        // Check if tip reached target
        if joints[n - 1].distance(target) <= tolerance {
            return true;
        }

        // Backward reach: from tip to root
        joints[n - 1] = target;
        for i in (0..(n - 1)).rev() {
            let to_prev = joints[i] - joints[i + 1];
            let len = to_prev.length();
            let dir = if len > 1e-5 {
                to_prev / len
            } else {
                Vec3::Y
            };
            joints[i] = joints[i + 1] + dir * lengths[i];
        }

        // Forward reach: from root to tip
        joints[0] = origin;
        for i in 0..(n - 1) {
            let to_next = joints[i + 1] - joints[i];
            let len = to_next.length();
            let dir = if len > 1e-5 {
                to_next / len
            } else {
                Vec3::Y
            };
            joints[i + 1] = joints[i] + dir * lengths[i];
        }
    }

    joints[n - 1].distance(target) <= tolerance
}

/// A single animation feature sample for Motion Matching.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MotionSample {
    pub sample_id: u32,
    pub anim_id: String,
    pub time: f32,
    /// Root linear velocity in local space.
    pub velocity: Vec3,
    /// Extrapolated root trajectory point 200ms in the future.
    pub future_trajectory_200ms: Vec3,
    /// Left foot position relative to root.
    pub left_foot_offset: Vec3,
    /// Right foot position relative to root.
    pub right_foot_offset: Vec3,
}

/// Weights used to tune feature importance when matching motions.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct MotionCostWeights {
    pub weight_velocity: f32,
    pub weight_trajectory: f32,
    pub weight_left_foot: f32,
    pub weight_right_foot: f32,
}

impl Default for MotionCostWeights {
    fn default() -> Self {
        Self {
            weight_velocity: 1.0,
            weight_trajectory: 1.5,
            weight_left_foot: 0.75,
            weight_right_foot: 0.75,
        }
    }
}

/// Query context passed into Motion Matching search.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct MotionQuery {
    pub target_velocity: Vec3,
    pub target_trajectory_200ms: Vec3,
    pub current_left_foot: Vec3,
    pub current_right_foot: Vec3,
}

/// Database of recorded motion samples for responsive feature-cost motion matching.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PoseSearchDatabase {
    pub samples: Vec<MotionSample>,
}

impl PoseSearchDatabase {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a motion sample to the database.
    pub fn add_sample(&mut self, sample: MotionSample) {
        self.samples.push(sample);
    }

    /// Number of indexed motion samples.
    pub fn len(&self) -> usize {
        self.samples.len()
    }

    /// True if the database contains zero samples.
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// Computes the normalized feature cost between a sample and a query.
    #[inline]
    pub fn compute_cost(
        sample: &MotionSample,
        query: &MotionQuery,
        weights: &MotionCostWeights,
    ) -> f32 {
        let vel_diff = sample.velocity.distance_squared(query.target_velocity);
        let traj_diff = sample
            .future_trajectory_200ms
            .distance_squared(query.target_trajectory_200ms);
        let lf_diff = sample
            .left_foot_offset
            .distance_squared(query.current_left_foot);
        let rf_diff = sample
            .right_foot_offset
            .distance_squared(query.current_right_foot);

        (weights.weight_velocity * vel_diff)
            + (weights.weight_trajectory * traj_diff)
            + (weights.weight_left_foot * lf_diff)
            + (weights.weight_right_foot * rf_diff)
    }

    /// Finds the best matching motion sample minimizing feature cost.
    pub fn best_match(
        &self,
        query: &MotionQuery,
        weights: &MotionCostWeights,
    ) -> Option<(&MotionSample, f32)> {
        let mut best: Option<(&MotionSample, f32)> = None;

        for sample in &self.samples {
            let cost = Self::compute_cost(sample, query, weights);
            match best {
                None => best = Some((sample, cost)),
                Some((_, best_cost)) if cost < best_cost => {
                    best = Some((sample, cost));
                }
                _ => {}
            }
        }

        best
    }

    /// Finds the top `k` matching motion samples sorted by increasing cost.
    pub fn search_k_nearest(
        &self,
        query: &MotionQuery,
        weights: &MotionCostWeights,
        k: usize,
    ) -> Vec<(&MotionSample, f32)> {
        let mut matches: Vec<(&MotionSample, f32)> = self
            .samples
            .iter()
            .map(|s| (s, Self::compute_cost(s, query, weights)))
            .collect();

        matches.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        matches.truncate(k);
        matches
    }
}

/// Inertializer decay state for seamless motion transitions without pose popping.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct InertialBlendState {
    pub transition_duration: f32,
    pub elapsed: f32,
    pub pos_offset: Vec3,
    pub vel_offset: Vec3,
}

impl InertialBlendState {
    /// Creates a new inertial blend transition from the discrepancy between source and target pose.
    pub fn new(pos_offset: Vec3, vel_offset: Vec3, transition_duration: f32) -> Self {
        Self {
            transition_duration: transition_duration.max(0.001),
            elapsed: 0.0,
            pos_offset,
            vel_offset,
        }
    }

    /// Evaluates the current inertial offset and advances elapsed time by `dt`.
    pub fn evaluate(&mut self, dt: f32) -> Vec3 {
        let t = (self.elapsed / self.transition_duration).clamp(0.0, 1.0);
        // Quintic smoothstep: (1 - t)^3 * (1 + 3t + 6t^2)
        let decay = (1.0 - t).powi(3) * (1.0 + 3.0 * t + 6.0 * t * t);
        let offset = self.pos_offset * decay + self.vel_offset * (decay * (1.0 - t) * 0.1);
        self.elapsed += dt;
        offset
    }

    /// Returns `true` if the inertial transition has finished decaying.
    pub fn is_finished(&self) -> bool {
        self.elapsed >= self.transition_duration
    }
}
