//! Atomic Polyops 1 Text Command Protocol (`foreplay-polyamour::polyops`).
//!
//! Provides:
//! - Full `polyops 1` parser, executor, and scene emitter.
//! - Atomic transactions (`begin`, `end`, `abort "<reason>"` with clean rollback).
//! - Handle bindings (`$H`), ID query replies (`@ID`), `add_primitive`, `add_mesh` (including `b64:` arrays),
//!   `mesh_edit`, `modifier_add`, `material_new`, `material_assign`, `set`, `parent`, `delete`.

use crate::io::{decode_base64, encode_base64};
use crate::mesh::PolyMesh;
use crate::modifiers::{evaluate_modifier_stack, PolyModifier};
use crate::ops::extrude_faces;
use crate::primitives::*;
use bevy_math::Vec3;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Protocol version number for `polyops 1`.
pub const POLYOPS_PROTOCOL_VERSION: u32 = 1;

/// A material definition in the PolyScene.
#[derive(Clone, Debug, PartialEq)]
pub struct PolyMaterial {
    pub id: u32,
    pub name: String,
    pub base_color: [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
}

impl Default for PolyMaterial {
    fn default() -> Self {
        Self {
            id: 0,
            name: "DefaultMaterial".into(),
            base_color: [0.8, 0.8, 0.8, 1.0],
            metallic: 0.0,
            roughness: 0.5,
        }
    }
}

/// An object in the PolyScene consisting of a base mesh, modifier stack, and spatial transform.
#[derive(Clone, Debug, PartialEq)]
pub struct PolyObject {
    pub id: u32,
    pub name: String,
    pub base_mesh: PolyMesh,
    pub modifiers: Vec<PolyModifier>,
    pub material_id: Option<u32>,
    pub position: Vec3,
    pub rotation: Vec3, // Euler angles in radians
    pub scale: Vec3,
    pub parent: Option<u32>,
}

impl PolyObject {
    pub fn new(id: u32, name: impl Into<String>, base_mesh: PolyMesh) -> Self {
        Self {
            id,
            name: name.into(),
            base_mesh,
            modifiers: Vec::new(),
            material_id: None,
            position: Vec3::ZERO,
            rotation: Vec3::ZERO,
            scale: Vec3::ONE,
            parent: None,
        }
    }

    /// Evaluates the modifier stack on the base mesh to produce the final evaluated geometry.
    pub fn evaluated_mesh(&self) -> PolyMesh {
        evaluate_modifier_stack(&self.base_mesh, &self.modifiers)
    }
}

/// 3D Scene Graph manipulated by `polyops 1`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PolyScene {
    pub objects: BTreeMap<u32, PolyObject>,
    pub materials: BTreeMap<u32, PolyMaterial>,
    pub handles: BTreeMap<String, u32>,
    next_object_id: u32,
    next_material_id: u32,
}

impl PolyScene {
    pub fn new() -> Self {
        Self {
            objects: BTreeMap::new(),
            materials: BTreeMap::new(),
            handles: BTreeMap::new(),
            next_object_id: 1,
            next_material_id: 1,
        }
    }

    /// Allocates a new unique object ID.
    pub fn alloc_object_id(&mut self) -> u32 {
        let id = self.next_object_id;
        self.next_object_id += 1;
        id
    }

    /// Allocates a new unique material ID.
    pub fn alloc_material_id(&mut self) -> u32 {
        let id = self.next_material_id;
        self.next_material_id += 1;
        id
    }

    /// Resolves an object handle or integer ID (e.g. "$cube1" or "42").
    pub fn resolve_object_id(&self, target: &str) -> Option<u32> {
        if target.starts_with('$') {
            self.handles.get(target).copied()
        } else {
            target.parse::<u32>().ok()
        }
    }

    /// Resolves a material handle or integer ID (e.g. "$mat_wood" or "1").
    pub fn resolve_material_id(&self, target: &str) -> Option<u32> {
        if target.starts_with('$') {
            self.handles.get(target).copied()
        } else {
            target.parse::<u32>().ok()
        }
    }
}

/// Execution report returned by `run_polyops`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PolyopsReport {
    pub commands_executed: usize,
    pub created_ids: Vec<u32>,
    pub query_replies: Vec<String>,
}

/// Errors raised during `polyops 1` parsing or execution.
#[derive(Clone, PartialEq, Eq, Debug, thiserror::Error)]
pub enum PolyopsError {
    #[error("protocol version mismatch or missing header: expected 'polyops 1', found '{0}'")]
    HeaderMismatch(String),
    #[error("unknown command '{0}' at line {1}")]
    UnknownCommand(String, usize),
    #[error("invalid argument '{arg}' for command '{cmd}' at line {line}: {reason}")]
    InvalidArgument {
        cmd: String,
        arg: String,
        line: usize,
        reason: String,
    },
    #[error("handle '{0}' not bound")]
    HandleNotFound(String),
    #[error("object {0} not found")]
    ObjectNotFound(u32),
    #[error("material {0} not found")]
    MaterialNotFound(u32),
    #[error("atomic script aborted explicitly: '{0}'")]
    Aborted(String),
}

/// Parses and executes a `polyops 1` script with atomic rollback on failure.
pub fn run_polyops(scene: &mut PolyScene, script: &str) -> Result<PolyopsReport, PolyopsError> {
    let backup = scene.clone();

    match execute_polyops_internal(scene, script) {
        Ok(report) => Ok(report),
        Err(err) => {
            // Atomic rollback
            *scene = backup;
            Err(err)
        }
    }
}

fn execute_polyops_internal(
    scene: &mut PolyScene,
    script: &str,
) -> Result<PolyopsReport, PolyopsError> {
    let mut lines = script.lines().enumerate().peekable();
    let mut report = PolyopsReport::default();

    // 1. Verify header: must start with `polyops 1`
    let mut found_header = false;
    for (_line_idx, line) in lines.by_ref() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//") {
            continue;
        }
        if trimmed == "polyops 1" {
            found_header = true;
            break;
        } else {
            return Err(PolyopsError::HeaderMismatch(trimmed.to_string()));
        }
    }

    if !found_header {
        return Err(PolyopsError::HeaderMismatch("empty script".into()));
    }

    // 2. Parse command stream
    for (line_idx, line) in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//") {
            continue;
        }

        let tokens = tokenize_line(trimmed);
        if tokens.is_empty() {
            continue;
        }

        let cmd = tokens[0].as_str();
        match cmd {
            "begin" => {
                // Transaction boundary (no-op at command level, entire script is atomic)
                report.commands_executed += 1;
            }
            "end" => {
                report.commands_executed += 1;
            }
            "abort" => {
                let reason = tokens.get(1).cloned().unwrap_or_else(|| "unspecified".into());
                return Err(PolyopsError::Aborted(reason));
            }
            "add_primitive" => {
                execute_add_primitive(scene, &tokens, line_idx, &mut report)?;
                report.commands_executed += 1;
            }
            "add_mesh" => {
                execute_add_mesh(scene, &tokens, line_idx, &mut report)?;
                report.commands_executed += 1;
            }
            "mesh_edit" => {
                execute_mesh_edit(scene, &tokens, line_idx)?;
                report.commands_executed += 1;
            }
            "modifier_add" => {
                execute_modifier_add(scene, &tokens, line_idx)?;
                report.commands_executed += 1;
            }
            "material_new" => {
                execute_material_new(scene, &tokens, line_idx, &mut report)?;
                report.commands_executed += 1;
            }
            "material_assign" => {
                execute_material_assign(scene, &tokens, line_idx)?;
                report.commands_executed += 1;
            }
            "set" => {
                execute_set_property(scene, &tokens, line_idx)?;
                report.commands_executed += 1;
            }
            "parent" => {
                execute_parent(scene, &tokens, line_idx)?;
                report.commands_executed += 1;
            }
            "delete" => {
                execute_delete(scene, &tokens, line_idx)?;
                report.commands_executed += 1;
            }
            _ => {
                return Err(PolyopsError::UnknownCommand(cmd.to_string(), line_idx + 1));
            }
        }
    }

    Ok(report)
}

fn tokenize_line(line: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut chars = line.chars().peekable();

    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
        } else if c == '"' {
            chars.next();
            let mut s = String::new();
            for ch in chars.by_ref() {
                if ch == '"' {
                    break;
                }
                s.push(ch);
            }
            tokens.push(s);
        } else {
            let mut s = String::new();
            while let Some(&ch) = chars.peek() {
                if ch.is_whitespace() || ch == '"' {
                    break;
                }
                s.push(ch);
                chars.next();
            }
            tokens.push(s);
        }
    }

    tokens
}

fn execute_add_primitive(
    scene: &mut PolyScene,
    tokens: &[String],
    line_idx: usize,
    report: &mut PolyopsReport,
) -> Result<(), PolyopsError> {
    if tokens.len() < 2 {
        return Err(PolyopsError::InvalidArgument {
            cmd: "add_primitive".into(),
            arg: "".into(),
            line: line_idx + 1,
            reason: "missing primitive type".into(),
        });
    }

    let prim_type = tokens[1].as_str();
    let mesh = match prim_type {
        "cube" => {
            let size = parse_opt_f32(tokens, 2, 2.0);
            primitive_cube(size)
        }
        "plane" => {
            let size = parse_opt_f32(tokens, 2, 2.0);
            primitive_plane(size)
        }
        "grid" => {
            let sx = parse_opt_f32(tokens, 2, 2.0);
            let sz = parse_opt_f32(tokens, 3, 2.0);
            let sub_x = parse_opt_u32(tokens, 4, 10);
            let sub_z = parse_opt_u32(tokens, 5, 10);
            primitive_grid(sx, sz, sub_x, sub_z)
        }
        "uv_sphere" => {
            let r = parse_opt_f32(tokens, 2, 1.0);
            let segs = parse_opt_u32(tokens, 3, 16);
            let rings = parse_opt_u32(tokens, 4, 8);
            primitive_uv_sphere(r, segs, rings)
        }
        "ico_sphere" => {
            let r = parse_opt_f32(tokens, 2, 1.0);
            let sub = parse_opt_u32(tokens, 3, 2);
            primitive_ico_sphere(r, sub)
        }
        "cylinder" => {
            let r = parse_opt_f32(tokens, 2, 1.0);
            let d = parse_opt_f32(tokens, 3, 2.0);
            let segs = parse_opt_u32(tokens, 4, 16);
            primitive_cylinder(r, d, segs)
        }
        "cone" => {
            let r = parse_opt_f32(tokens, 2, 1.0);
            let d = parse_opt_f32(tokens, 3, 2.0);
            let segs = parse_opt_u32(tokens, 4, 16);
            primitive_cone(r, d, segs)
        }
        "torus" => {
            let r_maj = parse_opt_f32(tokens, 2, 1.0);
            let r_min = parse_opt_f32(tokens, 3, 0.25);
            let seg_maj = parse_opt_u32(tokens, 4, 24);
            let seg_min = parse_opt_u32(tokens, 5, 12);
            primitive_torus(r_maj, r_min, seg_maj, seg_min)
        }
        "quarterpipe" => {
            let r = parse_opt_f32(tokens, 2, 2.0);
            let w = parse_opt_f32(tokens, 3, 4.0);
            let segs = parse_opt_u32(tokens, 4, 8);
            primitive_quarterpipe(r, w, segs)
        }
        "stair_set" => {
            let w = parse_opt_f32(tokens, 2, 3.0);
            let h = parse_opt_f32(tokens, 3, 1.5);
            let d = parse_opt_f32(tokens, 4, 3.0);
            let steps = parse_opt_u32(tokens, 5, 5);
            primitive_stair_set(w, h, d, steps)
        }
        _ => {
            return Err(PolyopsError::InvalidArgument {
                cmd: "add_primitive".into(),
                arg: prim_type.into(),
                line: line_idx + 1,
                reason: format!("unknown primitive type '{prim_type}'"),
            });
        }
    };

    let obj_id = scene.alloc_object_id();
    let obj = PolyObject::new(obj_id, prim_type, mesh);
    scene.objects.insert(obj_id, obj);
    report.created_ids.push(obj_id);
    report.query_replies.push(format!("@{obj_id}"));

    // Handle "as $handle"
    if let Some(pos) = tokens.iter().position(|t| t == "as")
        && let Some(handle) = tokens.get(pos + 1)
    {
        scene.handles.insert(handle.clone(), obj_id);
    }

    Ok(())
}

fn execute_add_mesh(
    scene: &mut PolyScene,
    tokens: &[String],
    line_idx: usize,
    report: &mut PolyopsReport,
) -> Result<(), PolyopsError> {
    let mut mesh = PolyMesh::new();

    // Look for verts and faces parameters
    for tok in &tokens[1..] {
        if tok.starts_with("verts=b64:") {
            let b64 = tok.trim_start_matches("verts=b64:");
            let bytes = decode_base64(b64).map_err(|e| PolyopsError::InvalidArgument {
                cmd: "add_mesh".into(),
                arg: tok.clone(),
                line: line_idx + 1,
                reason: format!("{e}"),
            })?;
            let num_floats = bytes.len() / 4;
            for i in 0..(num_floats / 3) {
                let off = i * 12;
                let x = f32::from_le_bytes([bytes[off], bytes[off + 1], bytes[off + 2], bytes[off + 3]]);
                let y = f32::from_le_bytes([bytes[off + 4], bytes[off + 5], bytes[off + 6], bytes[off + 7]]);
                let z = f32::from_le_bytes([bytes[off + 8], bytes[off + 9], bytes[off + 10], bytes[off + 11]]);
                mesh.add_vertex(Vec3::new(x, y, z));
            }
        } else if tok.starts_with("faces=b64:") {
            let b64 = tok.trim_start_matches("faces=b64:");
            let bytes = decode_base64(b64).map_err(|e| PolyopsError::InvalidArgument {
                cmd: "add_mesh".into(),
                arg: tok.clone(),
                line: line_idx + 1,
                reason: format!("{e}"),
            })?;
            let num_u32 = bytes.len() / 4;
            // Triangles payload
            for i in 0..(num_u32 / 3) {
                let off = i * 12;
                let i0 = u32::from_le_bytes([bytes[off], bytes[off + 1], bytes[off + 2], bytes[off + 3]]);
                let i1 = u32::from_le_bytes([bytes[off + 4], bytes[off + 5], bytes[off + 6], bytes[off + 7]]);
                let i2 = u32::from_le_bytes([bytes[off + 8], bytes[off + 9], bytes[off + 10], bytes[off + 11]]);
                mesh.add_face(vec![i0, i1, i2], vec![[0.0, 0.0]; 3], false, 0);
            }
        }
    }

    let obj_id = scene.alloc_object_id();
    let obj = PolyObject::new(obj_id, "CustomMesh", mesh);
    scene.objects.insert(obj_id, obj);
    report.created_ids.push(obj_id);
    report.query_replies.push(format!("@{obj_id}"));

    if let Some(pos) = tokens.iter().position(|t| t == "as")
        && let Some(handle) = tokens.get(pos + 1)
    {
        scene.handles.insert(handle.clone(), obj_id);
    }

    Ok(())
}

fn execute_mesh_edit(
    scene: &mut PolyScene,
    tokens: &[String],
    line_idx: usize,
) -> Result<(), PolyopsError> {
    if tokens.len() < 3 {
        return Err(PolyopsError::InvalidArgument {
            cmd: "mesh_edit".into(),
            arg: "".into(),
            line: line_idx + 1,
            reason: "expected target and edit operation".into(),
        });
    }

    let target = &tokens[1];
    let obj_id = scene
        .resolve_object_id(target)
        .ok_or_else(|| PolyopsError::HandleNotFound(target.clone()))?;

    let obj = scene
        .objects
        .get_mut(&obj_id)
        .ok_or(PolyopsError::ObjectNotFound(obj_id))?;

    let op = tokens[2].as_str();
    match op {
        "translate_verts" => {
            let dx = parse_opt_f32(tokens, 3, 0.0);
            let dy = parse_opt_f32(tokens, 4, 0.0);
            let dz = parse_opt_f32(tokens, 5, 0.0);
            let offset = Vec3::new(dx, dy, dz);
            for p in &mut obj.base_mesh.positions {
                *p += offset;
            }
        }
        "scale_verts" => {
            let sx = parse_opt_f32(tokens, 3, 1.0);
            let sy = parse_opt_f32(tokens, 4, 1.0);
            let sz = parse_opt_f32(tokens, 5, 1.0);
            let scale = Vec3::new(sx, sy, sz);
            for p in &mut obj.base_mesh.positions {
                *p *= scale;
            }
        }
        "extrude" => {
            let dist = parse_opt_f32(tokens, 3, 1.0);
            let face_indices: Vec<usize> = (0..obj.base_mesh.faces.len()).collect();
            extrude_faces(&mut obj.base_mesh, &face_indices, dist);
        }
        "subdivide" => {
            let sub_mod = PolyModifier::Subdivision {
                levels: 1,
                simple: false,
            };
            obj.base_mesh = evaluate_modifier_stack(&obj.base_mesh, &[sub_mod]);
        }
        "shade_smooth" => {
            let smooth = tokens.get(3).map(|s| s != "false").unwrap_or(true);
            for face in &mut obj.base_mesh.faces {
                face.smooth = smooth;
            }
        }
        "weld" => {
            let dist = parse_opt_f32(tokens, 3, 1e-4);
            obj.base_mesh.weld_vertices(dist);
        }
        _ => {
            return Err(PolyopsError::InvalidArgument {
                cmd: "mesh_edit".into(),
                arg: op.into(),
                line: line_idx + 1,
                reason: format!("unknown mesh_edit operation '{op}'"),
            });
        }
    }

    Ok(())
}

fn execute_modifier_add(
    scene: &mut PolyScene,
    tokens: &[String],
    line_idx: usize,
) -> Result<(), PolyopsError> {
    if tokens.len() < 3 {
        return Err(PolyopsError::InvalidArgument {
            cmd: "modifier_add".into(),
            arg: "".into(),
            line: line_idx + 1,
            reason: "expected target and modifier type".into(),
        });
    }

    let target = &tokens[1];
    let obj_id = scene
        .resolve_object_id(target)
        .ok_or_else(|| PolyopsError::HandleNotFound(target.clone()))?;

    let obj = scene
        .objects
        .get_mut(&obj_id)
        .ok_or(PolyopsError::ObjectNotFound(obj_id))?;

    let mod_type = tokens[2].as_str();
    let modifier = match mod_type {
        "subdivision" => {
            let levels = parse_opt_u32(tokens, 3, 1) as u8;
            let simple = tokens.get(4).map(|s| s == "simple").unwrap_or(false);
            PolyModifier::Subdivision { levels, simple }
        }
        "mirror" => {
            let axis_x = tokens.iter().any(|t| t == "x");
            let axis_y = tokens.iter().any(|t| t == "y");
            let axis_z = tokens.iter().any(|t| t == "z");
            PolyModifier::Mirror {
                axis_x: if !axis_x && !axis_y && !axis_z { true } else { axis_x },
                axis_y,
                axis_z,
                merge_distance: 1e-3,
            }
        }
        "array" => {
            let count = parse_opt_u32(tokens, 3, 2);
            let dx = parse_opt_f32(tokens, 4, 1.0);
            let dy = parse_opt_f32(tokens, 5, 0.0);
            let dz = parse_opt_f32(tokens, 6, 0.0);
            PolyModifier::Array {
                count,
                relative_offset: Vec3::new(dx, dy, dz),
                constant_offset: Vec3::ZERO,
            }
        }
        "solidify" => {
            let thickness = parse_opt_f32(tokens, 3, 0.1);
            PolyModifier::Solidify { thickness }
        }
        "bevel" => {
            let width = parse_opt_f32(tokens, 3, 0.05);
            PolyModifier::Bevel { width }
        }
        "triangulate" => PolyModifier::Triangulate,
        "weld" => {
            let dist = parse_opt_f32(tokens, 3, 1e-4);
            PolyModifier::Weld { distance: dist }
        }
        "displace" => {
            let str = parse_opt_f32(tokens, 3, 0.1);
            let freq = parse_opt_f32(tokens, 4, 1.0);
            PolyModifier::Displace {
                strength: str,
                frequency: freq,
            }
        }
        "smooth" => {
            let f = parse_opt_f32(tokens, 3, 0.5);
            let iters = parse_opt_u32(tokens, 4, 1);
            PolyModifier::Smooth {
                factor: f,
                iterations: iters,
            }
        }
        _ => {
            return Err(PolyopsError::InvalidArgument {
                cmd: "modifier_add".into(),
                arg: mod_type.into(),
                line: line_idx + 1,
                reason: format!("unknown modifier type '{mod_type}'"),
            });
        }
    };

    obj.modifiers.push(modifier);
    Ok(())
}

fn execute_material_new(
    scene: &mut PolyScene,
    tokens: &[String],
    _line_idx: usize,
    report: &mut PolyopsReport,
) -> Result<(), PolyopsError> {
    let name = tokens.get(1).cloned().unwrap_or_else(|| "Material".into());
    let r = parse_opt_f32(tokens, 2, 0.8);
    let g = parse_opt_f32(tokens, 3, 0.8);
    let b = parse_opt_f32(tokens, 4, 0.8);
    let a = parse_opt_f32(tokens, 5, 1.0);
    let metallic = parse_opt_f32(tokens, 6, 0.0);
    let roughness = parse_opt_f32(tokens, 7, 0.5);

    let mat_id = scene.alloc_material_id();
    scene.materials.insert(
        mat_id,
        PolyMaterial {
            id: mat_id,
            name,
            base_color: [r, g, b, a],
            metallic,
            roughness,
        },
    );

    report.created_ids.push(mat_id);
    report.query_replies.push(format!("@{mat_id}"));

    if let Some(pos) = tokens.iter().position(|t| t == "as")
        && let Some(handle) = tokens.get(pos + 1)
    {
        scene.handles.insert(handle.clone(), mat_id);
    }

    Ok(())
}

fn execute_material_assign(
    scene: &mut PolyScene,
    tokens: &[String],
    line_idx: usize,
) -> Result<(), PolyopsError> {
    if tokens.len() < 3 {
        return Err(PolyopsError::InvalidArgument {
            cmd: "material_assign".into(),
            arg: "".into(),
            line: line_idx + 1,
            reason: "expected target object and material".into(),
        });
    }

    let obj_target = &tokens[1];
    let mat_target = &tokens[2];

    let obj_id = scene
        .resolve_object_id(obj_target)
        .ok_or_else(|| PolyopsError::HandleNotFound(obj_target.clone()))?;
    let mat_id = scene
        .resolve_material_id(mat_target)
        .ok_or_else(|| PolyopsError::HandleNotFound(mat_target.clone()))?;

    let obj = scene
        .objects
        .get_mut(&obj_id)
        .ok_or(PolyopsError::ObjectNotFound(obj_id))?;

    obj.material_id = Some(mat_id);
    Ok(())
}

fn execute_set_property(
    scene: &mut PolyScene,
    tokens: &[String],
    line_idx: usize,
) -> Result<(), PolyopsError> {
    if tokens.len() < 4 {
        return Err(PolyopsError::InvalidArgument {
            cmd: "set".into(),
            arg: "".into(),
            line: line_idx + 1,
            reason: "expected target, property, and value".into(),
        });
    }

    let target = &tokens[1];
    let obj_id = scene
        .resolve_object_id(target)
        .ok_or_else(|| PolyopsError::HandleNotFound(target.clone()))?;

    let obj = scene
        .objects
        .get_mut(&obj_id)
        .ok_or(PolyopsError::ObjectNotFound(obj_id))?;

    let prop = tokens[2].as_str();
    match prop {
        "position" => {
            let x = parse_opt_f32(tokens, 3, 0.0);
            let y = parse_opt_f32(tokens, 4, 0.0);
            let z = parse_opt_f32(tokens, 5, 0.0);
            obj.position = Vec3::new(x, y, z);
        }
        "rotation" => {
            let x = parse_opt_f32(tokens, 3, 0.0);
            let y = parse_opt_f32(tokens, 4, 0.0);
            let z = parse_opt_f32(tokens, 5, 0.0);
            obj.rotation = Vec3::new(x, y, z);
        }
        "scale" => {
            let x = parse_opt_f32(tokens, 3, 1.0);
            let y = parse_opt_f32(tokens, 4, 1.0);
            let z = parse_opt_f32(tokens, 5, 1.0);
            obj.scale = Vec3::new(x, y, z);
        }
        "name" => {
            obj.name = tokens[3].clone();
        }
        _ => {
            return Err(PolyopsError::InvalidArgument {
                cmd: "set".into(),
                arg: prop.into(),
                line: line_idx + 1,
                reason: format!("unknown property '{prop}'"),
            });
        }
    }

    Ok(())
}

fn execute_parent(
    scene: &mut PolyScene,
    tokens: &[String],
    line_idx: usize,
) -> Result<(), PolyopsError> {
    if tokens.len() < 3 {
        return Err(PolyopsError::InvalidArgument {
            cmd: "parent".into(),
            arg: "".into(),
            line: line_idx + 1,
            reason: "expected child and parent targets".into(),
        });
    }

    let child_target = &tokens[1];
    let parent_target = &tokens[2];

    let child_id = scene
        .resolve_object_id(child_target)
        .ok_or_else(|| PolyopsError::HandleNotFound(child_target.clone()))?;

    let parent_id = if parent_target == "none" || parent_target == "null" {
        None
    } else {
        Some(
            scene
                .resolve_object_id(parent_target)
                .ok_or_else(|| PolyopsError::HandleNotFound(parent_target.clone()))?,
        )
    };

    let child_obj = scene
        .objects
        .get_mut(&child_id)
        .ok_or(PolyopsError::ObjectNotFound(child_id))?;

    child_obj.parent = parent_id;
    Ok(())
}

fn execute_delete(
    scene: &mut PolyScene,
    tokens: &[String],
    line_idx: usize,
) -> Result<(), PolyopsError> {
    if tokens.len() < 2 {
        return Err(PolyopsError::InvalidArgument {
            cmd: "delete".into(),
            arg: "".into(),
            line: line_idx + 1,
            reason: "expected target".into(),
        });
    }

    let target = &tokens[1];
    let obj_id = scene
        .resolve_object_id(target)
        .ok_or_else(|| PolyopsError::HandleNotFound(target.clone()))?;

    scene.objects.remove(&obj_id);
    scene.handles.retain(|_, &mut v| v != obj_id);
    Ok(())
}

fn parse_opt_f32(tokens: &[String], idx: usize, default: f32) -> f32 {
    tokens.get(idx).and_then(|s| s.parse().ok()).unwrap_or(default)
}

fn parse_opt_u32(tokens: &[String], idx: usize, default: u32) -> u32 {
    tokens.get(idx).and_then(|s| s.parse().ok()).unwrap_or(default)
}

/// Serializes an entire `PolyScene` into a valid, deterministic `polyops 1` script.
pub fn scene_to_polyops(scene: &PolyScene) -> String {
    let mut out = String::new();
    out.push_str("polyops 1\nbegin\n");

    // Materials
    for mat in scene.materials.values() {
        out.push_str(&format!(
            "material_new \"{}\" {:.4} {:.4} {:.4} {:.4} {:.4} {:.4} as $mat_{}\n",
            mat.name,
            mat.base_color[0],
            mat.base_color[1],
            mat.base_color[2],
            mat.base_color[3],
            mat.metallic,
            mat.roughness,
            mat.id
        ));
    }

    // Objects
    for obj in scene.objects.values() {
        // Encode mesh positions and triangle indices into b64 arrays
        let mut vert_bytes = Vec::new();
        for p in &obj.base_mesh.positions {
            vert_bytes.extend_from_slice(&p.x.to_le_bytes());
            vert_bytes.extend_from_slice(&p.y.to_le_bytes());
            vert_bytes.extend_from_slice(&p.z.to_le_bytes());
        }

        let tri_data = obj.base_mesh.triangulate();
        let mut face_bytes = Vec::new();
        for &idx in &tri_data.indices {
            face_bytes.extend_from_slice(&idx.to_le_bytes());
        }

        let v_b64 = encode_base64(&vert_bytes);
        let f_b64 = encode_base64(&face_bytes);

        out.push_str(&format!(
            "add_mesh verts=b64:{} faces=b64:{} as $obj_{}\n",
            v_b64, f_b64, obj.id
        ));

        // Spatial transform
        if obj.position != Vec3::ZERO {
            out.push_str(&format!(
                "set $obj_{} position {:.4} {:.4} {:.4}\n",
                obj.id, obj.position.x, obj.position.y, obj.position.z
            ));
        }
        if obj.rotation != Vec3::ZERO {
            out.push_str(&format!(
                "set $obj_{} rotation {:.4} {:.4} {:.4}\n",
                obj.id, obj.rotation.x, obj.rotation.y, obj.rotation.z
            ));
        }
        if obj.scale != Vec3::ONE {
            out.push_str(&format!(
                "set $obj_{} scale {:.4} {:.4} {:.4}\n",
                obj.id, obj.scale.x, obj.scale.y, obj.scale.z
            ));
        }

        // Modifiers
        for m in &obj.modifiers {
            match m {
                PolyModifier::Subdivision { levels, simple } => {
                    out.push_str(&format!(
                        "modifier_add $obj_{} subdivision {} {}\n",
                        obj.id,
                        levels,
                        if *simple { "simple" } else { "catmull" }
                    ));
                }
                PolyModifier::Mirror {
                    axis_x,
                    axis_y,
                    axis_z,
                    ..
                } => {
                    out.push_str(&format!(
                        "modifier_add $obj_{} mirror{}{}{}\n",
                        obj.id,
                        if *axis_x { " x" } else { "" },
                        if *axis_y { " y" } else { "" },
                        if *axis_z { " z" } else { "" },
                    ));
                }
                PolyModifier::Array {
                    count,
                    relative_offset,
                    ..
                } => {
                    out.push_str(&format!(
                        "modifier_add $obj_{} array {} {:.4} {:.4} {:.4}\n",
                        obj.id, count, relative_offset.x, relative_offset.y, relative_offset.z
                    ));
                }
                PolyModifier::Solidify { thickness } => {
                    out.push_str(&format!(
                        "modifier_add $obj_{} solidify {:.4}\n",
                        obj.id, thickness
                    ));
                }
                PolyModifier::Bevel { width } => {
                    out.push_str(&format!("modifier_add $obj_{} bevel {:.4}\n", obj.id, width));
                }
                PolyModifier::Triangulate => {
                    out.push_str(&format!("modifier_add $obj_{} triangulate\n", obj.id));
                }
                PolyModifier::Weld { distance } => {
                    out.push_str(&format!("modifier_add $obj_{} weld {:.4}\n", obj.id, distance));
                }
                PolyModifier::Displace {
                    strength,
                    frequency,
                } => {
                    out.push_str(&format!(
                        "modifier_add $obj_{} displace {:.4} {:.4}\n",
                        obj.id, strength, frequency
                    ));
                }
                PolyModifier::Smooth { factor, iterations } => {
                    out.push_str(&format!(
                        "modifier_add $obj_{} smooth {:.4} {}\n",
                        obj.id, factor, iterations
                    ));
                }
            }
        }

        // Material assignment
        if let Some(mat_id) = obj.material_id {
            out.push_str(&format!("material_assign $obj_{} $mat_{}\n", obj.id, mat_id));
        }

        // Parenting
        if let Some(parent_id) = obj.parent {
            out.push_str(&format!("parent $obj_{} $obj_{}\n", obj.id, parent_id));
        }
    }

    out.push_str("end\n");
    out
}
