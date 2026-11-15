//! GDScript-to-IR compatibility front-end (Track D RustPorts `[mechanical]` wave).
//! Allows existing Godot 4 `.tscn` + `.gd` projects to open directly in Foreplay without a second VM.
//!
//! Mirrors `GDScriptParser` (`modules/gdscript/gdscript_parser.cpp`).

pub mod vm;

pub use vm::{GdVm, VmError, VmExecutionResult};

use foreplay_ir::{FeelSpecKnob, IrValue};
use serde::{Deserialize, Serialize};

/// Extracted `@export` metadata from a `.gd` script header.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct GdExportMetadata {
    pub class_name: Option<String>,
    pub extends: String,
    pub knobs: Vec<FeelSpecKnob>,
}

/// Numeric range specification for an exported variable, extracted from `@export_range(min, max[, step])`.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct GdRangeHint {
    pub min: f64,
    pub max: f64,
    pub step: Option<f64>,
}

/// An exported variable declared with `@export` or `@export_range`.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct GdExportVar {
    pub name: String,
    pub type_hint: Option<String>,
    pub default_value: Option<IrValue>,
    pub range_hint: Option<GdRangeHint>,
}

/// A signal declaration declared with `signal <name>(...)`.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct GdSignalDef {
    pub name: String,
    pub parameters: Vec<String>,
}

/// A function declaration declared with `func <name>(...) -> ReturnType:`.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct GdFunctionDef {
    pub name: String,
    pub parameters: Vec<String>,
    pub return_type: Option<String>,
    pub is_lifecycle: bool,
}

/// Complete structural model of a parsed GDScript file.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct GdScriptModule {
    pub class_name: Option<String>,
    pub extends: String,
    pub exports: Vec<GdExportVar>,
    pub feel_knobs: Vec<FeelSpecKnob>,
    pub signals: Vec<GdSignalDef>,
    pub functions: Vec<GdFunctionDef>,
}

impl GdScriptModule {
    pub fn export_metadata(&self) -> GdExportMetadata {
        GdExportMetadata {
            class_name: self.class_name.clone(),
            extends: self.extends.clone(),
            knobs: self.feel_knobs.clone(),
        }
    }
}

/// Errors occurring during GDScript header and signature parsing.
#[derive(Clone, PartialEq, Eq, Debug, thiserror::Error)]
pub enum GdParseError {
    #[error("syntax error at line {line}: {message}")]
    SyntaxError { line: usize, message: String },
    #[error("invalid number '{literal}' at line {line}")]
    InvalidNumber { line: usize, literal: String },
}

/// Parses Godot 4 GDScript source code into a `GdScriptModule`.
///
/// Mirrors `GDScriptParser::parse` header and declaration extraction.
pub fn parse_gdscript(source: &str) -> Result<GdScriptModule, GdParseError> {
    let mut class_name: Option<String> = None;
    let mut extends = String::from("Node");
    let mut exports: Vec<GdExportVar> = Vec::new();
    let mut feel_knobs: Vec<FeelSpecKnob> = Vec::new();
    let mut signals: Vec<GdSignalDef> = Vec::new();
    let mut functions: Vec<GdFunctionDef> = Vec::new();

    let lines: Vec<&str> = source.lines().collect();
    let mut i = 0;
    let mut pending_range: Option<GdRangeHint> = None;
    let mut pending_export = false;

    while i < lines.len() {
        let raw_line = lines[i];
        let line_num = i + 1;
        let line = strip_comments(raw_line).trim();

        if line.is_empty() {
            i += 1;
            continue;
        }

        // 1. class_name <Ident>
        if let Some(rest) = line.strip_prefix("class_name")
            && rest.chars().next().is_none_or(char::is_whitespace)
        {
            let name = rest.trim();
            if !name.is_empty() {
                class_name = Some(name.to_string());
            }
            pending_range = None;
            pending_export = false;
            i += 1;
            continue;
        }

        // 2. extends <Ident>
        if let Some(rest) = line.strip_prefix("extends")
            && rest.chars().next().is_none_or(char::is_whitespace)
        {
            let base = rest.trim().trim_matches('"');
            if !base.is_empty() {
                extends = base.to_string();
            }
            pending_range = None;
            pending_export = false;
            i += 1;
            continue;
        }

        // 3. @export_range(...) decorator
        if let Some(rest) = line.strip_prefix("@export_range(") {
            let close_paren = rest.find(')').ok_or_else(|| GdParseError::SyntaxError {
                line: line_num,
                message: "unclosed @export_range(".into(),
            })?;
            let range_args = &rest[..close_paren];
            let hint = parse_range_args(range_args, line_num)?;

            let remaining = rest[close_paren + 1..].trim();
            if let Some(var_decl) = remaining.strip_prefix("var ") {
                // Combined on same line
                let export_var = parse_var_declaration(var_decl, Some(hint), line_num)?;
                if let Some(knob) = build_feel_knob(&export_var) {
                    feel_knobs.push(knob);
                }
                exports.push(export_var);
                pending_range = None;
            } else {
                pending_range = Some(hint);
            }
            i += 1;
            continue;
        }

        // 4. Standalone @export decorator
        if line == "@export" {
            pending_export = true;
            i += 1;
            continue;
        }

        // Combined `@export var ...` on same line
        if let Some(rest) = line.strip_prefix("@export ") {
            let trimmed = rest.trim();
            if let Some(var_decl) = trimmed.strip_prefix("var ") {
                let export_var = parse_var_declaration(var_decl, None, line_num)?;
                exports.push(export_var);
            }
            pending_export = false;
            pending_range = None;
            i += 1;
            continue;
        }

        // 5. `var ...` declaration with pending decorator
        if let Some(rest) = line.strip_prefix("var ") {
            if pending_range.is_some() || pending_export {
                let range = pending_range.take();
                let export_var = parse_var_declaration(rest.trim(), range, line_num)?;
                if let Some(knob) = build_feel_knob(&export_var) {
                    feel_knobs.push(knob);
                }
                exports.push(export_var);
                pending_export = false;
            }
            i += 1;
            continue;
        }

        // Reset export decorators if line is not a var declaration
        pending_range = None;
        pending_export = false;

        // 6. `signal <name>(...)` or `signal <name>`
        if let Some(rest) = line.strip_prefix("signal")
            && rest.chars().next().is_none_or(char::is_whitespace)
        {
            let sig_decl = rest.trim();
            let sig = parse_signal_declaration(sig_decl)?;
            signals.push(sig);
            i += 1;
            continue;
        }

        // 7. `func <name>(...) -> Type:`
        if let Some(rest) = line.strip_prefix("func")
            && rest.chars().next().is_none_or(char::is_whitespace)
        {
            let func_decl = rest.trim();
            let func = parse_func_declaration(func_decl)?;
            functions.push(func);
            i += 1;
            continue;
        }

        i += 1;
    }

    Ok(GdScriptModule {
        class_name,
        extends,
        exports,
        feel_knobs,
        signals,
        functions,
    })
}

fn strip_comments(line: &str) -> &str {
    let mut in_quote = false;
    let mut quote_char = '"';
    let bytes = line.as_bytes();

    for (idx, &b) in bytes.iter().enumerate() {
        if (b == b'"' || b == b'\'') && (idx == 0 || bytes[idx - 1] != b'\\') {
            if !in_quote {
                in_quote = true;
                quote_char = b as char;
            } else if b as char == quote_char {
                in_quote = false;
            }
        } else if b == b'#' && !in_quote {
            return &line[..idx];
        }
    }
    line
}

fn parse_range_args(args_str: &str, line_num: usize) -> Result<GdRangeHint, GdParseError> {
    let parts: Vec<&str> = args_str.split(',').map(|s| s.trim()).collect();
    if parts.len() < 2 {
        return Err(GdParseError::SyntaxError {
            line: line_num,
            message: format!("@export_range requires at least min and max, got '{args_str}'"),
        });
    }

    let min = parts[0]
        .parse::<f64>()
        .map_err(|_| GdParseError::InvalidNumber {
            line: line_num,
            literal: parts[0].to_string(),
        })?;
    let max = parts[1]
        .parse::<f64>()
        .map_err(|_| GdParseError::InvalidNumber {
            line: line_num,
            literal: parts[1].to_string(),
        })?;
    let step = if parts.len() >= 3 {
        Some(
            parts[2]
                .parse::<f64>()
                .map_err(|_| GdParseError::InvalidNumber {
                    line: line_num,
                    literal: parts[2].to_string(),
                })?,
        )
    } else {
        None
    };

    Ok(GdRangeHint { min, max, step })
}

fn parse_var_declaration(
    var_str: &str,
    range_hint: Option<GdRangeHint>,
    line_num: usize,
) -> Result<GdExportVar, GdParseError> {
    // Examples:
    // "max_speed: float = 14.5"
    // "skater_name: String = \"SanVan\""
    // "is_grounded = true"
    // "gravity: float"
    let (name_part, default_part) = if let Some(idx) = var_str.find('=') {
        (&var_str[..idx].trim(), Some(var_str[idx + 1..].trim()))
    } else {
        (&var_str.trim(), None)
    };

    let (name, type_hint) = if let Some(idx) = name_part.find(':') {
        (
            name_part[..idx].trim().to_string(),
            Some(name_part[idx + 1..].trim().to_string()),
        )
    } else {
        (name_part.trim().to_string(), None)
    };

    let default_value = if let Some(raw_val) = default_part {
        Some(parse_simple_gd_value(raw_val, line_num)?)
    } else {
        None
    };

    Ok(GdExportVar {
        name,
        type_hint,
        default_value,
        range_hint,
    })
}

fn build_feel_knob(export_var: &GdExportVar) -> Option<FeelSpecKnob> {
    let range = export_var.range_hint.as_ref()?;
    let default_val = match &export_var.default_value {
        Some(IrValue::F64(v)) => *v,
        Some(IrValue::I64(v)) => *v as f64,
        _ => range.min,
    };

    Some(FeelSpecKnob {
        key: export_var.name.clone(),
        intent: export_var.name.clone(),
        unit: String::new(),
        min: range.min,
        max: range.max,
        default_value: default_val,
    })
}

fn parse_simple_gd_value(val_str: &str, line_num: usize) -> Result<IrValue, GdParseError> {
    let s = val_str.trim();
    if s == "true" {
        return Ok(IrValue::Bool(true));
    }
    if s == "false" {
        return Ok(IrValue::Bool(false));
    }
    if s == "null" {
        return Ok(IrValue::Nil);
    }

    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        let inner = &s[1..s.len() - 1];
        return Ok(IrValue::String(inner.to_string()));
    }

    if s.starts_with("Vector3(") && s.ends_with(')') {
        let inner = &s["Vector3(".len()..s.len() - 1];
        let mut nums = Vec::new();
        for p in inner.split(',') {
            nums.push(p.trim().parse::<f32>().map_err(|_| {
                GdParseError::InvalidNumber {
                    line: line_num,
                    literal: p.trim().to_string(),
                }
            })?);
        }
        if nums.len() == 3 {
            return Ok(IrValue::Vec3([nums[0], nums[1], nums[2]]));
        }
    }

    if s.starts_with("Vector2(") && s.ends_with(')') {
        let inner = &s["Vector2(".len()..s.len() - 1];
        let mut nums = Vec::new();
        for p in inner.split(',') {
            nums.push(p.trim().parse::<f32>().map_err(|_| {
                GdParseError::InvalidNumber {
                    line: line_num,
                    literal: p.trim().to_string(),
                }
            })?);
        }
        if nums.len() == 2 {
            return Ok(IrValue::Vec2([nums[0], nums[1]]));
        }
    }

    if let Ok(i) = s.parse::<i64>() {
        return Ok(IrValue::I64(i));
    }
    if let Ok(f) = s.parse::<f64>() {
        return Ok(IrValue::F64(f));
    }

    Ok(IrValue::String(s.to_string()))
}

fn parse_signal_declaration(sig_str: &str) -> Result<GdSignalDef, GdParseError> {
    let s = sig_str.trim();
    if let Some(open) = s.find('(') {
        let name = s[..open].trim().to_string();
        let close = s.rfind(')').unwrap_or(s.len());
        let params_str = &s[open + 1..close].trim();
        let parameters = if params_str.is_empty() {
            Vec::new()
        } else {
            params_str
                .split(',')
                .map(|p| {
                    let clean = p.trim();
                    if let Some(idx) = clean.find(':') {
                        clean[..idx].trim().to_string()
                    } else if let Some(idx) = clean.find('=') {
                        clean[..idx].trim().to_string()
                    } else {
                        clean.to_string()
                    }
                })
                .filter(|p| !p.is_empty())
                .collect()
        };
        Ok(GdSignalDef { name, parameters })
    } else {
        Ok(GdSignalDef {
            name: s.to_string(),
            parameters: Vec::new(),
        })
    }
}

fn parse_func_declaration(func_str: &str) -> Result<GdFunctionDef, GdParseError> {
    // "func _ready() -> void:"
    // "func _physics_process(delta: float):"
    // "func custom_handler(trick: String, score: int):"
    let clean = func_str.trim().trim_end_matches(':');
    let (sig_part, ret_part) = if let Some(arrow_idx) = clean.find("->") {
        (
            clean[..arrow_idx].trim(),
            Some(clean[arrow_idx + 2..].trim().to_string()),
        )
    } else {
        (clean.trim(), None)
    };

    let (name, parameters) = if let Some(open) = sig_part.find('(') {
        let name = sig_part[..open].trim().to_string();
        let close = sig_part.rfind(')').unwrap_or(sig_part.len());
        let params_str = &sig_part[open + 1..close].trim();
        let params = if params_str.is_empty() {
            Vec::new()
        } else {
            params_str
                .split(',')
                .map(|p| {
                    let part = p.trim();
                    if let Some(idx) = part.find(':') {
                        part[..idx].trim().to_string()
                    } else {
                        part.to_string()
                    }
                })
                .filter(|p| !p.is_empty())
                .collect()
        };
        (name, params)
    } else {
        (sig_part.to_string(), Vec::new())
    };

    let is_lifecycle = matches!(
        name.as_str(),
        "_ready"
            | "_enter_tree"
            | "_exit_tree"
            | "_process"
            | "_physics_process"
            | "_input"
            | "_unhandled_input"
            | "_draw"
            | "_init"
    );

    Ok(GdFunctionDef {
        name,
        parameters,
        return_type: ret_part,
        is_lifecycle,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_GDSCRIPT: &str = r#"
class_name SkaterController
extends CharacterBody3D

# Tunable skate feel knobs
@export_range(0.0, 30.0, 0.5) var max_speed: float = 14.5
@export_range(1.0, 15.0, 0.2)
var ollie_impulse: float = 6.8
@export var skater_name: String = "SanVanSkater"
@export var is_grounded: bool = true

signal trick_landed(trick_name: String, score: int)
signal bailed()

func _ready() -> void:
    pass

func _physics_process(delta: float) -> void:
    pass

func _on_trick_landed(trick_name: String, score: int):
    pass
"#;

    #[test]
    fn parses_gdscript_exports_knobs_and_signals() {
        let module = parse_gdscript(SAMPLE_GDSCRIPT).expect("GDScript parse succeeds");
        assert_eq!(module.class_name.as_deref(), Some("SkaterController"));
        assert_eq!(module.extends, "CharacterBody3D");

        // FeelSpec knobs generated from @export_range
        assert_eq!(module.feel_knobs.len(), 2);
        let speed_knob = &module.feel_knobs[0];
        assert_eq!(speed_knob.key, "max_speed");
        assert_eq!(speed_knob.min, 0.0);
        assert_eq!(speed_knob.max, 30.0);
        assert_eq!(speed_knob.default_value, 14.5);

        let ollie_knob = &module.feel_knobs[1];
        assert_eq!(ollie_knob.key, "ollie_impulse");
        assert_eq!(ollie_knob.min, 1.0);
        assert_eq!(ollie_knob.max, 15.0);
        assert_eq!(ollie_knob.default_value, 6.8);

        // Exports
        assert_eq!(module.exports.len(), 4);
        assert_eq!(module.exports[2].name, "skater_name");
        assert_eq!(
            module.exports[2].default_value,
            Some(IrValue::String("SanVanSkater".into()))
        );

        // Signals
        assert_eq!(module.signals.len(), 2);
        assert_eq!(module.signals[0].name, "trick_landed");
        assert_eq!(module.signals[0].parameters, vec!["trick_name", "score"]);
        assert_eq!(module.signals[1].name, "bailed");
        assert!(module.signals[1].parameters.is_empty());

        // Functions
        assert_eq!(module.functions.len(), 3);
        assert_eq!(module.functions[0].name, "_ready");
        assert!(module.functions[0].is_lifecycle);
        assert_eq!(module.functions[1].name, "_physics_process");
        assert!(module.functions[1].is_lifecycle);
        assert_eq!(module.functions[1].parameters, vec!["delta"]);
        assert_eq!(module.functions[2].name, "_on_trick_landed");
        assert!(!module.functions[2].is_lifecycle);

        // Export metadata contract
        let meta = module.export_metadata();
        assert_eq!(meta.class_name.as_deref(), Some("SkaterController"));
        assert_eq!(meta.extends, "CharacterBody3D");
        assert_eq!(meta.knobs.len(), 2);
    }

    #[test]
    fn parses_starter_template_skater_controller_gd_end_to_end() {
        let gd_src = include_str!("../../../templates/godot-migration/skater_controller.gd");
        let module = parse_gdscript(gd_src).expect("template .gd parse must succeed");
        assert_eq!(module.class_name.as_deref(), Some("SkaterController"));
        assert_eq!(module.extends, "CharacterBody3D");
        assert_eq!(module.feel_knobs.len(), 3);
        assert!(module.feel_knobs.iter().any(|k| k.key == "max_speed"));
        assert!(module.feel_knobs.iter().any(|k| k.key == "ollie_impulse"));
        assert!(module.feel_knobs.iter().any(|k| k.key == "grind_balance_damping"));
        assert!(module.signals.iter().any(|s| s.name == "trick_landed"));
        assert!(module.signals.iter().any(|s| s.name == "bailed"));
    }

    #[test]
    fn parses_starter_template_player_2d_gd_end_to_end() {
        let gd_src = include_str!("../../../templates/2d-platformer/player_2d.gd");
        let module = parse_gdscript(gd_src).expect("2d platformer .gd parse must succeed");
        assert_eq!(module.class_name.as_deref(), Some("Player2DController"));
        assert_eq!(module.extends, "CharacterBody2D");
        assert_eq!(module.feel_knobs.len(), 4);
        assert!(module.feel_knobs.iter().any(|k| k.key == "speed"));
        assert!(module.feel_knobs.iter().any(|k| k.key == "jump_velocity"));
        assert!(module.feel_knobs.iter().any(|k| k.key == "gravity"));
        assert!(module.feel_knobs.iter().any(|k| k.key == "coyote_time"));
        assert!(module.signals.iter().any(|s| s.name == "jumped"));
        assert!(module.signals.iter().any(|s| s.name == "landed"));
        assert!(module.signals.iter().any(|s| s.name == "goal_reached"));
    }
}
