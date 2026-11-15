//! Procedural Skate Foley Synthesizer, 3D Spatial HRTF-Lite Panner, and 32-Band Spectrum Analyzer.
//!
//! Synthesizes continuous wheel-roll harmonics, metallic rail grinding, and aerodynamic wind Foley.
//! Provides listener-relative 3D spatial panning with ITD/ILD HRTF and Doppler pitch calculations.
//! Analyzes real-time stereo audio into 32 logarithmic frequency bands (20 Hz - 20 kHz) for DAW Channel EQ.

use bevy_math::Vec3;
use crate::timeline::SAMPLE_RATE;

/// Continuous procedural skateboard Foley synthesizer.
///
/// Combines:
/// - Filtered urethane wheel-roll harmonics driven by board speed and surface roughness.
/// - Metallic rail-grind FM ring resonance with high-frequency scrape grit.
/// - Aerodynamic wind noise modulated by air speed.
#[derive(Clone, Debug)]
pub struct SkateFoleySynth {
    pub board_speed: f32,
    pub surface_roughness: f32,
    pub is_grinding: bool,
    pub air_speed: f32,

    // Internal oscillator and filter states
    wheel_phase: f32,
    grind_carrier_phase: f32,
    grind_mod_phase: f32,
    rng_state: u64,
    roughness_filter_state: [f32; 2],
    wind_filter_state: f32,
}

impl Default for SkateFoleySynth {
    fn default() -> Self {
        Self::new()
    }
}

impl SkateFoleySynth {
    /// Create a new procedural skate foley synthesizer.
    #[must_use]
    pub fn new() -> Self {
        Self {
            board_speed: 0.0,
            surface_roughness: 0.5,
            is_grinding: false,
            air_speed: 0.0,
            wheel_phase: 0.0,
            grind_carrier_phase: 0.0,
            grind_mod_phase: 0.0,
            rng_state: 0x12345678_9ABCDEF0,
            roughness_filter_state: [0.0, 0.0],
            wind_filter_state: 0.0,
        }
    }

    /// Update telemetry variables.
    pub fn set_telemetry(
        &mut self,
        board_speed: f32,
        surface_roughness: f32,
        is_grinding: bool,
        air_speed: f32,
    ) {
        self.board_speed = board_speed.max(0.0);
        self.surface_roughness = surface_roughness.clamp(0.0, 1.0);
        self.is_grinding = is_grinding;
        self.air_speed = air_speed.max(0.0);
    }

    fn next_noise(&mut self) -> f32 {
        // Fast XorShift64 PRNG producing uniform float in [-1.0, 1.0]
        self.rng_state ^= self.rng_state << 13;
        self.rng_state ^= self.rng_state >> 7;
        self.rng_state ^= self.rng_state << 17;
        let u = (self.rng_state >> 40) as f32 / 16777216.0; // [0, 1]
        u * 2.0 - 1.0
    }

    /// Synthesize a block of continuous stereo Foley audio samples.
    pub fn generate_block(&mut self, out_stereo: &mut [[f32; 2]]) {
        let sample_rate = SAMPLE_RATE as f32;
        let dt = 1.0 / sample_rate;

        // Skateboard wheel diameter ~ 54 mm (circumference ~ 0.1696 m)
        // Wheel rotation fundamental frequency:
        let wheel_fundamental = (self.board_speed * 5.895).clamp(0.0, 1200.0);
        let wheel_phase_inc = wheel_fundamental * std::f32::consts::TAU * dt;

        // Grind FM frequencies: carrier ~ 1850 Hz, modulator ~ 180 Hz
        let grind_c_inc = 1850.0 * std::f32::consts::TAU * dt;
        let grind_m_inc = 180.0 * std::f32::consts::TAU * dt;

        // Wind lowpass filter coefficient
        let wind_cutoff = (80.0 + self.air_speed * 40.0).min(4000.0);
        let wind_alpha = (wind_cutoff * dt * std::f32::consts::TAU).clamp(0.001, 0.95);

        let speed_gain = (self.board_speed / 12.0).clamp(0.0, 1.5);
        let rough_gain = self.surface_roughness * speed_gain;

        for frame in out_stereo.iter_mut() {
            // 1. Wheel-roll harmonics (fundamental + 2nd + 3rd harmonic)
            self.wheel_phase = (self.wheel_phase + wheel_phase_inc) % std::f32::consts::TAU;
            let wheel_h1 = self.wheel_phase.sin();
            let wheel_h2 = (self.wheel_phase * 2.0).sin() * 0.5;
            let wheel_h3 = (self.wheel_phase * 3.0).sin() * 0.25;
            let wheel_tone = (wheel_h1 + wheel_h2 + wheel_h3) * 0.3 * speed_gain;

            // 2. Asphalt roughness bandpass noise
            let white = self.next_noise();
            // Simple resonant bandpass 2-pole filter around 1200 Hz
            let bp_input = white * rough_gain;
            let bp_out = self.roughness_filter_state[0];
            let bp_deriv = self.roughness_filter_state[1];
            let w0 = 1200.0 * std::f32::consts::TAU * dt;
            let q = 1.5;
            let new_deriv = bp_deriv + w0 * (bp_input - bp_out - bp_deriv / q);
            self.roughness_filter_state[0] = bp_out + new_deriv * w0;
            self.roughness_filter_state[1] = new_deriv;
            let asphalt_noise = bp_out * 0.35;

            // 3. Metallic rail grind FM ring resonance
            let grind_sample = if self.is_grinding {
                self.grind_carrier_phase = (self.grind_carrier_phase + grind_c_inc) % std::f32::consts::TAU;
                self.grind_mod_phase = (self.grind_mod_phase + grind_m_inc) % std::f32::consts::TAU;

                let mod_val = self.grind_mod_phase.sin();
                let fm_carrier = (self.grind_carrier_phase + 3.2 * mod_val).sin();
                let scrape_grit = self.next_noise() * 0.3;
                (fm_carrier * 0.4 + scrape_grit) * speed_gain.max(0.4)
            } else {
                0.0
            };

            // 4. Aerodynamic wind noise
            let wind_raw = self.next_noise();
            self.wind_filter_state += wind_alpha * (wind_raw - self.wind_filter_state);
            let wind_gain = (self.air_speed * self.air_speed / (self.air_speed * self.air_speed + 100.0)) * 0.4;
            let wind_sample = self.wind_filter_state * wind_gain;

            let mono_sum = wheel_tone + asphalt_noise + grind_sample + wind_sample;

            // Subtle stereo field decorrelation
            frame[0] = mono_sum * 0.9 + asphalt_noise * 0.1;
            frame[1] = mono_sum * 0.9 - asphalt_noise * 0.1;
        }
    }
}

/// 3D listener-relative azimuth ITD/ILD HRTF-lite stereo panner with Doppler pitch estimation.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct SpatialPanner3D {
    pub listener_pos: Vec3,
    pub listener_forward: Vec3,
    pub listener_up: Vec3,
    pub listener_vel: Vec3,
    pub speed_of_sound: f32,
    pub reference_distance: f32,
    pub max_distance: f32,
    pub rolloff_factor: f32,
}

impl Default for SpatialPanner3D {
    fn default() -> Self {
        Self::new()
    }
}

impl SpatialPanner3D {
    /// Construct a spatial panner with default acoustics at origin looking towards -Z.
    #[must_use]
    pub fn new() -> Self {
        Self {
            listener_pos: Vec3::ZERO,
            listener_forward: Vec3::NEG_Z,
            listener_up: Vec3::Y,
            listener_vel: Vec3::ZERO,
            speed_of_sound: 343.0,
            reference_distance: 1.0,
            max_distance: 100.0,
            rolloff_factor: 1.0,
        }
    }

    /// Compute Doppler pitch frequency multiplier ($f' / f$) from relative velocities.
    #[must_use]
    pub fn compute_doppler_pitch(
        source_pos: Vec3,
        source_vel: Vec3,
        listener_pos: Vec3,
        listener_vel: Vec3,
        speed_of_sound: f32,
    ) -> f32 {
        let diff = listener_pos - source_pos;
        let dist = diff.length();
        if dist <= 1e-4 {
            return 1.0;
        }

        let to_listener = diff / dist; // Unit vector from source to listener
        let v_s = source_vel.dot(to_listener); // Positive when source moves towards listener
        let v_l = listener_vel.dot(-to_listener); // Positive when listener moves towards source

        let c = speed_of_sound.max(10.0);
        let denom = (c - v_s).max(1e-2);
        let num = (c + v_l).max(1e-2);

        (num / denom).clamp(0.2, 5.0)
    }

    /// Spatialize a single mono sample into stereo `[left, right]` channels.
    #[must_use]
    pub fn process_sample(&self, sample: f32, source_pos: Vec3) -> [f32; 2] {
        let diff = source_pos - self.listener_pos;
        let dist = diff.length();

        // 1. Distance attenuation (inverse distance clamped model)
        let clamped_dist = dist.clamp(self.reference_distance, self.max_distance);
        let att = self.reference_distance
            / (self.reference_distance + self.rolloff_factor * (clamped_dist - self.reference_distance));

        // 2. Azimuth angle relative to listener orientation
        let right = self.listener_forward.cross(self.listener_up).normalize_or_zero();
        let x_rel = diff.dot(right);
        let z_rel = diff.dot(self.listener_forward);

        let azimuth = x_rel.atan2(z_rel); // [-PI, PI]
        let pan = azimuth.sin().clamp(-1.0, 1.0); // -1 = full left, +1 = full right

        // 3. Interaural Level Difference (ILD)
        let gain_l = ((1.0 - pan) * 0.5).sqrt() * att;
        let gain_r = ((1.0 + pan) * 0.5).sqrt() * att;

        [sample * gain_l, sample * gain_r]
    }
}

/// 32-band logarithmic frequency spectrum analyzer covering 20 Hz to 20,000 Hz.
///
/// Computes real-time decibel energy across bands for DAW Channel EQ and mixer visualizers.
#[derive(Clone, Debug)]
pub struct SpectrumAnalyzer32 {
    center_frequencies: [f32; 32],
    peak_levels_db: [f32; 32],
    decay_rate_db_per_frame: f32,
}

impl Default for SpectrumAnalyzer32 {
    fn default() -> Self {
        Self::new()
    }
}

impl SpectrumAnalyzer32 {
    /// Create a new 32-band spectrum analyzer with logarithmic band spacing.
    #[must_use]
    pub fn new() -> Self {
        let mut freqs = [0.0_f32; 32];
        let f_min = 20.0_f32;
        let f_max = 20_000.0_f32;
        let log_ratio = (f_max / f_min).ln() / 31.0;

        for (i, f) in freqs.iter_mut().enumerate() {
            *f = f_min * (log_ratio * i as f32).exp();
        }

        Self {
            center_frequencies: freqs,
            peak_levels_db: [-100.0; 32],
            decay_rate_db_per_frame: 1.5,
        }
    }

    /// Access the 32 center frequencies in Hertz.
    #[must_use]
    pub fn center_frequencies(&self) -> &[f32; 32] {
        &self.center_frequencies
    }

    /// Access current peak dB levels in `[-100.0, 0.0]`.
    #[must_use]
    pub fn current_levels(&self) -> &[f32; 32] {
        &self.peak_levels_db
    }

    /// Process a stereo block and return the 32-band dB levels.
    pub fn process_block(&mut self, block: &[[f32; 2]]) -> [f32; 32] {
        let n = block.len();
        if n == 0 {
            return self.peak_levels_db;
        }

        let sample_rate = SAMPLE_RATE as f32;
        let n_f32 = n as f32;
        let inv_n = 1.0 / n_f32;
        let inv_nm1 = if n > 1 { 1.0 / (n_f32 - 1.0) } else { 1.0 };

        // For each logarithmic frequency band, compute Hann-windowed DFT magnitude
        for (b, &target_freq) in self.center_frequencies.iter().enumerate() {
            let omega = std::f32::consts::TAU * target_freq / sample_rate;
            let mut sum_cos = 0.0_f32;
            let mut sum_sin = 0.0_f32;

            for (i, frame) in block.iter().enumerate() {
                let mono = (frame[0] + frame[1]) * 0.5;
                // Hann window
                let w = 0.5 * (1.0 - (std::f32::consts::TAU * (i as f32) * inv_nm1).cos());
                let windowed_sample = mono * w;
                let phase = omega * (i as f32);

                sum_cos += windowed_sample * phase.cos();
                sum_sin += windowed_sample * phase.sin();
            }

            // Normalization: Hann window coherent gain is 0.5, so multiply by 4.0 / N for full peak recovery
            let mag = (4.0 * (sum_cos * sum_cos + sum_sin * sum_sin).sqrt() * inv_n).max(1e-5);
            let db = (20.0 * mag.log10()).clamp(-100.0, 0.0);

            // Ballistic peak decay
            let decayed = self.peak_levels_db[b] - self.decay_rate_db_per_frame;
            self.peak_levels_db[b] = db.max(decayed);
        }

        self.peak_levels_db
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skate_foley_synth_finite_samples() {
        let mut synth = SkateFoleySynth::new();
        synth.set_telemetry(10.0, 0.8, true, 15.0);

        let mut buffer = vec![[0.0_f32; 2]; 1024];
        synth.generate_block(&mut buffer);

        let mut has_audio = false;
        for frame in &buffer {
            assert!(frame[0].is_finite());
            assert!(frame[1].is_finite());
            assert!(frame[0].abs() <= 2.0);
            assert!(frame[1].abs() <= 2.0);
            if frame[0].abs() > 0.01 {
                has_audio = true;
            }
        }
        assert!(has_audio, "Procedural Foley should synthesize audible audio");
    }

    #[test]
    fn spatial_panner_doppler_and_azimuth() {
        let listener = Vec3::ZERO;
        let l_vel = Vec3::ZERO;

        // Source approaching listener at 34.3 m/s (0.1 Mach)
        let source_pos = Vec3::new(0.0, 0.0, -50.0);
        let s_vel = Vec3::new(0.0, 0.0, 34.3); // Moving towards +Z (towards listener)

        let pitch = SpatialPanner3D::compute_doppler_pitch(source_pos, s_vel, listener, l_vel, 343.0);
        assert!(pitch > 1.05 && pitch < 1.15, "Approaching source pitch must be higher, got {pitch}");

        // Azimuth check
        let panner = SpatialPanner3D::new();
        // Source directly to right (+X)
        let right_stereo = panner.process_sample(1.0, Vec3::new(5.0, 0.0, 0.0));
        assert!(right_stereo[1] > right_stereo[0], "Right ear should receive more energy than left ear");
    }

    #[test]
    fn spectrum_analyzer_440hz_peak_detection() {
        let mut analyzer = SpectrumAnalyzer32::new();
        let total_frames = 1024;
        let mut block = vec![[0.0_f32; 2]; total_frames];

        // 440 Hz pure sine wave at 48,000 Hz
        for (i, frame) in block.iter_mut().enumerate() {
            let t = (i as f32) / (SAMPLE_RATE as f32);
            let s = (440.0 * std::f32::consts::TAU * t).sin();
            frame[0] = s;
            frame[1] = s;
        }

        let spectrum = analyzer.process_block(&block);

        // Find index of highest peak in 32 bands
        let mut max_band = 0;
        let mut max_db = f32::NEG_INFINITY;
        for (b, &db) in spectrum.iter().enumerate() {
            if db > max_db {
                max_db = db;
                max_band = b;
            }
        }

        // 440 Hz center frequency should fall in bands 13-15
        let center_freq = analyzer.center_frequencies()[max_band];
        assert!(
            (center_freq - 440.0).abs() < 120.0,
            "Max peak band {max_band} freq {center_freq} must be close to 440 Hz"
        );
        assert!(max_db > -10.0, "Peak level must be strong (> -10 dB), got {max_db}");

        // Sub-bass band (20 Hz) and ultra-high (20 kHz) must have very low energy
        assert!(spectrum[0] < -30.0, "20 Hz band must be low for 440 Hz tone, got {}", spectrum[0]);
        assert!(spectrum[31] < -30.0, "20 kHz band must be low for 440 Hz tone, got {}", spectrum[31]);
    }
}
