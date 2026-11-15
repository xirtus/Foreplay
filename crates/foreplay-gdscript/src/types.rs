//! Type system and type coercion for GDScript 2.0.

use foreplay_ir::IrValue;
use serde::{Deserialize, Serialize};

/// GDScript 2.0 static type representation.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum GdType {
    Any,
    Void,
    Int,
    Float,
    String,
    Bool,
    Vector2,
    Vector3,
    Vector4,
    Color,
    Callable,
    Array(Box<GdType>),
    Dictionary(Box<GdType>, Box<GdType>),
    Custom(String),
}

impl GdType {
    /// Parses a GDScript type annotation string into a `GdType`.
    pub fn parse(s: &str) -> Self {
        let clean = s.trim();
        if clean.is_empty() || clean == "Variant" {
            return Self::Any;
        }
        if clean == "void" {
            return Self::Void;
        }
        if clean == "int" {
            return Self::Int;
        }
        if clean == "float" {
            return Self::Float;
        }
        if clean == "String" || clean == "StringName" {
            return Self::String;
        }
        if clean == "bool" {
            return Self::Bool;
        }
        if clean == "Vector2" {
            return Self::Vector2;
        }
        if clean == "Vector3" {
            return Self::Vector3;
        }
        if clean == "Vector4" {
            return Self::Vector4;
        }
        if clean == "Color" {
            return Self::Color;
        }
        if clean == "Callable" {
            return Self::Callable;
        }

        // Typed array: Array[int], Array[float], Array[String], etc.
        if let Some(inner) = clean.strip_prefix("Array[")
            && let Some(end) = inner.rfind(']')
        {
            let elem_str = &inner[..end];
            return Self::Array(Box::new(Self::parse(elem_str)));
        }

        if clean == "Array" {
            return Self::Array(Box::new(Self::Any));
        }

        // Typed dictionary: Dictionary[String, int], Dictionary[Variant, Variant]
        if let Some(inner) = clean.strip_prefix("Dictionary[")
            && let Some(end) = inner.rfind(']')
        {
            let parts: Vec<&str> = inner[..end].split(',').map(|p| p.trim()).collect();
            if parts.len() == 2 {
                return Self::Dictionary(
                    Box::new(Self::parse(parts[0])),
                    Box::new(Self::parse(parts[1])),
                );
            }
        }

        if clean == "Dictionary" {
            return Self::Dictionary(Box::new(Self::Any), Box::new(Self::Any));
        }

        Self::Custom(clean.to_string())
    }

    /// Enforces or coerces an `IrValue` to conform to this type annotation.
    pub fn coerce_value(&self, val: IrValue) -> Result<IrValue, String> {
        match self {
            Self::Any => Ok(val),
            Self::Void => Ok(IrValue::Nil),
            Self::Int => match val {
                IrValue::I64(i) => Ok(IrValue::I64(i)),
                IrValue::F64(f) => Ok(IrValue::I64(f as i64)),
                IrValue::Bool(b) => Ok(IrValue::I64(if b { 1 } else { 0 })),
                IrValue::String(s) => s
                    .trim()
                    .parse::<i64>()
                    .map(IrValue::I64)
                    .map_err(|_| format!("cannot coerce string '{s}' to int")),
                other => Err(format!("expected int, got {other:?}")),
            },
            Self::Float => match val {
                IrValue::F64(f) => Ok(IrValue::F64(f)),
                IrValue::I64(i) => Ok(IrValue::F64(i as f64)),
                IrValue::Bool(b) => Ok(IrValue::F64(if b { 1.0 } else { 0.0 })),
                IrValue::String(s) => s
                    .trim()
                    .parse::<f64>()
                    .map(IrValue::F64)
                    .map_err(|_| format!("cannot coerce string '{s}' to float")),
                other => Err(format!("expected float, got {other:?}")),
            },
            Self::String => match val {
                IrValue::String(s) => Ok(IrValue::String(s)),
                IrValue::I64(i) => Ok(IrValue::String(i.to_string())),
                IrValue::F64(f) => Ok(IrValue::String(f.to_string())),
                IrValue::Bool(b) => Ok(IrValue::String(b.to_string())),
                IrValue::Nil => Ok(IrValue::String("null".into())),
                other => Ok(IrValue::String(format!("{other:?}"))),
            },
            Self::Bool => match val {
                IrValue::Bool(b) => Ok(IrValue::Bool(b)),
                IrValue::I64(i) => Ok(IrValue::Bool(i != 0)),
                IrValue::F64(f) => Ok(IrValue::Bool(f != 0.0)),
                IrValue::Nil => Ok(IrValue::Bool(false)),
                IrValue::String(s) => Ok(IrValue::Bool(!s.is_empty())),
                _ => Ok(IrValue::Bool(true)),
            },
            Self::Vector2 => match val {
                IrValue::Vec2(v) => Ok(IrValue::Vec2(v)),
                other => Err(format!("expected Vector2, got {other:?}")),
            },
            Self::Vector3 => match val {
                IrValue::Vec3(v) => Ok(IrValue::Vec3(v)),
                other => Err(format!("expected Vector3, got {other:?}")),
            },
            Self::Vector4 => match val {
                IrValue::Quat(q) => Ok(IrValue::Quat(q)),
                other => Err(format!("expected Vector4, got {other:?}")),
            },
            Self::Color => match val {
                IrValue::Color(c) => Ok(IrValue::Color(c)),
                other => Err(format!("expected Color, got {other:?}")),
            },
            Self::Callable => match val {
                IrValue::Map(ref m) if m.get("__type__") == Some(&IrValue::String("Callable".into())) => {
                    Ok(val)
                }
                other => Err(format!("expected Callable, got {other:?}")),
            },
            Self::Array(elem_type) => match val {
                IrValue::Array(items) => {
                    let mut coerced = Vec::with_capacity(items.len());
                    for item in items {
                        coerced.push(elem_type.coerce_value(item)?);
                    }
                    Ok(IrValue::Array(coerced))
                }
                other => Err(format!("expected Array, got {other:?}")),
            },
            Self::Dictionary(key_type, val_type) => match val {
                IrValue::Map(map) => {
                    let mut coerced = std::collections::BTreeMap::new();
                    for (k, v) in map {
                        let coerced_k = match key_type.coerce_value(IrValue::String(k.clone()))? {
                            IrValue::String(s) => s,
                            other => format!("{other:?}"),
                        };
                        let coerced_v = val_type.coerce_value(v)?;
                        coerced.insert(coerced_k, coerced_v);
                    }
                    Ok(IrValue::Map(coerced))
                }
                other => Err(format!("expected Dictionary, got {other:?}")),
            },
            Self::Custom(_) => Ok(val),
        }
    }
}

/// Godot 4 `typeof(...)` integer type code.
pub fn typeof_code(val: &IrValue) -> i64 {
    match val {
        IrValue::Nil => 0, // TYPE_NIL
        IrValue::Bool(_) => 1, // TYPE_BOOL
        IrValue::I64(_) => 2, // TYPE_INT
        IrValue::F64(_) => 3, // TYPE_FLOAT
        IrValue::String(_) => 4, // TYPE_STRING
        IrValue::Vec2(_) => 5, // TYPE_VECTOR2
        IrValue::Vec3(_) => 9, // TYPE_VECTOR3
        IrValue::Quat(_) => 15, // TYPE_QUATERNION
        IrValue::Color(_) => 20, // TYPE_COLOR
        IrValue::NodePath(_) => 22, // TYPE_NODE_PATH
        IrValue::ResourcePath(_) => 21, // TYPE_STRING_NAME
        IrValue::Map(m) => {
            if m.get("__type__") == Some(&IrValue::String("Callable".into())) {
                25 // TYPE_CALLABLE
            } else {
                27 // TYPE_DICTIONARY
            }
        }
        IrValue::Array(_) => 28, // TYPE_ARRAY
    }
}

/// Godot 4 `type_string(...)` human-readable type name.
pub fn type_string(code: i64) -> &'static str {
    match code {
        0 => "Nil",
        1 => "bool",
        2 => "int",
        3 => "float",
        4 => "String",
        5 => "Vector2",
        6 => "Vector2i",
        7 => "Rect2",
        8 => "Rect2i",
        9 => "Vector3",
        10 => "Vector3i",
        11 => "Transform2D",
        12 => "Vector4",
        13 => "Vector4i",
        14 => "Plane",
        15 => "Quaternion",
        16 => "AABB",
        17 => "Basis",
        18 => "Transform3D",
        19 => "Projection",
        20 => "Color",
        21 => "StringName",
        22 => "NodePath",
        23 => "RID",
        24 => "Object",
        25 => "Callable",
        26 => "Signal",
        27 => "Dictionary",
        28 => "Array",
        _ => "Unknown",
    }
}
