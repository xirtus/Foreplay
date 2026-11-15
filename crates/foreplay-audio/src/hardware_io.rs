//! Lock-free real-time audio output engine via `cpal 0.18.2` and lock-free SPSC ringbuffer.
//!
//! Provides `CpalAudioDriver` which connects to the platform default audio output device
//! or transparently falls back to `AudioOutputBackend::HeadlessVirtual` for headless test/CI
//! execution without failing.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};
use std::fmt;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

/// Pack an interleaved stereo sample `[f32; 2]` into an atomic 64-bit integer.
#[inline]
fn pack_stereo(sample: [f32; 2]) -> u64 {
    let l_bits = sample[0].to_bits() as u64;
    let r_bits = (sample[1].to_bits() as u64) << 32;
    l_bits | r_bits
}

/// Unpack an atomic 64-bit integer into an interleaved stereo sample `[f32; 2]`.
#[inline]
fn unpack_stereo(packed: u64) -> [f32; 2] {
    let l = f32::from_bits(packed as u32);
    let r = f32::from_bits((packed >> 32) as u32);
    [l, r]
}

/// Lock-free Single-Producer Single-Consumer (SPSC) stereo audio ringbuffer.
///
/// Implements lock-free atomic transfers between game loop threads and hardware
/// real-time audio callback threads without lock contention or allocations.
pub struct AudioRingBuffer {
    buffer: Box<[AtomicU64]>,
    capacity: usize,
    head: AtomicUsize,
    tail: AtomicUsize,
}

impl fmt::Debug for AudioRingBuffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AudioRingBuffer")
            .field("capacity", &self.capacity)
            .field("available_read", &self.available_read())
            .field("available_write", &self.available_write())
            .finish()
    }
}

impl AudioRingBuffer {
    /// Create a new SPSC ringbuffer with the given frame capacity.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        let cap = capacity.max(16);
        let mut v = Vec::with_capacity(cap);
        for _ in 0..cap {
            v.push(AtomicU64::new(0));
        }

        Self {
            buffer: v.into_boxed_slice(),
            capacity: cap,
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        }
    }

    /// Buffer capacity in stereo frames.
    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    /// Number of available frames ready to be read.
    #[must_use]
    pub fn available_read(&self) -> usize {
        let head = self.head.load(Ordering::Acquire);
        let tail = self.tail.load(Ordering::Acquire);
        head.saturating_sub(tail)
    }

    /// Number of free frame slots available for writing.
    #[must_use]
    pub fn available_write(&self) -> usize {
        self.capacity.saturating_sub(self.available_read())
    }

    /// Whether the ringbuffer is currently empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.available_read() == 0
    }

    /// Whether the ringbuffer is full.
    #[must_use]
    pub fn is_full(&self) -> bool {
        self.available_write() == 0
    }

    /// Push a single stereo sample frame. Returns `false` if buffer is full.
    pub fn push(&self, sample: [f32; 2]) -> bool {
        let head = self.head.load(Ordering::Acquire);
        let tail = self.tail.load(Ordering::Acquire);

        if head.saturating_sub(tail) >= self.capacity {
            return false;
        }

        let idx = head % self.capacity;
        self.buffer[idx].store(pack_stereo(sample), Ordering::Release);
        self.head.store(head + 1, Ordering::Release);
        true
    }

    /// Pop a single stereo sample frame. Returns `None` if buffer is empty.
    pub fn pop(&self) -> Option<[f32; 2]> {
        let tail = self.tail.load(Ordering::Acquire);
        let head = self.head.load(Ordering::Acquire);

        if tail >= head {
            return None;
        }

        let idx = tail % self.capacity;
        let packed = self.buffer[idx].load(Ordering::Acquire);
        self.tail.store(tail + 1, Ordering::Release);
        Some(unpack_stereo(packed))
    }

    /// Batch push slice of stereo frames. Returns count of frames actually pushed.
    pub fn push_slice(&self, samples: &[[f32; 2]]) -> usize {
        let mut count = 0;
        for &s in samples {
            if !self.push(s) {
                break;
            }
            count += 1;
        }
        count
    }

    /// Batch pop stereo frames into destination buffer. Fills missing samples with silence `[0.0, 0.0]`.
    pub fn pop_slice(&self, out: &mut [[f32; 2]]) -> usize {
        let mut popped = 0;
        for slot in out.iter_mut() {
            if let Some(s) = self.pop() {
                *slot = s;
                popped += 1;
            } else {
                *slot = [0.0, 0.0];
            }
        }
        popped
    }

    /// Clear all pending frames in the buffer.
    pub fn clear(&self) {
        let head = self.head.load(Ordering::Acquire);
        self.tail.store(head, Ordering::Release);
    }
}

/// Active audio backend mode.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AudioOutputBackend {
    /// Live hardware audio output via CPAL.
    CpalHardware,
    /// Headless virtual audio backend (used in CI, automated tests, or headless servers).
    HeadlessVirtual,
}

/// Real-time audio driver managing hardware device streams and ringbuffer synchronization.
pub struct CpalAudioDriver {
    backend: AudioOutputBackend,
    sample_rate: u32,
    channels: u16,
    ring_buffer: Arc<AudioRingBuffer>,
    #[allow(dead_code)]
    stream: Option<cpal::Stream>,
}

impl fmt::Debug for CpalAudioDriver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CpalAudioDriver")
            .field("backend", &self.backend)
            .field("sample_rate", &self.sample_rate)
            .field("channels", &self.channels)
            .field("has_active_stream", &self.stream.is_some())
            .field("ring_buffer", &self.ring_buffer)
            .finish()
    }
}

impl CpalAudioDriver {
    /// Buffer frame capacity (approx 100ms at 48kHz).
    pub const DEFAULT_BUFFER_FRAMES: usize = 4800;

    /// Attempt to initialize the default OS audio output device via CPAL.
    ///
    /// If an audio device is unavailable, unpermitted, or fails to initialize,
    /// transparently falls back to `AudioOutputBackend::HeadlessVirtual`.
    #[must_use]
    pub fn try_start_default_output(target_sample_rate: u32) -> Self {
        let ring_buffer = Arc::new(AudioRingBuffer::new(Self::DEFAULT_BUFFER_FRAMES));

        // Attempt CPAL hardware stream setup
        if let Some(driver) = Self::try_init_cpal_hardware(target_sample_rate, Arc::clone(&ring_buffer)) {
            return driver;
        }

        // Fallback to HeadlessVirtual backend
        Self {
            backend: AudioOutputBackend::HeadlessVirtual,
            sample_rate: target_sample_rate,
            channels: 2,
            ring_buffer,
            stream: None,
        }
    }

    /// Explicitly initialize a headless virtual audio driver for tests and benchmarking.
    #[must_use]
    pub fn new_virtual(sample_rate: u32) -> Self {
        Self {
            backend: AudioOutputBackend::HeadlessVirtual,
            sample_rate,
            channels: 2,
            ring_buffer: Arc::new(AudioRingBuffer::new(Self::DEFAULT_BUFFER_FRAMES)),
            stream: None,
        }
    }

    fn try_init_cpal_hardware(
        target_sample_rate: u32,
        ring_buffer: Arc<AudioRingBuffer>,
    ) -> Option<Self> {
        let host = cpal::default_host();
        let device = host.default_output_device()?;
        let default_config = device.default_output_config().ok()?;

        let sample_rate = if default_config.sample_rate() > 0 {
            default_config.sample_rate()
        } else {
            target_sample_rate
        };

        let config = StreamConfig {
            channels: 2,
            sample_rate,
            buffer_size: cpal::BufferSize::Default,
        };

        let err_fn = |err| {
            eprintln!("Audio stream error: {err}");
        };

        let rb_clone = Arc::clone(&ring_buffer);
        let sample_format = default_config.sample_format();

        let stream_result = match sample_format {
            SampleFormat::F32 => device.build_output_stream(
                config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    for chunk in data.chunks_exact_mut(2) {
                        let frame = rb_clone.pop().unwrap_or([0.0, 0.0]);
                        chunk[0] = frame[0];
                        chunk[1] = frame[1];
                    }
                },
                err_fn,
                None,
            ),
            SampleFormat::I16 => device.build_output_stream(
                config,
                move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                    for chunk in data.chunks_exact_mut(2) {
                        let frame = rb_clone.pop().unwrap_or([0.0, 0.0]);
                        chunk[0] = (frame[0].clamp(-1.0, 1.0) * 32767.0).round() as i16;
                        chunk[1] = (frame[1].clamp(-1.0, 1.0) * 32767.0).round() as i16;
                    }
                },
                err_fn,
                None,
            ),
            SampleFormat::U16 => device.build_output_stream(
                config,
                move |data: &mut [u16], _: &cpal::OutputCallbackInfo| {
                    for chunk in data.chunks_exact_mut(2) {
                        let frame = rb_clone.pop().unwrap_or([0.0, 0.0]);
                        let l_u16 = (((frame[0].clamp(-1.0, 1.0) + 1.0) * 0.5) * 65535.0).round() as u16;
                        let r_u16 = (((frame[1].clamp(-1.0, 1.0) + 1.0) * 0.5) * 65535.0).round() as u16;
                        chunk[0] = l_u16;
                        chunk[1] = r_u16;
                    }
                },
                err_fn,
                None,
            ),
            _ => return None,
        };

        if let Ok(stream) = stream_result
            && stream.play().is_ok()
        {
            return Some(Self {
                backend: AudioOutputBackend::CpalHardware,
                sample_rate,
                channels: 2,
                ring_buffer,
                stream: Some(stream),
            });
        }

        None
    }

    /// Active audio backend (Hardware or Virtual).
    #[must_use]
    pub const fn backend(&self) -> AudioOutputBackend {
        self.backend
    }

    /// Active hardware or virtual sample rate.
    #[must_use]
    pub const fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Number of output channels (always stereo = 2).
    #[must_use]
    pub const fn channels(&self) -> u16 {
        self.channels
    }

    /// Reference to the internal lock-free ringbuffer.
    #[must_use]
    pub fn ring_buffer(&self) -> &Arc<AudioRingBuffer> {
        &self.ring_buffer
    }

    /// Whether this driver is in headless virtual mode.
    #[must_use]
    pub fn is_virtual(&self) -> bool {
        matches!(self.backend, AudioOutputBackend::HeadlessVirtual)
    }

    /// Whether this driver is connected to live hardware output.
    #[must_use]
    pub fn is_hardware(&self) -> bool {
        matches!(self.backend, AudioOutputBackend::CpalHardware)
    }

    /// Push stereo audio frames into the output stream queue.
    pub fn push_frames(&self, frames: &[[f32; 2]]) -> usize {
        self.ring_buffer.push_slice(frames)
    }

    /// Drain a specified number of frames from the ringbuffer in headless virtual mode.
    pub fn pump_virtual(&self, num_frames: usize) -> Vec<[f32; 2]> {
        let mut out = vec![[0.0, 0.0]; num_frames];
        self.ring_buffer.pop_slice(&mut out);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ring_buffer_push_pop_order() {
        let rb = AudioRingBuffer::new(64);
        assert!(rb.is_empty());
        assert_eq!(rb.available_read(), 0);
        assert_eq!(rb.available_write(), 64);

        let s1 = [0.1, -0.1];
        let s2 = [0.5, -0.5];
        assert!(rb.push(s1));
        assert!(rb.push(s2));

        assert_eq!(rb.available_read(), 2);
        assert!(!rb.is_empty());

        let p1 = rb.pop().expect("pop s1");
        assert_eq!(p1, s1);
        let p2 = rb.pop().expect("pop s2");
        assert_eq!(p2, s2);

        assert!(rb.is_empty());
        assert_eq!(rb.pop(), None);
    }

    #[test]
    fn test_ring_buffer_capacity_and_overflow() {
        let cap = 16;
        let rb = AudioRingBuffer::new(cap);

        for i in 0..cap {
            let sample = [(i as f32) * 0.1, -(i as f32) * 0.1];
            assert!(rb.push(sample));
        }

        assert!(rb.is_full());
        assert_eq!(rb.available_write(), 0);
        assert_eq!(rb.available_read(), cap);

        // Attempting to push when full returns false and does not corrupt data
        assert!(!rb.push([9.9, 9.9]));

        // Pop one, now write slot available
        assert!(rb.pop().is_some());
        assert!(!rb.is_full());
        assert_eq!(rb.available_write(), 1);
        assert!(rb.push([1.0, 1.0]));
    }

    #[test]
    fn test_ring_buffer_slice_batch_operations() {
        let rb = AudioRingBuffer::new(32);
        let input = vec![[0.1, 0.2], [0.3, 0.4], [0.5, 0.6]];

        let pushed = rb.push_slice(&input);
        assert_eq!(pushed, 3);
        assert_eq!(rb.available_read(), 3);

        let mut output = vec![[0.0, 0.0]; 5];
        let popped = rb.pop_slice(&mut output);
        assert_eq!(popped, 3);

        assert_eq!(output[0], [0.1, 0.2]);
        assert_eq!(output[1], [0.3, 0.4]);
        assert_eq!(output[2], [0.5, 0.6]);
        // Underflow samples must be filled with silence
        assert_eq!(output[3], [0.0, 0.0]);
        assert_eq!(output[4], [0.0, 0.0]);
    }

    #[test]
    fn test_ring_buffer_clear() {
        let rb = AudioRingBuffer::new(32);
        rb.push([0.5, 0.5]);
        rb.push([0.8, 0.8]);
        assert_eq!(rb.available_read(), 2);

        rb.clear();
        assert!(rb.is_empty());
        assert_eq!(rb.available_read(), 0);
        assert_eq!(rb.pop(), None);
    }

    #[test]
    fn test_cpal_driver_virtual_pipeline() {
        let driver = CpalAudioDriver::new_virtual(48000);
        assert!(driver.is_virtual());
        assert!(!driver.is_hardware());
        assert_eq!(driver.sample_rate(), 48000);
        assert_eq!(driver.channels(), 2);
        assert_eq!(driver.backend(), AudioOutputBackend::HeadlessVirtual);

        let frames = vec![[0.25, -0.25]; 128];
        let pushed = driver.push_frames(&frames);
        assert_eq!(pushed, 128);

        let pumped = driver.pump_virtual(128);
        assert_eq!(pumped.len(), 128);
        assert_eq!(pumped[0], [0.25, -0.25]);
    }

    #[test]
    fn test_cpal_driver_try_start_default_output() {
        // Must succeed without panic on any platform, falling back to HeadlessVirtual if no OS device
        let driver = CpalAudioDriver::try_start_default_output(48000);
        assert!(driver.sample_rate() > 0);
        assert_eq!(driver.channels(), 2);
    }
}
