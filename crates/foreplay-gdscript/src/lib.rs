//! GDScript-to-IR compatibility front-end (Track D RustPorts `[mechanical]` wave).
//! Allows existing Godot 4 `.tscn` + `.gd` projects to open directly in Foreplay without a second VM.

use foreplay_ir::FeelSpecKnob;

/// Extracted `@export` metadata from a `.gd` script header.
#[derive(Clone, PartialEq, Debug)]
pub struct GdExportMetadata {
    pub class_name: Option<String>,
    pub extends: String,
    pub knobs: Vec<FeelSpecKnob>,
}
