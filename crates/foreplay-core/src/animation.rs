//! AnimationPlayer, AnimationTree State Machine, and BlendSpace Solvers.

use foreplay_ir::{IrNode, IrScene, IrValue};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// Interpolation mode for animation track keyframes.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum InterpolationMode {
    Nearest,
    #[default]
    Linear,
    Cubic,
}

/// Playback looping behavior matching Godot's Animation::LoopMode.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum LoopMode {
    #[default]
    None,
    Linear,
    PingPong,
}

/// A timed keyframe holding a value of type `T`.
#[derive(Clone, PartialEq, Debug)]
pub struct Keyframe<T> {
    pub time: f32,
    pub value: T,
}

impl<T> Keyframe<T> {
    pub fn new(time: f32, value: T) -> Self {
        Self { time, value }
    }
}

/// A timed method call keyframe.
#[derive(Clone, PartialEq, Debug)]
pub struct MethodKeyframe {
    pub time: f32,
    pub method: String,
    pub args: Vec<IrValue>,
}

impl MethodKeyframe {
    pub fn new(time: f32, method: impl Into<String>, args: Vec<IrValue>) -> Self {
        Self {
            time,
            method: method.into(),
            args,
        }
    }
}

/// Typed animation track modifying a specific property or spatial transform.
#[derive(Clone, PartialEq, Debug)]
pub enum AnimationTrack {
    ValueTrack {
        node_path: String,
        property: String,
        keyframes: Vec<Keyframe<IrValue>>,
        interpolation: InterpolationMode,
    },
    Position3DTrack {
        node_path: String,
        keyframes: Vec<Keyframe<[f32; 3]>>,
        interpolation: InterpolationMode,
    },
    RotationQuatTrack {
        node_path: String,
        keyframes: Vec<Keyframe<[f32; 4]>>,
        interpolation: InterpolationMode,
    },
    Scale3DTrack {
        node_path: String,
        keyframes: Vec<Keyframe<[f32; 3]>>,
        interpolation: InterpolationMode,
    },
    MethodCallTrack {
        node_path: String,
        keyframes: Vec<MethodKeyframe>,
    },
}

impl AnimationTrack {
    pub fn node_path(&self) -> &str {
        match self {
            Self::ValueTrack { node_path, .. }
            | Self::Position3DTrack { node_path, .. }
            | Self::RotationQuatTrack { node_path, .. }
            | Self::Scale3DTrack { node_path, .. }
            | Self::MethodCallTrack { node_path, .. } => node_path,
        }
    }
}

/// Canonical animation clip holding multiple timeline tracks.
#[derive(Clone, PartialEq, Debug)]
pub struct AnimationClip {
    pub name: String,
    pub duration: f32,
    pub step: f32,
    pub loop_mode: LoopMode,
    pub tracks: Vec<AnimationTrack>,
}

impl AnimationClip {
    pub fn new(name: impl Into<String>, duration: f32) -> Self {
        Self {
            name: name.into(),
            duration: duration.max(0.001),
            step: 1.0 / 60.0,
            loop_mode: LoopMode::None,
            tracks: Vec::new(),
        }
    }

    pub fn with_loop(mut self, loop_mode: LoopMode) -> Self {
        self.loop_mode = loop_mode;
        self
    }

    pub fn add_track(&mut self, track: AnimationTrack) {
        self.tracks.push(track);
    }
}

/// Playback engine executing animation clips and mutating `IrScene` nodes.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct AnimationPlayer {
    pub animations: BTreeMap<String, AnimationClip>,
    pub current_animation: Option<String>,
    pub playback_position: f32,
    pub speed_scale: f32,
    pub is_playing: bool,
    pub is_backwards: bool,
    ping_pong_forward: bool,
}

impl AnimationPlayer {
    pub fn new() -> Self {
        Self {
            speed_scale: 1.0,
            ping_pong_forward: true,
            ..Self::default()
        }
    }

    pub fn add_clip(&mut self, clip: AnimationClip) {
        self.animations.insert(clip.name.clone(), clip);
    }

    pub fn play(&mut self, anim_name: &str) -> bool {
        if self.animations.contains_key(anim_name) {
            self.current_animation = Some(anim_name.to_string());
            self.playback_position = 0.0;
            self.is_playing = true;
            self.ping_pong_forward = true;
            true
        } else {
            false
        }
    }

    pub fn pause(&mut self) {
        self.is_playing = false;
    }

    pub fn stop(&mut self) {
        self.is_playing = false;
        self.playback_position = 0.0;
    }

    pub fn seek(&mut self, time: f32) {
        if let Some(clip) = self.current_clip() {
            self.playback_position = time.clamp(0.0, clip.duration);
        }
    }

    pub fn current_clip(&self) -> Option<&AnimationClip> {
        self.current_animation.as_deref().and_then(|name| self.animations.get(name))
    }

    /// Advances playback position by `dt` seconds, handling looping and ping-pong bounds.
    /// Returns `true` if an `animation_finished` boundary was triggered this tick.
    pub fn advance(&mut self, dt: f32) -> bool {
        if !self.is_playing {
            return false;
        }

        let Some(clip) = self.current_clip().cloned() else {
            return false;
        };

        let effective_dt = dt * self.speed_scale;
        let mut finished = false;

        match clip.loop_mode {
            LoopMode::None => {
                self.playback_position += effective_dt;
                if self.playback_position >= clip.duration {
                    self.playback_position = clip.duration;
                    self.is_playing = false;
                    finished = true;
                }
            }
            LoopMode::Linear => {
                self.playback_position += effective_dt;
                if self.playback_position >= clip.duration {
                    self.playback_position %= clip.duration;
                    finished = true;
                }
            }
            LoopMode::PingPong => {
                if self.ping_pong_forward {
                    self.playback_position += effective_dt;
                    if self.playback_position >= clip.duration {
                        self.playback_position = clip.duration;
                        self.ping_pong_forward = false;
                        finished = true;
                    }
                } else {
                    self.playback_position -= effective_dt;
                    if self.playback_position <= 0.0 {
                        self.playback_position = 0.0;
                        self.ping_pong_forward = true;
                        finished = true;
                    }
                }
            }
        }

        finished
    }

    /// Evaluates the active clip's keyframes at the current playback position and writes values directly into `scene`.
    /// Also returns triggered method calls as `(node_path, method_name, args)`.
    pub fn evaluate_into_scene(
        &mut self,
        scene: &mut IrScene,
        dt: f32,
    ) -> (bool, Vec<(String, String, Vec<IrValue>)>) {
        let finished = self.advance(dt);
        let mut method_calls = Vec::new();

        let Some(clip) = self.current_clip().cloned() else {
            return (finished, method_calls);
        };

        let t = self.playback_position;

        for track in &clip.tracks {
            let target_node = find_node_mut_by_path(scene, track.node_path());

            match track {
                AnimationTrack::ValueTrack {
                    property,
                    keyframes,
                    interpolation,
                    ..
                } => {
                    if let Some(val) = sample_value_track(keyframes, t, *interpolation)
                        && let Some(node) = target_node
                    {
                        node.properties.insert(property.clone(), val);
                    }
                }
                AnimationTrack::Position3DTrack {
                    keyframes,
                    interpolation,
                    ..
                } => {
                    if let Some(pos) = sample_vec3_track(keyframes, t, *interpolation)
                        && let Some(node) = target_node
                    {
                        node.properties.insert("translation".into(), IrValue::Vec3(pos));
                    }
                }
                AnimationTrack::RotationQuatTrack {
                    keyframes,
                    interpolation,
                    ..
                } => {
                    if let Some(rot) = sample_quat_track(keyframes, t, *interpolation)
                        && let Some(node) = target_node
                    {
                        node.properties.insert("rotation".into(), IrValue::Quat(rot));
                    }
                }
                AnimationTrack::Scale3DTrack {
                    keyframes,
                    interpolation,
                    ..
                } => {
                    if let Some(scale) = sample_vec3_track(keyframes, t, *interpolation)
                        && let Some(node) = target_node
                    {
                        node.properties.insert("scale".into(), IrValue::Vec3(scale));
                    }
                }
                AnimationTrack::MethodCallTrack {
                    node_path,
                    keyframes,
                } => {
                    for k in keyframes {
                        // Check if keyframe was crossed in this dt interval
                        let prev_t = (t - dt * self.speed_scale).max(0.0);
                        if (prev_t > t || k.time >= prev_t) && k.time <= t {
                            method_calls.push((node_path.clone(), k.method.clone(), k.args.clone()));
                        }
                    }
                }
            }
        }

        (finished, method_calls)
    }
}

fn find_node_mut_by_path<'a>(scene: &'a mut IrScene, path: &str) -> Option<&'a mut IrNode> {
    scene.nodes.values_mut().find(|n| n.name == path)
}

fn sample_vec3_track(
    keyframes: &[Keyframe<[f32; 3]>],
    t: f32,
    mode: InterpolationMode,
) -> Option<[f32; 3]> {
    if keyframes.is_empty() {
        return None;
    }
    if keyframes.len() == 1 || t <= keyframes[0].time {
        return Some(keyframes[0].value);
    }
    if t >= keyframes.last().unwrap().time {
        return Some(keyframes.last().unwrap().value);
    }

    let idx = keyframes.partition_point(|k| k.time <= t);
    let k0 = &keyframes[idx - 1];
    let k1 = &keyframes[idx];

    let factor = ((t - k0.time) / (k1.time - k0.time)).clamp(0.0, 1.0);

    match mode {
        InterpolationMode::Nearest => {
            if factor < 0.5 {
                Some(k0.value)
            } else {
                Some(k1.value)
            }
        }
        InterpolationMode::Linear => {
            let x = k0.value[0] + (k1.value[0] - k0.value[0]) * factor;
            let y = k0.value[1] + (k1.value[1] - k0.value[1]) * factor;
            let z = k0.value[2] + (k1.value[2] - k0.value[2]) * factor;
            Some([x, y, z])
        }
        InterpolationMode::Cubic => {
            // Smoothstep S-curve hermite interpolation
            let smooth = factor * factor * (3.0 - 2.0 * factor);
            let x = k0.value[0] + (k1.value[0] - k0.value[0]) * smooth;
            let y = k0.value[1] + (k1.value[1] - k0.value[1]) * smooth;
            let z = k0.value[2] + (k1.value[2] - k0.value[2]) * smooth;
            Some([x, y, z])
        }
    }
}

fn sample_quat_track(
    keyframes: &[Keyframe<[f32; 4]>],
    t: f32,
    mode: InterpolationMode,
) -> Option<[f32; 4]> {
    if keyframes.is_empty() {
        return None;
    }
    if keyframes.len() == 1 || t <= keyframes[0].time {
        return Some(keyframes[0].value);
    }
    if t >= keyframes.last().unwrap().time {
        return Some(keyframes.last().unwrap().value);
    }

    let idx = keyframes.partition_point(|k| k.time <= t);
    let k0 = &keyframes[idx - 1];
    let k1 = &keyframes[idx];

    let factor = ((t - k0.time) / (k1.time - k0.time)).clamp(0.0, 1.0);

    match mode {
        InterpolationMode::Nearest => {
            if factor < 0.5 {
                Some(k0.value)
            } else {
                Some(k1.value)
            }
        }
        InterpolationMode::Linear | InterpolationMode::Cubic => {
            // Normalized linear quaternion blend (nlerp)
            let dot = k0.value[0] * k1.value[0]
                + k0.value[1] * k1.value[1]
                + k0.value[2] * k1.value[2]
                + k0.value[3] * k1.value[3];
            let sign = if dot < 0.0 { -1.0 } else { 1.0 };
            let qx = k0.value[0] + (k1.value[0] * sign - k0.value[0]) * factor;
            let qy = k0.value[1] + (k1.value[1] * sign - k0.value[1]) * factor;
            let qz = k0.value[2] + (k1.value[2] * sign - k0.value[2]) * factor;
            let qw = k0.value[3] + (k1.value[3] * sign - k0.value[3]) * factor;
            let len = (qx * qx + qy * qy + qz * qz + qw * qw).sqrt().max(1e-6);
            Some([qx / len, qy / len, qz / len, qw / len])
        }
    }
}

fn sample_value_track(
    keyframes: &[Keyframe<IrValue>],
    t: f32,
    mode: InterpolationMode,
) -> Option<IrValue> {
    if keyframes.is_empty() {
        return None;
    }
    if keyframes.len() == 1 || t <= keyframes[0].time {
        return Some(keyframes[0].value.clone());
    }
    if t >= keyframes.last().unwrap().time {
        return Some(keyframes.last().unwrap().value.clone());
    }

    let idx = keyframes.partition_point(|k| k.time <= t);
    let k0 = &keyframes[idx - 1];
    let k1 = &keyframes[idx];

    let factor = ((t - k0.time) / (k1.time - k0.time)).clamp(0.0, 1.0);

    match (&k0.value, &k1.value) {
        (IrValue::F64(v0), IrValue::F64(v1)) => {
            let factor_f64 = f64::from(factor);
            let interp = match mode {
                InterpolationMode::Nearest => {
                    if factor < 0.5 { *v0 } else { *v1 }
                }
                InterpolationMode::Linear => v0 + (v1 - v0) * factor_f64,
                InterpolationMode::Cubic => {
                    let s = factor_f64 * factor_f64 * (3.0 - 2.0 * factor_f64);
                    v0 + (v1 - v0) * s
                }
            };
            Some(IrValue::F64(interp))
        }
        (IrValue::Vec3(v0), IrValue::Vec3(v1)) => {
            let res = sample_vec3_track(
                &[Keyframe::new(k0.time, *v0), Keyframe::new(k1.time, *v1)],
                t,
                mode,
            );
            res.map(IrValue::Vec3)
        }
        _ => {
            // Discrete types (String, Bool, Nil, Map, Array): step at 50%
            if factor < 0.5 {
                Some(k0.value.clone())
            } else {
                Some(k1.value.clone())
            }
        }
    }
}

// -----------------------------------------------------------------------------
// AnimationTree & State Machine
// -----------------------------------------------------------------------------

/// Transition between two states in an AnimationNodeStateMachine.
#[derive(Clone, PartialEq, Debug)]
pub struct StateTransition {
    pub from: String,
    pub to: String,
    pub crossfade_time: f32,
    pub advance_condition: Option<String>,
}

impl StateTransition {
    pub fn new(from: impl Into<String>, to: impl Into<String>, crossfade_time: f32) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
            crossfade_time,
            advance_condition: None,
        }
    }

    pub fn with_condition(mut self, condition: impl Into<String>) -> Self {
        self.advance_condition = Some(condition.into());
        self
    }
}

/// State machine playback mimicking Godot 4's `AnimationNodeStateMachinePlayback`.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct AnimationNodeStateMachine {
    pub states: BTreeMap<String, String>, // state_name -> animation clip name
    pub transitions: Vec<StateTransition>,
    pub current_state: String,
}

impl AnimationNodeStateMachine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_state(&mut self, state_name: impl Into<String>, clip_name: impl Into<String>) {
        let s = state_name.into();
        if self.states.is_empty() {
            self.current_state = s.clone();
        }
        self.states.insert(s, clip_name.into());
    }

    pub fn add_transition(&mut self, transition: StateTransition) {
        self.transitions.push(transition);
    }

    /// Godot-parity `travel`: finds the shortest transition path using BFS and executes travel to `target_state`.
    /// Returns the sequence of states traversed if a path exists.
    pub fn travel(&mut self, target_state: &str) -> Result<Vec<String>, String> {
        if !self.states.contains_key(target_state) {
            return Err(format!("unknown target state '{target_state}'"));
        }
        if self.current_state == target_state {
            return Ok(vec![self.current_state.clone()]);
        }

        // Build adjacency map
        let mut adj: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for t in &self.transitions {
            adj.entry(&t.from).or_default().push(&t.to);
        }

        // BFS shortest path
        let mut queue: VecDeque<Vec<String>> = VecDeque::new();
        let mut visited: BTreeSet<String> = BTreeSet::new();

        queue.push_back(vec![self.current_state.clone()]);
        visited.insert(self.current_state.clone());

        while let Some(path) = queue.pop_front() {
            let last = path.last().unwrap();
            if last == target_state {
                self.current_state = target_state.to_string();
                return Ok(path);
            }

            if let Some(neighbors) = adj.get(last.as_str()) {
                for &neighbor in neighbors {
                    if visited.insert(neighbor.to_string()) {
                        let mut next_path = path.clone();
                        next_path.push(neighbor.to_string());
                        queue.push_back(next_path);
                    }
                }
            }
        }

        Err(format!(
            "no valid transition path from '{}' to '{}'",
            self.current_state, target_state
        ))
    }
}

// -----------------------------------------------------------------------------
// BlendSpaces (1D & 2D)
// -----------------------------------------------------------------------------

/// 1D linear blending node (e.g. Walk -> Run blend).
#[derive(Clone, PartialEq, Debug, Default)]
pub struct BlendSpace1D {
    pub points: Vec<(f32, String)>, // position -> animation clip name
}

impl BlendSpace1D {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_point(&mut self, pos: f32, clip_name: impl Into<String>) {
        self.points.push((pos, clip_name.into()));
        self.points.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    }

    /// Evaluates blend weights for a given 1D blend scalar. Weights sum to 1.0.
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

        let factor = ((blend_val - p0.0) / (p1.0 - p0.0)).clamp(0.0, 1.0);
        vec![
            (p0.1.clone(), 1.0 - factor),
            (p1.1.clone(), factor),
        ]
    }
}

/// 2D directional blend space node (e.g. 8-way directional movement / skate stance).
#[derive(Clone, PartialEq, Debug, Default)]
pub struct BlendSpace2D {
    pub points: Vec<([f32; 2], String)>, // 2D position -> clip name
}

impl BlendSpace2D {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_point(&mut self, pos: [f32; 2], clip_name: impl Into<String>) {
        self.points.push((pos, clip_name.into()));
    }

    /// Computes normalized blend weights for a 2D blend position using Inverse Distance Weighting (IDW).
    pub fn sample(&self, blend_pos: [f32; 2]) -> Vec<(String, f32)> {
        if self.points.is_empty() {
            return Vec::new();
        }

        // Exact match check
        for (pos, clip) in &self.points {
            let dx = pos[0] - blend_pos[0];
            let dy = pos[1] - blend_pos[1];
            if dx * dx + dy * dy < 1e-6 {
                return vec![(clip.clone(), 1.0)];
            }
        }

        // Calculate inverse distance weights
        let mut raw_weights = Vec::with_capacity(self.points.len());
        let mut total_weight = 0.0;

        for (pos, clip) in &self.points {
            let dist_sq = (pos[0] - blend_pos[0]).powi(2) + (pos[1] - blend_pos[1]).powi(2);
            let w = 1.0 / dist_sq;
            raw_weights.push((clip.clone(), w));
            total_weight += w;
        }

        if total_weight <= 0.0 {
            return vec![(self.points[0].1.clone(), 1.0)];
        }

        raw_weights
            .into_iter()
            .map(|(clip, w)| (clip, w / total_weight))
            .collect()
    }
}
