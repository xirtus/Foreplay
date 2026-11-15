//! Godot 4 `.tres` Resource Codec & Round-Trip Serializer.
//!
//! Mirrors Godot 4's `ResourceFormatLoaderText` & `ResourceFormatSaverText`.
//! Handles `[gd_resource ...]`, `[ext_resource ...]`, `[sub_resource ...]`, and `[resource]`
//! sections with complete Godot 4 typed Variant literals (`Vector2..4`, `Color`, `Transform3D`,
//! `Basis`, `Quaternion`, `NodePath`, `ExtResource`, `SubResource`, packed arrays, and dictionaries).

use foreplay_ir::IrValue;
use std::collections::BTreeMap;
use thiserror::Error;

/// Errors that can occur when parsing Godot 4 `.tres` text.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum TresParseError {
    #[error("unexpected end of file: {0}")]
    UnexpectedEof(String),
    #[error("missing resource header [gd_resource ...]")]
    MissingHeader,
    #[error("syntax error at line {line}: {message}")]
    SyntaxError { line: usize, message: String },
    #[error("invalid value expression: '{0}'")]
    InvalidValue(String),
}

/// Godot 4 Variant typed value representation for resource codecs.
#[derive(Clone, PartialEq, Debug)]
pub enum TresValue {
    Nil,
    Bool(bool),
    I64(i64),
    F64(f64),
    String(String),
    StringName(String),
    Vector2([f32; 2]),
    Vector2i([i32; 2]),
    Vector3([f32; 3]),
    Vector3i([i32; 3]),
    Vector4([f32; 4]),
    Vector4i([i32; 4]),
    Color([f32; 4]),
    Rect2([f32; 4]),
    Rect2i([i32; 4]),
    Transform2D([f32; 6]),
    Transform3D([f32; 12]),
    Basis([f32; 9]),
    Quaternion([f32; 4]),
    NodePath(String),
    ExtResource(String),
    SubResource(String),
    Array(Vec<TresValue>),
    Dictionary(BTreeMap<String, TresValue>),
    PackedByteArray(Vec<u8>),
    PackedInt32Array(Vec<i32>),
    PackedInt64Array(Vec<i64>),
    PackedFloat32Array(Vec<f32>),
    PackedFloat64Array(Vec<f64>),
    PackedStringArray(Vec<String>),
    PackedVector2Array(Vec<[f32; 2]>),
    PackedVector3Array(Vec<[f32; 3]>),
    PackedColorArray(Vec<[f32; 4]>),
}

impl From<&TresValue> for IrValue {
    fn from(tv: &TresValue) -> Self {
        match tv {
            TresValue::Nil => IrValue::Nil,
            TresValue::Bool(b) => IrValue::Bool(*b),
            TresValue::I64(i) => IrValue::I64(*i),
            TresValue::F64(f) => IrValue::F64(*f),
            TresValue::String(s) | TresValue::StringName(s) => IrValue::String(s.clone()),
            TresValue::Vector2(v) => IrValue::Vec2(*v),
            TresValue::Vector2i(v) => IrValue::Vec2([v[0] as f32, v[1] as f32]),
            TresValue::Vector3(v) => IrValue::Vec3(*v),
            TresValue::Vector3i(v) => IrValue::Vec3([v[0] as f32, v[1] as f32, v[2] as f32]),
            TresValue::Vector4(v) => IrValue::Color(*v),
            TresValue::Vector4i(v) => IrValue::Color([v[0] as f32, v[1] as f32, v[2] as f32, v[3] as f32]),
            TresValue::Color(c) => IrValue::Color(*c),
            TresValue::Rect2(r) => IrValue::Color(*r),
            TresValue::Rect2i(r) => IrValue::Color([r[0] as f32, r[1] as f32, r[2] as f32, r[3] as f32]),
            TresValue::Transform2D(t) => {
                IrValue::Array(t.iter().map(|&x| IrValue::F64(x as f64)).collect())
            }
            TresValue::Transform3D(t) => {
                IrValue::Array(t.iter().map(|&x| IrValue::F64(x as f64)).collect())
            }
            TresValue::Basis(b) => {
                IrValue::Array(b.iter().map(|&x| IrValue::F64(x as f64)).collect())
            }
            TresValue::Quaternion(q) => IrValue::Quat(*q),
            TresValue::NodePath(p) => IrValue::NodePath(p.clone()),
            TresValue::ExtResource(id) => IrValue::ResourcePath(id.clone()),
            TresValue::SubResource(id) => IrValue::ResourcePath(format!("sub://{id}")),
            TresValue::Array(arr) => IrValue::Array(arr.iter().map(IrValue::from).collect()),
            TresValue::Dictionary(map) => {
                let mut out = BTreeMap::new();
                for (k, v) in map {
                    out.insert(k.clone(), IrValue::from(v));
                }
                IrValue::Map(out)
            }
            TresValue::PackedByteArray(b) => {
                IrValue::Array(b.iter().map(|&x| IrValue::I64(x as i64)).collect())
            }
            TresValue::PackedInt32Array(i) => {
                IrValue::Array(i.iter().map(|&x| IrValue::I64(x as i64)).collect())
            }
            TresValue::PackedInt64Array(i) => {
                IrValue::Array(i.iter().map(|&x| IrValue::I64(x)).collect())
            }
            TresValue::PackedFloat32Array(f) => {
                IrValue::Array(f.iter().map(|&x| IrValue::F64(x as f64)).collect())
            }
            TresValue::PackedFloat64Array(f) => {
                IrValue::Array(f.iter().map(|&x| IrValue::F64(x)).collect())
            }
            TresValue::PackedStringArray(s) => {
                IrValue::Array(s.iter().map(|x| IrValue::String(x.clone())).collect())
            }
            TresValue::PackedVector2Array(v) => {
                IrValue::Array(v.iter().map(|&x| IrValue::Vec2(x)).collect())
            }
            TresValue::PackedVector3Array(v) => {
                IrValue::Array(v.iter().map(|&x| IrValue::Vec3(x)).collect())
            }
            TresValue::PackedColorArray(c) => {
                IrValue::Array(c.iter().map(|&x| IrValue::Color(x)).collect())
            }
        }
    }
}

/// Header metadata declared in `[gd_resource ...]`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TresHeader {
    pub res_type: String,
    pub script_class: Option<String>,
    pub load_steps: Option<u32>,
    pub format: u32,
    pub uid: Option<String>,
}

/// External resource link declared in `[ext_resource ...]`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TresExtResource {
    pub res_type: String,
    pub path: String,
    pub id: String,
    pub uid: Option<String>,
}

/// Embedded sub-resource declared in `[sub_resource ...]`.
#[derive(Clone, PartialEq, Debug)]
pub struct TresSubResource {
    pub res_type: String,
    pub id: String,
    pub properties: BTreeMap<String, TresValue>,
}

/// Main resource properties declared under `[resource]`.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct TresResource {
    pub properties: BTreeMap<String, TresValue>,
}

/// Complete parsed Godot 4 `.tres` document.
#[derive(Clone, PartialEq, Debug)]
pub struct TresFile {
    pub header: TresHeader,
    pub ext_resources: Vec<TresExtResource>,
    pub sub_resources: Vec<TresSubResource>,
    pub resource: TresResource,
}

/// Parses Godot 4 `.tres` document text into a structured `TresFile`.
pub fn parse_tres(source: &str) -> Result<TresFile, TresParseError> {
    let mut header = None;
    let mut ext_resources = Vec::new();
    let mut sub_resources = Vec::new();
    let mut main_resource = TresResource::default();

    let lines: Vec<&str> = source.lines().collect();
    let mut i = 0;

    #[derive(PartialEq, Eq)]
    enum CurrentSection {
        None,
        SubResource(usize),
        MainResource,
    }

    let mut current_section = CurrentSection::None;

    while i < lines.len() {
        let line = lines[i].trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            i += 1;
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            let section_content = &line[1..line.len() - 1].trim();
            let (tag, attrs_str) = split_first_word(section_content);
            let attrs = parse_attributes(attrs_str);

            match tag {
                "gd_resource" => {
                    let res_type = attrs
                        .get("type")
                        .cloned()
                        .unwrap_or_default()
                        .trim_matches('"')
                        .to_string();
                    let script_class = attrs.get("script_class").map(|s| s.trim_matches('"').to_string());
                    let load_steps = attrs.get("load_steps").and_then(|s| s.parse::<u32>().ok());
                    let format = attrs
                        .get("format")
                        .and_then(|s| s.parse::<u32>().ok())
                        .unwrap_or(3);
                    let uid = attrs.get("uid").map(|s| s.trim_matches('"').to_string());

                    header = Some(TresHeader {
                        res_type,
                        script_class,
                        load_steps,
                        format,
                        uid,
                    });
                    current_section = CurrentSection::None;
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

                    ext_resources.push(TresExtResource {
                        res_type,
                        path,
                        id,
                        uid,
                    });
                    current_section = CurrentSection::None;
                    i += 1;
                }
                "sub_resource" => {
                    let res_type = attrs
                        .get("type")
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

                    sub_resources.push(TresSubResource {
                        res_type,
                        id,
                        properties: BTreeMap::new(),
                    });
                    current_section = CurrentSection::SubResource(sub_resources.len() - 1);
                    i += 1;
                }
                "resource" => {
                    current_section = CurrentSection::MainResource;
                    i += 1;
                }
                _ => {
                    // Unknown section, ignore
                    current_section = CurrentSection::None;
                    i += 1;
                }
            }
            continue;
        }

        // Must be a key = value property line (potentially multiline)
        if let Some((key, val_initial)) = split_key_value(line) {
            let mut accumulated_val = val_initial.to_string();
            // Check if brackets, braces, parentheses, or quotes are unclosed
            while !is_balanced(&accumulated_val) && i + 1 < lines.len() {
                i += 1;
                accumulated_val.push('\n');
                accumulated_val.push_str(lines[i]);
            }

            let parsed_val = parse_tres_value(&accumulated_val)?;
            match current_section {
                CurrentSection::SubResource(idx) => {
                    if let Some(sub) = sub_resources.get_mut(idx) {
                        sub.properties.insert(key.to_string(), parsed_val);
                    }
                }
                CurrentSection::MainResource => {
                    main_resource.properties.insert(key.to_string(), parsed_val);
                }
                CurrentSection::None => {}
            }
        }

        i += 1;
    }

    let header = header.ok_or(TresParseError::MissingHeader)?;

    Ok(TresFile {
        header,
        ext_resources,
        sub_resources,
        resource: main_resource,
    })
}

/// Emits deterministic Godot 4 `.tres` document text from structured `TresFile`.
pub fn emit_tres(file: &TresFile) -> String {
    let mut out = String::new();

    // 1. [gd_resource ...]
    out.push_str(&format!("[gd_resource type=\"{}\"", escape_godot_str(&file.header.res_type)));
    if let Some(sc) = &file.header.script_class {
        out.push_str(&format!(" script_class=\"{}\"", escape_godot_str(sc)));
    }
    if let Some(ls) = file.header.load_steps {
        out.push_str(&format!(" load_steps={ls}"));
    }
    out.push_str(&format!(" format={}", file.header.format));
    if let Some(uid) = &file.header.uid {
        out.push_str(&format!(" uid=\"{}\"", escape_godot_str(uid)));
    }
    out.push_str("]\n");

    // 2. [ext_resource ...]
    let mut sorted_ext = file.ext_resources.clone();
    sorted_ext.sort_by(|a, b| a.id.cmp(&b.id));
    for ext in sorted_ext {
        out.push('\n');
        out.push_str(&format!("[ext_resource type=\"{}\"", escape_godot_str(&ext.res_type)));
        if let Some(uid) = &ext.uid {
            out.push_str(&format!(" uid=\"{}\"", escape_godot_str(uid)));
        }
        out.push_str(&format!(
            " path=\"{}\" id=\"{}\"]\n",
            escape_godot_str(&ext.path),
            escape_godot_str(&ext.id)
        ));
    }

    // 3. [sub_resource ...]
    let mut sorted_sub = file.sub_resources.clone();
    sorted_sub.sort_by(|a, b| a.id.cmp(&b.id));
    for sub in sorted_sub {
        out.push('\n');
        out.push_str(&format!(
            "[sub_resource type=\"{}\" id=\"{}\"]\n",
            escape_godot_str(&sub.res_type),
            escape_godot_str(&sub.id)
        ));
        for (k, v) in &sub.properties {
            out.push_str(&format!("{k} = {}\n", format_tres_value(v)));
        }
    }

    // 4. [resource]
    out.push_str("\n[resource]\n");
    for (k, v) in &file.resource.properties {
        out.push_str(&format!("{k} = {}\n", format_tres_value(v)));
    }

    out
}

/// Parses a Godot 4 Variant textual expression into `TresValue`.
pub fn parse_tres_value(expr: &str) -> Result<TresValue, TresParseError> {
    let s = expr.trim();
    if s == "null" || s.is_empty() {
        return Ok(TresValue::Nil);
    }
    if s == "true" {
        return Ok(TresValue::Bool(true));
    }
    if s == "false" {
        return Ok(TresValue::Bool(false));
    }

    // String literal "..."
    if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
        let inner = &s[1..s.len() - 1];
        return Ok(TresValue::String(unescape_godot_str(inner)));
    }

    // StringName &"..."
    if s.starts_with("&\"") && s.ends_with('"') && s.len() >= 3 {
        let inner = &s[2..s.len() - 1];
        return Ok(TresValue::StringName(unescape_godot_str(inner)));
    }

    // NodePath("...") or ^"..."
    if s.starts_with("NodePath(") && s.ends_with(')') {
        let inner = &s["NodePath(".len()..s.len() - 1].trim();
        let path = inner.trim_matches('"');
        return Ok(TresValue::NodePath(unescape_godot_str(path)));
    }
    if s.starts_with("^\"") && s.ends_with('"') && s.len() >= 3 {
        let inner = &s[2..s.len() - 1];
        return Ok(TresValue::NodePath(unescape_godot_str(inner)));
    }

    // ExtResource("...")
    if s.starts_with("ExtResource(") && s.ends_with(')') {
        let inner = &s["ExtResource(".len()..s.len() - 1].trim();
        let id = inner.trim_matches('"');
        return Ok(TresValue::ExtResource(id.to_string()));
    }

    // SubResource("...")
    if s.starts_with("SubResource(") && s.ends_with(')') {
        let inner = &s["SubResource(".len()..s.len() - 1].trim();
        let id = inner.trim_matches('"');
        return Ok(TresValue::SubResource(id.to_string()));
    }

    // Vector2(x, y)
    if s.starts_with("Vector2(") && s.ends_with(')') {
        let inner = &s["Vector2(".len()..s.len() - 1];
        let nums = parse_f32_list(inner)?;
        if nums.len() == 2 {
            return Ok(TresValue::Vector2([nums[0], nums[1]]));
        }
        return Err(TresParseError::InvalidValue(s.to_string()));
    }

    // Vector2i(x, y)
    if s.starts_with("Vector2i(") && s.ends_with(')') {
        let inner = &s["Vector2i(".len()..s.len() - 1];
        let nums = parse_i32_list(inner)?;
        if nums.len() == 2 {
            return Ok(TresValue::Vector2i([nums[0], nums[1]]));
        }
        return Err(TresParseError::InvalidValue(s.to_string()));
    }

    // Vector3(x, y, z)
    if s.starts_with("Vector3(") && s.ends_with(')') {
        let inner = &s["Vector3(".len()..s.len() - 1];
        let nums = parse_f32_list(inner)?;
        if nums.len() == 3 {
            return Ok(TresValue::Vector3([nums[0], nums[1], nums[2]]));
        }
        return Err(TresParseError::InvalidValue(s.to_string()));
    }

    // Vector3i(x, y, z)
    if s.starts_with("Vector3i(") && s.ends_with(')') {
        let inner = &s["Vector3i(".len()..s.len() - 1];
        let nums = parse_i32_list(inner)?;
        if nums.len() == 3 {
            return Ok(TresValue::Vector3i([nums[0], nums[1], nums[2]]));
        }
        return Err(TresParseError::InvalidValue(s.to_string()));
    }

    // Vector4(x, y, z, w)
    if s.starts_with("Vector4(") && s.ends_with(')') {
        let inner = &s["Vector4(".len()..s.len() - 1];
        let nums = parse_f32_list(inner)?;
        if nums.len() == 4 {
            return Ok(TresValue::Vector4([nums[0], nums[1], nums[2], nums[3]]));
        }
        return Err(TresParseError::InvalidValue(s.to_string()));
    }

    // Vector4i(x, y, z, w)
    if s.starts_with("Vector4i(") && s.ends_with(')') {
        let inner = &s["Vector4i(".len()..s.len() - 1];
        let nums = parse_i32_list(inner)?;
        if nums.len() == 4 {
            return Ok(TresValue::Vector4i([nums[0], nums[1], nums[2], nums[3]]));
        }
        return Err(TresParseError::InvalidValue(s.to_string()));
    }

    // Color(r, g, b, a)
    if s.starts_with("Color(") && s.ends_with(')') {
        let inner = &s["Color(".len()..s.len() - 1];
        let nums = parse_f32_list(inner)?;
        if nums.len() == 4 {
            return Ok(TresValue::Color([nums[0], nums[1], nums[2], nums[3]]));
        }
        if nums.len() == 3 {
            return Ok(TresValue::Color([nums[0], nums[1], nums[2], 1.0]));
        }
        return Err(TresParseError::InvalidValue(s.to_string()));
    }

    // Rect2(x, y, w, h)
    if s.starts_with("Rect2(") && s.ends_with(')') {
        let inner = &s["Rect2(".len()..s.len() - 1];
        let nums = parse_f32_list(inner)?;
        if nums.len() == 4 {
            return Ok(TresValue::Rect2([nums[0], nums[1], nums[2], nums[3]]));
        }
        return Err(TresParseError::InvalidValue(s.to_string()));
    }

    // Rect2i(x, y, w, h)
    if s.starts_with("Rect2i(") && s.ends_with(')') {
        let inner = &s["Rect2i(".len()..s.len() - 1];
        let nums = parse_i32_list(inner)?;
        if nums.len() == 4 {
            return Ok(TresValue::Rect2i([nums[0], nums[1], nums[2], nums[3]]));
        }
        return Err(TresParseError::InvalidValue(s.to_string()));
    }

    // Transform2D(...)
    if s.starts_with("Transform2D(") && s.ends_with(')') {
        let inner = &s["Transform2D(".len()..s.len() - 1];
        let nums = parse_f32_list(inner)?;
        if nums.len() == 6 {
            let mut arr = [0.0f32; 6];
            arr.copy_from_slice(&nums[..6]);
            return Ok(TresValue::Transform2D(arr));
        }
        return Err(TresParseError::InvalidValue(s.to_string()));
    }

    // Transform3D(...)
    if s.starts_with("Transform3D(") && s.ends_with(')') {
        let inner = &s["Transform3D(".len()..s.len() - 1];
        let nums = parse_f32_list(inner)?;
        if nums.len() == 12 {
            let mut arr = [0.0f32; 12];
            arr.copy_from_slice(&nums[..12]);
            return Ok(TresValue::Transform3D(arr));
        }
        return Err(TresParseError::InvalidValue(s.to_string()));
    }

    // Basis(...)
    if s.starts_with("Basis(") && s.ends_with(')') {
        let inner = &s["Basis(".len()..s.len() - 1];
        let nums = parse_f32_list(inner)?;
        if nums.len() == 9 {
            let mut arr = [0.0f32; 9];
            arr.copy_from_slice(&nums[..9]);
            return Ok(TresValue::Basis(arr));
        }
        return Err(TresParseError::InvalidValue(s.to_string()));
    }

    // Quaternion(...)
    if s.starts_with("Quaternion(") && s.ends_with(')') {
        let inner = &s["Quaternion(".len()..s.len() - 1];
        let nums = parse_f32_list(inner)?;
        if nums.len() == 4 {
            return Ok(TresValue::Quaternion([nums[0], nums[1], nums[2], nums[3]]));
        }
        return Err(TresParseError::InvalidValue(s.to_string()));
    }

    // PackedByteArray(...)
    if s.starts_with("PackedByteArray(") && s.ends_with(')') {
        let inner = &s["PackedByteArray(".len()..s.len() - 1];
        let bytes = parse_u8_list(inner)?;
        return Ok(TresValue::PackedByteArray(bytes));
    }

    // PackedInt32Array(...)
    if s.starts_with("PackedInt32Array(") && s.ends_with(')') {
        let inner = &s["PackedInt32Array(".len()..s.len() - 1];
        let ints = parse_i32_list(inner)?;
        return Ok(TresValue::PackedInt32Array(ints));
    }

    // PackedInt64Array(...)
    if s.starts_with("PackedInt64Array(") && s.ends_with(')') {
        let inner = &s["PackedInt64Array(".len()..s.len() - 1];
        let ints = parse_i64_list(inner)?;
        return Ok(TresValue::PackedInt64Array(ints));
    }

    // PackedFloat32Array(...)
    if s.starts_with("PackedFloat32Array(") && s.ends_with(')') {
        let inner = &s["PackedFloat32Array(".len()..s.len() - 1];
        let floats = parse_f32_list(inner)?;
        return Ok(TresValue::PackedFloat32Array(floats));
    }

    // PackedFloat64Array(...)
    if s.starts_with("PackedFloat64Array(") && s.ends_with(')') {
        let inner = &s["PackedFloat64Array(".len()..s.len() - 1];
        let floats = parse_f64_list(inner)?;
        return Ok(TresValue::PackedFloat64Array(floats));
    }

    // PackedStringArray(...)
    if s.starts_with("PackedStringArray(") && s.ends_with(')') {
        let inner = &s["PackedStringArray(".len()..s.len() - 1];
        let strings = split_comma_separated(inner)
            .into_iter()
            .map(|part| unescape_godot_str(part.trim().trim_matches('"')))
            .collect();
        return Ok(TresValue::PackedStringArray(strings));
    }

    // PackedVector2Array(...)
    if s.starts_with("PackedVector2Array(") && s.ends_with(')') {
        let inner = &s["PackedVector2Array(".len()..s.len() - 1];
        let nums = parse_f32_list(inner)?;
        let mut vecs = Vec::with_capacity(nums.len() / 2);
        for chunk in nums.chunks_exact(2) {
            vecs.push([chunk[0], chunk[1]]);
        }
        return Ok(TresValue::PackedVector2Array(vecs));
    }

    // PackedVector3Array(...)
    if s.starts_with("PackedVector3Array(") && s.ends_with(')') {
        let inner = &s["PackedVector3Array(".len()..s.len() - 1];
        let nums = parse_f32_list(inner)?;
        let mut vecs = Vec::with_capacity(nums.len() / 3);
        for chunk in nums.chunks_exact(3) {
            vecs.push([chunk[0], chunk[1], chunk[2]]);
        }
        return Ok(TresValue::PackedVector3Array(vecs));
    }

    // PackedColorArray(...)
    if s.starts_with("PackedColorArray(") && s.ends_with(')') {
        let inner = &s["PackedColorArray(".len()..s.len() - 1];
        let nums = parse_f32_list(inner)?;
        let mut colors = Vec::with_capacity(nums.len() / 4);
        for chunk in nums.chunks_exact(4) {
            colors.push([chunk[0], chunk[1], chunk[2], chunk[3]]);
        }
        return Ok(TresValue::PackedColorArray(colors));
    }

    // Untyped Array [...]
    if s.starts_with('[') && s.ends_with(']') {
        let inner = &s[1..s.len() - 1];
        let parts = split_comma_separated(inner);
        let mut vals = Vec::with_capacity(parts.len());
        for p in parts {
            let pt = p.trim();
            if !pt.is_empty() {
                vals.push(parse_tres_value(pt)?);
            }
        }
        return Ok(TresValue::Array(vals));
    }

    // Dictionary { ... }
    if s.starts_with('{') && s.ends_with('}') {
        let inner = &s[1..s.len() - 1];
        let pairs = split_comma_separated(inner);
        let mut map = BTreeMap::new();
        for pair in pairs {
            let pt = pair.trim();
            if pt.is_empty() {
                continue;
            }
            if let Some((k_expr, v_expr)) = split_dict_entry(pt) {
                let clean_key = k_expr.trim().trim_start_matches('&').trim_matches('"');
                let val = parse_tres_value(v_expr.trim())?;
                map.insert(clean_key.to_string(), val);
            }
        }
        return Ok(TresValue::Dictionary(map));
    }

    // Integer
    if let Ok(i) = s.parse::<i64>() {
        return Ok(TresValue::I64(i));
    }

    // Float
    if let Ok(f) = s.parse::<f64>() {
        return Ok(TresValue::F64(f));
    }

    // Fallback unquoted string
    Ok(TresValue::String(s.to_string()))
}

/// Formats `TresValue` into deterministic Godot 4 text.
pub fn format_tres_value(val: &TresValue) -> String {
    match val {
        TresValue::Nil => "null".to_string(),
        TresValue::Bool(b) => {
            if *b {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
        TresValue::I64(i) => format!("{i}"),
        TresValue::F64(f) => format_f64(*f),
        TresValue::String(s) => format!("\"{}\"", escape_godot_str(s)),
        TresValue::StringName(s) => format!("&\"{}\"", escape_godot_str(s)),
        TresValue::Vector2([x, y]) => format!("Vector2({x}, {y})"),
        TresValue::Vector2i([x, y]) => format!("Vector2i({x}, {y})"),
        TresValue::Vector3([x, y, z]) => format!("Vector3({x}, {y}, {z})"),
        TresValue::Vector3i([x, y, z]) => format!("Vector3i({x}, {y}, {z})"),
        TresValue::Vector4([x, y, z, w]) => format!("Vector4({x}, {y}, {z}, {w})"),
        TresValue::Vector4i([x, y, z, w]) => format!("Vector4i({x}, {y}, {z}, {w})"),
        TresValue::Color([r, g, b, a]) => format!("Color({r}, {g}, {b}, {a})"),
        TresValue::Rect2([x, y, w, h]) => format!("Rect2({x}, {y}, {w}, {h})"),
        TresValue::Rect2i([x, y, w, h]) => format!("Rect2i({x}, {y}, {w}, {h})"),
        TresValue::Transform2D([xx, xy, yx, yy, ox, oy]) => {
            format!("Transform2D({xx}, {xy}, {yx}, {yy}, {ox}, {oy})")
        }
        TresValue::Transform3D(arr) => {
            let inner = arr.iter().map(|f| f.to_string()).collect::<Vec<_>>().join(", ");
            format!("Transform3D({inner})")
        }
        TresValue::Basis(arr) => {
            let inner = arr.iter().map(|f| f.to_string()).collect::<Vec<_>>().join(", ");
            format!("Basis({inner})")
        }
        TresValue::Quaternion([x, y, z, w]) => format!("Quaternion({x}, {y}, {z}, {w})"),
        TresValue::NodePath(p) => format!("NodePath(\"{}\")", escape_godot_str(p)),
        TresValue::ExtResource(id) => format!("ExtResource(\"{}\")", escape_godot_str(id)),
        TresValue::SubResource(id) => format!("SubResource(\"{}\")", escape_godot_str(id)),
        TresValue::Array(arr) => {
            let inner = arr.iter().map(format_tres_value).collect::<Vec<_>>().join(", ");
            format!("[{inner}]")
        }
        TresValue::Dictionary(map) => {
            let inner = map
                .iter()
                .map(|(k, v)| format!("\"{}\": {}", escape_godot_str(k), format_tres_value(v)))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{{{inner}}}")
        }
        TresValue::PackedByteArray(b) => {
            let inner = b.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(", ");
            format!("PackedByteArray({inner})")
        }
        TresValue::PackedInt32Array(ia) => {
            let inner = ia.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(", ");
            format!("PackedInt32Array({inner})")
        }
        TresValue::PackedInt64Array(ia) => {
            let inner = ia.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(", ");
            format!("PackedInt64Array({inner})")
        }
        TresValue::PackedFloat32Array(fa) => {
            let inner = fa.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(", ");
            format!("PackedFloat32Array({inner})")
        }
        TresValue::PackedFloat64Array(fa) => {
            let inner = fa.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(", ");
            format!("PackedFloat64Array({inner})")
        }
        TresValue::PackedStringArray(sa) => {
            let inner = sa
                .iter()
                .map(|s| format!("\"{}\"", escape_godot_str(s)))
                .collect::<Vec<_>>()
                .join(", ");
            format!("PackedStringArray({inner})")
        }
        TresValue::PackedVector2Array(va) => {
            let inner = va
                .iter()
                .flat_map(|[x, y]| vec![x.to_string(), y.to_string()])
                .collect::<Vec<_>>()
                .join(", ");
            format!("PackedVector2Array({inner})")
        }
        TresValue::PackedVector3Array(va) => {
            let inner = va
                .iter()
                .flat_map(|[x, y, z]| vec![x.to_string(), y.to_string(), z.to_string()])
                .collect::<Vec<_>>()
                .join(", ");
            format!("PackedVector3Array({inner})")
        }
        TresValue::PackedColorArray(ca) => {
            let inner = ca
                .iter()
                .flat_map(|[r, g, b, a]| vec![r.to_string(), g.to_string(), b.to_string(), a.to_string()])
                .collect::<Vec<_>>()
                .join(", ");
            format!("PackedColorArray({inner})")
        }
    }
}

fn format_f64(f: f64) -> String {
    if f.fract() == 0.0 && !f.is_infinite() && !f.is_nan() {
        format!("{f:.1}")
    } else {
        format!("{f}")
    }
}

fn escape_godot_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            other => out.push(other),
        }
    }
    out
}

fn unescape_godot_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
                match next {
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    'r' => out.push('\r'),
                    '\"' => out.push('\"'),
                    '\\' => out.push('\\'),
                    other => {
                        out.push('\\');
                        out.push(other);
                    }
                }
            } else {
                out.push('\\');
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn is_balanced(s: &str) -> bool {
    let mut depth_paren = 0;
    let mut depth_bracket = 0;
    let mut depth_brace = 0;
    let mut in_quote = false;
    let chars: Vec<char> = s.chars().collect();

    for i in 0..chars.len() {
        let c = chars[i];
        if c == '"' && (i == 0 || chars[i - 1] != '\\') {
            in_quote = !in_quote;
        } else if !in_quote {
            match c {
                '(' => depth_paren += 1,
                ')' if depth_paren > 0 => depth_paren -= 1,
                '[' => depth_bracket += 1,
                ']' if depth_bracket > 0 => depth_bracket -= 1,
                '{' => depth_brace += 1,
                '}' if depth_brace > 0 => depth_brace -= 1,
                _ => {}
            }
        }
    }

    !in_quote && depth_paren == 0 && depth_bracket == 0 && depth_brace == 0
}

fn split_first_word(s: &str) -> (&str, &str) {
    let s = s.trim();
    if let Some(idx) = s.find(char::is_whitespace) {
        (&s[..idx], s[idx..].trim())
    } else {
        (s, "")
    }
}

fn parse_attributes(attrs_str: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    let chars: Vec<char> = attrs_str.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        if i >= chars.len() {
            break;
        }

        let key_start = i;
        while i < chars.len() && chars[i] != '=' && !chars[i].is_whitespace() {
            i += 1;
        }
        let key: String = chars[key_start..i].iter().collect();

        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        if i < chars.len() && chars[i] == '=' {
            i += 1;
        } else {
            if !key.is_empty() {
                map.insert(key, "true".to_string());
            }
            continue;
        }

        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        if i >= chars.len() {
            map.insert(key, String::new());
            break;
        }

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

    map
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

fn split_dict_entry(s: &str) -> Option<(&str, &str)> {
    let mut in_quote = false;
    let mut depth_paren = 0;
    let mut depth_bracket = 0;
    let mut depth_brace = 0;

    for (idx, c) in s.char_indices() {
        if c == '"' {
            in_quote = !in_quote;
        } else if !in_quote {
            match c {
                '(' => depth_paren += 1,
                ')' if depth_paren > 0 => depth_paren -= 1,
                '[' => depth_bracket += 1,
                ']' if depth_bracket > 0 => depth_bracket -= 1,
                '{' => depth_brace += 1,
                '}' if depth_brace > 0 => depth_brace -= 1,
                ':' if depth_paren == 0 && depth_bracket == 0 && depth_brace == 0 => {
                    let k = s[..idx].trim();
                    let v = s[idx + 1..].trim();
                    return Some((k, v));
                }
                _ => {}
            }
        }
    }
    None
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
                ')' if depth_paren > 0 => depth_paren -= 1,
                '[' => depth_bracket += 1,
                ']' if depth_bracket > 0 => depth_bracket -= 1,
                '{' => depth_brace += 1,
                '}' if depth_brace > 0 => depth_brace -= 1,
                ',' if depth_paren == 0 && depth_bracket == 0 && depth_brace == 0 => {
                    parts.push(s[start..idx].trim());
                    start = idx + 1;
                }
                _ => {}
            }
        }
    }

    if start < s.len() {
        let rest = s[start..].trim();
        if !rest.is_empty() {
            parts.push(rest);
        }
    }

    parts
}

fn parse_f32_list(s: &str) -> Result<Vec<f32>, TresParseError> {
    let mut out = Vec::new();
    for part in s.split(',') {
        let p = part.trim();
        if !p.is_empty() {
            let val = p
                .parse::<f32>()
                .map_err(|_| TresParseError::InvalidValue(s.to_string()))?;
            out.push(val);
        }
    }
    Ok(out)
}

fn parse_f64_list(s: &str) -> Result<Vec<f64>, TresParseError> {
    let mut out = Vec::new();
    for part in s.split(',') {
        let p = part.trim();
        if !p.is_empty() {
            let val = p
                .parse::<f64>()
                .map_err(|_| TresParseError::InvalidValue(s.to_string()))?;
            out.push(val);
        }
    }
    Ok(out)
}

fn parse_i32_list(s: &str) -> Result<Vec<i32>, TresParseError> {
    let mut out = Vec::new();
    for part in s.split(',') {
        let p = part.trim();
        if !p.is_empty() {
            let val = p
                .parse::<i32>()
                .map_err(|_| TresParseError::InvalidValue(s.to_string()))?;
            out.push(val);
        }
    }
    Ok(out)
}

fn parse_i64_list(s: &str) -> Result<Vec<i64>, TresParseError> {
    let mut out = Vec::new();
    for part in s.split(',') {
        let p = part.trim();
        if !p.is_empty() {
            let val = p
                .parse::<i64>()
                .map_err(|_| TresParseError::InvalidValue(s.to_string()))?;
            out.push(val);
        }
    }
    Ok(out)
}

fn parse_u8_list(s: &str) -> Result<Vec<u8>, TresParseError> {
    let mut out = Vec::new();
    for part in s.split(',') {
        let p = part.trim();
        if !p.is_empty() {
            let val = p
                .parse::<u8>()
                .map_err(|_| TresParseError::InvalidValue(s.to_string()))?;
            out.push(val);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_standard_material_3d() {
        let text = r#"[gd_resource type="StandardMaterial3D" load_steps=2 format=3 uid="uid://mat_pbr_01"]

[ext_resource type="Texture2D" uid="uid://tex_albedo_01" path="res://textures/brick_albedo.png" id="1_tex"]

[resource]
albedo_color = Color(0.8, 0.2, 0.2, 1)
albedo_texture = ExtResource("1_tex")
metallic = 0.75
metallic_specular = 0.5
roughness = 0.2
"#;
        let parsed = parse_tres(text).expect("StandardMaterial3D should parse");
        assert_eq!(parsed.header.res_type, "StandardMaterial3D");
        assert_eq!(parsed.header.load_steps, Some(2));
        assert_eq!(parsed.header.uid.as_deref(), Some("uid://mat_pbr_01"));
        assert_eq!(parsed.ext_resources.len(), 1);
        assert_eq!(parsed.resource.properties.len(), 5);

        let emitted = emit_tres(&parsed);
        let re_parsed = parse_tres(&emitted).expect("emitted StandardMaterial3D should re-parse");
        assert_eq!(re_parsed, parsed);
    }

    #[test]
    fn roundtrip_shader_material_with_subresource() {
        let text = r#"[gd_resource type="ShaderMaterial" load_steps=3 format=3 uid="uid://shd_mat_01"]

[sub_resource type="Shader" id="Shader_water"]
code = "shader_type spatial;\nvoid fragment() { ALBEDO = vec3(0.1, 0.4, 0.8); }"

[resource]
shader = SubResource("Shader_water")
shader_parameter/depth_blend = 2.5
shader_parameter/wave_speed = 1.2
"#;
        let parsed = parse_tres(text).expect("ShaderMaterial should parse");
        assert_eq!(parsed.sub_resources.len(), 1);
        assert_eq!(parsed.sub_resources[0].res_type, "Shader");
        assert_eq!(parsed.sub_resources[0].id, "Shader_water");

        let emitted = emit_tres(&parsed);
        let re_parsed = parse_tres(&emitted).expect("emitted ShaderMaterial should re-parse");
        assert_eq!(re_parsed, parsed);
    }

    #[test]
    fn roundtrip_environment_resource() {
        let text = r#"[gd_resource type="Environment" format=3 uid="uid://env_world_01"]

[resource]
ambient_light_color = Color(0.15, 0.15, 0.2, 1)
ambient_light_energy = 1.0
background_mode = 2
glow_blend_mode = 0
glow_enabled = true
glow_intensity = 0.8
tonemap_mode = 3
"#;
        let parsed = parse_tres(text).expect("Environment should parse");
        assert_eq!(parsed.header.res_type, "Environment");
        assert_eq!(parsed.resource.properties.get("glow_enabled"), Some(&TresValue::Bool(true)));

        let emitted = emit_tres(&parsed);
        let re_parsed = parse_tres(&emitted).expect("emitted Environment should re-parse");
        assert_eq!(re_parsed, parsed);
    }

    #[test]
    fn roundtrip_box_shape_3d() {
        let text = r#"[gd_resource type="BoxShape3D" format=3 uid="uid://box_col_01"]

[resource]
size = Vector3(2, 4.5, 6)
"#;
        let parsed = parse_tres(text).expect("BoxShape3D should parse");
        assert_eq!(parsed.resource.properties.get("size"), Some(&TresValue::Vector3([2.0, 4.5, 6.0])));

        let emitted = emit_tres(&parsed);
        let re_parsed = parse_tres(&emitted).expect("emitted BoxShape3D should re-parse");
        assert_eq!(re_parsed, parsed);
    }

    #[test]
    fn roundtrip_capsule_shape_3d() {
        let text = r#"[gd_resource type="CapsuleShape3D" format=3 uid="uid://cap_col_01"]

[resource]
height = 1.8
radius = 0.4
"#;
        let parsed = parse_tres(text).expect("CapsuleShape3D should parse");
        assert_eq!(parsed.resource.properties.get("height"), Some(&TresValue::F64(1.8)));
        assert_eq!(parsed.resource.properties.get("radius"), Some(&TresValue::F64(0.4)));

        let emitted = emit_tres(&parsed);
        let re_parsed = parse_tres(&emitted).expect("emitted CapsuleShape3D should re-parse");
        assert_eq!(re_parsed, parsed);
    }

    #[test]
    fn roundtrip_sprite_frames_with_dictionary_array() {
        let text = r#"[gd_resource type="SpriteFrames" format=3 uid="uid://anim_frames_01"]

[resource]
animations = [{
"frames": [],
"loop": true,
"name": &"run",
"speed": 10.0
}]
"#;
        let parsed = parse_tres(text).expect("SpriteFrames should parse");
        let anims = parsed.resource.properties.get("animations").expect("animations prop");
        if let TresValue::Array(arr) = anims {
            assert_eq!(arr.len(), 1);
            if let TresValue::Dictionary(dict) = &arr[0] {
                assert_eq!(dict.get("loop"), Some(&TresValue::Bool(true)));
                assert_eq!(dict.get("name"), Some(&TresValue::StringName("run".to_string())));
                assert_eq!(dict.get("speed"), Some(&TresValue::F64(10.0)));
            } else {
                panic!("expected dictionary entry");
            }
        } else {
            panic!("expected array");
        }

        let emitted = emit_tres(&parsed);
        let re_parsed = parse_tres(&emitted).expect("emitted SpriteFrames should re-parse");
        assert_eq!(re_parsed, parsed);
    }

    #[test]
    fn roundtrip_theme_resource() {
        let text = r#"[gd_resource type="Theme" format=3 uid="uid://gui_theme_01"]

[resource]
default_font_size = 14
"#;
        let parsed = parse_tres(text).expect("Theme should parse");
        assert_eq!(parsed.resource.properties.get("default_font_size"), Some(&TresValue::I64(14)));

        let emitted = emit_tres(&parsed);
        let re_parsed = parse_tres(&emitted).expect("emitted Theme should re-parse");
        assert_eq!(re_parsed, parsed);
    }

    #[test]
    fn roundtrip_audio_bus_layout() {
        let text = r#"[gd_resource type="AudioBusLayout" format=3 uid="uid://audio_bus_01"]

[resource]
bus/0/bypass_fx = false
bus/0/mute = false
bus/0/name = &"Master"
bus/0/solo = false
bus/0/volume_db = 0.0
"#;
        let parsed = parse_tres(text).expect("AudioBusLayout should parse");
        assert_eq!(parsed.resource.properties.get("bus/0/name"), Some(&TresValue::StringName("Master".to_string())));

        let emitted = emit_tres(&parsed);
        let re_parsed = parse_tres(&emitted).expect("emitted AudioBusLayout should re-parse");
        assert_eq!(re_parsed, parsed);
    }

    #[test]
    fn roundtrip_tileset_with_vector2i() {
        let text = r#"[gd_resource type="TileSet" format=3 uid="uid://tile_set_01"]

[resource]
tile_size = Vector2i(32, 32)
"#;
        let parsed = parse_tres(text).expect("TileSet should parse");
        assert_eq!(parsed.resource.properties.get("tile_size"), Some(&TresValue::Vector2i([32, 32])));

        let emitted = emit_tres(&parsed);
        let re_parsed = parse_tres(&emitted).expect("emitted TileSet should re-parse");
        assert_eq!(re_parsed, parsed);
    }

    #[test]
    fn roundtrip_input_event_action() {
        let text = r#"[gd_resource type="InputEventAction" format=3 uid="uid://input_act_01"]

[resource]
action = &"jump"
pressed = true
strength = 1.0
"#;
        let parsed = parse_tres(text).expect("InputEventAction should parse");
        assert_eq!(parsed.resource.properties.get("action"), Some(&TresValue::StringName("jump".to_string())));
        assert_eq!(parsed.resource.properties.get("pressed"), Some(&TresValue::Bool(true)));
        assert_eq!(parsed.resource.properties.get("strength"), Some(&TresValue::F64(1.0)));

        let emitted = emit_tres(&parsed);
        let re_parsed = parse_tres(&emitted).expect("emitted InputEventAction should re-parse");
        assert_eq!(re_parsed, parsed);
    }

    #[test]
    fn roundtrip_custom_script_resource() {
        let text = r#"[gd_resource type="Resource" script_class="PlayerStats" load_steps=2 format=3 uid="uid://custom_stats_01"]

[ext_resource type="Script" path="res://scripts/player_stats.gd" id="1_scr"]

[resource]
invincible = false
max_health = 100
move_speed = 5.5
script = ExtResource("1_scr")
"#;
        let parsed = parse_tres(text).expect("custom Resource should parse");
        assert_eq!(parsed.header.script_class.as_deref(), Some("PlayerStats"));
        assert_eq!(parsed.resource.properties.get("script"), Some(&TresValue::ExtResource("1_scr".to_string())));
        assert_eq!(parsed.resource.properties.get("max_health"), Some(&TresValue::I64(100)));

        let emitted = emit_tres(&parsed);
        let re_parsed = parse_tres(&emitted).expect("emitted custom Resource should re-parse");
        assert_eq!(re_parsed, parsed);
    }

    #[test]
    fn roundtrip_transforms_basis_and_packed_arrays() {
        let text = r#"[gd_resource type="Resource" format=3 uid="uid://math_test_01"]

[resource]
basis_val = Basis(1, 0, 0, 0, 1, 0, 0, 0, 1)
packed_bytes = PackedByteArray(1, 2, 3, 255)
packed_floats = PackedFloat32Array(0.5, 1.5, 2.5)
packed_ints = PackedInt32Array(10, 20, -30)
packed_strings = PackedStringArray("apple", "banana", "cherry")
quat_val = Quaternion(0, 0, 0.7071068, 0.7071068)
transform_val = Transform3D(1, 0, 0, 0, 1, 0, 0, 0, 1, 10, 20, 30)
vec4_val = Vector4(1, 2, 3, 4)
"#;
        let parsed = parse_tres(text).expect("math types should parse");
        let emitted = emit_tres(&parsed);
        let re_parsed = parse_tres(&emitted).expect("emitted math types should re-parse");
        assert_eq!(re_parsed, parsed);
    }

    #[test]
    fn conversion_between_tres_value_and_ir_value() {
        let tv_vec = TresValue::Vector3([1.0, 2.0, 3.0]);
        let ir_vec: IrValue = (&tv_vec).into();
        assert_eq!(ir_vec, IrValue::Vec3([1.0, 2.0, 3.0]));

        let tv_dict = TresValue::Dictionary(BTreeMap::from([
            ("speed".to_string(), TresValue::F64(4.0)),
            ("label".to_string(), TresValue::String("fast".to_string())),
        ]));
        let ir_dict: IrValue = (&tv_dict).into();
        if let IrValue::Map(m) = ir_dict {
            assert_eq!(m.get("speed"), Some(&IrValue::F64(4.0)));
            assert_eq!(m.get("label"), Some(&IrValue::String("fast".to_string())));
        } else {
            panic!("expected IrValue::Map");
        }
    }
}
