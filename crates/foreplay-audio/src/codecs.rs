//! Multi-format audio decoding via `symphonia 0.6.1` and high-fidelity resampling.
//!
//! Mirrors Godot 4 `AudioStream` importers and runtime decoders, supporting WAV,
//! Ogg Vorbis, FLAC, MP3, and raw PCM streams. Converted into interleaved stereo `f32`
//! frames with cubic Hermite / linear resampling to the engine sample rate.

use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

/// Errors that may occur during multi-format audio decoding.
#[derive(Debug, thiserror::Error)]
pub enum AudioCodecError {
    #[error("I/O error during audio decoding: {0}")]
    Io(#[from] std::io::Error),
    #[error("Symphonia codec error: {0}")]
    Symphonia(String),
    #[error("No supported audio tracks found in container")]
    NoTrackFound,
    #[error("Unsupported audio codec or container format: {0}")]
    UnsupportedFormat(String),
    #[error("Audio stream contains no decodable sample frames")]
    EmptyStream,
    #[error("Invalid raw PCM data length for channels and sample width")]
    InvalidPcmLength,
}

/// Quality preset for sample rate conversion.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ResampleQuality {
    Linear,
    #[default]
    Hermite,
}

/// In-memory decoded interleaved stereo 32-bit floating-point audio asset.
#[derive(Clone, Debug, PartialEq)]
pub struct DecodedAudioAsset {
    pub sample_rate: u32,
    pub channels: u16,
    pub duration_seconds: f32,
    pub frames: Vec<[f32; 2]>,
}

impl DecodedAudioAsset {
    /// Total number of stereo frames in the asset.
    #[must_use]
    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    /// Whether the asset has zero frames.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// Resample this audio asset to a new sample rate using cubic Hermite interpolation.
    #[must_use]
    pub fn resample(&self, target_sample_rate: u32) -> Self {
        if self.sample_rate == target_sample_rate || self.frames.is_empty() {
            return self.clone();
        }
        let resampled_frames = resample_interleaved_stereo(&self.frames, self.sample_rate, target_sample_rate);
        let duration = if target_sample_rate > 0 {
            (resampled_frames.len() as f32) / (target_sample_rate as f32)
        } else {
            0.0
        };
        Self {
            sample_rate: target_sample_rate,
            channels: 2,
            duration_seconds: duration,
            frames: resampled_frames,
        }
    }

    /// Encode this decoded audio asset into standard 16-bit PCM RIFF/WAV byte stream.
    #[must_use]
    pub fn to_wav_bytes(&self) -> Vec<u8> {
        encode_pcm_wav_i16(&self.frames, self.sample_rate)
    }
}

/// Resample interleaved stereo audio frames from `from_rate` to `to_rate` using cubic Hermite interpolation.
#[must_use]
pub fn resample_interleaved_stereo(frames: &[[f32; 2]], from_rate: u32, to_rate: u32) -> Vec<[f32; 2]> {
    resample_interleaved_stereo_with_quality(frames, from_rate, to_rate, ResampleQuality::Hermite)
}

/// Resample interleaved stereo frames with explicit quality setting (Linear or Hermite).
#[must_use]
pub fn resample_interleaved_stereo_with_quality(
    frames: &[[f32; 2]],
    from_rate: u32,
    to_rate: u32,
    quality: ResampleQuality,
) -> Vec<[f32; 2]> {
    if frames.is_empty() || from_rate == 0 || to_rate == 0 {
        return Vec::new();
    }
    if from_rate == to_rate {
        return frames.to_vec();
    }

    let ratio = (from_rate as f64) / (to_rate as f64);
    let output_len = ((frames.len() as f64) / ratio).round() as usize;
    let mut out = Vec::with_capacity(output_len);

    for i in 0..output_len {
        let src_idx = (i as f64) * ratio;
        let idx0 = src_idx.floor() as usize;
        let t = (src_idx - (idx0 as f64)) as f32;

        match quality {
            ResampleQuality::Linear => {
                let s0 = frames.get(idx0).copied().unwrap_or([0.0, 0.0]);
                let s1 = frames.get(idx0 + 1).copied().unwrap_or(s0);
                let l = s0[0] + t * (s1[0] - s0[0]);
                let r = s0[1] + t * (s1[1] - s0[1]);
                out.push([l, r]);
            }
            ResampleQuality::Hermite => {
                let ym1 = if idx0 > 0 {
                    frames.get(idx0 - 1).copied().unwrap_or([0.0, 0.0])
                } else {
                    frames.first().copied().unwrap_or([0.0, 0.0])
                };
                let y0 = frames.get(idx0).copied().unwrap_or([0.0, 0.0]);
                let y1 = frames.get(idx0 + 1).copied().unwrap_or(y0);
                let y2 = frames.get(idx0 + 2).copied().unwrap_or(y1);

                let l = hermite_sample(ym1[0], y0[0], y1[0], y2[0], t);
                let r = hermite_sample(ym1[1], y0[1], y1[1], y2[1], t);
                out.push([l, r]);
            }
        }
    }

    out
}

#[inline]
fn hermite_sample(ym1: f32, y0: f32, y1: f32, y2: f32, t: f32) -> f32 {
    let c0 = y0;
    let c1 = 0.5 * (y1 - ym1);
    let c2 = ym1 - 2.5 * y0 + 2.0 * y1 - 0.5 * y2;
    let c3 = 0.5 * (y2 - ym1) + 1.5 * (y0 - y1);
    ((c3 * t + c2) * t + c1) * t + c0
}

/// Synthesize a valid RIFF/WAV 16-bit Little-Endian stereo byte stream from float frames.
#[must_use]
pub fn encode_pcm_wav_i16(frames: &[[f32; 2]], sample_rate: u32) -> Vec<u8> {
    let num_channels = 2u16;
    let bits_per_sample = 16u16;
    let block_align = num_channels * (bits_per_sample / 8); // 4 bytes
    let byte_rate = sample_rate * (block_align as u32);
    let data_len = (frames.len() * (block_align as usize)) as u32;
    let riff_chunk_size = 36u32.saturating_add(data_len);

    let mut wav = Vec::with_capacity(44 + data_len as usize);
    // RIFF Header
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&riff_chunk_size.to_le_bytes());
    wav.extend_from_slice(b"WAVE");

    // fmt subchunk
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes()); // Subchunk1Size for PCM
    wav.extend_from_slice(&1u16.to_le_bytes());  // AudioFormat = 1 (PCM)
    wav.extend_from_slice(&num_channels.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&bits_per_sample.to_le_bytes());

    // data subchunk
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());

    for frame in frames {
        let l_i16 = (frame[0].clamp(-1.0, 1.0) * 32767.0).round() as i16;
        let r_i16 = (frame[1].clamp(-1.0, 1.0) * 32767.0).round() as i16;
        wav.extend_from_slice(&l_i16.to_le_bytes());
        wav.extend_from_slice(&r_i16.to_le_bytes());
    }

    wav
}

/// Decode raw 16-bit signed integer PCM byte stream into a `DecodedAudioAsset`.
pub fn decode_raw_pcm_i16(
    bytes: &[u8],
    sample_rate: u32,
    channels: u16,
) -> Result<DecodedAudioAsset, AudioCodecError> {
    if !bytes.len().is_multiple_of(2) || channels == 0 {
        return Err(AudioCodecError::InvalidPcmLength);
    }

    let ch = channels as usize;
    let frame_byte_size = ch * 2;
    if !bytes.len().is_multiple_of(frame_byte_size) {
        return Err(AudioCodecError::InvalidPcmLength);
    }

    let total_frames = bytes.len() / frame_byte_size;
    let mut frames = Vec::with_capacity(total_frames);

    for chunk in bytes.chunks_exact(frame_byte_size) {
        if ch == 1 {
            let sample = i16::from_le_bytes([chunk[0], chunk[1]]) as f32 / 32768.0;
            frames.push([sample, sample]);
        } else if ch >= 2 {
            let l = i16::from_le_bytes([chunk[0], chunk[1]]) as f32 / 32768.0;
            let r = i16::from_le_bytes([chunk[2], chunk[3]]) as f32 / 32768.0;
            frames.push([l, r]);
        }
    }

    if frames.is_empty() {
        return Err(AudioCodecError::EmptyStream);
    }

    let duration_seconds = (frames.len() as f32) / (sample_rate as f32);
    Ok(DecodedAudioAsset {
        sample_rate,
        channels: 2,
        duration_seconds,
        frames,
    })
}

/// Decode encoded audio container bytes (WAV, Ogg Vorbis, FLAC, MP3) into an interleaved stereo asset.
pub fn decode_audio_bytes(
    bytes: &[u8],
    hint_ext: Option<&str>,
) -> Result<DecodedAudioAsset, AudioCodecError> {
    if bytes.is_empty() {
        return Err(AudioCodecError::EmptyStream);
    }

    let cursor = Box::new(std::io::Cursor::new(bytes.to_vec()));
    let mss = MediaSourceStream::new(cursor, Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = hint_ext {
        hint.with_extension(ext);
    }

    let mut format_reader = symphonia::default::get_probe()
        .probe(&hint, mss, FormatOptions::default(), MetadataOptions::default())
        .map_err(|e| AudioCodecError::Symphonia(e.to_string()))?;

    // Find the first track with valid audio codec parameters
    let (track_id, audio_params) = format_reader
        .tracks()
        .iter()
        .find_map(|t| {
            let audio = t.codec_params.as_ref()?.audio()?;
            if audio.codec != symphonia::core::codecs::audio::AudioCodecId::default() {
                Some((t.id, audio.clone()))
            } else {
                None
            }
        })
        .ok_or(AudioCodecError::NoTrackFound)?;

    let sample_rate = audio_params.sample_rate.unwrap_or(44100);

    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(&audio_params, &AudioDecoderOptions::default())
        .map_err(|e| AudioCodecError::Symphonia(e.to_string()))?;

    let mut all_frames: Vec<[f32; 2]> = Vec::new();
    let mut packet_samples = Vec::new();

    loop {
        let packet_opt = match format_reader.next_packet() {
            Ok(p) => p,
            Err(SymphoniaError::IoError(ref e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                break;
            }
            Err(SymphoniaError::ResetRequired) => {
                break;
            }
            Err(e) => return Err(AudioCodecError::Symphonia(e.to_string())),
        };

        let Some(packet) = packet_opt else {
            break;
        };

        if packet.track_id != track_id {
            continue;
        }

        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(SymphoniaError::IoError(ref e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                break;
            }
            Err(SymphoniaError::DecodeError(_)) => {
                // Ignore single corrupted frame in streaming audio
                continue;
            }
            Err(e) => return Err(AudioCodecError::Symphonia(e.to_string())),
        };

        let ch_count = decoded.spec().channels().count();
        packet_samples.clear();
        decoded.copy_to_vec_interleaved(&mut packet_samples);

        if ch_count == 1 {
            for &s in &packet_samples {
                all_frames.push([s, s]);
            }
        } else if ch_count == 2 {
            for chunk in packet_samples.chunks_exact(2) {
                all_frames.push([chunk[0], chunk[1]]);
            }
        } else if ch_count > 2 {
            for chunk in packet_samples.chunks_exact(ch_count) {
                let center = chunk.get(2).copied().unwrap_or(0.0) * std::f32::consts::FRAC_1_SQRT_2;
                let l = chunk[0] + center;
                let r = chunk[1] + center;
                all_frames.push([l, r]);
            }
        }
    }

    if all_frames.is_empty() {
        return Err(AudioCodecError::EmptyStream);
    }

    let duration_seconds = (all_frames.len() as f32) / (sample_rate as f32);

    Ok(DecodedAudioAsset {
        sample_rate,
        channels: 2,
        duration_seconds,
        frames: all_frames,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_and_decode_wav_roundtrip() {
        // Generate a 100ms 440 Hz stereo test tone at 44,100 Hz
        let sample_rate = 44100;
        let num_frames = 4410; // 0.1 sec
        let mut original_frames = Vec::with_capacity(num_frames);

        for i in 0..num_frames {
            let t = (i as f32) / (sample_rate as f32);
            let s_l = (2.0 * std::f32::consts::PI * 440.0 * t).sin() * 0.5;
            let s_r = (2.0 * std::f32::consts::PI * 880.0 * t).sin() * 0.5;
            original_frames.push([s_l, s_r]);
        }

        let wav_bytes = encode_pcm_wav_i16(&original_frames, sample_rate);
        assert!(wav_bytes.len() > 44);

        let decoded = decode_audio_bytes(&wav_bytes, Some("wav")).expect("Decoding synthetic WAV must succeed");
        assert_eq!(decoded.sample_rate, sample_rate);
        assert_eq!(decoded.channels, 2);
        assert_eq!(decoded.frame_count(), num_frames);
        assert!((decoded.duration_seconds - 0.1).abs() < 1e-3);

        // Verify audio values are preserved within 16-bit quantization noise (~1/32767)
        for (orig, dec) in original_frames.iter().zip(decoded.frames.iter()) {
            assert!((orig[0] - dec[0]).abs() < 0.01);
            assert!((orig[1] - dec[1]).abs() < 0.01);
        }
    }

    #[test]
    fn test_resample_interleaved_stereo_rates() {
        let from_rate = 44100;
        let to_rate = 48000;
        let mut frames = Vec::new();
        for i in 0..4410 {
            let s = ((i as f32) * 0.01).sin();
            frames.push([s, -s]);
        }

        let resampled = resample_interleaved_stereo(&frames, from_rate, to_rate);
        let expected_len = ((frames.len() as f64) * 48000.0 / 44100.0).round() as usize;
        assert_eq!(resampled.len(), expected_len);

        // Resample with Linear quality
        let resampled_lin = resample_interleaved_stereo_with_quality(&frames, from_rate, to_rate, ResampleQuality::Linear);
        assert_eq!(resampled_lin.len(), expected_len);

        // Downsampling
        let downsampled = resample_interleaved_stereo(&resampled, 48000, 24000);
        assert_eq!(downsampled.len(), expected_len / 2);
    }

    #[test]
    fn test_resample_identical_rates_and_empty() {
        let frames = vec![[0.2, 0.4], [0.6, 0.8]];
        let same = resample_interleaved_stereo(&frames, 48000, 48000);
        assert_eq!(same, frames);

        let empty: Vec<[f32; 2]> = Vec::new();
        let res_empty = resample_interleaved_stereo(&empty, 44100, 48000);
        assert!(res_empty.is_empty());

        let res_zero = resample_interleaved_stereo(&frames, 0, 48000);
        assert!(res_zero.is_empty());
    }

    #[test]
    fn test_decoded_audio_asset_methods() {
        let asset = DecodedAudioAsset {
            sample_rate: 44100,
            channels: 2,
            duration_seconds: 0.5,
            frames: vec![[0.1, 0.2]; 22050],
        };

        assert_eq!(asset.frame_count(), 22050);
        assert!(!asset.is_empty());

        let resampled = asset.resample(48000);
        assert_eq!(resampled.sample_rate, 48000);
        assert_eq!(resampled.channels, 2);
        assert!((resampled.duration_seconds - 0.5).abs() < 1e-2);

        let wav = asset.to_wav_bytes();
        assert!(wav.starts_with(b"RIFF"));
    }

    #[test]
    fn test_decode_raw_pcm_i16() {
        // 2 stereo frames of 16-bit PCM (4 samples = 8 bytes)
        let s0_l: i16 = 16384;  // ~0.5
        let s0_r: i16 = -16384; // ~-0.5
        let s1_l: i16 = 32767;  // ~1.0
        let s1_r: i16 = 0;      // 0.0

        let mut bytes = Vec::new();
        bytes.extend_from_slice(&s0_l.to_le_bytes());
        bytes.extend_from_slice(&s0_r.to_le_bytes());
        bytes.extend_from_slice(&s1_l.to_le_bytes());
        bytes.extend_from_slice(&s1_r.to_le_bytes());

        let decoded = decode_raw_pcm_i16(&bytes, 48000, 2).expect("decode pcm stereo");
        assert_eq!(decoded.frame_count(), 2);
        assert!((decoded.frames[0][0] - 0.5).abs() < 1e-3);
        assert!((decoded.frames[0][1] - (-0.5)).abs() < 1e-3);
        assert!((decoded.frames[1][0] - 1.0).abs() < 1e-3);
        assert_eq!(decoded.frames[1][1], 0.0);

        // Odd length must error
        assert!(decode_raw_pcm_i16(&[0, 1, 2], 48000, 2).is_err());
    }

    #[test]
    fn test_decode_audio_bytes_empty_input() {
        let res = decode_audio_bytes(&[], None);
        assert!(matches!(res, Err(AudioCodecError::EmptyStream)));
    }
}
