//! Godot 4 parity 3D Spatial Audio processing (`AudioStreamPlayer3D` & `AudioListener3D`).
//!
//! Mirrors `AudioStreamPlayer3D` distance attenuation models (Inverse, Inverse Square, Logarithmic),
//! equal-power azimuth panning, 3D Doppler pitch shift (`speed_of_sound = 343.0 m/s`), emission cone
//! directivity, and one-pole air absorption lowpass filtering.

use crate::db_to_linear;
use bevy_math::Vec3;

/// Distance attenuation models matching Godot 4 `AudioStreamPlayer3D.AttenuationModel`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum AttenuationModel {
    /// Inverse distance attenuation ($1 / d$).
    #[default]
    InverseDistance,
    /// Inverse square distance attenuation ($1 / d^2$, physically accurate sound pressure roll-off).
    InverseSquareDistance,
    /// Logarithmic roll-off curve.
    Logarithmic,
    /// No distance attenuation (constant 0 dB regardless of distance).
    Disabled,
}

/// 3D audio listener representing ears in the virtual world (Godot 4 `AudioListener3D`).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct AudioListener3D {
    /// World position of the listener.
    pub position: Vec3,
    /// Forward orientation vector (default `(0.0, 0.0, -1.0)`).
    pub forward: Vec3,
    /// Up orientation vector (default `(0.0, 1.0, 0.0)`).
    pub up: Vec3,
    /// Linear velocity vector for Doppler calculation in m/s.
    pub velocity: Vec3,
}

impl Default for AudioListener3D {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            forward: Vec3::NEG_Z,
            up: Vec3::Y,
            velocity: Vec3::ZERO,
        }
    }
}

impl AudioListener3D {
    /// Create a new 3D audio listener with position and orientation.
    #[must_use]
    pub fn new(position: Vec3, forward: Vec3, up: Vec3) -> Self {
        Self {
            position,
            forward: forward.normalize_or_zero(),
            up: up.normalize_or_zero(),
            velocity: Vec3::ZERO,
        }
    }

    /// Compute the right-hand orthogonal vector.
    #[must_use]
    pub fn right(&self) -> Vec3 {
        let f = if self.forward.length_squared() > 1e-6 {
            self.forward.normalize()
        } else {
            Vec3::NEG_Z
        };
        let u = if self.up.length_squared() > 1e-6 {
            self.up.normalize()
        } else {
            Vec3::Y
        };
        f.cross(u).normalize_or_zero()
    }
}

/// 3D positional audio stream emitter (Godot 4 `AudioStreamPlayer3D`).
#[derive(Clone, Debug)]
pub struct AudioStreamPlayer3D {
    /// World position of the emitter.
    pub position: Vec3,
    /// Forward emission orientation vector.
    pub forward: Vec3,
    /// Linear velocity vector for Doppler calculation in m/s.
    pub velocity: Vec3,
    /// Reference distance at which attenuation begins (Godot `unit_size`, default 1.0).
    pub unit_size: f32,
    /// Maximum distance beyond which audio is silent or clamped (default 100.0).
    pub max_distance: f32,
    /// Roll-off factor governing attenuation steepness (default 1.0).
    pub rolloff: f32,
    /// Attenuation curve model.
    pub attenuation_model: AttenuationModel,
    /// Whether Doppler frequency tracking is enabled.
    pub doppler_tracking: bool,
    /// Speed of sound in meters per second (default 343.0 m/s at 20°C).
    pub speed_of_sound: f32,
    /// Inner cone angle in degrees (default 180.0, omni-directional if >= 180.0).
    pub emission_angle_degrees: f32,
    /// Attenuation applied outside the emission cone in decibels (default -12.0 dB).
    pub emission_angle_filter_attenuation_db: f32,
    /// Whether high-frequency air absorption damping is active.
    pub air_absorption_enabled: bool,
    /// Air absorption attenuation strength coefficient (default 0.05).
    pub air_absorption_rolloff: f32,
    // Stateful one-pole lowpass filter memory
    prev_filter_l: f32,
    prev_filter_r: f32,
}

impl Default for AudioStreamPlayer3D {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioStreamPlayer3D {
    /// Initialize a 3D audio emitter with default Godot 4 acoustics.
    #[must_use]
    pub fn new() -> Self {
        Self {
            position: Vec3::ZERO,
            forward: Vec3::NEG_Z,
            velocity: Vec3::ZERO,
            unit_size: 1.0,
            max_distance: 100.0,
            rolloff: 1.0,
            attenuation_model: AttenuationModel::InverseDistance,
            doppler_tracking: false,
            speed_of_sound: 343.0,
            emission_angle_degrees: 180.0,
            emission_angle_filter_attenuation_db: -12.0,
            air_absorption_enabled: true,
            air_absorption_rolloff: 0.05,
            prev_filter_l: 0.0,
            prev_filter_r: 0.0,
        }
    }

    /// Reset internal IIR filter state.
    pub fn reset_filter(&mut self) {
        self.prev_filter_l = 0.0;
        self.prev_filter_r = 0.0;
    }

    /// Compute distance attenuation linear gain factor according to active `AttenuationModel`.
    #[must_use]
    pub fn compute_distance_attenuation(&self, distance: f32) -> f32 {
        if self.attenuation_model == AttenuationModel::Disabled {
            return 1.0;
        }

        let ref_dist = self.unit_size.max(0.001);
        let max_dist = self.max_distance.max(ref_dist + 0.001);

        if distance <= ref_dist {
            return 1.0;
        }
        if distance >= max_dist {
            return 0.0;
        }

        let delta = distance - ref_dist;
        let rolloff = self.rolloff.max(0.0);

        match self.attenuation_model {
            AttenuationModel::InverseDistance => {
                let denom = ref_dist + rolloff * delta;
                if denom > 1e-6 {
                    (ref_dist / denom).clamp(0.0, 1.0)
                } else {
                    1.0
                }
            }
            AttenuationModel::InverseSquareDistance => {
                let denom = ref_dist + rolloff * delta;
                if denom > 1e-6 {
                    let base = ref_dist / denom;
                    (base * base).clamp(0.0, 1.0)
                } else {
                    1.0
                }
            }
            AttenuationModel::Logarithmic => {
                let ratio = (delta / (max_dist - ref_dist)).clamp(0.0, 1.0);
                (1.0 - ratio).powf(rolloff.max(0.1)).clamp(0.0, 1.0)
            }
            AttenuationModel::Disabled => 1.0,
        }
    }

    /// Compute stereo equal-power azimuth panning gains `[gain_left, gain_right]`.
    ///
    /// Preserves total energy ($g_L^2 + g_R^2 = 1.0$).
    #[must_use]
    pub fn compute_equal_power_panning(&self, listener: &AudioListener3D) -> [f32; 2] {
        let diff = self.position - listener.position;
        let dist = diff.length();
        if dist <= 1e-6 {
            // Dead center: equal power 3dB drop on both channels
            let center = std::f32::consts::FRAC_1_SQRT_2;
            return [center, center];
        }

        let dir = diff / dist;
        let right = listener.right();
        let pan = dir.dot(right).clamp(-1.0, 1.0);

        // Map pan in [-1.0, 1.0] to angle in [0, pi/2]
        let angle = (pan + 1.0) * 0.25 * std::f32::consts::PI;
        let gain_l = angle.cos();
        let gain_r = angle.sin();

        [gain_l, gain_r]
    }

    /// Compute Doppler pitch frequency multiplier ($f' / f$).
    #[must_use]
    pub fn compute_doppler_pitch(&self, listener: &AudioListener3D) -> f32 {
        if !self.doppler_tracking {
            return 1.0;
        }

        let diff = self.position - listener.position;
        let dist = diff.length();
        if dist <= 1e-6 {
            return 1.0;
        }

        let u = diff / dist; // Points from listener to emitter
        let c = self.speed_of_sound.max(10.0);

        let v_l = listener.velocity.dot(u);
        let v_s = self.velocity.dot(u);

        let numerator = c + v_l;
        let denominator = (c + v_s).max(1.0);

        (numerator / denominator).clamp(0.1, 5.0)
    }

    /// Compute emission cone directivity gain.
    #[must_use]
    pub fn compute_emission_cone_gain(&self, listener: &AudioListener3D) -> f32 {
        if self.emission_angle_degrees >= 180.0 {
            return 1.0;
        }

        let diff = listener.position - self.position;
        let dist = diff.length();
        if dist <= 1e-6 {
            return 1.0;
        }

        let dir_to_listener = diff / dist;
        let forward = if self.forward.length_squared() > 1e-6 {
            self.forward.normalize()
        } else {
            Vec3::NEG_Z
        };

        let cos_angle = forward.dot(dir_to_listener).clamp(-1.0, 1.0);
        let angle_deg = cos_angle.acos().to_degrees();
        let half_cone = self.emission_angle_degrees * 0.5;

        if angle_deg <= half_cone {
            1.0
        } else {
            db_to_linear(self.emission_angle_filter_attenuation_db)
        }
    }

    /// Process a single stereo frame through 3D spatial acoustics.
    pub fn process_spatial_frame(
        &mut self,
        listener: &AudioListener3D,
        input: [f32; 2],
    ) -> SpatialAudioFrame {
        let diff = self.position - listener.position;
        let distance = diff.length();

        let dist_gain = self.compute_distance_attenuation(distance);
        let pan_gains = self.compute_equal_power_panning(listener);
        let cone_gain = self.compute_emission_cone_gain(listener);
        let doppler_pitch = self.compute_doppler_pitch(listener);

        // Mono downmix of input for point-source emission
        let mono_in = (input[0] + input[1]) * 0.5;
        let total_emitter_gain = dist_gain * cone_gain;

        let mut out_l = mono_in * pan_gains[0] * total_emitter_gain;
        let mut out_r = mono_in * pan_gains[1] * total_emitter_gain;

        let filter_alpha = if self.air_absorption_enabled {
            (1.0 - (-self.air_absorption_rolloff * distance * 0.1).exp()).clamp(0.0, 0.95)
        } else {
            0.0
        };

        if self.air_absorption_enabled && filter_alpha > 0.0 {
            out_l = (1.0 - filter_alpha) * out_l + filter_alpha * self.prev_filter_l;
            out_r = (1.0 - filter_alpha) * out_r + filter_alpha * self.prev_filter_r;
            self.prev_filter_l = out_l;
            self.prev_filter_r = out_r;
        }

        SpatialAudioFrame {
            left: out_l,
            right: out_r,
            distance,
            distance_gain: dist_gain,
            pan_gains,
            cone_gain,
            doppler_pitch,
            filter_alpha,
        }
    }

    /// Process an entire buffer of interleaved stereo frames in-place with 3D acoustics.
    pub fn process_buffer(
        &mut self,
        listener: &AudioListener3D,
        frames: &mut [[f32; 2]],
    ) {
        for frame in frames.iter_mut() {
            let processed = self.process_spatial_frame(listener, *frame);
            *frame = [processed.left, processed.right];
        }
    }
}

/// Diagnostic metadata result from processing a 3D spatial audio frame.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct SpatialAudioFrame {
    pub left: f32,
    pub right: f32,
    pub distance: f32,
    pub distance_gain: f32,
    pub pan_gains: [f32; 2],
    pub cone_gain: f32,
    pub doppler_pitch: f32,
    pub filter_alpha: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_distance_attenuation_curves() {
        let mut player = AudioStreamPlayer3D::new();
        player.unit_size = 1.0;
        player.max_distance = 10.0;
        player.rolloff = 1.0;

        // At reference distance or closer, gain is 1.0
        assert_eq!(player.compute_distance_attenuation(0.5), 1.0);
        assert_eq!(player.compute_distance_attenuation(1.0), 1.0);

        // At or beyond max distance, gain is 0.0
        assert_eq!(player.compute_distance_attenuation(10.0), 0.0);
        assert_eq!(player.compute_distance_attenuation(15.0), 0.0);

        // Inverse distance at d = 2.0: 1.0 / (1.0 + 1.0 * (2.0 - 1.0)) = 0.5
        player.attenuation_model = AttenuationModel::InverseDistance;
        let g_inv = player.compute_distance_attenuation(2.0);
        assert!((g_inv - 0.5).abs() < 1e-4);

        // Inverse square distance at d = 2.0: 0.5^2 = 0.25
        player.attenuation_model = AttenuationModel::InverseSquareDistance;
        let g_sq = player.compute_distance_attenuation(2.0);
        assert!((g_sq - 0.25).abs() < 1e-4);

        // Logarithmic attenuation
        player.attenuation_model = AttenuationModel::Logarithmic;
        let g_log = player.compute_distance_attenuation(5.5);
        assert!(g_log > 0.0 && g_log < 1.0);

        // Disabled
        player.attenuation_model = AttenuationModel::Disabled;
        assert_eq!(player.compute_distance_attenuation(50.0), 1.0);
    }

    #[test]
    fn test_equal_power_azimuth_panning_energy_conservation() {
        let player = AudioStreamPlayer3D::new();
        let listener = AudioListener3D {
            position: Vec3::ZERO,
            forward: Vec3::NEG_Z,
            up: Vec3::Y,
            velocity: Vec3::ZERO,
        };

        // Listener's right is +X
        assert!((listener.right() - Vec3::X).length() < 1e-4);

        // Emitter placed at right (+X)
        let mut p_right = player.clone();
        p_right.position = Vec3::new(5.0, 0.0, 0.0);
        let pan_r = p_right.compute_equal_power_panning(&listener);
        assert!(pan_r[1] > pan_r[0], "Right gain must exceed left gain");
        // Equal power law: g_L^2 + g_R^2 == 1.0
        let power_r = pan_r[0] * pan_r[0] + pan_r[1] * pan_r[1];
        assert!((power_r - 1.0).abs() < 1e-4);

        // Emitter placed at left (-X)
        let mut p_left = player.clone();
        p_left.position = Vec3::new(-5.0, 0.0, 0.0);
        let pan_l = p_left.compute_equal_power_panning(&listener);
        assert!(pan_l[0] > pan_l[1], "Left gain must exceed right gain");
        let power_l = pan_l[0] * pan_l[0] + pan_l[1] * pan_l[1];
        assert!((power_l - 1.0).abs() < 1e-4);

        // Emitter placed dead center ahead (-Z)
        let mut p_center = player;
        p_center.position = Vec3::new(0.0, 0.0, -5.0);
        let pan_c = p_center.compute_equal_power_panning(&listener);
        assert!((pan_c[0] - pan_c[1]).abs() < 1e-4, "Left and right gains must be equal at center");
        let power_c = pan_c[0] * pan_c[0] + pan_c[1] * pan_c[1];
        assert!((power_c - 1.0).abs() < 1e-4);
    }

    #[test]
    fn test_doppler_pitch_shift() {
        let mut player = AudioStreamPlayer3D::new();
        player.doppler_tracking = true;
        player.speed_of_sound = 343.0;
        player.position = Vec3::new(0.0, 0.0, 10.0); // emitter ahead of listener (+Z)

        let mut listener = AudioListener3D {
            position: Vec3::ZERO,
            ..Default::default()
        };

        // Stationary
        assert_eq!(player.compute_doppler_pitch(&listener), 1.0);

        // Listener moves towards emitter (+Z velocity) -> pitch increases
        listener.velocity = Vec3::new(0.0, 0.0, 34.3); // 10% of speed of sound
        let pitch_approach = player.compute_doppler_pitch(&listener);
        assert!(pitch_approach > 1.05, "Approaching listener must increase pitch: {pitch_approach}");

        // Listener moves away from emitter (-Z velocity) -> pitch decreases
        listener.velocity = Vec3::new(0.0, 0.0, -34.3);
        let pitch_recede = player.compute_doppler_pitch(&listener);
        assert!(pitch_recede < 0.95, "Receding listener must decrease pitch: {pitch_recede}");
    }

    #[test]
    fn test_emission_cone_directivity() {
        let mut player = AudioStreamPlayer3D::new();
        player.position = Vec3::ZERO;
        player.forward = Vec3::NEG_Z; // Emitter points towards -Z
        player.emission_angle_degrees = 90.0; // 90 degree cone (45 deg half-angle)
        player.emission_angle_filter_attenuation_db = -6.0;

        let mut listener = AudioListener3D {
            position: Vec3::new(0.0, 0.0, -10.0),
            ..Default::default()
        };

        // Listener inside forward cone (at -Z)
        let cone_inside = player.compute_emission_cone_gain(&listener);
        assert_eq!(cone_inside, 1.0);

        // Listener behind emitter (at +Z, 180 degrees away)
        listener.position = Vec3::new(0.0, 0.0, 10.0);
        let cone_outside = player.compute_emission_cone_gain(&listener);
        assert!((cone_outside - 0.5011).abs() < 0.05, "Outside cone should be attenuated ~ -6dB (~0.5)");
    }

    #[test]
    fn test_spatial_frame_and_buffer_processing() {
        let mut player = AudioStreamPlayer3D::new();
        player.position = Vec3::new(2.0, 0.0, 0.0); // 2m to the right
        player.air_absorption_enabled = true;

        let listener = AudioListener3D::default();
        let frame = player.process_spatial_frame(&listener, [1.0, 1.0]);

        assert!(frame.distance > 0.0);
        assert!(frame.right > frame.left, "Right ear must receive louder signal");
        assert!(frame.filter_alpha >= 0.0);

        // Buffer processing
        let mut buf = vec![[0.8, 0.8]; 64];
        player.process_buffer(&listener, &mut buf);
        assert!(buf[0][1] > buf[0][0]);
    }
}
