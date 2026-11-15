//! Skeleton3D Hierarchical Pose Solver and AnimationTree Composable Blend Graph.
//!
//! Provides hardware-ready skeletal skinning matrix evaluation, composable blend nodes
//! (`AnimationNodeAnimation`, `AnimationNodeBlend2`, `AnimationNodeOneShot`, `AnimationNodeBlendSpace1D`,
//! `AnimationNodeBlendSpace2D` with barycentric triangle interpolation), and `AnimationNodeStateMachine`
//! with BFS pathfinding (`travel`), crossfading, and parameter predicate auto-advance.

use bevy_math::Mat4;
use foreplay_ir::IrValue;
use std::collections::{BTreeMap, BTreeSet};

// ============================================================================
// Skeleton3D
// ============================================================================

/// A single 3D skeletal bone node within a hierarchical skeleton.
#[derive(Debug, Clone, PartialEq)]
pub struct Bone3D {
    pub name: String,
    pub parent_idx: Option<usize>,
    pub rest_local: Mat4,
    pub pose_local: Mat4,
    pub inverse_bind: Mat4,
    pub global_pose: Mat4,
}

impl Bone3D {
    pub fn new(name: impl Into<String>, parent_idx: Option<usize>) -> Self {
        Self {
            name: name.into(),
            parent_idx,
            rest_local: Mat4::IDENTITY,
            pose_local: Mat4::IDENTITY,
            inverse_bind: Mat4::IDENTITY,
            global_pose: Mat4::IDENTITY,
        }
    }

    pub fn with_rest(mut self, rest: Mat4) -> Self {
        self.rest_local = rest;
        self
    }

    pub fn with_inverse_bind(mut self, inv_bind: Mat4) -> Self {
        self.inverse_bind = inv_bind;
        self
    }
}

/// Godot-parity `Skeleton3D` managing hierarchical bone transforms and skinning matrices.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Skeleton3D {
    pub bones: Vec<Bone3D>,
}

impl Skeleton3D {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_bone(&mut self, bone: Bone3D) -> usize {
        let idx = self.bones.len();
        self.bones.push(bone);
        idx
    }

    pub fn bone_count(&self) -> usize {
        self.bones.len()
    }

    pub fn find_bone(&self, name: &str) -> Option<usize> {
        self.bones.iter().position(|b| b.name == name)
    }

    pub fn set_bone_pose(&mut self, idx: usize, pose: Mat4) {
        if let Some(b) = self.bones.get_mut(idx) {
            b.pose_local = pose;
        }
    }

    pub fn reset_bone_poses(&mut self) {
        for b in &mut self.bones {
            b.pose_local = Mat4::IDENTITY;
        }
    }

    /// Evaluates world/global poses for all bones down the hierarchy.
    ///
    /// For bone `i`:
    /// `global_pose = parent_global * (rest_local * pose_local)`
    pub fn evaluate_global_poses(&mut self) {
        for i in 0..self.bones.len() {
            let parent_idx = self.bones[i].parent_idx;
            let local_transform = self.bones[i].rest_local * self.bones[i].pose_local;

            let global = match parent_idx {
                Some(p) if p < i => self.bones[p].global_pose * local_transform,
                Some(p) if p < self.bones.len() => {
                    let parent_global = self.evaluate_bone_global_recursive(p);
                    parent_global * local_transform
                }
                _ => local_transform,
            };

            self.bones[i].global_pose = global;
        }
    }

    fn evaluate_bone_global_recursive(&self, idx: usize) -> Mat4 {
        let mut curr = idx;
        let mut chain = Vec::new();
        let mut visited = 0;
        while visited < self.bones.len() {
            chain.push(self.bones[curr].rest_local * self.bones[curr].pose_local);
            if let Some(p) = self.bones[curr].parent_idx {
                curr = p;
                visited += 1;
            } else {
                break;
            }
        }
        let mut result = Mat4::IDENTITY;
        for t in chain.into_iter().rev() {
            result *= t;
        }
        result
    }

    /// Computes the GPU skeletal skinning matrices: `global_pose * inverse_bind`.
    pub fn compute_skin_matrices(&mut self) -> Vec<Mat4> {
        self.evaluate_global_poses();
        self.bones
            .iter()
            .map(|b| b.global_pose * b.inverse_bind)
            .collect()
    }
}

// ============================================================================
// AnimationTree Nodes
// ============================================================================

/// Plays a named animation clip.
#[derive(Debug, Clone, PartialEq)]
pub struct AnimationNodeAnimation {
    pub animation: String,
    pub playback_time: f32,
    pub speed_scale: f32,
}

impl AnimationNodeAnimation {
    pub fn new(animation: impl Into<String>) -> Self {
        Self {
            animation: animation.into(),
            playback_time: 0.0,
            speed_scale: 1.0,
        }
    }

    pub fn advance(&mut self, dt: f32) {
        self.playback_time += dt * self.speed_scale;
    }
}

/// Blends between two inputs with a scalar weight (0.0 = A, 1.0 = B) and optional per-bone filter mask.
#[derive(Debug, Clone, PartialEq)]
pub struct AnimationNodeBlend2 {
    pub input_a: Box<AnimationTreeNode>,
    pub input_b: Box<AnimationTreeNode>,
    pub blend_amount: f32,
    pub filter_enabled: bool,
    pub filtered_bones: BTreeSet<String>,
}

impl AnimationNodeBlend2 {
    pub fn new(input_a: AnimationTreeNode, input_b: AnimationTreeNode) -> Self {
        Self {
            input_a: Box::new(input_a),
            input_b: Box::new(input_b),
            blend_amount: 0.0,
            filter_enabled: false,
            filtered_bones: BTreeSet::new(),
        }
    }

    pub fn with_filter(mut self, bones: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.filter_enabled = true;
        self.filtered_bones = bones.into_iter().map(Into::into).collect();
        self
    }

    pub fn blend_weight_for_bone(&self, bone_name: &str) -> f32 {
        if !self.filter_enabled || self.filtered_bones.contains(bone_name) {
            self.blend_amount.clamp(0.0, 1.0)
        } else {
            0.0
        }
    }
}

/// One-shot action animation overlay (e.g. shoot, dodge, attack).
#[derive(Debug, Clone, PartialEq)]
pub struct AnimationNodeOneShot {
    pub base_input: Box<AnimationTreeNode>,
    pub shot_input: Box<AnimationTreeNode>,
    pub fade_in: f32,
    pub hold: f32,
    pub fade_out: f32,
    pub active: bool,
    pub elapsed_time: f32,
}

impl AnimationNodeOneShot {
    pub fn new(
        base: AnimationTreeNode,
        shot: AnimationTreeNode,
        fade_in: f32,
        hold: f32,
        fade_out: f32,
    ) -> Self {
        Self {
            base_input: Box::new(base),
            shot_input: Box::new(shot),
            fade_in: fade_in.max(0.0001),
            hold: hold.max(0.0),
            fade_out: fade_out.max(0.0001),
            active: false,
            elapsed_time: 0.0,
        }
    }

    pub fn fire(&mut self) {
        self.active = true;
        self.elapsed_time = 0.0;
    }

    pub fn abort(&mut self) {
        self.active = false;
        self.elapsed_time = 0.0;
    }

    pub fn current_weight(&self) -> f32 {
        if !self.active {
            return 0.0;
        }

        let t = self.elapsed_time;
        if t < self.fade_in {
            (t / self.fade_in).clamp(0.0, 1.0)
        } else if t < self.fade_in + self.hold {
            1.0
        } else if t < self.fade_in + self.hold + self.fade_out {
            let rel = t - (self.fade_in + self.hold);
            (1.0 - (rel / self.fade_out)).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    pub fn advance(&mut self, dt: f32) -> f32 {
        if !self.active {
            return 0.0;
        }
        self.elapsed_time += dt;
        let w = self.current_weight();
        if self.elapsed_time >= self.fade_in + self.hold + self.fade_out {
            self.active = false;
        }
        w
    }
}

/// 1D linear blend space (e.g. Idle -> Walk -> Run).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AnimationNodeBlendSpace1D {
    pub points: Vec<(f32, String)>,
}

impl AnimationNodeBlendSpace1D {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_point(&mut self, pos: f32, clip_name: impl Into<String>) {
        self.points.push((pos, clip_name.into()));
        self.points.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    }

    pub fn sample(&self, blend_val: f32) -> Vec<(String, f32)> {
        if self.points.is_empty() {
            return Vec::new();
        }
        if self.points.len() == 1 || blend_val <= self.points[0].0 {
            return vec![(self.points[0].1.clone(), 1.0)];
        }
        if blend_val >= self.points.last().unwrap().0 {
            return vec![(self.points.last().unwrap().1.clone(), 1.0)];
        }

        let idx = self.points.partition_point(|p| p.0 <= blend_val);
        let p0 = &self.points[idx - 1];
        let p1 = &self.points[idx];

        let span = (p1.0 - p0.0).max(1e-6);
        let factor = ((blend_val - p0.0) / span).clamp(0.0, 1.0);

        vec![
            (p0.1.clone(), 1.0 - factor),
            (p1.1.clone(), factor),
        ]
    }
}

/// 2D parameter blend space supporting Barycentric Triangle Interpolation.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AnimationNodeBlendSpace2D {
    pub points: Vec<([f32; 2], String)>,
    pub triangles: Vec<[usize; 3]>,
}

impl AnimationNodeBlendSpace2D {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_point(&mut self, pos: [f32; 2], clip_name: impl Into<String>) -> usize {
        let idx = self.points.len();
        self.points.push((pos, clip_name.into()));
        idx
    }

    pub fn add_triangle(&mut self, i0: usize, i1: usize, i2: usize) {
        self.triangles.push([i0, i1, i2]);
    }

    /// Evaluates blend weights for a given 2D parameter `[x, y]` using barycentric triangle coordinates.
    pub fn sample_barycentric(&self, param: [f32; 2]) -> Vec<(String, f32)> {
        if self.points.is_empty() {
            return Vec::new();
        }
        if self.points.len() == 1 {
            return vec![(self.points[0].1.clone(), 1.0)];
        }

        // Exact point match check
        for (pos, clip) in &self.points {
            let dx = pos[0] - param[0];
            let dy = pos[1] - param[1];
            if dx * dx + dy * dy < 1e-6 {
                return vec![(clip.clone(), 1.0)];
            }
        }

        // Check if inside any configured triangle
        for &[i0, i1, i2] in &self.triangles {
            if i0 >= self.points.len() || i1 >= self.points.len() || i2 >= self.points.len() {
                continue;
            }
            let a = self.points[i0].0;
            let b = self.points[i1].0;
            let c = self.points[i2].0;

            let det = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
            if det.abs() < 1e-6 {
                continue;
            }

            let u = ((b[1] - c[1]) * (param[0] - c[0]) + (c[0] - b[0]) * (param[1] - c[1])) / det;
            let v = ((c[1] - a[1]) * (param[0] - c[0]) + (a[0] - c[0]) * (param[1] - c[1])) / det;
            let w = 1.0 - u - v;

            const EPS: f32 = -1e-4;
            if u >= EPS && v >= EPS && w >= EPS {
                let u_clamped = u.max(0.0);
                let v_clamped = v.max(0.0);
                let w_clamped = w.max(0.0);
                let sum = (u_clamped + v_clamped + w_clamped).max(1e-6);

                let mut res = Vec::new();
                if u_clamped > 1e-4 {
                    res.push((self.points[i0].1.clone(), u_clamped / sum));
                }
                if v_clamped > 1e-4 {
                    res.push((self.points[i1].1.clone(), v_clamped / sum));
                }
                if w_clamped > 1e-4 {
                    res.push((self.points[i2].1.clone(), w_clamped / sum));
                }
                return res;
            }
        }

        // Fallback: Inverse Distance Weighting across all points
        let mut total_w = 0.0;
        let mut raw = Vec::with_capacity(self.points.len());
        for (pos, clip) in &self.points {
            let dist_sq = (pos[0] - param[0]).powi(2) + (pos[1] - param[1]).powi(2);
            let w = 1.0 / dist_sq.max(1e-6);
            raw.push((clip.clone(), w));
            total_w += w;
        }

        raw.into_iter()
            .map(|(clip, w)| (clip, w / total_w))
            .collect()
    }
}

// ============================================================================
// State Machine & Auto-Advance Transitions
// ============================================================================

/// Predicate condition governing auto-advance transitions in `AnimationNodeStateMachine`.
#[derive(Debug, Clone, PartialEq)]
pub enum AdvanceCondition {
    Equals(String, IrValue),
    NotEquals(String, IrValue),
    GreaterThan(String, f64),
    LessThan(String, f64),
    BoolParam(String),
}

impl AdvanceCondition {
    pub fn evaluate(&self, params: &BTreeMap<String, IrValue>) -> bool {
        match self {
            Self::Equals(param, expected) => {
                params.get(param) == Some(expected)
            }
            Self::NotEquals(param, not_expected) => {
                params.get(param) != Some(not_expected)
            }
            Self::GreaterThan(param, threshold) => {
                match params.get(param) {
                    Some(IrValue::F64(v)) => *v > *threshold,
                    Some(IrValue::I64(v)) => (*v as f64) > *threshold,
                    _ => false,
                }
            }
            Self::LessThan(param, threshold) => {
                match params.get(param) {
                    Some(IrValue::F64(v)) => *v < *threshold,
                    Some(IrValue::I64(v)) => (*v as f64) < *threshold,
                    _ => false,
                }
            }
            Self::BoolParam(param) => {
                match params.get(param) {
                    Some(IrValue::Bool(b)) => *b,
                    _ => false,
                }
            }
        }
    }
}

/// Transition rule linking two states in `AnimationNodeStateMachine`.
#[derive(Debug, Clone, PartialEq)]
pub struct AnimTreeTransition {
    pub from: String,
    pub to: String,
    pub xfade_time: f32,
    pub condition: Option<AdvanceCondition>,
}

impl AnimTreeTransition {
    pub fn new(from: impl Into<String>, to: impl Into<String>, xfade_time: f32) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
            xfade_time: xfade_time.max(0.0),
            condition: None,
        }
    }

    pub fn with_condition(mut self, cond: AdvanceCondition) -> Self {
        self.condition = Some(cond);
        self
    }
}

pub use crate::animation::{AnimationNodeStateMachine, StateTransition};

impl From<AnimTreeTransition> for StateTransition {
    fn from(t: AnimTreeTransition) -> Self {
        Self {
            from: t.from,
            to: t.to,
            crossfade_time: t.xfade_time,
            advance_condition: t.condition.map(|c| match c {
                AdvanceCondition::Equals(k, _)
                | AdvanceCondition::NotEquals(k, _)
                | AdvanceCondition::GreaterThan(k, _)
                | AdvanceCondition::LessThan(k, _)
                | AdvanceCondition::BoolParam(k) => k,
            }),
        }
    }
}

// ============================================================================
// AnimationTree & Evaluator
// ============================================================================

/// Polymorphic node in an `AnimationTree` blend graph.
#[derive(Debug, Clone, PartialEq)]
pub enum AnimationTreeNode {
    Animation(AnimationNodeAnimation),
    Blend2(AnimationNodeBlend2),
    OneShot(AnimationNodeOneShot),
    BlendSpace1D(AnimationNodeBlendSpace1D),
    BlendSpace2D(AnimationNodeBlendSpace2D),
    StateMachine(AnimationNodeStateMachine),
}

impl AnimationTreeNode {
    pub fn evaluate_weights(&self, param1d: f32, param2d: [f32; 2]) -> Vec<(String, f32)> {
        match self {
            Self::Animation(a) => vec![(a.animation.clone(), 1.0)],
            Self::Blend2(b) => {
                let wa = b.input_a.evaluate_weights(param1d, param2d);
                let wb = b.input_b.evaluate_weights(param1d, param2d);
                let factor = b.blend_amount.clamp(0.0, 1.0);
                let mut res = Vec::new();
                for (name, w) in wa {
                    res.push((name, w * (1.0 - factor)));
                }
                for (name, w) in wb {
                    res.push((name, w * factor));
                }
                res
            }
            Self::OneShot(os) => {
                let w_shot = os.current_weight();
                let w_base = 1.0 - w_shot;
                let wa = os.base_input.evaluate_weights(param1d, param2d);
                let wb = os.shot_input.evaluate_weights(param1d, param2d);
                let mut res = Vec::new();
                for (name, w) in wa {
                    res.push((name, w * w_base));
                }
                for (name, w) in wb {
                    res.push((name, w * w_shot));
                }
                res
            }
            Self::BlendSpace1D(bs) => bs.sample(param1d),
            Self::BlendSpace2D(bs) => bs.sample_barycentric(param2d),
            Self::StateMachine(sm) => {
                let clip = sm.states.get(&sm.current_state).cloned().unwrap_or_default();
                vec![(clip, 1.0)]
            }
        }
    }
}

/// Helper function to interpolate two Mat4 transforms using TRS decomposition.
pub fn interpolate_transform_mat4(a: Mat4, b: Mat4, t: f32) -> Mat4 {
    let factor = t.clamp(0.0, 1.0);
    let (scale_a, rot_a, trans_a) = a.to_scale_rotation_translation();
    let (scale_b, rot_b, trans_b) = b.to_scale_rotation_translation();

    let trans = trans_a.lerp(trans_b, factor);
    let scale = scale_a.lerp(scale_b, factor);
    let rot = rot_a.slerp(rot_b, factor);

    Mat4::from_scale_rotation_translation(scale, rot, trans)
}

/// Godot-parity `AnimationTree` combining skeletal playback with composable blend graphs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AnimationTree {
    pub active: bool,
    pub skeleton_path: Option<String>,
    pub anim_player_path: Option<String>,
    pub parameters: BTreeMap<String, IrValue>,
    pub root_node: Option<AnimationTreeNode>,
}

impl AnimationTree {
    pub fn new() -> Self {
        Self {
            active: true,
            ..Self::default()
        }
    }

    pub fn set_root(&mut self, root: AnimationTreeNode) {
        self.root_node = Some(root);
    }

    pub fn set_parameter(&mut self, param: impl Into<String>, val: IrValue) {
        self.parameters.insert(param.into(), val);
    }

    pub fn get_parameter(&self, param: &str) -> Option<&IrValue> {
        self.parameters.get(param)
    }
}
