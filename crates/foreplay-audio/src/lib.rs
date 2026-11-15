//! `AudioServer` bus hierarchy and pluggable stream codec trait.

/// Pluggable audio decoder contract for standard formats and `rustports` codecs (e.g., XMA).
pub trait AudioStreamDecoder: Send + Sync {
    fn format_name(&self) -> &'static str;
    fn sample_rate(&self) -> u32;
    fn channels(&self) -> u16;
}

#[derive(Debug, Default)]
pub struct AudioServer {
    pub master_volume_db: f32,
}
