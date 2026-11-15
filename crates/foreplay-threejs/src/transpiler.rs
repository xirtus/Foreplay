use crate::error::ThreeJsError;
use foreplay_gdscript::{GdFunctionDef, parse_gdscript};
use foreplay_ir::{FeelSpecKnob, IrScene, IrValue, NodeId};
use std::collections::{BTreeMap, BTreeSet};

/// Transpiles a GDScript source string into a clean, complete TypeScript `Behavior` class.
pub fn transpile_gdscript_to_ts(
    gd_source: &str,
    node_name: &str,
    node_id: NodeId,
    feel_knobs: &[FeelSpecKnob],
) -> Result<String, ThreeJsError> {
    let module = parse_gdscript(gd_source).map_err(|e| {
        ThreeJsError::Transpilation(format!("GDScript parsing failed for '{node_name}': {e}"))
    })?;

    let class_name = module
        .class_name
        .clone()
        .unwrap_or_else(|| format!("{}Behavior", sanitize_ident(node_name)));

    let mut member_vars: BTreeSet<String> = BTreeSet::new();
    for exp in &module.exports {
        member_vars.insert(exp.name.clone());
    }
    // Also scan top-level var declarations that might not be exported
    for line in gd_source.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("var ") {
            let var_name = rest
                .split([':', '=', ' '])
                .next()
                .unwrap_or("")
                .trim();
            if !var_name.is_empty() {
                member_vars.insert(var_name.to_string());
            }
        }
    }
    member_vars.insert("is_grounded".to_string());
    member_vars.insert("time_since_grounded".to_string());

    let mut ts = String::new();
    ts.push_str(&format!("export class {class_name} implements Behavior {{\n"));
    ts.push_str(&format!("    nodeId: number = {};\n", node_id.index));
    ts.push_str("    object3d: THREE.Object3D;\n");
    ts.push_str("    ctx?: BehaviorContext;\n");
    ts.push_str("    velocity: THREE.Vector3 = new THREE.Vector3();\n");
    ts.push_str("    position: THREE.Vector3;\n");
    ts.push_str("    rotation: THREE.Euler;\n");

    // Member fields from exports and vars
    for exp in &module.exports {
        let ts_type = map_gd_type_to_ts(exp.type_hint.as_deref());
        let default_expr = exp
            .default_value
            .as_ref()
            .map(format_ir_value_to_ts)
            .unwrap_or_else(|| default_for_ts_type(&ts_type));
        ts.push_str(&format!(
            "    {}: {} = {};\n",
            exp.name, ts_type, default_expr
        ));
    }

    // Additional detected non-exported vars
    for var_name in &member_vars {
        if !module.exports.iter().any(|e| &e.name == var_name) {
            ts.push_str(&format!("    {var_name}: any = 0;\n"));
        }
    }

    // Constructor
    ts.push_str("\n    constructor(nodeId: number, object3d: THREE.Object3D, feel: FeelStore) {\n");
    ts.push_str("        this.nodeId = nodeId;\n");
    ts.push_str("        this.object3d = object3d;\n");
    ts.push_str("        this.position = object3d.position;\n");
    ts.push_str("        this.rotation = object3d.rotation;\n");

    // Bind feel knobs
    for knob in feel_knobs {
        if member_vars.contains(&knob.key) {
            ts.push_str(&format!(
                "        feel.subscribe({:?}, (val) => {{ this.{} = val; }});\n",
                knob.key, knob.key
            ));
        }
    }
    ts.push_str("    }\n\n");

    // Built-in kinematic helpers
    ts.push_str(
        "    isOnFloor(): boolean {\n\
        \x20       return this.position.y <= 0.05;\n\
        \x20   }\n\n\
        \x20   moveAndSlide(delta: number): void {\n\
        \x20       this.position.x += this.velocity.x * delta;\n\
        \x20       this.position.y += this.velocity.y * delta;\n\
        \x20       this.position.z += this.velocity.z * delta;\n\
        \x20   }\n\n\
        \x20   emitSignal(signalName: string, ...args: any[]): void {\n\
        \x20       if (this.ctx) {\n\
        \x20           this.ctx.signals.emit(this.nodeId, signalName, ...args);\n\
        \x20       }\n\
        \x20   }\n\n",
    );

    // Transpile functions
    for func in &module.functions {
        let ts_func_name = match func.name.as_str() {
            "_ready" => "ready".to_string(),
            "_physics_process" => "physicsProcess".to_string(),
            "_process" => "process".to_string(),
            other => sanitize_method_name(other),
        };

        let params_ts = format_func_params(func);
        let ret_ts = func
            .return_type
            .as_deref()
            .map(|t| map_gd_type_to_ts(Some(t)))
            .unwrap_or_else(|| "void".into());

        ts.push_str(&format!("    {ts_func_name}({params_ts}): {ret_ts} {{\n"));

        // Extract body lines
        let body_ts = transpile_function_body(gd_source, &func.name, &member_vars);
        if body_ts.trim().is_empty() {
            ts.push_str("        // pass\n");
        } else {
            ts.push_str(&body_ts);
        }

        ts.push_str("    }\n\n");
    }

    ts.push_str("}\n");
    Ok(ts)
}

fn transpile_function_body(
    source: &str,
    func_name: &str,
    member_vars: &BTreeSet<String>,
) -> String {
    let mut out = String::new();
    let lines: Vec<&str> = source.lines().collect();

    let mut in_func = false;
    let mut base_indent = 0;
    let mut indent_stack: Vec<usize> = Vec::new();

    let func_prefix = format!("func {func_name}");

    for line in lines {
        let trimmed = line.trim();

        if !in_func {
            if trimmed.starts_with(&func_prefix) {
                in_func = true;
                base_indent = count_indent(line);
                indent_stack.clear();
            }
            continue;
        }

        // Inside function: check if we reached next top-level declaration
        let current_indent = count_indent(line);
        if !trimmed.is_empty() && current_indent <= base_indent {
            // Next top-level declaration reached
            break;
        }

        if trimmed.is_empty() {
            out.push('\n');
            continue;
        }

        // Relative indentation within function
        let rel_indent = current_indent.saturating_sub(base_indent + 4);

        // Pop closing braces if current indent is less than top of stack
        while let Some(&top) = indent_stack.last() {
            if rel_indent < top {
                indent_stack.pop();
                let pad = " ".repeat(8 + indent_stack.len() * 4);
                out.push_str(&format!("{pad}}}\n"));
            } else {
                break;
            }
        }

        let pad = " ".repeat(8 + indent_stack.len() * 4);

        // Check if line opens a new block
        if trimmed.ends_with(':') {
            let header = trimmed.trim_end_matches(':');
            let (ts_header, is_block) = translate_block_header(header, member_vars);
            out.push_str(&format!("{pad}{ts_header} {{\n"));
            if is_block {
                indent_stack.push(rel_indent + 4);
            }
            continue;
        }

        // Regular statement
        let ts_stmt = translate_statement(trimmed, member_vars);
        out.push_str(&format!("{pad}{ts_stmt}\n"));
    }

    // Close any remaining open blocks
    while indent_stack.pop().is_some() {
        let pad = " ".repeat(8 + indent_stack.len() * 4);
        out.push_str(&format!("{pad}}}\n"));
    }

    out
}

fn translate_block_header(header: &str, member_vars: &BTreeSet<String>) -> (String, bool) {
    let trimmed = header.trim();

    if let Some(rest) = trimmed.strip_prefix("if ") {
        let cond = translate_expr(rest.trim(), member_vars);
        (format!("if ({cond})"), true)
    } else if let Some(rest) = trimmed.strip_prefix("elif ") {
        let cond = translate_expr(rest.trim(), member_vars);
        (format!("}} else if ({cond})"), true)
    } else if trimmed == "else" {
        ("} else".into(), true)
    } else if let Some(rest) = trimmed.strip_prefix("while ") {
        let cond = translate_expr(rest.trim(), member_vars);
        (format!("while ({cond})"), true)
    } else if let Some(rest) = trimmed.strip_prefix("for ") {
        if let Some((var, iter)) = rest.split_once(" in ") {
            let iter_ts = translate_expr(iter.trim(), member_vars);
            (format!("for (const {} of {})", var.trim(), iter_ts), true)
        } else {
            (format!("// for {rest}"), false)
        }
    } else {
        (format!("// {header}"), false)
    }
}

fn translate_statement(stmt: &str, member_vars: &BTreeSet<String>) -> String {
    let trimmed = stmt.trim();

    if trimmed == "pass" {
        return "// pass".to_string();
    }

    if let Some(comment) = trimmed.strip_prefix('#') {
        return format!("// {}", comment.trim());
    }

    if let Some(rest) = trimmed.strip_prefix("return ") {
        let expr = translate_expr(rest.trim(), member_vars);
        return format!("return {expr};");
    }

    if trimmed == "return" {
        return "return;".to_string();
    }

    // print(...) -> console.log(...)
    if let Some(rest) = trimmed.strip_prefix("print(") {
        let inner = rest.trim_end_matches(')').trim();
        let expr = translate_expr(inner, member_vars);
        return format!("console.log({expr});");
    }

    // var x = ... -> let x = ...
    if let Some(rest) = trimmed.strip_prefix("var ") {
        if let Some((name_part, val_part)) = rest.split_once('=') {
            let (name, _type_hint) = if let Some((n, t)) = name_part.split_once(':') {
                (n.trim(), Some(t.trim()))
            } else {
                (name_part.trim(), None)
            };
            let val = translate_expr(val_part.trim(), member_vars);
            return format!("let {name} = {val};");
        } else {
            let name = rest.trim().split(':').next().unwrap_or(rest.trim());
            return format!("let {name} = null;");
        }
    }

    // emit_signal("foo", args...) -> this.emitSignal("foo", args...)
    if let Some(rest) = trimmed.strip_prefix("emit_signal(") {
        let inner = rest.trim_end_matches(')').trim();
        let translated_args = translate_expr(inner, member_vars);
        return format!("this.emitSignal({translated_args});");
    }

    // move_and_slide() -> this.moveAndSlide(delta);
    if trimmed.starts_with("move_and_slide(") {
        return "this.moveAndSlide(delta);".to_string();
    }

    // Assignment or expression statement
    let mut translated = translate_expr(trimmed, member_vars);
    if !translated.ends_with(';') {
        translated.push(';');
    }
    translated
}

fn translate_expr(expr: &str, member_vars: &BTreeSet<String>) -> String {
    let mut s = expr.to_string();

    // Comments in expr
    if let Some(idx) = s.find('#') {
        s.truncate(idx);
        s = s.trim().to_string();
    }

    // Keywords and operators
    s = s.replace(" not ", " !");
    if s.starts_with("not ") {
        s = format!("!{}", &s[4..]);
    }
    s = s.replace(" and ", " && ");
    s = s.replace(" or ", " || ");
    s = s.replace("self", "this");

    // Kinematic & built-in calls
    s = s.replace("is_on_floor()", "this.isOnFloor()");
    s = s.replace("Vector3(", "new THREE.Vector3(");
    s = s.replace("Vector2(", "new THREE.Vector2(");
    s = s.replace("rad_to_deg(", "THREE.MathUtils.radToDeg(");
    s = s.replace("deg_to_rad(", "THREE.MathUtils.degToRad(");
    s = s.replace("lerp(", "THREE.MathUtils.lerp(");
    s = s.replace("clamp(", "THREE.MathUtils.clamp(");

    // Prefix member variables with `this.`
    for var in member_vars {
        s = replace_word(&s, var, &format!("this.{var}"));
    }

    // Also prefix velocity, position, rotation
    s = replace_word(&s, "velocity", "this.velocity");
    s = replace_word(&s, "position", "this.position");
    s = replace_word(&s, "rotation", "this.rotation");

    // Clean any double `this.this.`
    s = s.replace("this.this.", "this.");

    s
}

fn replace_word(text: &str, word: &str, replacement: &str) -> String {
    let mut result = String::new();
    let bytes = text.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if text[i..].starts_with(word) {
            let before_ok = i == 0 || !is_ident_char(bytes[i - 1] as char);
            let end_idx = i + word.len();
            let after_ok = end_idx == bytes.len() || !is_ident_char(bytes[end_idx] as char);

            // Avoid replacing if preceded by dot: e.g. "foo.position" -> keep
            let preceded_by_dot = i > 0 && bytes[i - 1] == b'.';

            if before_ok && after_ok && !preceded_by_dot {
                result.push_str(replacement);
                i += word.len();
                continue;
            }
        }
        result.push(bytes[i] as char);
        i += 1;
    }
    result
}

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn count_indent(line: &str) -> usize {
    let mut count = 0;
    for c in line.chars() {
        if c == ' ' {
            count += 1;
        } else if c == '\t' {
            count += 4;
        } else {
            break;
        }
    }
    count
}

fn sanitize_ident(name: &str) -> String {
    let clean: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    if clean.is_empty() {
        "Node".into()
    } else {
        clean
    }
}

fn sanitize_method_name(name: &str) -> String {
    let trimmed = name.trim_start_matches('_');
    if trimmed.is_empty() {
        "method".into()
    } else {
        trimmed.to_string()
    }
}

fn map_gd_type_to_ts(gd_type: Option<&str>) -> String {
    match gd_type {
        Some("float") | Some("int") => "number".into(),
        Some("bool") => "boolean".into(),
        Some("String") => "string".into(),
        Some("Vector3") => "THREE.Vector3".into(),
        Some("Vector2") => "THREE.Vector2".into(),
        Some("void") => "void".into(),
        Some("Array") => "any[]".into(),
        Some("Dictionary") => "Record<string, any>".into(),
        Some(other) => other.to_string(),
        None => "any".into(),
    }
}

fn default_for_ts_type(ts_type: &str) -> String {
    match ts_type {
        "number" => "0".into(),
        "boolean" => "false".into(),
        "string" => "\"\"".into(),
        "THREE.Vector3" => "new THREE.Vector3()".into(),
        "THREE.Vector2" => "new THREE.Vector2()".into(),
        "any[]" => "[]".into(),
        _ => "null".into(),
    }
}

fn format_ir_value_to_ts(val: &IrValue) -> String {
    match val {
        IrValue::Nil => "null".into(),
        IrValue::Bool(b) => b.to_string(),
        IrValue::I64(i) => i.to_string(),
        IrValue::F64(f) => {
            if f.fract() == 0.0 {
                format!("{f:.1}")
            } else {
                f.to_string()
            }
        }
        IrValue::String(s) => format!("{s:?}"),
        IrValue::Vec3(v) => format!("new THREE.Vector3({}, {}, {})", v[0], v[1], v[2]),
        IrValue::Vec2(v) => format!("new THREE.Vector2({}, {})", v[0], v[1]),
        _ => "null".into(),
    }
}

fn format_func_params(func: &GdFunctionDef) -> String {
    let mut params = Vec::new();
    if func.name == "_physics_process" || func.name == "_process" {
        params.push("delta: number".to_string());
        params.push("ctx: BehaviorContext".to_string());
    } else if func.name == "_ready" {
        params.push("ctx: BehaviorContext".to_string());
    } else {
        for p in &func.parameters {
            params.push(format!("{p}: any"));
        }
    }
    params.join(", ")
}

/// Generates the complete `src/scripts/behaviors.ts` module with contracts and all transpiled behaviors.
pub fn generate_behaviors_ts(
    scene: &IrScene,
    gd_scripts: &BTreeMap<NodeId, String>,
    ts_scripts: &BTreeMap<NodeId, String>,
    all_knobs: &[FeelSpecKnob],
) -> Result<String, ThreeJsError> {
    let mut ts = String::new();

    ts.push_str(
        "// Foreplay Game Engine TypeScript Behaviors & Component Bus\n\
        // Generated automatically by foreplay-threejs transpiler.\n\n\
        import * as THREE from 'three';\n\
        import { SignalBus } from '../signals';\n\
        import { FeelStore } from '../feel';\n\n\
        export interface InputState {\n\
            keysDown: Set<string>;\n\
            keysJustPressed: Set<string>;\n\
            isActionPressed(action: string): boolean;\n\
            isActionJustPressed(action: string): boolean;\n\
        }\n\n\
        export interface BehaviorContext {\n\
            nodeId: number;\n\
            object3d: THREE.Object3D;\n\
            scene: THREE.Scene;\n\
            camera: THREE.PerspectiveCamera;\n\
            signals: SignalBus;\n\
            feel: FeelStore;\n\
            input: InputState;\n\
            nodes: Map<number, THREE.Object3D | HTMLElement>;\n\
        }\n\n\
        export interface Behavior {\n\
            nodeId: number;\n\
            object3d: THREE.Object3D;\n\
            ctx?: BehaviorContext;\n\
            ready?(ctx: BehaviorContext): void;\n\
            physicsProcess?(delta: number, ctx: BehaviorContext): void;\n\
            process?(delta: number, ctx: BehaviorContext): void;\n\
            [key: string]: any;\n\
        }\n\n",
    );

    let mut class_registrations: Vec<(String, NodeId)> = Vec::new();

    // 1. Transpile GDScripts
    for (&node_id, gd_code) in gd_scripts {
        let node_name = scene
            .nodes
            .get(&node_id)
            .map(|n| n.name.as_str())
            .unwrap_or("Node");

        let class_ts = transpile_gdscript_to_ts(gd_code, node_name, node_id, all_knobs)?;
        ts.push_str(&class_ts);
        ts.push('\n');

        // Extract class name
        if let Some(line) = class_ts.lines().find(|l| l.starts_with("export class ")) {
            let cname = line["export class ".len()..]
                .split_whitespace()
                .next()
                .unwrap_or("Behavior");
            class_registrations.push((cname.to_string(), node_id));
        }
    }

    // 2. Include TypeScript scripts
    for (&node_id, ts_code) in ts_scripts {
        let node_name = scene
            .nodes
            .get(&node_id)
            .map(|n| n.name.as_str())
            .unwrap_or("Node");
        let class_name = format!("{}TsBehavior", sanitize_ident(node_name));

        if ts_code.contains("implements Behavior") || ts_code.contains("class ") {
            ts.push_str(ts_code);
            ts.push('\n');
            class_registrations.push((class_name, node_id));
        } else {
            // Wrap snippet in Behavior class
            ts.push_str(&format!(
                "export class {class_name} implements Behavior {{\n\
                \x20   nodeId: number = {};\n\
                \x20   object3d: THREE.Object3D;\n\
                \x20   ctx?: BehaviorContext;\n\
                \x20   constructor(nodeId: number, object3d: THREE.Object3D, _feel: FeelStore) {{\n\
                \x20       this.nodeId = nodeId;\n\
                \x20       this.object3d = object3d;\n\
                \x20   }}\n\
                \x20   physicsProcess(delta: number, ctx: BehaviorContext): void {{\n\
                \x20       {}\n\
                \x20   }}\n\
                }}\n\n",
                node_id.index,
                ts_code.trim()
            ));
            class_registrations.push((class_name, node_id));
        }
    }

    // Registration factory function
    ts.push_str(
        "export function registerBehaviors(\n\
        \x20   nodes: Map<number, THREE.Object3D | HTMLElement>,\n\
        \x20   _signals: SignalBus,\n\
        \x20   feel: FeelStore\n\
        ): Behavior[] {\n\
        \x20   const behaviors: Behavior[] = [];\n",
    );

    for (cname, node_id) in class_registrations {
        ts.push_str(&format!(
            "    const obj_{} = nodes.get({});\n\
            \x20   if (obj_{} && obj_{} instanceof THREE.Object3D) {{\n\
            \x20       const b = new {}({}, obj_{}, feel);\n\
            \x20       behaviors.push(b);\n\
            \x20   }}\n",
            node_id.index, node_id.index, node_id.index, node_id.index, cname, node_id.index, node_id.index
        ));
    }

    ts.push_str("    return behaviors;\n}\n");

    Ok(ts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transpiles_skater_controller_gdscript_to_valid_typescript() {
        let gd_src = include_str!("../../../templates/godot-migration/skater_controller.gd");
        let node_id = NodeId::new(1, 1).unwrap();
        let knobs = vec![
            FeelSpecKnob {
                key: "max_speed".into(),
                intent: "Top speed".into(),
                unit: "m/s".into(),
                min: 0.0,
                max: 30.0,
                default_value: 14.5,
            },
            FeelSpecKnob {
                key: "ollie_impulse".into(),
                intent: "Ollie pop".into(),
                unit: "N".into(),
                min: 1.0,
                max: 15.0,
                default_value: 6.8,
            },
        ];

        let ts = transpile_gdscript_to_ts(gd_src, "Skater", node_id, &knobs).unwrap();
        assert!(ts.contains("export class SkaterController implements Behavior"));
        assert!(ts.contains("nodeId: number = 1;"));
        assert!(ts.contains("velocity: THREE.Vector3 = new THREE.Vector3();"));
        assert!(ts.contains("feel.subscribe(\"max_speed\", (val) => { this.max_speed = val; });"));
        assert!(ts.contains("feel.subscribe(\"ollie_impulse\", (val) => { this.ollie_impulse = val; });"));
        assert!(ts.contains("ready(ctx: BehaviorContext): void"));
        assert!(ts.contains("physicsProcess(delta: number, ctx: BehaviorContext): void"));
        assert!(ts.contains("on_trick_landed(trick_name: any, score: any): void"));
        assert!(ts.contains("isOnFloor()"));
        assert!(ts.contains("moveAndSlide("));
    }

    #[test]
    fn transpiles_platformer_2d_gdscript_logic() {
        let gd_src = include_str!("../../../templates/2d-platformer/player_2d.gd");
        let node_id = NodeId::new(2, 1).unwrap();
        let knobs = vec![];

        let ts = transpile_gdscript_to_ts(gd_src, "Player2D", node_id, &knobs).unwrap();
        assert!(ts.contains("export class Player2DController implements Behavior"));
        assert!(ts.contains("if (!this.isOnFloor())"));
        assert!(ts.contains("this.velocity.y += this.gravity * delta"));
        assert!(ts.contains("this.emitSignal(\"landed\")"));
    }
}
