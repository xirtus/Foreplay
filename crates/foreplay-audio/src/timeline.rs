//! Sample-Accurate 48,000 Hz Non-Destructive Audio Clip Timeline Engine.
//!
//! Inspired by SexOS `moan-core` and professional DAWs (Logic Pro / Final Cut Pro).
//! Supports immutable `Arc<Vec<[i16; 2]>>` PCM sources, non-destructive splitting and trimming,
//! fade envelopes, track volume/pan/mute/solo, and zero-allocation inner block rendering.

use std::sync::Arc;

/// Native internal mixing sample rate: 48,000 Hz stereo.
pub const SAMPLE_RATE: u32 = 48_000;

/// Non-destructive audio clip referencing an immutable PCM source buffer.
#[derive(Clone, Debug)]
pub struct AudioClip {
    /// Immutable 48kHz stereo 16-bit PCM source data.
    pub source: Arc<Vec<[i16; 2]>>,
    /// Frame offset into the source buffer where this clip begins playing.
    pub source_offset_frames: u64,
    /// Playback duration in frames.
    pub duration_frames: u64,
    /// Absolute start position in frames on the master timeline.
    pub timeline_start_frame: u64,
    /// Linear clip gain in `0.0..=4.0` (default 1.0).
    pub gain: f32,
    /// Stereo panning in `-1.0..=1.0` (-1.0 = full left, 0.0 = center, 1.0 = full right).
    pub pan: f32,
    /// Fade-in envelope length in frames.
    pub fade_in_frames: u64,
    /// Fade-out envelope length in frames.
    pub fade_out_frames: u64,
}

impl AudioClip {
    /// Construct a new full-duration audio clip placed at `timeline_start_frame`.
    #[must_use]
    pub fn new(source: Arc<Vec<[i16; 2]>>, timeline_start_frame: u64) -> Self {
        let duration_frames = source.len() as u64;
        Self {
            source,
            source_offset_frames: 0,
            duration_frames,
            timeline_start_frame,
            gain: 1.0,
            pan: 0.0,
            fade_in_frames: 0,
            fade_out_frames: 0,
        }
    }

    /// Non-destructively split the audio clip into two adjacent clips at `frame`.
    ///
    /// Both resulting clips share the underlying immutable PCM buffer with zero copies,
    /// updating their source offsets, durations, and fade boundaries cleanly.
    /// Returns `None` if `frame` is outside the clip's interior range.
    #[must_use]
    pub fn split_at(&self, frame: u64) -> Option<(Self, Self)> {
        let clip_end = self.timeline_start_frame + self.duration_frames;
        if frame <= self.timeline_start_frame || frame >= clip_end {
            return None;
        }

        let split_rel = frame - self.timeline_start_frame;

        // Left clip [start, frame)
        let left = Self {
            source: Arc::clone(&self.source),
            source_offset_frames: self.source_offset_frames,
            duration_frames: split_rel,
            timeline_start_frame: self.timeline_start_frame,
            gain: self.gain,
            pan: self.pan,
            fade_in_frames: self.fade_in_frames.min(split_rel),
            fade_out_frames: 0, // Clean split
        };

        // Right clip [frame, end)
        let right_duration = self.duration_frames - split_rel;
        let right = Self {
            source: Arc::clone(&self.source),
            source_offset_frames: self.source_offset_frames + split_rel,
            duration_frames: right_duration,
            timeline_start_frame: frame,
            gain: self.gain,
            pan: self.pan,
            fade_in_frames: 0, // Clean split
            fade_out_frames: self.fade_out_frames.min(right_duration),
        };

        Some((left, right))
    }

    /// Non-destructively trim the clip to the specified timeline frame boundaries.
    ///
    /// Preserves existing fade envelopes cleanly clamped to the new trimmed duration.
    pub fn trim(&mut self, start_frame: u64, end_frame: u64) {
        let current_end = self.timeline_start_frame + self.duration_frames;
        let clamped_start = start_frame.max(self.timeline_start_frame).min(current_end);
        let clamped_end = end_frame.max(clamped_start).min(current_end);

        let delta_front = clamped_start - self.timeline_start_frame;
        self.source_offset_frames += delta_front;
        self.timeline_start_frame = clamped_start;
        self.duration_frames = clamped_end - clamped_start;

        self.fade_in_frames = self.fade_in_frames.min(self.duration_frames);
        self.fade_out_frames = self.fade_out_frames.min(self.duration_frames);
    }
}

/// An audio timeline track containing sequenced clips, volume, pan, and mute/solo controls.
#[derive(Clone, Debug)]
pub struct AudioTimelineTrack {
    pub id: u32,
    pub name: String,
    pub clips: Vec<AudioClip>,
    pub volume_db: f32,
    pub pan: f32,
    pub mute: bool,
    pub solo: bool,
}

impl AudioTimelineTrack {
    /// Construct a new empty audio track.
    #[must_use]
    pub fn new(id: u32, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            clips: Vec::new(),
            volume_db: 0.0,
            pan: 0.0,
            mute: false,
            solo: false,
        }
    }

    /// Add an audio clip to this track.
    pub fn add_clip(&mut self, clip: AudioClip) {
        self.clips.push(clip);
    }

    /// Render a block of stereo f32 frames with zero heap allocations in the inner loop.
    pub fn render_block(&self, start_frame: u64, out_stereo_f32: &mut [[f32; 2]]) {
        if self.mute {
            return;
        }

        let track_gain = crate::db_to_linear(self.volume_db);
        if track_gain <= 1e-6 {
            return;
        }

        let trk_pan_l = ((1.0 - self.pan) * 0.5).clamp(0.0, 1.0);
        let trk_pan_r = ((1.0 + self.pan) * 0.5).clamp(0.0, 1.0);

        let block_len = out_stereo_f32.len() as u64;
        let block_end = start_frame + block_len;

        for clip in &self.clips {
            let clip_end = clip.timeline_start_frame + clip.duration_frames;
            if clip_end <= start_frame || clip.timeline_start_frame >= block_end {
                continue; // No overlap with this block
            }

            let overlap_start = start_frame.max(clip.timeline_start_frame);
            let overlap_end = block_end.min(clip_end);

            let block_offset_start = (overlap_start - start_frame) as usize;
            let block_offset_end = (overlap_end - start_frame) as usize;

            let clip_gain = clip.gain.clamp(0.0, 4.0);
            let clip_pan_l = ((1.0 - clip.pan) * 0.5).clamp(0.0, 1.0);
            let clip_pan_r = ((1.0 + clip.pan) * 0.5).clamp(0.0, 1.0);

            let total_gain_l = track_gain * trk_pan_l * clip_gain * clip_pan_l;
            let total_gain_r = track_gain * trk_pan_r * clip_gain * clip_pan_r;

            for (bi, out_frame) in out_stereo_f32[block_offset_start..block_offset_end].iter_mut().enumerate() {
                let timeline_f = start_frame + (block_offset_start + bi) as u64;
                let clip_f = timeline_f - clip.timeline_start_frame;
                let src_f = (clip.source_offset_frames + clip_f) as usize;

                if let Some(&raw) = clip.source.get(src_f) {
                    let mut env = 1.0_f32;

                    // Smooth fade-in S-curve
                    if clip.fade_in_frames > 0 && clip_f < clip.fade_in_frames {
                        let t = (clip_f as f32) / (clip.fade_in_frames as f32);
                        env *= t * t * (3.0 - 2.0 * t);
                    }

                    // Smooth fade-out S-curve
                    let frames_remaining = clip.duration_frames.saturating_sub(clip_f + 1);
                    if clip.fade_out_frames > 0 && frames_remaining < clip.fade_out_frames {
                        let t = (frames_remaining as f32) / (clip.fade_out_frames as f32);
                        env *= t * t * (3.0 - 2.0 * t);
                    }

                    let sl = (raw[0] as f32) * (1.0 / 32768.0);
                    let sr = (raw[1] as f32) * (1.0 / 32768.0);

                    out_frame[0] += sl * env * total_gain_l;
                    out_frame[1] += sr * env * total_gain_r;
                }
            }
        }
    }
}

/// Master multitrack audio timeline coordinating multiple tracks, solo/mute resolution,
/// and block-size invariant rendering.
#[derive(Clone, Debug, Default)]
pub struct AudioTimeline {
    pub tracks: Vec<AudioTimelineTrack>,
    pub master_volume_db: f32,
}

impl AudioTimeline {
    /// Construct a new empty audio timeline.
    #[must_use]
    pub fn new() -> Self {
        Self {
            tracks: Vec::new(),
            master_volume_db: 0.0,
        }
    }

    /// Add a track to the timeline.
    pub fn add_track(&mut self, track: AudioTimelineTrack) {
        self.tracks.push(track);
    }

    /// Render a block of audio into `out_stereo_f32`.
    ///
    /// Clears the destination buffer, resolves solo/mute states across all tracks,
    /// and renders with master gain scaling.
    pub fn render_block(&self, start_frame: u64, out_stereo_f32: &mut [[f32; 2]]) {
        // Zero output buffer
        for frame in out_stereo_f32.iter_mut() {
            frame[0] = 0.0;
            frame[1] = 0.0;
        }

        let master_gain = crate::db_to_linear(self.master_volume_db);
        if master_gain <= 1e-6 {
            return;
        }

        let has_solo = self.tracks.iter().any(|t| t.solo && !t.mute);

        for track in &self.tracks {
            if has_solo {
                if !track.solo || track.mute {
                    continue;
                }
            } else if track.mute {
                continue;
            }

            track.render_block(start_frame, out_stereo_f32);
        }

        // Apply master gain
        if (master_gain - 1.0).abs() > 1e-5 {
            for frame in out_stereo_f32.iter_mut() {
                frame[0] *= master_gain;
                frame[1] *= master_gain;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_timeline_split_trim_and_fade() {
        // Generate a 1-second 48kHz stereo ramp PCM buffer
        let total_frames = 48_000;
        let mut pcm = Vec::with_capacity(total_frames);
        for i in 0..total_frames {
            let val = (i as i16) % 10000;
            pcm.push([val, val]);
        }
        let source = Arc::new(pcm);

        let mut clip = AudioClip::new(Arc::clone(&source), 0);
        clip.fade_in_frames = 480;
        clip.fade_out_frames = 480;

        // Split clip in half at frame 24,000
        let (left, right) = clip.split_at(24_000).expect("split succeeds");
        assert_eq!(left.timeline_start_frame, 0);
        assert_eq!(left.duration_frames, 24_000);
        assert_eq!(right.timeline_start_frame, 24_000);
        assert_eq!(right.duration_frames, 24_000);
        assert_eq!(right.source_offset_frames, 24_000);

        // Trim clip
        let mut trimmed = clip.clone();
        trimmed.trim(1000, 10000);
        assert_eq!(trimmed.timeline_start_frame, 1000);
        assert_eq!(trimmed.duration_frames, 9000);
        assert_eq!(trimmed.source_offset_frames, 1000);
    }

    #[test]
    fn audio_timeline_block_size_invariance() {
        let total_frames = 4800;
        let mut pcm = Vec::with_capacity(total_frames);
        for i in 0..total_frames {
            let val = ((i * 5) as i16) % 15000;
            pcm.push([val, -val]);
        }
        let source = Arc::new(pcm);

        let mut track = AudioTimelineTrack::new(0, "Music");
        let clip = AudioClip::new(source, 0);
        track.add_clip(clip);

        let mut timeline = AudioTimeline::new();
        timeline.add_track(track);

        // Render once in a single large block of 4800 frames
        let mut render_large = vec![[0.0_f32; 2]; total_frames];
        timeline.render_block(0, &mut render_large);

        // Render in small chunks of 256 frames
        let mut render_chunked = vec![[0.0_f32; 2]; total_frames];
        let chunk_size = 256;
        let mut offset = 0;
        while offset < total_frames {
            let count = chunk_size.min(total_frames - offset);
            timeline.render_block(offset as u64, &mut render_chunked[offset..offset + count]);
            offset += count;
        }

        // Compare: must be bit-identical across chunk sizes
        for i in 0..total_frames {
            assert_eq!(render_large[i][0], render_chunked[i][0], "Frame {i} left mismatch");
            assert_eq!(render_large[i][1], render_chunked[i][1], "Frame {i} right mismatch");
        }
    }
}
