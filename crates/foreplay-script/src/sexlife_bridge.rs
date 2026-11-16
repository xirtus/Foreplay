//! `sexlife` Creative Pro-Apps Suite Bridges.
//!
//! Provides live interop bridges connecting Foreplay with:
//! 1. `Moan`: Adaptive Music DAW & DSP Bus Mixer (stem manifests, feel-intensity curves, quantized crossfades).
//! 2. `Splicy`: Non-Linear Cutscene Director (magnetic timeline EDL, camera keyframe orbits, J/L-cut split audio, subtitle tracks).

use foreplay_ir::{EditorCommand, IrValue, NodeId, SignalConnection};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

// -----------------------------------------------------------------------------
// Moan: Adaptive Music DAW & DSP Stem Bridge
// -----------------------------------------------------------------------------

/// Canonical stem categories for adaptive music mixes.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum MoanStemType {
    Drums,
    Bass,
    Lead,
    FoleyAmbience,
    Custom,
}

/// A single audio stem track within a `Moan` project.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct MoanStemTrack {
    pub id: String,
    pub name: String,
    pub stem_type: MoanStemType,
    pub file_path: String,
    pub base_gain: f32,
    pub pan: f32,
    pub mute: bool,
    pub solo: bool,
}

/// Musical quantization grid for adaptive transitions.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum QuantizeGrid {
    Bar,
    HalfBar,
    Beat,
    Eighth,
    Sixteenth,
}

impl QuantizeGrid {
    pub fn beats_per_division(&self, beats_per_bar: u32) -> f64 {
        match self {
            Self::Bar => beats_per_bar as f64,
            Self::HalfBar => (beats_per_bar as f64) / 2.0,
            Self::Beat => 1.0,
            Self::Eighth => 0.5,
            Self::Sixteenth => 0.25,
        }
    }
}

/// 5-parameter master DSP bus sends controlling live environmental audio feel.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct MoanDspBusSends {
    pub reverb: f32,
    pub delay: f32,
    pub drive: f32,
    pub cutoff_hz: f32,
    pub resonance: f32,
}

impl Default for MoanDspBusSends {
    fn default() -> Self {
        Self {
            reverb: 0.15,
            delay: 0.05,
            drive: 0.0,
            cutoff_hz: 20000.0,
            resonance: 0.707,
        }
    }
}

/// Control point along an intensity response curve [0.0..=1.0].
#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub struct IntensityControlPoint {
    pub intensity: f32,
    pub gain: f32,
    pub cutoff_hz: f32,
}

/// Dynamic intensity mapping curve for an individual stem.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct StemIntensityCurve {
    pub stem_id: String,
    pub points: Vec<IntensityControlPoint>,
    pub combo_gain_boost: f32,
}

/// Live adaptive mix snapshot evaluated at a specific playback position.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct MoanMixSnapshot {
    pub current_beat: f64,
    pub current_bar: u32,
    pub beat_in_bar: f64,
    pub intensity: f32,
    pub combo_active: bool,
    pub stem_gains: BTreeMap<String, f32>,
    pub stem_cutoffs_hz: BTreeMap<String, f32>,
    pub dsp_sends: MoanDspBusSends,
    pub is_quantized_transition_point: bool,
}

/// Adaptive music manifest bridging `Moan` DAW projects with Foreplay.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct MoanProjectStemLink {
    pub name: String,
    pub bpm: f64,
    pub beats_per_bar: u32,
    pub beat_unit: u32,
    pub quantize_grid: QuantizeGrid,
    pub stems: Vec<MoanStemTrack>,
    pub dsp_bus: MoanDspBusSends,
    pub intensity_curves: Vec<StemIntensityCurve>,
    pub crossfade_duration_beats: f64,
}

impl MoanProjectStemLink {
    /// Constructs a standard skate-game adaptive soundtrack with 4 stems (Drums, Bass, Lead, FoleyAmbience)
    /// and dynamic intensity curves reacting to speed, airtime, and combo streaks.
    pub fn default_skate_soundtrack() -> Self {
        let stems = vec![
            MoanStemTrack {
                id: "stem_drums".into(),
                name: "Drums".into(),
                stem_type: MoanStemType::Drums,
                file_path: "res://audio/music/stems/drums.ogg".into(),
                base_gain: 1.0,
                pan: 0.0,
                mute: false,
                solo: false,
            },
            MoanStemTrack {
                id: "stem_bass".into(),
                name: "Bass".into(),
                stem_type: MoanStemType::Bass,
                file_path: "res://audio/music/stems/bass.ogg".into(),
                base_gain: 0.9,
                pan: 0.0,
                mute: false,
                solo: false,
            },
            MoanStemTrack {
                id: "stem_lead".into(),
                name: "Lead".into(),
                stem_type: MoanStemType::Lead,
                file_path: "res://audio/music/stems/lead.ogg".into(),
                base_gain: 0.85,
                pan: 0.0,
                mute: false,
                solo: false,
            },
            MoanStemTrack {
                id: "stem_foley".into(),
                name: "FoleyAmbience".into(),
                stem_type: MoanStemType::FoleyAmbience,
                file_path: "res://audio/music/stems/foley.ogg".into(),
                base_gain: 0.8,
                pan: 0.0,
                mute: false,
                solo: false,
            },
        ];

        let intensity_curves = vec![
            StemIntensityCurve {
                stem_id: "stem_drums".into(),
                points: vec![
                    IntensityControlPoint { intensity: 0.0, gain: 0.0, cutoff_hz: 500.0 },
                    IntensityControlPoint { intensity: 0.3, gain: 0.3, cutoff_hz: 4000.0 },
                    IntensityControlPoint { intensity: 0.6, gain: 0.75, cutoff_hz: 12000.0 },
                    IntensityControlPoint { intensity: 1.0, gain: 1.0, cutoff_hz: 20000.0 },
                ],
                combo_gain_boost: 0.1,
            },
            StemIntensityCurve {
                stem_id: "stem_bass".into(),
                points: vec![
                    IntensityControlPoint { intensity: 0.0, gain: 0.25, cutoff_hz: 800.0 },
                    IntensityControlPoint { intensity: 0.5, gain: 0.7, cutoff_hz: 3000.0 },
                    IntensityControlPoint { intensity: 1.0, gain: 1.0, cutoff_hz: 20000.0 },
                ],
                combo_gain_boost: 0.15,
            },
            StemIntensityCurve {
                stem_id: "stem_lead".into(),
                points: vec![
                    IntensityControlPoint { intensity: 0.0, gain: 0.0, cutoff_hz: 400.0 },
                    IntensityControlPoint { intensity: 0.4, gain: 0.2, cutoff_hz: 4000.0 },
                    IntensityControlPoint { intensity: 0.7, gain: 0.65, cutoff_hz: 10000.0 },
                    IntensityControlPoint { intensity: 1.0, gain: 1.0, cutoff_hz: 20000.0 },
                ],
                combo_gain_boost: 0.25,
            },
            StemIntensityCurve {
                stem_id: "stem_foley".into(),
                points: vec![
                    IntensityControlPoint { intensity: 0.0, gain: 1.0, cutoff_hz: 18000.0 },
                    IntensityControlPoint { intensity: 0.5, gain: 0.6, cutoff_hz: 12000.0 },
                    IntensityControlPoint { intensity: 1.0, gain: 0.25, cutoff_hz: 8000.0 },
                ],
                combo_gain_boost: -0.1,
            },
        ];

        Self {
            name: "SanVanelonaDowntownLoop".into(),
            bpm: 128.0,
            beats_per_bar: 4,
            beat_unit: 4,
            quantize_grid: QuantizeGrid::Bar,
            stems,
            dsp_bus: MoanDspBusSends::default(),
            intensity_curves,
            crossfade_duration_beats: 2.0,
        }
    }

    /// Evaluates the adaptive stem mix and DSP bus state for a given intensity [0.0..=1.0],
    /// combo status, and song playback beat position.
    pub fn evaluate_adaptive_mix(
        &self,
        intensity: f32,
        combo_active: bool,
        playback_beat: f64,
    ) -> MoanMixSnapshot {
        let clamped_intensity = intensity.clamp(0.0, 1.0);
        let bpb = self.beats_per_bar.max(1) as f64;
        let current_bar = (playback_beat / bpb).floor() as u32;
        let beat_in_bar = playback_beat.rem_euclid(bpb);

        let division = self.quantize_grid.beats_per_division(self.beats_per_bar);
        let div_offset = playback_beat.rem_euclid(division);
        let is_quantized_transition_point = div_offset < 0.08 || div_offset > (division - 0.08);

        let mut stem_gains = BTreeMap::new();
        let mut stem_cutoffs_hz = BTreeMap::new();

        for stem in &self.stems {
            if stem.mute {
                stem_gains.insert(stem.id.clone(), 0.0);
                stem_cutoffs_hz.insert(stem.id.clone(), 20.0);
                continue;
            }

            if let Some(curve) = self.intensity_curves.iter().find(|c| c.stem_id == stem.id) {
                let (gain, cutoff) = evaluate_intensity_curve(&curve.points, clamped_intensity);
                let combo_boost = if combo_active { curve.combo_gain_boost } else { 0.0 };
                let final_gain = ((gain + combo_boost) * stem.base_gain).clamp(0.0, 2.0);
                stem_gains.insert(stem.id.clone(), final_gain);
                stem_cutoffs_hz.insert(stem.id.clone(), cutoff.clamp(20.0, 20000.0));
            } else {
                stem_gains.insert(stem.id.clone(), stem.base_gain);
                stem_cutoffs_hz.insert(stem.id.clone(), 20000.0);
            }
        }

        // Modulate master DSP sends by gameplay intensity and combo
        let mut dsp_sends = self.dsp_bus.clone();
        dsp_sends.drive = (dsp_sends.drive + clamped_intensity * 0.4 + if combo_active { 0.25 } else { 0.0 }).clamp(0.0, 1.0);
        dsp_sends.reverb = (dsp_sends.reverb + (1.0 - clamped_intensity) * 0.15).clamp(0.0, 1.0);

        MoanMixSnapshot {
            current_beat: playback_beat,
            current_bar,
            beat_in_bar,
            intensity: clamped_intensity,
            combo_active,
            stem_gains,
            stem_cutoffs_hz,
            dsp_sends,
            is_quantized_transition_point,
        }
    }

    /// Lossless serialisation into `.moan.json` manifest string.
    pub fn to_moan_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Lossless deserialisation from `.moan.json` manifest string.
    pub fn from_moan_json(json_str: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json_str)
    }
}

fn evaluate_intensity_curve(points: &[IntensityControlPoint], intensity: f32) -> (f32, f32) {
    if points.is_empty() {
        return (1.0, 20000.0);
    }
    if points.len() == 1 || intensity <= points[0].intensity {
        return (points[0].gain, points[0].cutoff_hz);
    }
    if intensity >= points[points.len() - 1].intensity {
        let last = points[points.len() - 1];
        return (last.gain, last.cutoff_hz);
    }

    for i in 0..(points.len() - 1) {
        let p0 = points[i];
        let p1 = points[i + 1];
        if intensity >= p0.intensity && intensity <= p1.intensity {
            let span = (p1.intensity - p0.intensity).max(1e-5);
            let alpha = (intensity - p0.intensity) / span;
            let gain = p0.gain * (1.0 - alpha) + p1.gain * alpha;
            let cutoff = p0.cutoff_hz * (1.0 - alpha) + p1.cutoff_hz * alpha;
            return (gain, cutoff);
        }
    }

    (points[0].gain, points[0].cutoff_hz)
}

// -----------------------------------------------------------------------------
// Splicy: Non-Linear Cutscene Timeline Director Bridge
// -----------------------------------------------------------------------------

/// 3D Camera keyframe for orbital, dolly, or panning movements.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct CameraKeyframe {
    pub time_offset_sec: f64,
    pub position: [f32; 3],
    pub rotation_quat: [f32; 4],
    pub fov_degrees: f32,
}

/// Camera shot clip within the magnetic cutscene timeline.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct CameraShotClip {
    pub id: String,
    pub camera_node: NodeId,
    pub start_time_sec: f64,
    pub end_time_sec: f64,
    pub focal_length_mm: f32,
    pub aperture_f_stop: f32,
    pub dolly_orbit_keyframes: Vec<CameraKeyframe>,
}

/// Audio clip supporting FCP-style J-cut / L-cut split edit offsets.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct AudioTrackClip {
    pub id: String,
    pub asset_path: String,
    pub clip_start_sec: f64,
    pub clip_end_sec: f64,
    /// J-cut lead-in: audio begins N seconds before the visual shot starts.
    pub audio_lead_in_sec: f64,
    /// L-cut tail-out: audio continues playing N seconds after the visual shot ends.
    pub audio_tail_out_sec: f64,
    pub base_volume: f32,
}

/// Signal trigger marker scheduled on the timeline.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct SignalTriggerMarker {
    pub id: String,
    pub timestamp_sec: f64,
    pub target_node: NodeId,
    pub signal_name: String,
    pub payload: Option<IrValue>,
}

/// Subtitle text entry with timecodes.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct SubtitleEntry {
    pub start_sec: f64,
    pub end_sec: f64,
    pub speaker: String,
    pub text: String,
}

/// Subtitle track for a specific localization language code.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct SubtitleTrack {
    pub language_code: String,
    pub entries: Vec<SubtitleEntry>,
}

/// Evaluated active camera shot state.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct ActiveCameraState {
    pub shot_id: String,
    pub camera_node: NodeId,
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub focal_length_mm: f32,
    pub aperture_f_stop: f32,
    pub fov_degrees: f32,
}

/// Evaluated active audio clip with J/L-cut split state and crossfade gain.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct ActiveAudioState {
    pub clip_id: String,
    pub asset_path: String,
    pub playback_offset_sec: f64,
    pub effective_volume: f32,
    pub in_j_cut_lead: bool,
    pub in_l_cut_tail: bool,
}

/// Evaluated active subtitle state.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct ActiveSubtitleState {
    pub speaker: String,
    pub text: String,
    pub progress: f32,
}

/// Comprehensive timeline evaluation snapshot at scrub timestamp `t`.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct SplicyEvaluationSnapshot {
    pub timestamp_sec: f64,
    pub active_camera: Option<ActiveCameraState>,
    pub active_audio: Vec<ActiveAudioState>,
    pub active_subtitles: Vec<ActiveSubtitleState>,
    pub triggered_markers: Vec<SignalTriggerMarker>,
}

/// Errors occurring during Splicy EDL codec or conversion operations.
#[derive(Clone, PartialEq, Eq, Debug, Error, Serialize, Deserialize)]
pub enum SplicyEdlError {
    #[error("JSON codec error: {0}")]
    JsonCodec(String),
    #[error("invalid timeline format: {0}")]
    InvalidFormat(String),
    #[error("shot not found: {0}")]
    ShotNotFound(String),
}

/// Non-linear cutscene Edit Decision List (EDL) bridging `Splicy` with Foreplay.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct SplicyCutsceneEdl {
    pub name: String,
    pub duration_sec: f64,
    pub frame_rate: f64,
    pub camera_shots: Vec<CameraShotClip>,
    pub audio_clips: Vec<AudioTrackClip>,
    pub signal_markers: Vec<SignalTriggerMarker>,
    pub subtitle_tracks: Vec<SubtitleTrack>,
}

impl SplicyCutsceneEdl {
    pub fn new(name: impl Into<String>, duration_sec: f64, frame_rate: f64) -> Self {
        Self {
            name: name.into(),
            duration_sec,
            frame_rate,
            camera_shots: Vec::new(),
            audio_clips: Vec::new(),
            signal_markers: Vec::new(),
            subtitle_tracks: Vec::new(),
        }
    }

    /// Evaluates the complete camera, audio, subtitle, and marker state at scrub timestamp `t`.
    pub fn evaluate_at(&self, timestamp_sec: f64) -> SplicyEvaluationSnapshot {
        let t = timestamp_sec.clamp(0.0, self.duration_sec);

        // 1. Camera shot evaluation with keyframe interpolation
        let active_camera = self
            .camera_shots
            .iter()
            .find(|shot| t >= shot.start_time_sec && t <= shot.end_time_sec)
            .map(|shot| {
                let (position, rotation, fov) = interpolate_camera_keyframes(
                    &shot.dolly_orbit_keyframes,
                    t - shot.start_time_sec,
                );
                ActiveCameraState {
                    shot_id: shot.id.clone(),
                    camera_node: shot.camera_node,
                    position,
                    rotation,
                    focal_length_mm: shot.focal_length_mm,
                    aperture_f_stop: shot.aperture_f_stop,
                    fov_degrees: fov,
                }
            });

        // 2. Audio clips evaluation with J-cut and L-cut split boundaries
        let mut active_audio = Vec::new();
        for clip in &self.audio_clips {
            let effective_start = clip.clip_start_sec - clip.audio_lead_in_sec;
            let effective_end = clip.clip_end_sec + clip.audio_tail_out_sec;

            if t >= effective_start && t <= effective_end {
                let in_j_cut_lead = t < clip.clip_start_sec;
                let in_l_cut_tail = t > clip.clip_end_sec;
                let playback_offset_sec = t - effective_start;

                let effective_volume = if in_j_cut_lead {
                    let lead = clip.audio_lead_in_sec.max(0.001);
                    let phase = ((t - effective_start) / lead).clamp(0.0, 1.0) as f32;
                    clip.base_volume * phase
                } else if in_l_cut_tail {
                    let tail = clip.audio_tail_out_sec.max(0.001);
                    let phase = ((effective_end - t) / tail).clamp(0.0, 1.0) as f32;
                    clip.base_volume * phase
                } else {
                    clip.base_volume
                };

                active_audio.push(ActiveAudioState {
                    clip_id: clip.id.clone(),
                    asset_path: clip.asset_path.clone(),
                    playback_offset_sec,
                    effective_volume,
                    in_j_cut_lead,
                    in_l_cut_tail,
                });
            }
        }

        // 3. Subtitles evaluation
        let mut active_subtitles = Vec::new();
        for track in &self.subtitle_tracks {
            for entry in &track.entries {
                if t >= entry.start_sec && t <= entry.end_sec {
                    let span = (entry.end_sec - entry.start_sec).max(0.001);
                    let progress = ((t - entry.start_sec) / span).clamp(0.0, 1.0) as f32;
                    active_subtitles.push(ActiveSubtitleState {
                        speaker: entry.speaker.clone(),
                        text: entry.text.clone(),
                        progress,
                    });
                }
            }
        }

        // 4. Triggered signal markers
        let triggered_markers: Vec<SignalTriggerMarker> = self
            .signal_markers
            .iter()
            .filter(|m| (t - m.timestamp_sec).abs() <= (1.0 / self.frame_rate.max(1.0)))
            .cloned()
            .collect();

        SplicyEvaluationSnapshot {
            timestamp_sec: t,
            active_camera,
            active_audio,
            active_subtitles,
            triggered_markers,
        }
    }

    /// Compiles cutscene EDL timeline into a batch of `foreplay_ir::EditorCommand` node updates.
    pub fn to_editor_commands(&self, cutscene_root_id: NodeId) -> Vec<EditorCommand> {
        let mut commands = Vec::new();

        // 1. Root timeline properties
        commands.push(EditorCommand::SetProperty {
            node_id: cutscene_root_id,
            key: "cutscene_name".into(),
            old_value: None,
            new_value: Some(IrValue::String(self.name.clone())),
        });
        commands.push(EditorCommand::SetProperty {
            node_id: cutscene_root_id,
            key: "duration_sec".into(),
            old_value: None,
            new_value: Some(IrValue::F64(self.duration_sec)),
        });
        commands.push(EditorCommand::SetProperty {
            node_id: cutscene_root_id,
            key: "frame_rate".into(),
            old_value: None,
            new_value: Some(IrValue::F64(self.frame_rate)),
        });

        // 2. Camera shot properties
        for shot in &self.camera_shots {
            commands.push(EditorCommand::SetProperty {
                node_id: shot.camera_node,
                key: "focal_length_mm".into(),
                old_value: None,
                new_value: Some(IrValue::F64(shot.focal_length_mm as f64)),
            });
            commands.push(EditorCommand::SetProperty {
                node_id: shot.camera_node,
                key: "aperture_f_stop".into(),
                old_value: None,
                new_value: Some(IrValue::F64(shot.aperture_f_stop as f64)),
            });
        }

        // 3. Signal markers connections
        for marker in &self.signal_markers {
            commands.push(EditorCommand::ConnectSignal {
                connection: SignalConnection {
                    from_node: cutscene_root_id,
                    signal_name: marker.signal_name.clone(),
                    to_node: marker.target_node,
                    method_name: format!("_on_{}", marker.signal_name),
                },
            });
        }

        commands
    }

    /// Reconstructs cutscene EDL metadata from a sequence of `EditorCommand`s.
    pub fn from_editor_commands(
        cutscene_root_id: NodeId,
        commands: &[EditorCommand],
    ) -> Result<Self, SplicyEdlError> {
        let mut edl = Self::default();

        for cmd in commands {
            match cmd {
                EditorCommand::SetProperty {
                    node_id,
                    key,
                    new_value: Some(val),
                    ..
                } if *node_id == cutscene_root_id => {
                    match (key.as_str(), val) {
                        ("cutscene_name", IrValue::String(s)) => edl.name = s.clone(),
                        ("duration_sec", IrValue::F64(f)) => edl.duration_sec = *f,
                        ("frame_rate", IrValue::F64(f)) => edl.frame_rate = *f,
                        _ => {}
                    }
                }
                EditorCommand::ConnectSignal { connection }
                    if connection.from_node == cutscene_root_id =>
                {
                    edl.signal_markers.push(SignalTriggerMarker {
                        id: format!("marker_{}", edl.signal_markers.len()),
                        timestamp_sec: 0.0,
                        target_node: connection.to_node,
                        signal_name: connection.signal_name.clone(),
                        payload: None,
                    });
                }
                _ => {}
            }
        }

        Ok(edl)
    }

    /// Lossless serialisation into `.splicy.json` string.
    pub fn to_splicy_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Lossless deserialisation from `.splicy.json` string.
    pub fn from_splicy_json(json_str: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json_str)
    }
}

fn interpolate_camera_keyframes(
    keyframes: &[CameraKeyframe],
    local_time: f64,
) -> ([f32; 3], [f32; 4], f32) {
    if keyframes.is_empty() {
        return ([0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0], 60.0);
    }
    if keyframes.len() == 1 || local_time <= keyframes[0].time_offset_sec {
        let k = &keyframes[0];
        return (k.position, k.rotation_quat, k.fov_degrees);
    }
    if local_time >= keyframes[keyframes.len() - 1].time_offset_sec {
        let k = &keyframes[keyframes.len() - 1];
        return (k.position, k.rotation_quat, k.fov_degrees);
    }

    for i in 0..(keyframes.len() - 1) {
        let k0 = &keyframes[i];
        let k1 = &keyframes[i + 1];
        if local_time >= k0.time_offset_sec && local_time <= k1.time_offset_sec {
            let span = (k1.time_offset_sec - k0.time_offset_sec).max(1e-5);
            let alpha = ((local_time - k0.time_offset_sec) / span).clamp(0.0, 1.0) as f32;

            // Linear position interpolation
            let pos = [
                k0.position[0] * (1.0 - alpha) + k1.position[0] * alpha,
                k0.position[1] * (1.0 - alpha) + k1.position[1] * alpha,
                k0.position[2] * (1.0 - alpha) + k1.position[2] * alpha,
            ];

            // Quaternion normalized linear interpolation (nlerp)
            let mut q1 = k1.rotation_quat;
            let dot = k0.rotation_quat[0] * q1[0]
                + k0.rotation_quat[1] * q1[1]
                + k0.rotation_quat[2] * q1[2]
                + k0.rotation_quat[3] * q1[3];
            if dot < 0.0 {
                q1 = [-q1[0], -q1[1], -q1[2], -q1[3]];
            }

            let mut rot = [
                k0.rotation_quat[0] * (1.0 - alpha) + q1[0] * alpha,
                k0.rotation_quat[1] * (1.0 - alpha) + q1[1] * alpha,
                k0.rotation_quat[2] * (1.0 - alpha) + q1[2] * alpha,
                k0.rotation_quat[3] * (1.0 - alpha) + q1[3] * alpha,
            ];
            let norm = (rot[0] * rot[0] + rot[1] * rot[1] + rot[2] * rot[2] + rot[3] * rot[3]).sqrt();
            if norm > 1e-6 {
                rot[0] /= norm;
                rot[1] /= norm;
                rot[2] /= norm;
                rot[3] /= norm;
            }

            // FOV interpolation
            let fov = k0.fov_degrees * (1.0 - alpha) + k1.fov_degrees * alpha;

            return (pos, rot, fov);
        }
    }

    (keyframes[0].position, keyframes[0].rotation_quat, keyframes[0].fov_degrees)
}
