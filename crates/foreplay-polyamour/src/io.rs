//! 3D Asset Import/Export & Clean-Room Blender `.blend` DNA Header Reader (`foreplay-polyamour::io`).
//!
//! Provides bidirectional support for:
//! - Wavefront OBJ (`export_obj`, `import_obj`)
//! - STL ASCII & Binary (`export_stl_ascii`, `import_stl`)
//! - Stanford PLY (`export_ply`, `import_ply`)
//! - glTF 2.0 Embedded JSON (`export_gltf_json`, `import_gltf_json`)
//! - Safe clean-room Blender `.blend` file summary parser (`BlendFileSummary::parse_bytes`)
//!   supporting legacy 2.79/3.x/4.x and Blender 5.0 versioned headers without panics.

use crate::mesh::PolyMesh;
use bevy_math::Vec3;
use serde::{Deserialize, Serialize};

/// Errors encountered during asset serialization or parsing.
#[derive(Clone, PartialEq, Eq, Debug, thiserror::Error)]
pub enum IoError {
    #[error("format error: {0}")]
    FormatError(String),
    #[error("unexpected end of file: {0}")]
    UnexpectedEof(String),
    #[error("unsupported version or feature: {0}")]
    Unsupported(String),
}

/// Errors encountered when parsing Blender `.blend` files.
#[derive(Clone, PartialEq, Eq, Debug, thiserror::Error)]
pub enum BlendParseError {
    #[error("file smaller than 12-byte Blender header (got {0} bytes)")]
    HeaderTooShort(usize),
    #[error("invalid magic identifier: expected 'BLENDER' prefix, found '{0}'")]
    InvalidMagic(String),
    #[error("invalid pointer size marker: expected '_' (32-bit) or '-' (64-bit), found '{0}'")]
    InvalidPointerSize(char),
    #[error("invalid endianness marker: expected 'v' (little-endian) or 'V' (big-endian), found '{0}'")]
    InvalidEndianness(char),
    #[error("corrupt block header at offset {offset}: {reason}")]
    CorruptBlock { offset: usize, reason: String },
}

/// A parsed file block record from a `.blend` file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlendBlockSummary {
    pub code: String,
    pub size: usize,
    pub address: u64,
    pub sdna_index: u32,
    pub count: u32,
}

/// High-level summary of a parsed Blender `.blend` file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlendFileSummary {
    pub header_magic: String,
    pub version: String,
    pub pointer_size: u8, // 4 or 8 bytes
    pub is_little_endian: bool,
    pub block_count: usize,
    pub mesh_block_count: usize,
    pub has_dna: bool,
    pub blocks: Vec<BlendBlockSummary>,
}

impl BlendFileSummary {
    /// Safe clean-room parser for Blender `.blend` headers and DNA file blocks.
    ///
    /// Never panics on hostile, malformed, or truncated inputs.
    pub fn parse_bytes(bytes: &[u8]) -> Result<Self, BlendParseError> {
        if bytes.len() < 12 {
            return Err(BlendParseError::HeaderTooShort(bytes.len()));
        }

        // Check magic: first 7 bytes must be "BLENDER"
        let magic_prefix = std::str::from_utf8(&bytes[0..7])
            .map_err(|_| BlendParseError::InvalidMagic("non-utf8 magic".into()))?;

        if magic_prefix != "BLENDER" {
            return Err(BlendParseError::InvalidMagic(magic_prefix.to_string()));
        }

        // Pointer size: '_' = 4 bytes (32-bit), '-' = 8 bytes (64-bit).
        // For Blender 5.0+ or custom headers starting with BLENDER17 / BLENDER50, detect 64-bit fallback.
        let ptr_byte = bytes[7] as char;
        let pointer_size = match ptr_byte {
            '-' => 8,
            '_' => 4,
            '1'..='9' => 8, // Modern 64-bit versioned header e.g. BLENDER17
            _ => {
                return Err(BlendParseError::InvalidPointerSize(ptr_byte));
            }
        };

        // Endianness: 'v' = little-endian, 'V' = big-endian
        let endian_byte = bytes[8] as char;
        let is_little_endian = match endian_byte {
            'v' | 'l' => true,
            'V' | 'B' => false,
            // Fallback for modern headers
            _ if endian_byte.is_ascii_alphanumeric() => true,
            _ => {
                return Err(BlendParseError::InvalidEndianness(endian_byte));
            }
        };

        // Version: bytes 9..12 (e.g. "280", "300", "400", "500")
        let version = String::from_utf8_lossy(&bytes[9..12]).trim().to_string();

        let mut blocks = Vec::new();
        let mut mesh_block_count = 0;
        let mut has_dna = false;

        let mut offset = 12;

        let read_u32 = |slice: &[u8], is_le: bool| -> u32 {
            if is_le {
                u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]])
            } else {
                u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]])
            }
        };

        let read_u64 = |slice: &[u8], is_le: bool| -> u64 {
            if is_le {
                u64::from_le_bytes([
                    slice[0], slice[1], slice[2], slice[3], slice[4], slice[5], slice[6], slice[7],
                ])
            } else {
                u64::from_be_bytes([
                    slice[0], slice[1], slice[2], slice[3], slice[4], slice[5], slice[6], slice[7],
                ])
            }
        };

        let bhead_size = if pointer_size == 8 { 24 } else { 20 };

        while offset + bhead_size <= bytes.len() {
            let code_bytes = &bytes[offset..offset + 4];
            let code = String::from_utf8_lossy(code_bytes).to_string();

            let size = read_u32(&bytes[offset + 4..offset + 8], is_little_endian) as usize;

            let (address, sdna_index, count) = if pointer_size == 8 {
                let addr = read_u64(&bytes[offset + 8..offset + 16], is_little_endian);
                let sdna = read_u32(&bytes[offset + 16..offset + 20], is_little_endian);
                let cnt = read_u32(&bytes[offset + 20..offset + 24], is_little_endian);
                (addr, sdna, cnt)
            } else {
                let addr = read_u32(&bytes[offset + 8..offset + 12], is_little_endian) as u64;
                let sdna = read_u32(&bytes[offset + 12..offset + 16], is_little_endian);
                let cnt = read_u32(&bytes[offset + 16..offset + 20], is_little_endian);
                (addr, sdna, cnt)
            };

            let trimmed_code = code.trim_end_matches('\0').to_string();

            if trimmed_code == "DNA1" {
                has_dna = true;
            } else if trimmed_code == "ME" || trimmed_code.starts_with("MESH") {
                mesh_block_count += 1;
            }

            blocks.push(BlendBlockSummary {
                code: trimmed_code.clone(),
                size,
                address,
                sdna_index,
                count,
            });

            offset += bhead_size;

            if trimmed_code == "ENDB" {
                break;
            }

            offset = offset.saturating_add(size);
            if offset > bytes.len() {
                // Truncated file payload, safely break
                break;
            }
        }

        Ok(Self {
            header_magic: magic_prefix.to_string(),
            version,
            pointer_size,
            is_little_endian,
            block_count: blocks.len(),
            mesh_block_count,
            has_dna,
            blocks,
        })
    }
}

// =========================================================================
// Wavefront OBJ
// =========================================================================

/// Exports a `PolyMesh` into Wavefront OBJ format text.
pub fn export_obj(mesh: &PolyMesh) -> String {
    let mut out = String::new();
    out.push_str("# Foreplay Polyamour OBJ Export\n");

    for p in &mesh.positions {
        out.push_str(&format!("v {:.6} {:.6} {:.6}\n", p.x, p.y, p.z));
    }

    // Accumulate UVs
    for face in &mesh.faces {
        for uv in &face.uvs {
            out.push_str(&format!("vt {:.6} {:.6}\n", uv[0], uv[1]));
        }
    }

    // Accumulate Normals
    for face in &mesh.faces {
        let n = mesh.compute_face_normal(face);
        out.push_str(&format!("vn {:.6} {:.6} {:.6}\n", n.x, n.y, n.z));
    }

    let mut uv_offset = 1;

    for (normal_offset, face) in (1..).zip(mesh.faces.iter()) {
        out.push('f');
        for &v in &face.verts {
            out.push_str(&format!(" {}/{}/{}", v + 1, uv_offset, normal_offset));
            uv_offset += 1;
        }
        out.push('\n');
    }

    out
}

/// Imports a Wavefront OBJ string into a `PolyMesh`.
pub fn import_obj(content: &str) -> Result<PolyMesh, IoError> {
    let mut mesh = PolyMesh::new();
    let mut uv_pool: Vec<[f32; 2]> = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        match parts.as_slice() {
            ["v", x, y, z, ..] => {
                let px: f32 = x.parse().map_err(|e| IoError::FormatError(format!("{e}")))?;
                let py: f32 = y.parse().map_err(|e| IoError::FormatError(format!("{e}")))?;
                let pz: f32 = z.parse().map_err(|e| IoError::FormatError(format!("{e}")))?;
                mesh.add_vertex(Vec3::new(px, py, pz));
            }
            ["vt", u, v, ..] => {
                let tu: f32 = u.parse().unwrap_or(0.0);
                let tv: f32 = v.parse().unwrap_or(0.0);
                uv_pool.push([tu, tv]);
            }
            ["f", face_tokens @ ..] => {
                let mut verts = Vec::new();
                let mut uvs = Vec::new();

                for tok in face_tokens {
                    let sub_tokens: Vec<&str> = tok.split('/').collect();
                    if let Some(v_str) = sub_tokens.first()
                        && let Ok(v_idx) = v_str.parse::<i32>()
                    {
                        let actual_idx = if v_idx > 0 {
                            (v_idx - 1) as u32
                        } else {
                            (mesh.positions.len() as i32 + v_idx) as u32
                        };
                        verts.push(actual_idx);

                        let mut face_uv = [0.0, 0.0];
                        if sub_tokens.len() > 1
                            && let Ok(vt_idx) = sub_tokens[1].parse::<i32>()
                        {
                            let uv_actual = if vt_idx > 0 {
                                (vt_idx - 1) as usize
                            } else {
                                (uv_pool.len() as i32 + vt_idx) as usize
                            };
                            if let Some(&uv) = uv_pool.get(uv_actual) {
                                face_uv = uv;
                            }
                        }
                        uvs.push(face_uv);
                    }
                }

                if verts.len() >= 3 {
                    mesh.add_face(verts, uvs, false, 0);
                }
            }
            _ => {}
        }
    }

    Ok(mesh)
}

// =========================================================================
// STL (ASCII & Binary)
// =========================================================================

/// Exports a `PolyMesh` into ASCII STL format.
pub fn export_stl_ascii(mesh: &PolyMesh, solid_name: &str) -> String {
    let tri_data = mesh.triangulate();
    let mut out = String::new();
    out.push_str(&format!("solid {solid_name}\n"));

    let num_tris = tri_data.indices.len() / 3;
    for t in 0..num_tris {
        let i0 = tri_data.indices[t * 3] as usize;
        let i1 = tri_data.indices[t * 3 + 1] as usize;
        let i2 = tri_data.indices[t * 3 + 2] as usize;

        let n = tri_data.normals[i0];
        let p0 = tri_data.positions[i0];
        let p1 = tri_data.positions[i1];
        let p2 = tri_data.positions[i2];

        out.push_str(&format!(
            "  facet normal {:.6} {:.6} {:.6}\n    outer loop\n      vertex {:.6} {:.6} {:.6}\n      vertex {:.6} {:.6} {:.6}\n      vertex {:.6} {:.6} {:.6}\n    endloop\n  endfacet\n",
            n.x, n.y, n.z, p0.x, p0.y, p0.z, p1.x, p1.y, p1.z, p2.x, p2.y, p2.z
        ));
    }

    out.push_str(&format!("endsolid {solid_name}\n"));
    out
}

/// Imports an ASCII or Binary STL file into a `PolyMesh`.
pub fn import_stl(content: &[u8]) -> Result<PolyMesh, IoError> {
    // Check if ASCII STL (starts with "solid" and contains printable ascii)
    if content.starts_with(b"solid")
        && let Ok(text) = std::str::from_utf8(content)
        && text.contains("facet normal")
    {
        return import_stl_ascii_text(text);
    }

    // Binary STL parser
    if content.len() < 84 {
        return Err(IoError::UnexpectedEof("STL file smaller than 84 bytes".into()));
    }

    let num_tris = u32::from_le_bytes([content[80], content[81], content[82], content[83]]) as usize;
    let expected_len = 84 + num_tris * 50;
    if content.len() < expected_len {
        return Err(IoError::UnexpectedEof("truncated binary STL payload".into()));
    }

    let mut mesh = PolyMesh::new();

    let read_f32 = |offset: usize| -> f32 {
        f32::from_le_bytes([
            content[offset],
            content[offset + 1],
            content[offset + 2],
            content[offset + 3],
        ])
    };

    let mut offset = 84;
    for _ in 0..num_tris {
        // Skip normal (12 bytes)
        offset += 12;

        let p0 = Vec3::new(read_f32(offset), read_f32(offset + 4), read_f32(offset + 8));
        offset += 12;
        let p1 = Vec3::new(read_f32(offset), read_f32(offset + 4), read_f32(offset + 8));
        offset += 12;
        let p2 = Vec3::new(read_f32(offset), read_f32(offset + 4), read_f32(offset + 8));
        offset += 12;

        // Attribute byte count (2 bytes)
        offset += 2;

        let v0 = mesh.add_vertex(p0);
        let v1 = mesh.add_vertex(p1);
        let v2 = mesh.add_vertex(p2);

        mesh.add_face(
            vec![v0, v1, v2],
            vec![[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]],
            false,
            0,
        );
    }

    mesh.weld_vertices(1e-5);
    Ok(mesh)
}

fn import_stl_ascii_text(text: &str) -> Result<PolyMesh, IoError> {
    let mut mesh = PolyMesh::new();
    let mut current_loop = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("vertex") {
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() >= 4 {
                let x: f32 = parts[1].parse().unwrap_or(0.0);
                let y: f32 = parts[2].parse().unwrap_or(0.0);
                let z: f32 = parts[3].parse().unwrap_or(0.0);
                let v = mesh.add_vertex(Vec3::new(x, y, z));
                current_loop.push(v);
            }
        } else if trimmed.starts_with("endloop") {
            if current_loop.len() >= 3 {
                let uvs = vec![[0.0, 0.0]; current_loop.len()];
                mesh.add_face(current_loop.clone(), uvs, false, 0);
            }
            current_loop.clear();
        }
    }

    mesh.weld_vertices(1e-5);
    Ok(mesh)
}

// =========================================================================
// Stanford PLY
// =========================================================================

/// Exports a `PolyMesh` into ASCII Stanford PLY format.
pub fn export_ply(mesh: &PolyMesh) -> String {
    let mut out = String::new();
    out.push_str("ply\n");
    out.push_str("format ascii 1.0\n");
    out.push_str(&format!("element vertex {}\n", mesh.positions.len()));
    out.push_str("property float x\n");
    out.push_str("property float y\n");
    out.push_str("property float z\n");
    out.push_str(&format!("element face {}\n", mesh.faces.len()));
    out.push_str("property list uchar uint vertex_indices\n");
    out.push_str("end_header\n");

    for p in &mesh.positions {
        out.push_str(&format!("{:.6} {:.6} {:.6}\n", p.x, p.y, p.z));
    }

    for face in &mesh.faces {
        out.push_str(&format!("{}", face.verts.len()));
        for &v in &face.verts {
            out.push_str(&format!(" {v}"));
        }
        out.push('\n');
    }

    out
}

/// Imports an ASCII Stanford PLY string into a `PolyMesh`.
pub fn import_ply(content: &str) -> Result<PolyMesh, IoError> {
    let mut lines = content.lines();

    let mut vertex_count = 0;
    let mut face_count = 0;

    // Parse header
    let mut in_header = true;
    for line in lines.by_ref() {
        let trimmed = line.trim();
        if trimmed == "end_header" {
            in_header = false;
            break;
        }
        if trimmed.starts_with("element vertex") {
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() >= 3 {
                vertex_count = parts[2].parse().unwrap_or(0);
            }
        } else if trimmed.starts_with("element face") {
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() >= 3 {
                face_count = parts[2].parse().unwrap_or(0);
            }
        }
    }

    if in_header {
        return Err(IoError::FormatError("missing end_header in PLY".into()));
    }

    let mut mesh = PolyMesh::new();

    // Read vertices
    for _ in 0..vertex_count {
        let line = lines
            .next()
            .ok_or_else(|| IoError::UnexpectedEof("expected vertex line".into()))?;
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 {
            let x: f32 = parts[0].parse().unwrap_or(0.0);
            let y: f32 = parts[1].parse().unwrap_or(0.0);
            let z: f32 = parts[2].parse().unwrap_or(0.0);
            mesh.add_vertex(Vec3::new(x, y, z));
        }
    }

    // Read faces
    for _ in 0..face_count {
        let line = lines
            .next()
            .ok_or_else(|| IoError::UnexpectedEof("expected face line".into()))?;
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }
        let count: usize = parts[0].parse().unwrap_or(0);
        let mut verts = Vec::with_capacity(count);
        for i in 1..=count {
            if let Some(tok) = parts.get(i)
                && let Ok(idx) = tok.parse::<u32>()
            {
                verts.push(idx);
            }
        }
        if verts.len() >= 3 {
            let uvs = vec![[0.0, 0.0]; verts.len()];
            mesh.add_face(verts, uvs, false, 0);
        }
    }

    Ok(mesh)
}

// =========================================================================
// glTF 2.0 (Embedded JSON)
// =========================================================================

/// Safe base64 encoding without external dependencies.
pub fn encode_base64(bytes: &[u8]) -> String {
    const CHARSET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);

    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);

        out.push(CHARSET[(b0 >> 2) as usize] as char);
        out.push(CHARSET[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);

        if chunk.len() > 1 {
            out.push(CHARSET[(((b1 & 0x0F) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            out.push('=');
        }

        if chunk.len() > 2 {
            out.push(CHARSET[(b2 & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
    }

    out
}

/// Safe base64 decoding without external dependencies.
pub fn decode_base64(s: &str) -> Result<Vec<u8>, IoError> {
    let clean: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    let bytes = clean.as_bytes();
    if !bytes.len().is_multiple_of(4) {
        return Err(IoError::FormatError("invalid base64 length".into()));
    }

    let decode_char = |c: u8| -> Result<u8, IoError> {
        match c {
            b'A'..=b'Z' => Ok(c - b'A'),
            b'a'..=b'z' => Ok(c - b'a' + 26),
            b'0'..=b'9' => Ok(c - b'0' + 52),
            b'+' => Ok(62),
            b'/' => Ok(63),
            _ => Err(IoError::FormatError(format!("invalid base64 char {c}"))),
        }
    };

    let mut out = Vec::with_capacity((bytes.len() / 4) * 3);

    for chunk in bytes.chunks(4) {
        let c0 = decode_char(chunk[0])?;
        let c1 = decode_char(chunk[1])?;

        let b0 = (c0 << 2) | (c1 >> 4);
        out.push(b0);

        if chunk[2] != b'=' {
            let c2 = decode_char(chunk[2])?;
            let b1 = ((c1 & 0x0F) << 4) | (c2 >> 2);
            out.push(b1);

            if chunk[3] != b'=' {
                let c3 = decode_char(chunk[3])?;
                let b2 = ((c2 & 0x03) << 6) | c3;
                out.push(b2);
            }
        }
    }

    Ok(out)
}

/// Exports a `PolyMesh` into an embedded glTF 2.0 JSON document.
pub fn export_gltf_json(mesh: &PolyMesh) -> String {
    let tri_data = mesh.triangulate();

    // Construct raw binary buffer: positions, normals, indices
    let mut raw_bytes = Vec::new();

    // Buffer 0: positions (vec3 f32)
    let pos_offset = raw_bytes.len();
    for p in &tri_data.positions {
        raw_bytes.extend_from_slice(&p.x.to_le_bytes());
        raw_bytes.extend_from_slice(&p.y.to_le_bytes());
        raw_bytes.extend_from_slice(&p.z.to_le_bytes());
    }
    let pos_len = raw_bytes.len() - pos_offset;

    // Buffer 1: normals (vec3 f32)
    let norm_offset = raw_bytes.len();
    for n in &tri_data.normals {
        raw_bytes.extend_from_slice(&n.x.to_le_bytes());
        raw_bytes.extend_from_slice(&n.y.to_le_bytes());
        raw_bytes.extend_from_slice(&n.z.to_le_bytes());
    }
    let norm_len = raw_bytes.len() - norm_offset;

    // Buffer 2: indices (scalar u32)
    let idx_offset = raw_bytes.len();
    for &idx in &tri_data.indices {
        raw_bytes.extend_from_slice(&idx.to_le_bytes());
    }
    let idx_len = raw_bytes.len() - idx_offset;

    let b64 = encode_base64(&raw_bytes);
    let uri = format!("data:application/octet-stream;base64,{b64}");

    serde_json::json!({
        "asset": {
            "version": "2.0",
            "generator": "foreplay-polyamour"
        },
        "scene": 0,
        "scenes": [
            { "nodes": [0] }
        ],
        "nodes": [
            { "mesh": 0 }
        ],
        "meshes": [
            {
                "primitives": [
                    {
                        "attributes": {
                            "POSITION": 0,
                            "NORMAL": 1
                        },
                        "indices": 2
                    }
                ]
            }
        ],
        "buffers": [
            {
                "byteLength": raw_bytes.len(),
                "uri": uri
            }
        ],
        "bufferViews": [
            { "buffer": 0, "byteOffset": pos_offset, "byteLength": pos_len, "target": 34962 },
            { "buffer": 0, "byteOffset": norm_offset, "byteLength": norm_len, "target": 34962 },
            { "buffer": 0, "byteOffset": idx_offset, "byteLength": idx_len, "target": 34963 }
        ],
        "accessors": [
            {
                "bufferView": 0,
                "componentType": 5126,
                "count": tri_data.positions.len(),
                "type": "VEC3"
            },
            {
                "bufferView": 1,
                "componentType": 5126,
                "count": tri_data.normals.len(),
                "type": "VEC3"
            },
            {
                "bufferView": 2,
                "componentType": 5125,
                "count": tri_data.indices.len(),
                "type": "SCALAR"
            }
        ]
    })
    .to_string()
}

/// Imports an embedded glTF 2.0 JSON document into a `PolyMesh`.
pub fn import_gltf_json(json_str: &str) -> Result<PolyMesh, IoError> {
    let val: serde_json::Value =
        serde_json::from_str(json_str).map_err(|e| IoError::FormatError(format!("{e}")))?;

    // Extract embedded base64 buffer
    let uri = val["buffers"][0]["uri"]
        .as_str()
        .ok_or_else(|| IoError::FormatError("missing buffer uri in glTF".into()))?;

    let b64_payload = uri
        .split("base64,")
        .nth(1)
        .ok_or_else(|| IoError::FormatError("expected data:base64 uri in glTF".into()))?;

    let raw_bytes = decode_base64(b64_payload)?;

    let pos_view = &val["bufferViews"][0];
    let pos_offset = pos_view["byteOffset"].as_u64().unwrap_or(0) as usize;
    let pos_count = val["accessors"][0]["count"].as_u64().unwrap_or(0) as usize;

    let idx_view = &val["bufferViews"][2];
    let idx_offset = idx_view["byteOffset"].as_u64().unwrap_or(0) as usize;
    let idx_count = val["accessors"][2]["count"].as_u64().unwrap_or(0) as usize;

    let mut mesh = PolyMesh::new();

    let read_f32 = |offset: usize| -> f32 {
        f32::from_le_bytes([
            raw_bytes[offset],
            raw_bytes[offset + 1],
            raw_bytes[offset + 2],
            raw_bytes[offset + 3],
        ])
    };

    let read_u32 = |offset: usize| -> u32 {
        u32::from_le_bytes([
            raw_bytes[offset],
            raw_bytes[offset + 1],
            raw_bytes[offset + 2],
            raw_bytes[offset + 3],
        ])
    };

    for i in 0..pos_count {
        let off = pos_offset + i * 12;
        if off + 12 <= raw_bytes.len() {
            let p = Vec3::new(read_f32(off), read_f32(off + 4), read_f32(off + 8));
            mesh.add_vertex(p);
        }
    }

    let num_tris = idx_count / 3;
    for t in 0..num_tris {
        let off = idx_offset + t * 12;
        if off + 12 <= raw_bytes.len() {
            let i0 = read_u32(off);
            let i1 = read_u32(off + 4);
            let i2 = read_u32(off + 8);
            mesh.add_face(
                vec![i0, i1, i2],
                vec![[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]],
                false,
                0,
            );
        }
    }

    mesh.weld_vertices(1e-5);
    Ok(mesh)
}
