//! GDScript-to-IR compatibility front-end (Track D RustPorts `[mechanical]` wave).
//! Allows existing Godot 4 `.tscn` + `.gd` projects to open directly in Foreplay without a second VM.
//!
//! Mirrors `GDScriptParser` (`modules/gdscript/gdscript_parser.cpp`).

pub mod builtins_catalog;
pub mod callable;
pub mod coroutines;
pub mod lsp;
pub mod types;
pub mod vm;

pub use builtins_catalog::{GdBuiltinDoc, builtin_catalog};
pub use callable::GdCallable;
pub use coroutines::{AwaitTarget, CoroutineId, GdCoroutineFrame, GdCoroutineOutcome, GdStackFrame};
pub use lsp::{
    GdCompletionItem, GdCompletionKind, GdDiagnostic, GdDiagnosticSeverity, GdDocumentAnalysis,
    GdHoverInfo, GdLspAnalyzer, GdLocation, GdPosition, GdRange, GdSymbol, GdSymbolKind,
};
pub use types::{GdType, type_string, typeof_code};
pub use vm::{
    AssignOp, AssignTarget, BinOp, Expr, FunctionAst, GdPattern, GdVm, MatchArm, Stmt, UnaryOp,
    VmError, VmExecutionResult,
};

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

    if s.starts_with('[') && s.ends_with(']') {
        let inner = &s[1..s.len() - 1].trim();
        if inner.is_empty() {
            return Ok(IrValue::Array(Vec::new()));
        }
        let mut items = Vec::new();
        for p in inner.split(',') {
            let clean = p.trim();
            if !clean.is_empty() {
                items.push(parse_simple_gd_value(clean, line_num)?);
            }
        }
        return Ok(IrValue::Array(items));
    }

    if s.starts_with('{') && s.ends_with('}') {
        let inner = &s[1..s.len() - 1].trim();
        if inner.is_empty() {
            return Ok(IrValue::Map(std::collections::BTreeMap::new()));
        }
        let mut map = std::collections::BTreeMap::new();
        for p in inner.split(',') {
            let clean = p.trim();
            if let Some(col) = clean.find(':') {
                let k = clean[..col].trim().trim_matches('"').trim_matches('\'');
                let v = parse_simple_gd_value(clean[col + 1..].trim(), line_num)?;
                map.insert(k.to_string(), v);
            }
        }
        return Ok(IrValue::Map(map));
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
    use foreplay_ir::IrNode;

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

    fn make_test_node(name: &str) -> IrNode {
        IrNode {
            id: foreplay_ir::NodeId::new(1, 1).unwrap(),
            name: name.into(),
            class_name: "Node".into(),
            parent: None,
            instance_scene: None,
            properties: std::collections::BTreeMap::new(),
            groups: Vec::new(),
        }
    }

    // 1. Builtin Catalog Count & Structure
    #[test]
    fn test_builtin_catalog_has_over_400_entries() {
        let cat = builtin_catalog();
        assert!(cat.len() >= 400, "Catalog must have at least 400 entries, found {}", cat.len());
        assert!(cat.iter().any(|d| d.name == "sin" && d.receiver.is_none()));
        assert!(cat.iter().any(|d| d.name == "to_upper" && d.receiver.as_deref() == Some("String")));
        assert!(cat.iter().any(|d| d.name == "map" && d.receiver.as_deref() == Some("Array")));
        assert!(cat.iter().any(|d| d.name == "merge" && d.receiver.as_deref() == Some("Dictionary")));
        assert!(cat.iter().any(|d| d.name == "normalized" && d.receiver.as_deref() == Some("Vector3")));
        assert!(cat.iter().any(|d| d.name == "create_timer" && d.receiver.as_deref() == Some("Node")));
    }

    // 2. Global Math Builtins Execution
    #[test]
    fn test_global_math_builtins_eval() {
        let src = r#"
var res_sin = 0.0
var res_clamp = 0.0
var res_lerp = 0.0
var res_remap = 0.0
var res_smooth = 0.0
var res_move = 0.0

func _ready():
    res_sin = sin(0.0)
    res_clamp = clamp(15.0, 0.0, 10.0)
    res_lerp = lerp(10.0, 20.0, 0.5)
    res_remap = remap(5.0, 0.0, 10.0, 0.0, 100.0)
    res_smooth = smoothstep(0.0, 1.0, 0.5)
    res_move = move_toward(5.0, 10.0, 2.0)
"#;
        let mut vm = GdVm::from_source(src).expect("compile");
        let mut node = make_test_node("MathTest");
        let res = vm.ready(&mut node).expect("ready");
        assert!(res.steps_executed > 0);
        assert_eq!(vm.member_vars.get("res_sin"), Some(&IrValue::F64(0.0)));
        assert_eq!(vm.member_vars.get("res_clamp"), Some(&IrValue::F64(10.0)));
        assert_eq!(vm.member_vars.get("res_lerp"), Some(&IrValue::F64(15.0)));
        assert_eq!(vm.member_vars.get("res_remap"), Some(&IrValue::F64(50.0)));
        assert_eq!(vm.member_vars.get("res_smooth"), Some(&IrValue::F64(0.5)));
        assert_eq!(vm.member_vars.get("res_move"), Some(&IrValue::F64(7.0)));
    }

    // 3. Global Utility Builtins Execution
    #[test]
    fn test_global_utility_builtins() {
        let src = r#"
var type_code = 0
var type_name = ""
var stringified = ""
var cast_int = 0
var cast_float = 0.0
var arr_len = 0
var int_range = []

func _ready():
    type_code = typeof("hello")
    type_name = type_string(type_code)
    stringified = str(42)
    cast_int = int("123")
    cast_float = float("3.25")
    arr_len = len([1, 2, 3])
    int_range = range(0, 5)
"#;
        let mut vm = GdVm::from_source(src).expect("compile");
        let mut node = make_test_node("UtilTest");
        vm.ready(&mut node).expect("ready");
        assert_eq!(vm.member_vars.get("type_code"), Some(&IrValue::I64(4))); // TYPE_STRING = 4
        assert_eq!(vm.member_vars.get("type_name"), Some(&IrValue::String("String".into())));
        assert_eq!(vm.member_vars.get("stringified"), Some(&IrValue::String("42".into())));
        assert_eq!(vm.member_vars.get("cast_int"), Some(&IrValue::I64(123)));
        assert_eq!(vm.member_vars.get("cast_float"), Some(&IrValue::F64(3.25)));
        assert_eq!(vm.member_vars.get("arr_len"), Some(&IrValue::I64(3)));
        assert_eq!(
            vm.member_vars.get("int_range"),
            Some(&IrValue::Array(vec![
                IrValue::I64(0),
                IrValue::I64(1),
                IrValue::I64(2),
                IrValue::I64(3),
                IrValue::I64(4)
            ]))
        );
    }

    // 4. String Methods Execution
    #[test]
    fn test_string_methods_suite() {
        let src = r#"
var upper = ""
var snake = ""
var sub = ""
var joined = ""
var path = ""
var is_beg = false

func _ready():
    var txt = "godot_engine"
    upper = txt.to_upper()
    snake = "PlayerController".to_snake_case()
    sub = txt.substr(0, 5)
    joined = ",".join(["a", "b", "c"])
    path = "res://scenes".path_join("player.tscn")
    is_beg = txt.begins_with("godot")
"#;
        let mut vm = GdVm::from_source(src).expect("compile");
        let mut node = make_test_node("StrTest");
        vm.ready(&mut node).expect("ready");
        assert_eq!(vm.member_vars.get("upper"), Some(&IrValue::String("GODOT_ENGINE".into())));
        assert_eq!(vm.member_vars.get("snake"), Some(&IrValue::String("player_controller".into())));
        assert_eq!(vm.member_vars.get("sub"), Some(&IrValue::String("godot".into())));
        assert_eq!(vm.member_vars.get("joined"), Some(&IrValue::String("a,b,c".into())));
        assert_eq!(vm.member_vars.get("path"), Some(&IrValue::String("res://scenes/player.tscn".into())));
        assert_eq!(vm.member_vars.get("is_beg"), Some(&IrValue::Bool(true)));
    }

    // 5. Array Methods (Mutation, Slicing, Sorting)
    #[test]
    fn test_array_methods_basic() {
        let src = r#"
var list = [3, 1, 4]
var popped = 0
var has_item = false

func _ready():
    list.push_back(2)
    list.sort()
    popped = list.pop_back()
    has_item = list.has(3)
"#;
        let mut vm = GdVm::from_source(src).expect("compile");
        let mut node = make_test_node("ArrTest");
        vm.ready(&mut node).expect("ready");
        assert_eq!(vm.member_vars.get("popped"), Some(&IrValue::I64(4)));
        assert_eq!(vm.member_vars.get("has_item"), Some(&IrValue::Bool(true)));
        assert_eq!(
            vm.member_vars.get("list"),
            Some(&IrValue::Array(vec![
                IrValue::I64(1),
                IrValue::I64(2),
                IrValue::I64(3),
            ]))
        );
    }

    // 6. Higher-Order Array Methods with Lambdas: map, filter, reduce, any, all
    #[test]
    fn test_array_map_filter_reduce_lambdas() {
        let src = r#"
var doubled = []
var evens = []
var sum = 0
var has_large = false
var all_positive = false

func _ready():
    var nums = [1, 2, 3, 4]
    doubled = nums.map(func(x): return x * 2)
    evens = nums.filter(func(x): return x % 2 == 0)
    sum = nums.reduce(func(acc, x): return acc + x, 0)
    has_large = nums.any(func(x): return x > 3)
    all_positive = nums.all(func(x): return x > 0)
"#;
        let mut vm = GdVm::from_source(src).expect("compile");
        let mut node = make_test_node("HigherOrderTest");
        vm.ready(&mut node).expect("ready");

        assert_eq!(
            vm.member_vars.get("doubled"),
            Some(&IrValue::Array(vec![
                IrValue::I64(2),
                IrValue::I64(4),
                IrValue::I64(6),
                IrValue::I64(8),
            ]))
        );
        assert_eq!(
            vm.member_vars.get("evens"),
            Some(&IrValue::Array(vec![
                IrValue::I64(2),
                IrValue::I64(4),
            ]))
        );
        assert_eq!(vm.member_vars.get("sum"), Some(&IrValue::I64(10)));
        assert_eq!(vm.member_vars.get("has_large"), Some(&IrValue::Bool(true)));
        assert_eq!(vm.member_vars.get("all_positive"), Some(&IrValue::Bool(true)));
    }

    // 7. Dictionary Methods
    #[test]
    fn test_dictionary_methods_suite() {
        let src = r#"
var dict = {"hp": 100, "mana": 50}
var has_hp = false
var erased = false
var hp_val = 0

func _ready():
    has_hp = dict.has("hp")
    hp_val = dict.get("hp", 0)
    erased = dict.erase("mana")
"#;
        let mut vm = GdVm::from_source(src).expect("compile");
        let mut node = make_test_node("DictTest");
        vm.ready(&mut node).expect("ready");
        assert_eq!(vm.member_vars.get("has_hp"), Some(&IrValue::Bool(true)));
        assert_eq!(vm.member_vars.get("hp_val"), Some(&IrValue::I64(100)));
        assert_eq!(vm.member_vars.get("erased"), Some(&IrValue::Bool(true)));
    }

    // 8. Vector2 Builtin Operations
    #[test]
    fn test_vector2_methods_suite() {
        let src = r#"
var len = 0.0
var normalized_v = Vector2(0.0, 0.0)
var dist = 0.0
var dot_prod = 0.0

func _ready():
    var v1 = Vector2(3.0, 4.0)
    var v2 = Vector2(3.0, 0.0)
    len = v1.length()
    normalized_v = v1.normalized()
    dist = v1.distance_to(v2)
    dot_prod = v1.dot(v2)
"#;
        let mut vm = GdVm::from_source(src).expect("compile");
        let mut node = make_test_node("Vec2Test");
        vm.ready(&mut node).expect("ready");
        assert_eq!(vm.member_vars.get("len"), Some(&IrValue::F64(5.0)));
        assert_eq!(vm.member_vars.get("normalized_v"), Some(&IrValue::Vec2([0.6, 0.8])));
        assert_eq!(vm.member_vars.get("dist"), Some(&IrValue::F64(4.0)));
        assert_eq!(vm.member_vars.get("dot_prod"), Some(&IrValue::F64(9.0)));
    }

    // 9. Vector3 Builtin Operations
    #[test]
    fn test_vector3_methods_suite() {
        let src = r#"
var len = 0.0
var cross_res = Vector3(0.0, 0.0, 0.0)

func _ready():
    var a = Vector3(1.0, 0.0, 0.0)
    var b = Vector3(0.0, 1.0, 0.0)
    len = a.length()
    cross_res = a.cross(b)
"#;
        let mut vm = GdVm::from_source(src).expect("compile");
        let mut node = make_test_node("Vec3Test");
        vm.ready(&mut node).expect("ready");
        assert_eq!(vm.member_vars.get("len"), Some(&IrValue::F64(1.0)));
        assert_eq!(vm.member_vars.get("cross_res"), Some(&IrValue::Vec3([0.0, 0.0, 1.0])));
    }

    // 10. Color Methods
    #[test]
    fn test_color_methods_suite() {
        let src = r#"
var html = ""
var inv = Color(0.0, 0.0, 0.0, 0.0)

func _ready():
    var c = Color(1.0, 0.0, 0.0, 1.0)
    html = c.to_html()
    inv = c.inverted()
"#;
        let mut vm = GdVm::from_source(src).expect("compile");
        let mut node = make_test_node("ColorTest");
        vm.ready(&mut node).expect("ready");
        assert_eq!(vm.member_vars.get("html"), Some(&IrValue::String("ff0000ff".into())));
        assert_eq!(vm.member_vars.get("inv"), Some(&IrValue::Color([0.0, 1.0, 1.0, 1.0])));
    }

    // 11. First-Class Callable: Callable(self, name), bind(), call(), callv()
    #[test]
    fn test_callable_first_class_and_binding() {
        let src = r#"
var res_call = 0
var res_callv = 0
var is_valid_c = false

func multiply(a, b):
    return a * b

func _ready():
    var c = Callable(self, "multiply")
    is_valid_c = c.is_valid()
    var bound_mul = c.bind(3)
    res_call = bound_mul.call(10)
    res_callv = c.callv([7, 6])
"#;
        let mut vm = GdVm::from_source(src).expect("compile");
        let mut node = make_test_node("CallableTest");
        vm.ready(&mut node).expect("ready");
        assert_eq!(vm.member_vars.get("is_valid_c"), Some(&IrValue::Bool(true)));
        assert_eq!(vm.member_vars.get("res_call"), Some(&IrValue::I64(30)));
        assert_eq!(vm.member_vars.get("res_callv"), Some(&IrValue::I64(42)));
    }

    // 12. Inline Lambda Expressions
    #[test]
    fn test_inline_lambda_expression() {
        let src = r#"
var outcome = 0

func _ready():
    var add = func(x, y = 5): return x + y
    outcome = add.call(10)
"#;
        let mut vm = GdVm::from_source(src).expect("compile");
        let mut node = make_test_node("LambdaTest");
        vm.ready(&mut node).expect("ready");
        assert_eq!(vm.member_vars.get("outcome"), Some(&IrValue::I64(15)));
    }

    // 13. `super` Method Call Dispatch
    #[test]
    fn test_super_method_call_dispatch() {
        let base_src = r#"
var base_val = 100

func calculate_bonus(amount):
    base_val += amount
    return base_val
"#;
        let derived_src = r#"
extends BaseClass
var final_score = 0

func _ready():
    final_score = super.calculate_bonus(50) + 5
"#;
        let base_vm = GdVm::from_source(base_src).expect("compile base");
        let mut derived_vm = GdVm::from_source(derived_src).expect("compile derived");
        derived_vm.register_base_class("BaseClass", base_vm);

        let mut node = make_test_node("SuperTest");
        derived_vm.ready(&mut node).expect("ready");
        assert_eq!(derived_vm.member_vars.get("final_score"), Some(&IrValue::I64(155)));
    }

    // 14. `super` Direct Invocation
    #[test]
    fn test_super_direct_invocation() {
        let base_src = r#"
var initialized = false

func _ready():
    initialized = true
"#;
        let derived_src = r#"
extends BaseClass
var ready_called = false

func _ready():
    super()
    ready_called = true
"#;
        let base_vm = GdVm::from_source(base_src).expect("compile base");
        let mut derived_vm = GdVm::from_source(derived_src).expect("compile derived");
        derived_vm.register_base_class("BaseClass", base_vm);

        let mut node = make_test_node("SuperDirectTest");
        derived_vm.ready(&mut node).expect("ready");
        assert_eq!(derived_vm.member_vars.get("ready_called"), Some(&IrValue::Bool(true)));
        assert_eq!(derived_vm.member_vars.get("initialized"), Some(&IrValue::Bool(true)));
    }

    // 15. Godot 4 `match` Statements: Literals & Wildcard
    #[test]
    fn test_match_statement_literals_and_wildcard() {
        let src = r#"
var result1 = ""
var result2 = ""

func eval_code(code):
    match code:
        1:
            return "one"
        2:
            return "two"
        _:
            return "unknown"

func _ready():
    result1 = eval_code(1)
    result2 = eval_code(99)
"#;
        let mut vm = GdVm::from_source(src).expect("compile");
        let mut node = make_test_node("MatchLitTest");
        vm.ready(&mut node).expect("ready");
        assert_eq!(vm.member_vars.get("result1"), Some(&IrValue::String("one".into())));
        assert_eq!(vm.member_vars.get("result2"), Some(&IrValue::String("unknown".into())));
    }

    // 16. Godot 4 `match` Statements: Multiple Comma-Separated Patterns
    #[test]
    fn test_match_multiple_comma_separated_patterns() {
        let src = r#"
var matched_group = ""

func categorize(code):
    match code:
        1, 2, 3:
            return "low"
        4, 5, 6:
            return "mid"
        _:
            return "high"

func _ready():
    matched_group = categorize(2)
"#;
        let mut vm = GdVm::from_source(src).expect("compile");
        let mut node = make_test_node("MatchCommaTest");
        vm.ready(&mut node).expect("ready");
        assert_eq!(vm.member_vars.get("matched_group"), Some(&IrValue::String("low".into())));
    }

    // 17. Godot 4 `match` Statements: Variable Binding & `when` Guards
    #[test]
    fn test_match_variable_binding_with_when_guard() {
        let src = r#"
var category = ""

func check_val(x):
    match x:
        var n when n > 50:
            return "huge"
        var n when n > 10:
            return "medium"
        _:
            return "small"

func _ready():
    category = check_val(25)
"#;
        let mut vm = GdVm::from_source(src).expect("compile");
        let mut node = make_test_node("MatchGuardTest");
        vm.ready(&mut node).expect("ready");
        assert_eq!(vm.member_vars.get("category"), Some(&IrValue::String("medium".into())));
    }

    // 18. Godot 4 `match` Statements: Array Destructuring with `..` Rest
    #[test]
    fn test_match_array_destructuring_with_rest() {
        let src = r#"
var head_item = 0

func _ready():
    var items = [100, 200, 300]
    match items:
        [var head, ..]:
            head_item = head
        _:
            head_item = -1
"#;
        let mut vm = GdVm::from_source(src).expect("compile");
        let mut node = make_test_node("MatchArrayTest");
        vm.ready(&mut node).expect("ready");
        assert_eq!(vm.member_vars.get("head_item"), Some(&IrValue::I64(100)));
    }

    // 19. Godot 4 `match` Statements: Dictionary Destructuring with Guards
    #[test]
    fn test_match_dictionary_destructuring_with_guard() {
        let src = r#"
var hit_dmg = 0

func _ready():
    var event = {"type": "hit", "dmg": 75}
    match event:
        {"type": "hit", "dmg": var d} when d >= 50:
            hit_dmg = d
        _:
            hit_dmg = 0
"#;
        let mut vm = GdVm::from_source(src).expect("compile");
        let mut node = make_test_node("MatchDictTest");
        vm.ready(&mut node).expect("ready");
        assert_eq!(vm.member_vars.get("hit_dmg"), Some(&IrValue::I64(75)));
    }

    // 20. Typed Collections: Array[int] and Coercion
    #[test]
    fn test_typed_array_coercion() {
        let src = r#"
@export var numbers: Array[int] = [1.8, 2.3]

func _ready():
    pass
"#;
        let mut vm = GdVm::from_source(src).expect("compile");
        let mut node = make_test_node("TypedArrayTest");
        vm.ready(&mut node).expect("ready");
        assert_eq!(
            vm.member_vars.get("numbers"),
            Some(&IrValue::Array(vec![
                IrValue::I64(1),
                IrValue::I64(2),
            ]))
        );
    }

    // 21. Typed Collections: Dictionary[String, int]
    #[test]
    fn test_typed_dictionary_coercion() {
        let src = r#"
var stats: Dictionary[String, int] = {"level": 5.0}

func _ready():
    pass
"#;
        let mut vm = GdVm::from_source(src).expect("compile");
        let mut node = make_test_node("TypedDictTest");
        vm.ready(&mut node).expect("ready");
        assert_eq!(
            vm.member_vars.get("stats"),
            Some(&IrValue::Map({
                let mut m = std::collections::BTreeMap::new();
                m.insert("level".into(), IrValue::I64(5));
                m
            }))
        );
    }

    // 22. Suspendable Coroutines: Timer Await & tick_timers
    #[test]
    fn test_await_timer_coroutine_tick_and_resumption() {
        let src = r#"
var stage = 0

func _ready():
    stage = 1
    await create_timer(0.5).timeout
    stage = 2
"#;
        let mut vm = GdVm::from_source(src).expect("compile");
        let mut node = make_test_node("TimerCoroTest");

        // 1. Initial call suspends
        let outcome = vm.call_or_suspend("_ready", &[], &mut node).expect("call");
        match outcome {
            GdCoroutineOutcome::Suspended { coroutine_id, target } => {
                assert_eq!(coroutine_id, 1);
                assert!(matches!(target, AwaitTarget::Timer { remaining_sec } if remaining_sec == 0.5));
            }
            _ => panic!("Expected coroutine suspension on timer"),
        }
        assert_eq!(vm.member_vars.get("stage"), Some(&IrValue::I64(1)));

        // 2. Tick 0.2s -> Still suspended
        let resumed_mid = vm.tick_timers(0.2).expect("tick");
        assert!(resumed_mid.is_empty());
        assert_eq!(vm.member_vars.get("stage"), Some(&IrValue::I64(1)));

        // 3. Tick 0.4s -> Expires and completes!
        let resumed_final = vm.tick_timers(0.4).expect("tick");
        assert_eq!(resumed_final.len(), 1);
        assert_eq!(vm.member_vars.get("stage"), Some(&IrValue::I64(2)));
    }

    // 23. Suspendable Coroutines: Signal Await & emit_signal_resume
    #[test]
    fn test_await_signal_coroutine_resumption() {
        let src = r#"
var finished = false

func _ready():
    await self.level_cleared
    finished = true
"#;
        let mut vm = GdVm::from_source(src).expect("compile");
        let mut node = make_test_node("SignalCoroTest");

        let outcome = vm.call_or_suspend("_ready", &[], &mut node).expect("call");
        assert!(matches!(outcome, GdCoroutineOutcome::Suspended { .. }));
        assert_eq!(vm.member_vars.get("finished"), Some(&IrValue::Bool(false)));

        // Resume by emitting signal
        let res = vm.emit_signal_resume("level_cleared", &[]).expect("resume");
        assert_eq!(res.len(), 1);
        assert_eq!(vm.member_vars.get("finished"), Some(&IrValue::Bool(true)));
    }

    // 24. LSP: Document Analysis (Symbols, Diagnostics, Executable Lines)
    #[test]
    fn test_lsp_document_analysis_symbols_and_lines() {
        let src = r#"
class_name TestHero
extends CharacterBody3D

signal hero_spawned(pos: Vector3)

@export var max_health: int = 100
var current_health: int = 100

func _ready() -> void:
    current_health = max_health
    pass
"#;
        let analyzer = GdLspAnalyzer::new();
        let doc = analyzer.analyze_document("file:///hero.gd", src);

        assert_eq!(doc.uri, "file:///hero.gd");
        assert!(doc.symbols.iter().any(|s| s.name == "TestHero" && s.kind == GdSymbolKind::Class));
        assert!(doc.symbols.iter().any(|s| s.name == "hero_spawned" && s.kind == GdSymbolKind::Signal));
        assert!(doc.symbols.iter().any(|s| s.name == "max_health" && s.kind == GdSymbolKind::Property));
        assert!(doc.symbols.iter().any(|s| s.name == "_ready" && s.kind == GdSymbolKind::Function));
        assert!(doc.executable_lines.contains(&11)); // current_health = max_health
        assert!(!doc.executable_lines.contains(&12)); // pass is excluded
    }

    // 25. LSP: Auto-Completion (Context-Aware: Dot, Annotation, Keywords)
    #[test]
    fn test_lsp_completions_dot_and_annotations() {
        let analyzer = GdLspAnalyzer::new();
        let src = "func _ready():\n    my_str.\n";

        // After dot -> Method completions
        let completions_dot = analyzer.complete_at(src, 2, 11);
        assert!(completions_dot.iter().any(|c| c.label == "to_upper"));
        assert!(completions_dot.iter().any(|c| c.label == "length"));

        // After @ -> Annotations
        let src_anno = "@\n";
        let completions_anno = analyzer.complete_at(src_anno, 1, 1);
        assert!(completions_anno.iter().any(|c| c.label == "@export"));
        assert!(completions_anno.iter().any(|c| c.label == "@onready"));

        // General completions
        let src_gen = "va\n";
        let completions_gen = analyzer.complete_at(src_gen, 1, 2);
        assert!(completions_gen.iter().any(|c| c.label == "var"));
        assert!(completions_gen.iter().any(|c| c.label == "sin"));
    }

    // 26. LSP: Hover and Definition Lookup
    #[test]
    fn test_lsp_hover_and_definition_lookup() {
        let analyzer = GdLspAnalyzer::new();
        let src = "class_name Player\nfunc _ready():\n    var x = sin(0.5)\n";

        // Hover over `sin`
        let hover = analyzer.hover_at(src, 3, 14).expect("hover info found");
        assert!(hover.contents.contains("sin(angle_rad: float) -> float"));

        // Goto definition of `Player`
        let loc = analyzer.goto_definition(src, 1, 13).expect("definition found");
        assert_eq!(loc.range.start.line, 1);
    }
}

