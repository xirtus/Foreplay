pub mod effects;
pub use effects::{
    AudioEffect, AudioEffectChorus, AudioEffectEq3Band, AudioEffectLimiter, AudioEffectLowPassFilter,
    AudioEffectReverb,
};

use std::collections::BTreeMap;
use std::fmt;

/// Real-time stereo VU peak and RMS metering results for an audio bus.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct BusVuMeter {
    pub peak_left_db: f32,
    pub peak_right_db: f32,
    pub rms_left_db: f32,
    pub rms_right_db: f32,
}

/// Pluggable audio decoder contract for standard formats and `rustports` codecs (e.g., XMA).
pub trait AudioStreamDecoder: Send + Sync + fmt::Debug {
    fn format_name(&self) -> &'static str;
    fn sample_rate(&self) -> u32;
    fn channels(&self) -> u16;
}

/// Convert a decibel value to a linear gain factor ($10^{\text{db}/20}$).
#[must_use]
pub fn db_to_linear(db: f32) -> f32 {
    if !db.is_finite() || db <= -120.0 {
        0.0
    } else {
        10.0_f32.powf(db / 20.0)
    }
}

/// Convert a linear gain factor to decibels ($20 \log_{10}(\text{gain})$).
#[must_use]
pub fn linear_to_db(linear: f32) -> f32 {
    if !linear.is_finite() || linear <= 1e-6 {
        -120.0
    } else {
        20.0 * linear.log10()
    }
}

/// Core audio bus channels in the engine hierarchy.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum AudioBusKind {
    Master,
    Music,
    Sfx,
    Voice,
}

/// State of an individual audio bus channel.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct AudioBus {
    pub kind: AudioBusKind,
    pub volume_db: f32,
    pub mute: bool,
    pub solo: bool,
    pub parent: Option<AudioBusKind>,
    pub peak_l: f32,
    pub peak_r: f32,
}

impl AudioBus {
    /// Create a bus routed to a parent with unity gain (0.0 dB).
    #[must_use]
    pub const fn new(kind: AudioBusKind, parent: Option<AudioBusKind>) -> Self {
        Self {
            kind,
            volume_db: 0.0,
            mute: false,
            solo: false,
            parent,
            peak_l: 0.0,
            peak_r: 0.0,
        }
    }

    /// Linear gain factor of this bus alone.
    #[must_use]
    pub fn linear_gain(&self) -> f32 {
        if self.mute {
            0.0
        } else {
            db_to_linear(self.volume_db)
        }
    }
}

/// Central audio server managing buses, routing, metering, DSP effects, and codec decoders.
#[derive(Debug)]
pub struct AudioServer {
    /// Preserved Phase 0 field: master volume in dB.
    pub master_volume_db: f32,
    buses: BTreeMap<AudioBusKind, AudioBus>,
    bus_effects: BTreeMap<AudioBusKind, Vec<Box<dyn AudioEffect>>>,
    codecs: CodecRegistry,
}

impl Default for AudioServer {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioServer {
    /// Initialize audio server with default `Master`, `Music`, `Sfx`, and `Voice` bus hierarchy.
    #[must_use]
    pub fn new() -> Self {
        let mut buses = BTreeMap::new();
        buses.insert(AudioBusKind::Master, AudioBus::new(AudioBusKind::Master, None));
        buses.insert(AudioBusKind::Music, AudioBus::new(AudioBusKind::Music, Some(AudioBusKind::Master)));
        buses.insert(AudioBusKind::Sfx, AudioBus::new(AudioBusKind::Sfx, Some(AudioBusKind::Master)));
        buses.insert(AudioBusKind::Voice, AudioBus::new(AudioBusKind::Voice, Some(AudioBusKind::Master)));

        let mut bus_effects = BTreeMap::new();
        bus_effects.insert(AudioBusKind::Master, Vec::new());
        bus_effects.insert(AudioBusKind::Music, Vec::new());
        bus_effects.insert(AudioBusKind::Sfx, Vec::new());
        bus_effects.insert(AudioBusKind::Voice, Vec::new());

        Self {
            master_volume_db: 0.0,
            buses,
            bus_effects,
            codecs: CodecRegistry::new(),
        }
    }

    /// Add a DSP effect to an audio bus.
    pub fn add_bus_effect(&mut self, kind: AudioBusKind, effect: Box<dyn AudioEffect>) {
        self.bus_effects.entry(kind).or_default().push(effect);
    }

    /// Remove a DSP effect from an audio bus by index.
    pub fn remove_bus_effect(&mut self, kind: AudioBusKind, index: usize) -> bool {
        match self.bus_effects.get_mut(&kind) {
            Some(effects) if index < effects.len() => {
                effects.remove(index);
                true
            }
            _ => false,
        }
    }

    /// Process audio frames through bus DSP effects, apply gain, and calculate real-time stereo VU meter.
    pub fn process_bus_frames(&mut self, kind: AudioBusKind, frames: &mut [[f32; 2]]) -> BusVuMeter {
        let gain = self.effective_bus_gain(kind);
        let mut effects = self.bus_effects.get_mut(&kind);

        let mut max_l = 0.0_f32;
        let mut max_r = 0.0_f32;
        let mut sum_sq_l = 0.0_f32;
        let mut sum_sq_r = 0.0_f32;

        for frame in frames.iter_mut() {
            let mut l = frame[0];
            let mut r = frame[1];

            if let Some(effect_list) = &mut effects {
                for eff in effect_list.iter_mut() {
                    let (el, er) = eff.process_stereo(l, r);
                    l = el;
                    r = er;
                }
            }

            l *= gain;
            r *= gain;

            frame[0] = l;
            frame[1] = r;

            let abs_l = l.abs();
            let abs_r = r.abs();
            if abs_l > max_l { max_l = abs_l; }
            if abs_r > max_r { max_r = abs_r; }
            sum_sq_l += l * l;
            sum_sq_r += r * r;
        }

        let n = frames.len().max(1) as f32;
        let rms_l = (sum_sq_l / n).sqrt();
        let rms_r = (sum_sq_r / n).sqrt();

        self.update_bus_metering(kind, max_l, max_r);

        BusVuMeter {
            peak_left_db: linear_to_db(max_l),
            peak_right_db: linear_to_db(max_r),
            rms_left_db: linear_to_db(rms_l),
            rms_right_db: linear_to_db(rms_r),
        }
    }

    /// Read bus properties.
    #[must_use]
    pub fn get_bus(&self, kind: AudioBusKind) -> Option<&AudioBus> {
        self.buses.get(&kind)
    }

    /// Set volume in decibels for a given bus.
    pub fn set_bus_volume_db(&mut self, kind: AudioBusKind, db: f32) {
        if let Some(bus) = self.buses.get_mut(&kind) {
            bus.volume_db = db;
            if kind == AudioBusKind::Master {
                self.master_volume_db = db;
            }
        }
    }

    /// Set mute status for a bus.
    pub fn set_bus_mute(&mut self, kind: AudioBusKind, mute: bool) {
        if let Some(bus) = self.buses.get_mut(&kind) {
            bus.mute = mute;
        }
    }

    /// Set solo status for a bus.
    pub fn set_bus_solo(&mut self, kind: AudioBusKind, solo: bool) {
        if let Some(bus) = self.buses.get_mut(&kind) {
            bus.solo = solo;
        }
    }

    /// Update peak metering levels for a bus (clamped to 0.0..=1.0).
    pub fn update_bus_metering(&mut self, kind: AudioBusKind, peak_l: f32, peak_r: f32) {
        if let Some(bus) = self.buses.get_mut(&kind) {
            bus.peak_l = peak_l.clamp(0.0, 1.0);
            bus.peak_r = peak_r.clamp(0.0, 1.0);
        }
    }

    /// Compute cumulative effective linear gain from this bus through all parents to Master.
    ///
    /// Respects mute, solo isolation, and cumulative parent gains.
    #[must_use]
    pub fn effective_bus_gain(&self, kind: AudioBusKind) -> f32 {
        let any_solo = self.buses.values().any(|b| b.solo);

        if any_solo {
            // If solo is active on any bus, this bus is silent unless it or one of its parents is soloed
            let mut is_active = false;
            let mut check_bus = Some(kind);
            while let Some(k) = check_bus {
                if let Some(bus) = self.buses.get(&k) {
                    if bus.solo {
                        is_active = true;
                        break;
                    }
                    check_bus = bus.parent;
                } else {
                    break;
                }
            }

            if !is_active {
                return 0.0;
            }
        }

        // Traverse hierarchy to root multiplying gains
        let mut gain = 1.0;
        let mut curr = Some(kind);

        while let Some(k) = curr {
            if let Some(bus) = self.buses.get(&k) {
                if bus.mute {
                    return 0.0;
                }
                gain *= db_to_linear(bus.volume_db);
                curr = bus.parent;
            } else {
                break;
            }
        }

        gain
    }

    /// Access the registered audio stream codecs.
    #[must_use]
    pub const fn codecs(&self) -> &CodecRegistry {
        &self.codecs
    }

    /// Mutable access to audio codecs registry.
    pub fn codecs_mut(&mut self) -> &mut CodecRegistry {
        &mut self.codecs
    }
}

/// Registry of pluggable audio stream decoders.
#[derive(Debug, Default)]
pub struct CodecRegistry {
    decoders: Vec<Box<dyn AudioStreamDecoder>>,
}

impl CodecRegistry {
    /// Create an empty codec registry.
    #[must_use]
    pub fn new() -> Self {
        Self { decoders: Vec::new() }
    }

    /// Register a stream decoder.
    pub fn register_decoder(&mut self, decoder: Box<dyn AudioStreamDecoder>) {
        self.decoders.retain(|d| d.format_name() != decoder.format_name());
        self.decoders.push(decoder);
    }

    /// Find a registered decoder by format name (e.g. "WAV_PCM", "XMA2").
    #[must_use]
    pub fn get_decoder(&self, format_name: &str) -> Option<&dyn AudioStreamDecoder> {
        self.decoders.iter().find(|d| d.format_name() == format_name).map(AsRef::as_ref)
    }

    /// List all registered codec format names.
    #[must_use]
    pub fn supported_formats(&self) -> Vec<&'static str> {
        self.decoders.iter().map(|d| d.format_name()).collect()
    }
}

/// Errors encountered while parsing WAV/RIFF audio container headers.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum WavParseError {
    BufferTooSmall,
    InvalidRiffHeader,
    InvalidWaveFormat,
    MissingFmtChunk,
    MissingDataChunk,
    UnsupportedAudioFormat(u16),
    InvalidChannelCount(u16),
    InvalidSampleRate(u32),
}

impl fmt::Display for WavParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BufferTooSmall => write!(f, "WAV buffer is smaller than minimum 44-byte header"),
            Self::InvalidRiffHeader => write!(f, "Missing RIFF magic header"),
            Self::InvalidWaveFormat => write!(f, "Missing WAVE format identifier"),
            Self::MissingFmtChunk => write!(f, "Missing required 'fmt ' chunk in WAV file"),
            Self::MissingDataChunk => write!(f, "Missing required 'data' chunk in WAV file"),
            Self::UnsupportedAudioFormat(code) => write!(f, "Unsupported WAV audio format code: {code} (expected 1 for PCM or 3 for IEEE float)"),
            Self::InvalidChannelCount(ch) => write!(f, "Invalid channel count: {ch}"),
            Self::InvalidSampleRate(rate) => write!(f, "Invalid sample rate: {rate}"),
        }
    }
}

impl std::error::Error for WavParseError {}

/// Parsed clean-room WAV/PCM audio stream header.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct WavHeader {
    pub audio_format: u16,
    pub channels: u16,
    pub sample_rate: u32,
    pub byte_rate: u32,
    pub block_align: u16,
    pub bits_per_sample: u16,
    pub data_offset: usize,
    pub data_size: usize,
}

impl WavHeader {
    /// Safe, zero-panic clean-room WAV/RIFF header parser.
    pub fn parse(bytes: &[u8]) -> Result<Self, WavParseError> {
        if bytes.len() < 44 {
            return Err(WavParseError::BufferTooSmall);
        }

        // Check "RIFF"
        if &bytes[0..4] != b"RIFF" {
            return Err(WavParseError::InvalidRiffHeader);
        }

        // Check "WAVE"
        if &bytes[8..12] != b"WAVE" {
            return Err(WavParseError::InvalidWaveFormat);
        }

        let mut offset = 12;
        let mut fmt_info: Option<(u16, u16, u32, u32, u16, u16)> = None;
        let mut data_info: Option<(usize, usize)> = None;

        while offset + 8 <= bytes.len() {
            let chunk_id = &bytes[offset..offset + 4];
            let chunk_size = u32::from_le_bytes([
                bytes[offset + 4],
                bytes[offset + 5],
                bytes[offset + 6],
                bytes[offset + 7],
            ]) as usize;

            let data_start = offset + 8;
            if chunk_id == b"fmt " {
                if chunk_size < 16 || data_start + 16 > bytes.len() {
                    return Err(WavParseError::BufferTooSmall);
                }
                let audio_format = u16::from_le_bytes([bytes[data_start], bytes[data_start + 1]]);
                if audio_format != 1 && audio_format != 3 {
                    return Err(WavParseError::UnsupportedAudioFormat(audio_format));
                }
                let channels = u16::from_le_bytes([bytes[data_start + 2], bytes[data_start + 3]]);
                if channels == 0 || channels > 8 {
                    return Err(WavParseError::InvalidChannelCount(channels));
                }
                let sample_rate = u32::from_le_bytes([
                    bytes[data_start + 4],
                    bytes[data_start + 5],
                    bytes[data_start + 6],
                    bytes[data_start + 7],
                ]);
                if sample_rate == 0 || sample_rate > 192_000 {
                    return Err(WavParseError::InvalidSampleRate(sample_rate));
                }
                let byte_rate = u32::from_le_bytes([
                    bytes[data_start + 8],
                    bytes[data_start + 9],
                    bytes[data_start + 10],
                    bytes[data_start + 11],
                ]);
                let block_align = u16::from_le_bytes([bytes[data_start + 12], bytes[data_start + 13]]);
                let bits_per_sample = u16::from_le_bytes([bytes[data_start + 14], bytes[data_start + 15]]);

                fmt_info = Some((audio_format, channels, sample_rate, byte_rate, block_align, bits_per_sample));
            } else if chunk_id == b"data" {
                let actual_size = chunk_size.min(bytes.len().saturating_sub(data_start));
                data_info = Some((data_start, actual_size));
                break;
            }

            // Word-aligned chunk advancing
            let padded_size = (chunk_size + 1) & !1;
            offset = data_start.saturating_add(padded_size);
        }

        let (audio_format, channels, sample_rate, byte_rate, block_align, bits_per_sample) =
            fmt_info.ok_or(WavParseError::MissingFmtChunk)?;
        let (data_offset, data_size) = data_info.ok_or(WavParseError::MissingDataChunk)?;

        Ok(Self {
            audio_format,
            channels,
            sample_rate,
            byte_rate,
            block_align,
            bits_per_sample,
            data_offset,
            data_size,
        })
    }

    /// Compute total sound duration in seconds.
    #[must_use]
    pub fn duration_seconds(&self) -> f32 {
        if self.byte_rate > 0 {
            (self.data_size as f32) / (self.byte_rate as f32)
        } else {
            0.0
        }
    }
}

impl AudioStreamDecoder for WavHeader {
    fn format_name(&self) -> &'static str {
        "WAV_PCM"
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn channels(&self) -> u16 {
        self.channels
    }
}

/// Pluggable adapter and metadata descriptor for Xbox 360 / Retail Game Archive ported audio (XMA2).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct XmaStreamAdapter {
    pub channels: u16,
    pub sample_rate: u32,
    pub block_size_bytes: u32,
    pub packet_count: u32,
    pub loop_start_sample: u32,
    pub loop_end_sample: u32,
}

impl XmaStreamAdapter {
    /// Create an XMA2 stream adapter descriptor.
    #[must_use]
    pub const fn new(channels: u16, sample_rate: u32, block_size_bytes: u32, packet_count: u32) -> Self {
        Self {
            channels,
            sample_rate,
            block_size_bytes,
            packet_count,
            loop_start_sample: 0,
            loop_end_sample: 0,
        }
    }
}

impl AudioStreamDecoder for XmaStreamAdapter {
    fn format_name(&self) -> &'static str {
        "XMA2"
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn channels(&self) -> u16 {
        self.channels
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn db_linear_roundtrip() {
        assert_eq!(db_to_linear(0.0), 1.0);
        assert!((db_to_linear(-6.0206) - 0.5).abs() < 1e-4);
        assert!((db_to_linear(6.0206) - 2.0).abs() < 1e-4);
        assert_eq!(db_to_linear(-140.0), 0.0);

        assert!((linear_to_db(1.0) - 0.0).abs() < 1e-5);
        assert!((linear_to_db(0.5) - (-6.0206)).abs() < 1e-3);
    }

    #[test]
    fn bus_routing_and_effective_gain() {
        let mut server = AudioServer::new();
        assert_eq!(server.effective_bus_gain(AudioBusKind::Sfx), 1.0);

        // Lower master by -6 dB (~0.5 gain)
        server.set_bus_volume_db(AudioBusKind::Master, -6.0206);
        assert_eq!(server.master_volume_db, -6.0206);

        // Lower SFX by -6 dB (~0.5 gain)
        server.set_bus_volume_db(AudioBusKind::Sfx, -6.0206);

        // Combined effective SFX gain should be ~0.25 (0.5 * 0.5)
        let eff_sfx = server.effective_bus_gain(AudioBusKind::Sfx);
        assert!((eff_sfx - 0.25).abs() < 1e-3);

        // Mute master -> all children silenced
        server.set_bus_mute(AudioBusKind::Master, true);
        assert_eq!(server.effective_bus_gain(AudioBusKind::Sfx), 0.0);
        assert_eq!(server.effective_bus_gain(AudioBusKind::Music), 0.0);

        // Unmute master, solo Music -> SFX should be silenced
        server.set_bus_mute(AudioBusKind::Master, false);
        server.set_bus_solo(AudioBusKind::Music, true);
        assert_eq!(server.effective_bus_gain(AudioBusKind::Sfx), 0.0);
        assert!(server.effective_bus_gain(AudioBusKind::Music) > 0.0);
    }

    #[test]
    fn wav_header_cleanroom_parser() {
        // Construct a synthetic 44-byte standard 16-bit 44.1kHz Stereo PCM WAV file
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36u32 + 100u32).to_le_bytes()); // File size - 8
        wav.extend_from_slice(b"WAVE");

        // fmt chunk
        wav.extend_from_slice(b"fmt ");
        wav.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
        wav.extend_from_slice(&1u16.to_le_bytes());  // PCM format
        wav.extend_from_slice(&2u16.to_le_bytes());  // 2 channels
        wav.extend_from_slice(&44100u32.to_le_bytes()); // Sample rate
        wav.extend_from_slice(&(44100u32 * 4).to_le_bytes()); // Byte rate
        wav.extend_from_slice(&4u16.to_le_bytes());  // Block align
        wav.extend_from_slice(&16u16.to_le_bytes()); // Bits per sample

        // data chunk
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&100u32.to_le_bytes()); // Data size
        wav.resize(wav.len() + 100, 0); // 100 sample bytes

        let parsed = WavHeader::parse(&wav).expect("WAV parsing must succeed");
        assert_eq!(parsed.audio_format, 1);
        assert_eq!(parsed.channels, 2);
        assert_eq!(parsed.sample_rate, 44100);
        assert_eq!(parsed.bits_per_sample, 16);
        assert_eq!(parsed.data_size, 100);
        assert_eq!(parsed.data_offset, 44);
        assert!(parsed.duration_seconds() > 0.0);
    }

    #[test]
    fn codec_registry_and_xma_adapter() {
        let mut registry = CodecRegistry::new();
        let xma = XmaStreamAdapter::new(6, 48000, 2048, 120);
        registry.register_decoder(Box::new(xma));

        assert_eq!(registry.supported_formats(), vec!["XMA2"]);
        let dec = registry.get_decoder("XMA2").expect("decoder found");
        assert_eq!(dec.format_name(), "XMA2");
        assert_eq!(dec.channels(), 6);
        assert_eq!(dec.sample_rate(), 48000);
    }

    #[test]
    fn dsp_effects_processing_and_limiter() {
        let mut limiter = AudioEffectLimiter::new(-6.0); // -6 dB threshold (~0.5 linear)
        // High amplitude input
        let mut out_l = 0.0;
        let mut out_r = 0.0;
        for _ in 0..100 {
            let (l, r) = limiter.process_stereo(2.0, -2.0);
            out_l = l;
            out_r = r;
        }

        // Limiter must have clamped signal below ceiling
        assert!(out_l <= 0.6);
        assert!(out_r.abs() <= 0.6);
    }

    #[test]
    fn bus_dsp_frames_processing_and_vu_metering() {
        let mut server = AudioServer::new();
        server.add_bus_effect(AudioBusKind::Sfx, Box::new(AudioEffectLimiter::new(0.0)));

        let mut frames = vec![[0.8, 0.6]; 128];
        let vu = server.process_bus_frames(AudioBusKind::Sfx, &mut frames);

        assert!(vu.peak_left_db <= 0.1);
        assert!(vu.peak_right_db <= 0.1);
        assert!(vu.rms_left_db > -10.0);
    }
}
