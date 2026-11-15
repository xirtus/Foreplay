//! Godot 4 `.gdextension` manifest configuration parser and serializer.
//!
//! Handles `[configuration]`, `[libraries]`, and `[dependencies]` sections,
//! and provides host platform library resolution with fallback strategies.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

/// Errors produced during `.gdextension` manifest parsing or serialization.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ManifestError {
    #[error("syntax error on line {line}: {message}")]
    SyntaxError { line: usize, message: String },
    #[error("missing mandatory configuration field: {0}")]
    MissingField(String),
    #[error("invalid section header: {0}")]
    InvalidSection(String),
}

/// Configuration section of a `.gdextension` manifest file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigurationSection {
    pub entry_symbol: String,
    pub compatibility_minimum: String,
    pub reloadable: bool,
    pub android_aar_plugin: bool,
}

impl Default for ConfigurationSection {
    fn default() -> Self {
        Self {
            entry_symbol: String::new(),
            compatibility_minimum: "4.1".to_string(),
            reloadable: true,
            android_aar_plugin: false,
        }
    }
}

/// Fully parsed `.gdextension` file model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct GdExtensionManifest {
    pub configuration: ConfigurationSection,
    pub libraries: BTreeMap<String, String>,
    pub dependencies: BTreeMap<String, Vec<String>>,
}

impl GdExtensionManifest {
    /// Creates a new manifest with default configuration and entry symbol.
    pub fn new(entry_symbol: impl Into<String>) -> Self {
        Self {
            configuration: ConfigurationSection {
                entry_symbol: entry_symbol.into(),
                ..Default::default()
            },
            libraries: BTreeMap::new(),
            dependencies: BTreeMap::new(),
        }
    }

    /// Builder helper to set minimum compatibility version.
    pub fn with_compatibility_minimum(mut self, ver: impl Into<String>) -> Self {
        self.configuration.compatibility_minimum = ver.into();
        self
    }

    /// Builder helper to add a library mapping.
    pub fn with_library(mut self, platform: impl Into<String>, path: impl Into<String>) -> Self {
        self.libraries.insert(platform.into(), path.into());
        self
    }

    /// Builder helper to add a dependency mapping.
    pub fn with_dependency(mut self, platform: impl Into<String>, dep_path: impl Into<String>) -> Self {
        self.dependencies
            .entry(platform.into())
            .or_default()
            .push(dep_path.into());
        self
    }
}

/// Parses a `.gdextension` INI text into a structured [`GdExtensionManifest`].
pub fn parse_gdextension_manifest(input: &str) -> Result<GdExtensionManifest, ManifestError> {
    let mut manifest = GdExtensionManifest::default();
    let mut current_section: Option<String> = None;

    for (line_idx, raw_line) in input.lines().enumerate() {
        let line_num = line_idx + 1;
        let line = strip_comments(raw_line).trim();
        if line.is_empty() {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            let section_name = line[1..line.len() - 1].trim().to_lowercase();
            current_section = Some(section_name);
            continue;
        }

        let Some(ref section) = current_section else {
            return Err(ManifestError::SyntaxError {
                line: line_num,
                message: "key-value pair outside of section".to_string(),
            });
        };

        let Some((key_part, val_part)) = line.split_once('=') else {
            return Err(ManifestError::SyntaxError {
                line: line_num,
                message: format!("expected '=' in assignment: '{line}'"),
            });
        };

        let key = key_part.trim().to_string();
        let raw_val = val_part.trim();

        match section.as_str() {
            "configuration" => match key.as_str() {
                "entry_symbol" => {
                    manifest.configuration.entry_symbol = parse_string_value(raw_val);
                }
                "compatibility_minimum" => {
                    manifest.configuration.compatibility_minimum = parse_string_value(raw_val);
                }
                "reloadable" => {
                    manifest.configuration.reloadable = parse_bool_value(raw_val, true);
                }
                "android_aar_plugin" => {
                    manifest.configuration.android_aar_plugin = parse_bool_value(raw_val, false);
                }
                _ => {}
            },
            "libraries" => {
                let path = parse_string_value(raw_val);
                manifest.libraries.insert(key, path);
            }
            "dependencies" => {
                let deps = parse_dependencies_value(raw_val);
                manifest.dependencies.insert(key, deps);
            }
            _ => {
                // Ignore unrecognized user sections
            }
        }
    }

    if manifest.configuration.entry_symbol.is_empty() {
        return Err(ManifestError::MissingField("entry_symbol".to_string()));
    }

    Ok(manifest)
}

/// Serializes a [`GdExtensionManifest`] to `.gdextension` INI format string.
pub fn emit_gdextension_manifest(manifest: &GdExtensionManifest) -> String {
    let mut out = String::new();

    out.push_str("[configuration]\n");
    out.push_str(&format!(
        "entry_symbol = \"{}\"\n",
        manifest.configuration.entry_symbol
    ));
    out.push_str(&format!(
        "compatibility_minimum = \"{}\"\n",
        manifest.configuration.compatibility_minimum
    ));
    out.push_str(&format!(
        "reloadable = {}\n",
        manifest.configuration.reloadable
    ));
    if manifest.configuration.android_aar_plugin {
        out.push_str("android_aar_plugin = true\n");
    }
    out.push('\n');

    if !manifest.libraries.is_empty() {
        out.push_str("[libraries]\n");
        for (platform, path) in &manifest.libraries {
            out.push_str(&format!("{platform} = \"{path}\"\n"));
        }
        out.push('\n');
    }

    if !manifest.dependencies.is_empty() {
        out.push_str("[dependencies]\n");
        for (platform, deps) in &manifest.dependencies {
            if deps.len() == 1 {
                out.push_str(&format!("{platform} = [\"{deps_0}\"]\n", deps_0 = deps[0]));
            } else {
                let formatted: Vec<String> = deps.iter().map(|d| format!("\"{d}\"")).collect();
                out.push_str(&format!("{platform} = [{}]\n", formatted.join(", ")));
            }
        }
        out.push('\n');
    }

    out
}

/// Resolves the appropriate dynamic library path for the host system.
///
/// Tries exact match `<os>.<config>.<arch>`, then arch-agnostic `<os>.<config>`,
/// config-agnostic `<os>.<arch>`, general `<os>`, and alternate configuration fallbacks.
pub fn resolve_library_for_host(
    manifest: &GdExtensionManifest,
    os: &str,
    arch: &str,
    is_debug: bool,
) -> Option<String> {
    let norm_os = normalize_os(os);
    let norm_arch = normalize_arch(arch);
    let config = if is_debug { "debug" } else { "release" };
    let alt_config = if is_debug { "release" } else { "debug" };

    let candidate_keys = [
        format!("{norm_os}.{config}.{norm_arch}"),
        format!("{norm_os}.{config}"),
        format!("{norm_os}.{norm_arch}"),
        norm_os.clone(),
        format!("{norm_os}.{alt_config}.{norm_arch}"),
        format!("{norm_os}.{alt_config}"),
    ];

    for key in &candidate_keys {
        if let Some(path) = manifest.libraries.get(key) {
            return Some(path.clone());
        }
    }

    None
}

fn normalize_os(os: &str) -> String {
    let lower = os.to_lowercase();
    match lower.as_str() {
        "darwin" | "macos" | "osx" => "macos".to_string(),
        "win" | "win32" | "windows" => "windows".to_string(),
        "linux" => "linux".to_string(),
        "android" => "android".to_string(),
        "ios" => "ios".to_string(),
        "web" | "wasm" => "web".to_string(),
        _ => lower,
    }
}

fn normalize_arch(arch: &str) -> String {
    let lower = arch.to_lowercase();
    match lower.as_str() {
        "aarch64" | "arm64" => "arm64".to_string(),
        "x86_64" | "amd64" => "x86_64".to_string(),
        "x86" | "i686" | "x86_32" => "x86_32".to_string(),
        "arm" | "armv7" => "arm32".to_string(),
        "riscv64" | "rv64" => "rv64".to_string(),
        "wasm32" => "wasm32".to_string(),
        _ => lower,
    }
}

fn strip_comments(line: &str) -> &str {
    let mut in_quote = false;
    let mut quote_char = '"';
    for (i, c) in line.char_indices() {
        if (c == '"' || c == '\'') && !in_quote {
            in_quote = true;
            quote_char = c;
        } else if in_quote && c == quote_char {
            in_quote = false;
        } else if !in_quote && (c == '#' || c == ';') {
            return &line[..i];
        }
    }
    line
}

fn parse_string_value(val: &str) -> String {
    let trimmed = val.trim();
    if (trimmed.starts_with('"') && trimmed.ends_with('"'))
        || (trimmed.starts_with('\'') && trimmed.ends_with('\''))
    {
        if trimmed.len() >= 2 {
            trimmed[1..trimmed.len() - 1].to_string()
        } else {
            String::new()
        }
    } else {
        trimmed.to_string()
    }
}

fn parse_bool_value(val: &str, default: bool) -> bool {
    match val.trim().to_lowercase().as_str() {
        "true" | "1" | "yes" => true,
        "false" | "0" | "no" => false,
        _ => default,
    }
}

fn parse_dependencies_value(val: &str) -> Vec<String> {
    let trimmed = val.trim();
    if trimmed.starts_with('{') && trimmed.ends_with('}') {
        // Dict format: { "res://path": "" }
        let inner = &trimmed[1..trimmed.len() - 1];
        let mut deps = Vec::new();
        for item in inner.split(',') {
            if let Some((k, _)) = item.split_once(':') {
                let dep = parse_string_value(k);
                if !dep.is_empty() {
                    deps.push(dep);
                }
            }
        }
        deps
    } else if trimmed.starts_with('[') && trimmed.ends_with(']') {
        // Array format: ["res://path", "res://path2"]
        let inner = &trimmed[1..trimmed.len() - 1];
        let mut deps = Vec::new();
        for item in inner.split(',') {
            let dep = parse_string_value(item);
            if !dep.is_empty() {
                deps.push(dep);
            }
        }
        deps
    } else {
        let dep = parse_string_value(trimmed);
        if dep.is_empty() {
            Vec::new()
        } else {
            vec![dep]
        }
    }
}
