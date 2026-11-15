//! Godot 4 `.tscn` / `.tres` Faithful Importer.
//!
//! Mirrors `ResourceFormatLoaderText` (`scene/resources/resource_format_text.cpp`).
//! Translates Godot 4 text scenes into canonical `foreplay_ir::IrScene` with
//! deterministic `NodeId` allocation, hierarchy resolution, signal connections,
//! and variant property decoding.

use foreplay_ir::{IrNode, IrScene, IrValue, NodeId, SignalConnection};
use std::collections::BTreeMap;
use std::num::NonZeroU32;

/// Errors that can occur when parsing a Godot `.tscn` or `.tres` file.
#[derive(Clone, PartialEq, Eq, Debug, thiserror::Error)]
pub enum TscnParseError {
    #[error("unexpected end of file: {0}")]
    UnexpectedEof(String),
    #[error("syntax error at line {line}: {message}")]
    SyntaxError { line: usize, message: String },
    #[error("unresolved node path '{path}' referenced in '{context}'")]
    UnresolvedNodePath { path: String, context: String },
    #[error("missing root node in scene")]
    MissingRootNode,
    #[error("invalid value expression: '{0}'")]
    InvalidValue(String),
}

/// Metadata about an external resource declared via `[ext_resource ...]`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ExtResource {
    pub res_type: String,
    pub path: String,
    pub id: String,
    pub uid: Option<String>,
}

/// Parses Godot 4 `.tscn` text content into a canonical `IrScene`.
///
/// Mirrors Godot's `ResourceFormatLoaderText::load`.
pub fn parse_tscn_to_ir(source: &str) -> Result<IrScene, TscnParseError> {
    let mut header_scene_name = None;
    let mut ext_resources: BTreeMap<String, ExtResource> = BTreeMap::new();
    let mut parsed_nodes: Vec<RawNode> = Vec::new();
    let mut raw_connections: Vec<RawConnection> = Vec::new();

    let lines: Vec<&str> = source.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            i += 1;
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            let section_content = &line[1..line.len() - 1].trim();
            let (tag, attrs_str) = split_first_word(section_content);
            let attrs = parse_attributes(attrs_str, i + 1)?;

            match tag {
                "gd_scene" => {
                    // Scene header
                    if let Some(uid) = attrs.get("uid") {
                        let clean_uid = uid.trim_matches('"');
                        header_scene_name = Some(format!("scene_{clean_uid}"));
                    }
                    i += 1;
                }
                "ext_resource" => {
                    let res_type = attrs
                        .get("type")
                        .cloned()
                        .unwrap_or_default()
                        .trim_matches('"')
                        .to_string();
                    let path = attrs
                        .get("path")
                        .cloned()
                        .unwrap_or_default()
                        .trim_matches('"')
                        .to_string();
                    let id = attrs
                        .get("id")
                        .cloned()
                        .unwrap_or_default()
                        .trim_matches('"')
                        .to_string();
                    let uid = attrs.get("uid").map(|s| s.trim_matches('"').to_string());

                    ext_resources.insert(
                        id.clone(),
                        ExtResource {
                            res_type,
                            path,
                            id,
                            uid,
                        },
                    );
                    i += 1;
                }
                "sub_resource" => {
                    // Consume any sub_resource key = value lines until next section
                    i += 1;
                    while i < lines.len() {
                        let sub_line = lines[i].trim();
                        if sub_line.starts_with('[') {
                            break;
                        }
                        i += 1;
                    }
                }
                "node" => {
                    let name = attrs
                        .get("name")
                        .cloned()
                        .ok_or_else(|| TscnParseError::SyntaxError {
                            line: i + 1,
                            message: "node tag missing 'name' attribute".into(),
                        })?
                        .trim_matches('"')
                        .to_string();

                    let parent = attrs.get("parent").map(|s| s.trim_matches('"').to_string());
                    let class_name = attrs
                        .get("type")
                        .map(|s| s.trim_matches('"').to_string())
                        .unwrap_or_else(|| "Node3D".to_string());

                    let instance_attr = attrs.get("instance").cloned();
                    let groups = if let Some(g_str) = attrs.get("groups") {
                        parse_string_array(g_str)
                    } else {
                        Vec::new()
                    };

                    i += 1;
                    let mut properties = BTreeMap::new();

                    // Read properties for this node until next section
                    while i < lines.len() {
                        let prop_line = lines[i].trim();
                        if prop_line.starts_with('[') {
                            break;
                        }
                        if prop_line.is_empty()
                            || prop_line.starts_with(';')
                            || prop_line.starts_with('#')
                        {
                            i += 1;
                            continue;
                        }

                        if let Some((k, v_str)) = split_key_value(prop_line) {
                            let val = parse_godot_value(v_str, &ext_resources)?;
                            properties.insert(k.to_string(), val);
                        }
                        i += 1;
                    }

                    parsed_nodes.push(RawNode {
                        name,
                        class_name,
                        parent,
                        instance_attr,
                        groups,
                        properties,
                    });
                }
                "connection" => {
                    let signal = attrs
                        .get("signal")
                        .cloned()
                        .ok_or_else(|| TscnParseError::SyntaxError {
                            line: i + 1,
                            message: "connection missing 'signal'".into(),
                        })?
                        .trim_matches('"')
                        .to_string();
                    let from = attrs
                        .get("from")
                        .cloned()
                        .ok_or_else(|| TscnParseError::SyntaxError {
                            line: i + 1,
                            message: "connection missing 'from'".into(),
                        })?
                        .trim_matches('"')
                        .to_string();
                    let to = attrs
                        .get("to")
                        .cloned()
                        .ok_or_else(|| TscnParseError::SyntaxError {
                            line: i + 1,
                            message: "connection missing 'to'".into(),
                        })?
                        .trim_matches('"')
                        .to_string();
                    let method = attrs
                        .get("method")
                        .cloned()
                        .ok_or_else(|| TscnParseError::SyntaxError {
                            line: i + 1,
                            message: "connection missing 'method'".into(),
                        })?
                        .trim_matches('"')
                        .to_string();

                    raw_connections.push(RawConnection {
                        signal,
                        from,
                        to,
                        method,
                    });
                    i += 1;
                }
                _ => {
                    // Ignore unrecognized top-level sections (e.g. editable)
                    i += 1;
                }
            }
        } else {
            i += 1;
        }
    }

    if parsed_nodes.is_empty() {
        return Err(TscnParseError::MissingRootNode);
    }

    // Root node is the first node without parent (or parsed_nodes[0])
    let root_raw = &parsed_nodes[0];
    let scene_name = if root_raw.name.is_empty() {
        header_scene_name.unwrap_or_else(|| "GodotImportedScene".to_string())
    } else {
        root_raw.name.clone()
    };

    let mut scene = IrScene::new(scene_name);
    let mut path_to_id: BTreeMap<String, NodeId> = BTreeMap::new();
    let mut name_to_id: BTreeMap<String, NodeId> = BTreeMap::new();

    // Pass 1: Allocate deterministic NodeIds and index by path
    for (idx, raw) in parsed_nodes.iter().enumerate() {
        let node_id = NodeId {
            index: (idx + 1) as u32,
            generation: NonZeroU32::new(1).unwrap(),
        };

        let node_path = match &raw.parent {
            None => {
                // Root node
                path_to_id.insert(".".to_string(), node_id);
                path_to_id.insert(raw.name.clone(), node_id);
                raw.name.clone()
            }
            Some(p) if p == "." => {
                // Child of root
                path_to_id.insert(raw.name.clone(), node_id);
                raw.name.clone()
            }
            Some(p) => {
                // Child of sub-path
                let p_clean = p.trim_start_matches("./");
                let full = format!("{p_clean}/{}", raw.name);
                path_to_id.insert(full.clone(), node_id);
                path_to_id.insert(raw.name.clone(), node_id);
                full
            }
        };

        name_to_id.insert(raw.name.clone(), node_id);
        path_to_id.insert(node_path, node_id);
    }

    let root_id = NodeId {
        index: 1,
        generation: NonZeroU32::new(1).unwrap(),
    };

    // Pass 2: Build `IrNode`s with resolved parent handles
    for (idx, raw) in parsed_nodes.into_iter().enumerate() {
        let node_id = NodeId {
            index: (idx + 1) as u32,
            generation: NonZeroU32::new(1).unwrap(),
        };

        let resolved_parent = match raw.parent.as_deref() {
            None => None,
            Some(".") => Some(root_id),
            Some(p) => {
                let p_clean = p.trim_start_matches("./");
                path_to_id
                    .get(p_clean)
                    .or_else(|| name_to_id.get(p_clean))
                    .copied()
                    .or_else(|| {
                        // Basename fallback
                        let base = p_clean.split('/').next_back().unwrap_or(p_clean);
                        name_to_id.get(base).copied()
                    })
                    .or(Some(root_id))
            }
        };

        let instance_scene = if let Some(inst_str) = &raw.instance_attr {
            resolve_ext_resource_path(inst_str, &ext_resources)
        } else {
            None
        };

        let ir_node = IrNode {
            id: node_id,
            name: raw.name,
            class_name: raw.class_name,
            parent: resolved_parent,
            instance_scene,
            properties: raw.properties,
            groups: raw.groups,
        };

        scene.nodes.insert(node_id, ir_node);
    }

    // Pass 3: Resolve signal connections
    for raw_conn in raw_connections {
        let from_node = resolve_node_target(&raw_conn.from, &path_to_id, &name_to_id, root_id)
            .ok_or_else(|| TscnParseError::UnresolvedNodePath {
                path: raw_conn.from.clone(),
                context: format!("connection from {}", raw_conn.from),
            })?;

        let to_node = resolve_node_target(&raw_conn.to, &path_to_id, &name_to_id, root_id)
            .ok_or_else(|| TscnParseError::UnresolvedNodePath {
                path: raw_conn.to.clone(),
                context: format!("connection to {}", raw_conn.to),
            })?;

        scene.connections.push(SignalConnection {
            from_node,
            signal_name: raw_conn.signal,
            to_node,
            method_name: raw_conn.method,
        });
    }

    Ok(scene)
}

#[derive(Debug)]
struct RawNode {
    name: String,
    class_name: String,
    parent: Option<String>,
    instance_attr: Option<String>,
    groups: Vec<String>,
    properties: BTreeMap<String, IrValue>,
}

#[derive(Debug)]
struct RawConnection {
    signal: String,
    from: String,
    to: String,
    method: String,
}

fn resolve_node_target(
    target: &str,
    path_to_id: &BTreeMap<String, NodeId>,
    name_to_id: &BTreeMap<String, NodeId>,
    root_id: NodeId,
) -> Option<NodeId> {
    if target == "." {
        return Some(root_id);
    }
    let clean = target.trim_start_matches("./");
    if let Some(&id) = path_to_id.get(clean) {
        return Some(id);
    }
    if let Some(&id) = name_to_id.get(clean) {
        return Some(id);
    }
    let base = clean.split('/').next_back().unwrap_or(clean);
    name_to_id.get(base).copied()
}

fn resolve_ext_resource_path(
    attr_val: &str,
    ext_resources: &BTreeMap<String, ExtResource>,
) -> Option<String> {
    // e.g. ExtResource("1_obs") or ExtResource("1")
    let s = attr_val.trim();
    if s.starts_with("ExtResource(") && s.ends_with(')') {
        let inner = &s["ExtResource(".len()..s.len() - 1].trim();
        let id = inner.trim_matches('"');
        if let Some(ext) = ext_resources.get(id) {
            return Some(ext.path.clone());
        }
        return Some(id.to_string());
    }
    None
}

fn split_first_word(s: &str) -> (&str, &str) {
    let s = s.trim();
    if let Some(idx) = s.find(char::is_whitespace) {
        (&s[..idx], s[idx..].trim())
    } else {
        (s, "")
    }
}

fn parse_attributes(
    attrs_str: &str,
    _line_num: usize,
) -> Result<BTreeMap<String, String>, TscnParseError> {
    let mut map = BTreeMap::new();
    let chars: Vec<char> = attrs_str.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        // Skip whitespace
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        if i >= chars.len() {
            break;
        }

        // Read key
        let key_start = i;
        while i < chars.len() && chars[i] != '=' && !chars[i].is_whitespace() {
            i += 1;
        }
        let key: String = chars[key_start..i].iter().collect();

        // Skip to '='
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        if i < chars.len() && chars[i] == '=' {
            i += 1;
        } else {
            // Flag attribute without '='
            if !key.is_empty() {
                map.insert(key, "true".to_string());
            }
            continue;
        }

        // Skip whitespace after '='
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        if i >= chars.len() {
            map.insert(key, String::new());
            break;
        }

        // Parse value
        let val_start = i;
        let mut depth_paren = 0;
        let mut depth_bracket = 0;
        let mut in_quote = false;

        while i < chars.len() {
            let c = chars[i];
            if c == '"' && (i == 0 || chars[i - 1] != '\\') {
                in_quote = !in_quote;
            } else if !in_quote {
                if c == '(' {
                    depth_paren += 1;
                } else if c == ')' {
                    if depth_paren > 0 {
                        depth_paren -= 1;
                    }
                } else if c == '[' {
                    depth_bracket += 1;
                } else if c == ']' {
                    if depth_bracket > 0 {
                        depth_bracket -= 1;
                    }
                } else if c.is_whitespace() && depth_paren == 0 && depth_bracket == 0 {
                    break;
                }
            }
            i += 1;
        }

        let val: String = chars[val_start..i].iter().collect();
        map.insert(key, val);
    }

    Ok(map)
}

fn split_key_value(line: &str) -> Option<(&str, &str)> {
    let mut in_quote = false;
    for (idx, c) in line.char_indices() {
        if c == '"' {
            in_quote = !in_quote;
        } else if c == '=' && !in_quote {
            let k = line[..idx].trim();
            let v = line[idx + 1..].trim();
            return Some((k, v));
        }
    }
    None
}

fn parse_string_array(s: &str) -> Vec<String> {
    let trimmed = s.trim();
    if !trimmed.starts_with('[') || !trimmed.ends_with(']') {
        return Vec::new();
    }
    let inner = &trimmed[1..trimmed.len() - 1];
    inner
        .split(',')
        .map(|item| item.trim().trim_matches('"').to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

/// Parses a Godot Variant textual expression into canonical `IrValue`.
pub fn parse_godot_value(
    expr: &str,
    ext_resources: &BTreeMap<String, ExtResource>,
) -> Result<IrValue, TscnParseError> {
    let s = expr.trim();
    if s == "null" || s.is_empty() {
        return Ok(IrValue::Nil);
    }
    if s == "true" {
        return Ok(IrValue::Bool(true));
    }
    if s == "false" {
        return Ok(IrValue::Bool(false));
    }

    // String literal
    if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
        let inner = &s[1..s.len() - 1];
        let unescaped = unescape_string(inner);
        return Ok(IrValue::String(unescaped));
    }

    // Vector2(x, y)
    if s.starts_with("Vector2(") && s.ends_with(')') {
        let inner = &s["Vector2(".len()..s.len() - 1];
        let nums = parse_float_list(inner)?;
        if nums.len() == 2 {
            return Ok(IrValue::Vec2([nums[0], nums[1]]));
        }
        return Err(TscnParseError::InvalidValue(s.to_string()));
    }

    // Vector3(x, y, z)
    if s.starts_with("Vector3(") && s.ends_with(')') {
        let inner = &s["Vector3(".len()..s.len() - 1];
        let nums = parse_float_list(inner)?;
        if nums.len() == 3 {
            return Ok(IrValue::Vec3([nums[0], nums[1], nums[2]]));
        }
        return Err(TscnParseError::InvalidValue(s.to_string()));
    }

    // Quaternion(x, y, z, w)
    if s.starts_with("Quaternion(") && s.ends_with(')') {
        let inner = &s["Quaternion(".len()..s.len() - 1];
        let nums = parse_float_list(inner)?;
        if nums.len() == 4 {
            return Ok(IrValue::Quat([nums[0], nums[1], nums[2], nums[3]]));
        }
        return Err(TscnParseError::InvalidValue(s.to_string()));
    }

    // Color(r, g, b, a)
    if s.starts_with("Color(") && s.ends_with(')') {
        let inner = &s["Color(".len()..s.len() - 1];
        let nums = parse_float_list(inner)?;
        if nums.len() == 4 {
            return Ok(IrValue::Color([nums[0], nums[1], nums[2], nums[3]]));
        }
        if nums.len() == 3 {
            return Ok(IrValue::Color([nums[0], nums[1], nums[2], 1.0]));
        }
        return Err(TscnParseError::InvalidValue(s.to_string()));
    }

    // Transform3D(x1, y1, z1, x2, y2, z2, x3, y3, z3, ox, oy, oz)
    if s.starts_with("Transform3D(") && s.ends_with(')') {
        let inner = &s["Transform3D(".len()..s.len() - 1];
        let nums = parse_float_list(inner)?;
        let arr = nums.into_iter().map(|f| IrValue::F64(f as f64)).collect();
        return Ok(IrValue::Array(arr));
    }

    // NodePath("...")
    if s.starts_with("NodePath(") && s.ends_with(')') {
        let inner = &s["NodePath(".len()..s.len() - 1].trim();
        let path = inner.trim_matches('"');
        return Ok(IrValue::NodePath(path.to_string()));
    }

    // ExtResource("...")
    if s.starts_with("ExtResource(") && s.ends_with(')') {
        let inner = &s["ExtResource(".len()..s.len() - 1].trim();
        let id = inner.trim_matches('"');
        if let Some(ext) = ext_resources.get(id) {
            return Ok(IrValue::ResourcePath(ext.path.clone()));
        }
        return Ok(IrValue::ResourcePath(id.to_string()));
    }

    // SubResource("...")
    if s.starts_with("SubResource(") && s.ends_with(')') {
        let inner = &s["SubResource(".len()..s.len() - 1].trim();
        let id = inner.trim_matches('"');
        return Ok(IrValue::ResourcePath(format!("sub://{id}")));
    }

    // Array [...]
    if s.starts_with('[') && s.ends_with(']') {
        let inner = &s[1..s.len() - 1];
        let elements = split_comma_separated(inner);
        let mut vals = Vec::with_capacity(elements.len());
        for elem in elements {
            if !elem.trim().is_empty() {
                vals.push(parse_godot_value(elem.trim(), ext_resources)?);
            }
        }
        return Ok(IrValue::Array(vals));
    }

    // Integer
    if let Ok(i) = s.parse::<i64>() {
        return Ok(IrValue::I64(i));
    }

    // Float
    if let Ok(f) = s.parse::<f64>() {
        return Ok(IrValue::F64(f));
    }

    // Fallback: unquoted string/ident
    Ok(IrValue::String(s.to_string()))
}

fn parse_float_list(s: &str) -> Result<Vec<f32>, TscnParseError> {
    let mut out = Vec::new();
    for part in s.split(',') {
        let p = part.trim();
        if !p.is_empty() {
            let val = p
                .parse::<f32>()
                .map_err(|_| TscnParseError::InvalidValue(s.to_string()))?;
            out.push(val);
        }
    }
    Ok(out)
}

fn split_comma_separated(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut depth_paren = 0;
    let mut depth_bracket = 0;
    let mut depth_brace = 0;
    let mut in_quote = false;
    let chars: Vec<(usize, char)> = s.char_indices().collect();

    for &(idx, c) in &chars {
        if c == '"' {
            in_quote = !in_quote;
        } else if !in_quote {
            match c {
                '(' => depth_paren += 1,
                ')' => {
                    if depth_paren > 0 {
                        depth_paren -= 1;
                    }
                }
                '[' => depth_bracket += 1,
                ']' => {
                    if depth_bracket > 0 {
                        depth_bracket -= 1;
                    }
                }
                '{' => depth_brace += 1,
                '}' => {
                    if depth_brace > 0 {
                        depth_brace -= 1;
                    }
                }
                ',' if depth_paren == 0 && depth_bracket == 0 && depth_brace == 0 => {
                    parts.push(&s[start..idx]);
                    start = idx + 1;
                }
                _ => {}
            }
        }
    }

    if start <= s.len() {
        parts.push(&s[start..]);
    }

    parts
}

fn unescape_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_TSCN: &str = r#"
[gd_scene load_steps=3 format=3 uid="uid://c8xskatepark"]

[ext_resource type="PackedScene" uid="uid://obs01" path="res://skate_obstacle.tscn" id="1_obs"]

[node name="SkateSpot" type="Node3D"]

[node name="Skater" type="CharacterBody3D" parent="." groups=["players", "skaters"]]
position = Vector3(0, 1.5, 0)
rotation = Quaternion(0, 0, 0, 1)
max_speed = 14.5
trick_count = 5
is_grounded = true
camera_path = NodePath("../CameraRig")

[node name="CameraRig" type="Node3D" parent="."]
position = Vector3(0, 3, 5)

[node name="ObstacleRail" parent="." instance=ExtResource("1_obs")]
position = Vector3(5, 0, 0)

[connection signal="trick_landed" from="Skater" to="CameraRig" method="_on_trick_landed"]
"#;

    #[test]
    fn parses_godot4_tscn_scene_nodes_and_signals() {
        let ir = parse_tscn_to_ir(SAMPLE_TSCN).expect("tscn parse succeeds");
        assert_eq!(ir.name, "SkateSpot");
        assert_eq!(ir.nodes.len(), 4);

        // Find root node
        let root = ir.nodes.values().find(|n| n.name == "SkateSpot").unwrap();
        assert_eq!(root.parent, None);
        assert_eq!(root.class_name, "Node3D");

        // Find Skater
        let skater = ir.nodes.values().find(|n| n.name == "Skater").unwrap();
        assert_eq!(skater.parent, Some(root.id));
        assert_eq!(skater.class_name, "CharacterBody3D");
        assert_eq!(skater.groups, vec!["players", "skaters"]);
        assert_eq!(
            skater.properties.get("position"),
            Some(&IrValue::Vec3([0.0, 1.5, 0.0]))
        );
        assert_eq!(
            skater.properties.get("max_speed"),
            Some(&IrValue::F64(14.5))
        );
        assert_eq!(skater.properties.get("trick_count"), Some(&IrValue::I64(5)));
        assert_eq!(
            skater.properties.get("is_grounded"),
            Some(&IrValue::Bool(true))
        );

        // Find ObstacleRail with instance=ExtResource("1_obs")
        let obstacle = ir.nodes.values().find(|n| n.name == "ObstacleRail").unwrap();
        assert_eq!(
            obstacle.instance_scene.as_deref(),
            Some("res://skate_obstacle.tscn")
        );

        // Verify signal connection
        assert_eq!(ir.connections.len(), 1);
        let conn = &ir.connections[0];
        assert_eq!(conn.signal_name, "trick_landed");
        assert_eq!(conn.method_name, "_on_trick_landed");
        assert_eq!(conn.from_node, skater.id);
        let camera_rig = ir.nodes.values().find(|n| n.name == "CameraRig").unwrap();
        assert_eq!(conn.to_node, camera_rig.id);
    }

    #[test]
    fn parses_starter_template_skate_spot_end_to_end() {
        let tscn_src = include_str!("../../../templates/godot-migration/skate_spot.tscn");
        let ir = parse_tscn_to_ir(tscn_src).expect("template parse must succeed");
        assert_eq!(ir.name, "SkateSpot");
        assert!(ir.nodes.len() >= 5);

        let skater = ir.nodes.values().find(|n| n.name == "Skater").expect("skater node exists");
        assert_eq!(skater.class_name, "CharacterBody3D");
        assert!(skater.groups.contains(&"skaters".to_string()));

        let rail = ir.nodes.values().find(|n| n.name == "ObstacleRail").expect("obstacle rail exists");
        assert_eq!(rail.instance_scene.as_deref(), Some("res://skate_obstacle.tscn"));

        assert_eq!(ir.connections.len(), 1);
        assert_eq!(ir.connections[0].signal_name, "trick_landed");
    }
}

