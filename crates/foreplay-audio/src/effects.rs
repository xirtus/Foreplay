use std::fmt;
use crate::db_to_linear;

/// Common trait for all stereo audio DSP effect processors.
pub trait AudioEffect: Send + Sync + fmt::Debug {
    /// Process a single stereo sample frame (left, right) in-place.
    fn process_stereo(&mut self, in_l: f32, in_r: f32) -> (f32, f32);
    /// Reset internal delay lines and filter memory.
    fn reset(&mut self);
}

/// Schroeder reverberator combining 4 feedback comb filters and 2 allpass diffusers.
#[derive(Debug, Clone)]
pub struct AudioEffectReverb {
    pub room_size: f32,
    pub damping: f32,
    pub wet_mix: f32,
    pub dry_mix: f32,
    comb_buffers_l: [Vec<f32>; 4],
    comb_indices_l: [usize; 4],
    allpass_buffers_l: [Vec<f32>; 2],
    allpass_indices_l: [usize; 2],
}

impl Default for AudioEffectReverb {
    fn default() -> Self {
        Self::new(0.8, 0.5, 0.3)
    }
}

impl AudioEffectReverb {
    #[must_use]
    pub fn new(room_size: f32, damping: f32, wet: f32) -> Self {
        let comb_lens = [1116, 1188, 1277, 1356];
        let allpass_lens = [225, 556];

        let comb_buffers_l = [
            vec![0.0; comb_lens[0]],
            vec![0.0; comb_lens[1]],
            vec![0.0; comb_lens[2]],
            vec![0.0; comb_lens[3]],
        ];

        let allpass_buffers_l = [
            vec![0.0; allpass_lens[0]],
            vec![0.0; allpass_lens[1]],
        ];

        Self {
            room_size: room_size.clamp(0.0, 0.98),
            damping: damping.clamp(0.0, 1.0),
            wet_mix: wet.clamp(0.0, 1.0),
            dry_mix: 1.0 - wet.clamp(0.0, 1.0),
            comb_buffers_l,
            comb_indices_l: [0; 4],
            allpass_buffers_l,
            allpass_indices_l: [0; 2],
        }
    }
}

impl AudioEffect for AudioEffectReverb {
    fn process_stereo(&mut self, in_l: f32, in_r: f32) -> (f32, f32) {
        let input = (in_l + in_r) * 0.5;

        // Parallel Comb filters
        let mut comb_sum = 0.0;
        for i in 0..4 {
            let buf = &mut self.comb_buffers_l[i];
            let idx = self.comb_indices_l[i];
            let out = buf[idx];

            let fb = out * self.room_size;
            buf[idx] = input + fb * (1.0 - self.damping);

            self.comb_indices_l[i] = (idx + 1) % buf.len();
            comb_sum += out;
        }

        // Series Allpass filters
        let mut ap_out = comb_sum * 0.25;
        let g = 0.5;
        for i in 0..2 {
            let buf = &mut self.allpass_buffers_l[i];
            let idx = self.allpass_indices_l[i];
            let delayed = buf[idx];

            let new_val = ap_out + delayed * g;
            buf[idx] = new_val;
            ap_out = -ap_out * g + delayed;

            self.allpass_indices_l[i] = (idx + 1) % buf.len();
        }

        let out_l = in_l * self.dry_mix + ap_out * self.wet_mix;
        let out_r = in_r * self.dry_mix + ap_out * self.wet_mix;

        (out_l, out_r)
    }

    fn reset(&mut self) {
        for buf in &mut self.comb_buffers_l {
            buf.fill(0.0);
        }
        for buf in &mut self.allpass_buffers_l {
            buf.fill(0.0);
        }
    }
}

/// 3-Band Equalizer (Low, Mid, High gains in dB).
#[derive(Debug, Clone)]
pub struct AudioEffectEq3Band {
    pub low_gain_db: f32,
    pub mid_gain_db: f32,
    pub high_gain_db: f32,
    low_state_l: f32,
    low_state_r: f32,
    high_state_l: f32,
    high_state_r: f32,
}

impl Default for AudioEffectEq3Band {
    fn default() -> Self {
        Self::new(0.0, 0.0, 0.0)
    }
}

impl AudioEffectEq3Band {
    #[must_use]
    pub const fn new(low_db: f32, mid_db: f32, high_db: f32) -> Self {
        Self {
            low_gain_db: low_db,
            mid_gain_db: mid_db,
            high_gain_db: high_db,
            low_state_l: 0.0,
            low_state_r: 0.0,
            high_state_l: 0.0,
            high_state_r: 0.0,
        }
    }
}

impl AudioEffect for AudioEffectEq3Band {
    fn process_stereo(&mut self, in_l: f32, in_r: f32) -> (f32, f32) {
        let alpha_low = 0.04;
        let alpha_high = 0.40;

        // Low band
        self.low_state_l += alpha_low * (in_l - self.low_state_l);
        self.low_state_r += alpha_low * (in_r - self.low_state_r);

        // High band
        self.high_state_l += alpha_high * (in_l - self.high_state_l);
        self.high_state_r += alpha_high * (in_r - self.high_state_r);
        let high_l = in_l - self.high_state_l;
        let high_r = in_r - self.high_state_r;

        // Mid band
        let mid_l = in_l - self.low_state_l - high_l;
        let mid_r = in_r - self.low_state_r - high_r;

        let g_low = db_to_linear(self.low_gain_db);
        let g_mid = db_to_linear(self.mid_gain_db);
        let g_high = db_to_linear(self.high_gain_db);

        let out_l = self.low_state_l * g_low + mid_l * g_mid + high_l * g_high;
        let out_r = self.low_state_r * g_low + mid_r * g_mid + high_r * g_high;

        (out_l, out_r)
    }

    fn reset(&mut self) {
        self.low_state_l = 0.0;
        self.low_state_r = 0.0;
        self.high_state_l = 0.0;
        self.high_state_r = 0.0;
    }
}

/// Peak limiter clamping signal output below `ceiling_db`.
#[derive(Debug, Clone)]
pub struct AudioEffectLimiter {
    pub ceiling_db: f32,
    envelope: f32,
}

impl Default for AudioEffectLimiter {
    fn default() -> Self {
        Self::new(-0.1)
    }
}

impl AudioEffectLimiter {
    #[must_use]
    pub const fn new(ceiling_db: f32) -> Self {
        Self {
            ceiling_db,
            envelope: 0.0,
        }
    }
}

impl AudioEffect for AudioEffectLimiter {
    fn process_stereo(&mut self, in_l: f32, in_r: f32) -> (f32, f32) {
        let max_val = in_l.abs().max(in_r.abs());
        let attack = 0.8;
        let release = 0.999;

        if max_val > self.envelope {
            self.envelope = attack * self.envelope + (1.0 - attack) * max_val;
        } else {
            self.envelope = release * self.envelope + (1.0 - release) * max_val;
        }

        let ceiling_lin = db_to_linear(self.ceiling_db);
        let gain = if self.envelope > ceiling_lin && self.envelope > 1e-6 {
            ceiling_lin / self.envelope
        } else {
            1.0
        };

        (in_l * gain, in_r * gain)
    }

    fn reset(&mut self) {
        self.envelope = 0.0;
    }
}

/// One-pole low-pass filter with cutoff frequency and resonance.
#[derive(Debug, Clone)]
pub struct AudioEffectLowPassFilter {
    pub cutoff_hz: f32,
    state_l: f32,
    state_r: f32,
}

impl Default for AudioEffectLowPassFilter {
    fn default() -> Self {
        Self::new(2000.0)
    }
}

impl AudioEffectLowPassFilter {
    #[must_use]
    pub const fn new(cutoff_hz: f32) -> Self {
        Self {
            cutoff_hz,
            state_l: 0.0,
            state_r: 0.0,
        }
    }
}

impl AudioEffect for AudioEffectLowPassFilter {
    fn process_stereo(&mut self, in_l: f32, in_r: f32) -> (f32, f32) {
        // Sample rate assumed standard 44.1kHz
        let dt = 1.0 / 44100.0;
        let rc = 1.0 / (2.0 * std::f32::consts::PI * self.cutoff_hz.max(20.0));
        let alpha = dt / (rc + dt);

        self.state_l += alpha * (in_l - self.state_l);
        self.state_r += alpha * (in_r - self.state_r);

        (self.state_l, self.state_r)
    }

    fn reset(&mut self) {
        self.state_l = 0.0;
        self.state_r = 0.0;
    }
}

/// Modulated delay-line stereo chorus effect.
#[derive(Debug, Clone)]
pub struct AudioEffectChorus {
    pub rate_hz: f32,
    pub depth_ms: f32,
    pub wet: f32,
    buffer_l: Vec<f32>,
    buffer_r: Vec<f32>,
    idx: usize,
    phase: f32,
}

impl Default for AudioEffectChorus {
    fn default() -> Self {
        Self::new(1.2, 4.0, 0.4)
    }
}

impl AudioEffectChorus {
    #[must_use]
    pub fn new(rate_hz: f32, depth_ms: f32, wet: f32) -> Self {
        let max_samples = 2048;
        Self {
            rate_hz,
            depth_ms,
            wet: wet.clamp(0.0, 1.0),
            buffer_l: vec![0.0; max_samples],
            buffer_r: vec![0.0; max_samples],
            idx: 0,
            phase: 0.0,
        }
    }
}

impl AudioEffect for AudioEffectChorus {
    fn process_stereo(&mut self, in_l: f32, in_r: f32) -> (f32, f32) {
        let max_len = self.buffer_l.len();
        self.buffer_l[self.idx] = in_l;
        self.buffer_r[self.idx] = in_r;

        self.phase += (2.0 * std::f32::consts::PI * self.rate_hz) / 44100.0;
        if self.phase > 2.0 * std::f32::consts::PI {
            self.phase -= 2.0 * std::f32::consts::PI;
        }

        let lfo_l = (self.phase.sin() + 1.0) * 0.5;
        let lfo_r = ((self.phase + std::f32::consts::FRAC_PI_2).sin() + 1.0) * 0.5;

        let delay_samples_l = (self.depth_ms * 44.1 * lfo_l).clamp(1.0, 1000.0);
        let delay_samples_r = (self.depth_ms * 44.1 * lfo_r).clamp(1.0, 1000.0);

        let read_idx_l = (self.idx + max_len - delay_samples_l as usize) % max_len;
        let read_idx_r = (self.idx + max_len - delay_samples_r as usize) % max_len;

        let delayed_l = self.buffer_l[read_idx_l];
        let delayed_r = self.buffer_r[read_idx_r];

        self.idx = (self.idx + 1) % max_len;

        let out_l = in_l * (1.0 - self.wet) + delayed_l * self.wet;
        let out_r = in_r * (1.0 - self.wet) + delayed_r * self.wet;

        (out_l, out_r)
    }

    fn reset(&mut self) {
        self.buffer_l.fill(0.0);
        self.buffer_r.fill(0.0);
        self.idx = 0;
        self.phase = 0.0;
    }
}
