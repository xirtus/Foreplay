//! Phase 6 One-Click Export Pipeline (`foreplay-export`).
//!
//! Provides deterministic `.fpck` archive packing/unpacking and release bundle generation
//! for Linux/SteamDeck, Windows, macOS `.app` (with Game Mode), WebGPU HTML5, and SexOS.

pub mod archive;
pub mod bundle;

pub use archive::{FpckArchive, FpckError, TocEntry, fnv1a_64};
pub use bundle::{
    ExportConfig, ExportError, ExportOutput, ExportTarget, ProjectExporter,
    verify_self_contained_export_binary, verify_self_contained_export_bytes,
};
