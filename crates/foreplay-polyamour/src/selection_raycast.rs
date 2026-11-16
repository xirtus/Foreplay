//! Sub-Object Topological Picking (`1/2/3` Vertex/Edge/Face), Loop/Ring Select,
//! Proportional Editing (`O`) & `polyops 2` Parameterized Modeling (`foreplay-polyamour::selection_raycast`).

use crate::mesh::{HalfEdgeMesh, PolyMesh};
use crate::ops::{bevel_edges, extrude_faces, inset_faces, loop_cut};
use crate::semantic_tags::PolyamourSemanticAttributes;
use bevy_math::Vec3;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Canonicalizes an edge pair such that `[min(v0, v1), max(v0, v1)]`.
#[inline]
pub fn canonical_edge(v0: u32, v1: u32) -> [u32; 2] {
    if v0 <= v1 {
        [v0, v1]
    } else {
        [v1, v0]
    }
}

/// Selection mode for sub-object mesh editing (Blender keys `1`, `2`, `3`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SubObjectMode {
    /// Vertex selection mode (`1`).
    Vertex,
    /// Edge selection mode (`2`).
    Edge,
    /// Face selection mode (`3`).
    Face,
}

/// Discrete selection set of mesh sub-elements (vertices, canonical edges, faces).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubObjectSelection {
    /// Set of selected vertex indices.
    pub vertices: BTreeSet<u32>,
    /// Set of selected canonical edges `[min, max]`.
    pub edges: BTreeSet<[u32; 2]>,
    /// Set of selected face indices.
    pub faces: BTreeSet<u32>,
}

impl SubObjectSelection {
    /// Creates an empty selection.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns `true` if no vertices, edges, or faces are selected.
    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty() && self.edges.is_empty() && self.faces.is_empty()
    }

    /// Clears all selections.
    pub fn clear(&mut self) {
        self.vertices.clear();
        self.edges.clear();
        self.faces.clear();
    }

    /// Selects a vertex by index.
    pub fn select_vertex(&mut self, v: u32) {
        self.vertices.insert(v);
    }

    /// Deselects a vertex by index.
    pub fn deselect_vertex(&mut self, v: u32) {
        self.vertices.remove(&v);
    }

    /// Selects an edge, canonicalizing `[v0, v1]`.
    pub fn select_edge(&mut self, v0: u32, v1: u32) {
        self.edges.insert(canonical_edge(v0, v1));
    }

    /// Deselects an edge.
    pub fn deselect_edge(&mut self, v0: u32, v1: u32) {
        self.edges.remove(&canonical_edge(v0, v1));
    }

    /// Selects a face by index.
    pub fn select_face(&mut self, f: u32) {
        self.faces.insert(f);
    }

    /// Deselects a face by index.
    pub fn deselect_face(&mut self, f: u32) {
        self.faces.remove(&f);
    }

    /// Returns total count of selected entities (vertices + edges + faces).
    pub fn total_selected_count(&self) -> usize {
        self.vertices.len() + self.edges.len() + self.faces.len()
    }

    /// Returns the union of all vertex indices influenced by the selection:
    /// directly selected vertices, endpoints of selected edges, and vertices of selected faces.
    pub fn all_selected_vertex_indices(&self, mesh: &PolyMesh) -> BTreeSet<u32> {
        let mut set = self.vertices.clone();
        for edge in &self.edges {
            set.insert(edge[0]);
            set.insert(edge[1]);
        }
        for &face_idx in &self.faces {
            if let Some(face) = mesh.faces.get(face_idx as usize) {
                for &v in &face.verts {
                    set.insert(v);
                }
            }
        }
        set
    }
}

/// A 3D ray defined by an origin and a direction vector.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ray3D {
    pub origin: Vec3,
    pub direction: Vec3,
}

impl Ray3D {
    /// Creates a ray, normalizing `direction` if its length is non-zero.
    pub fn new(origin: Vec3, direction: Vec3) -> Self {
        let dir = if direction.length_squared() > 1e-12 {
            direction.normalize()
        } else {
            Vec3::Z
        };
        Self {
            origin,
            direction: dir,
        }
    }

    /// Evaluates a point along the ray: `origin + direction * t`.
    #[inline]
    pub fn at(&self, t: f32) -> Vec3 {
        self.origin + self.direction * t
    }
}

/// Intersection result for picking a face with a ray.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FaceRayHit {
    pub face_index: usize,
    pub distance: f32,
    pub hit_point: Vec3,
    pub face_normal: Vec3,
}

/// Intersection result for picking an edge with a ray.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EdgeRayHit {
    pub edge: [u32; 2],
    pub distance: f32,
    pub hit_point: Vec3,
    pub edge_point: Vec3,
}

/// Intersection result for picking a vertex with a ray.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VertexRayHit {
    pub vertex_index: u32,
    pub distance: f32,
    pub hit_point: Vec3,
    pub vertex_pos: Vec3,
}

/// Picks the closest face hit by `ray` using Möller–Trumbore ray-triangle intersection
/// over the triangulated fan of each face.
pub fn pick_face_ray(mesh: &PolyMesh, ray: Ray3D) -> Option<FaceRayHit> {
    let mut closest_hit: Option<FaceRayHit> = None;

    for (face_idx, face) in mesh.faces.iter().enumerate() {
        let n = face.verts.len();
        if n < 3 {
            continue;
        }

        let normal = mesh.compute_face_normal(face);

        // Triangulate n-gon into fan (v0, vi, vi+1)
        for i in 1..(n - 1) {
            let v0 = match mesh.positions.get(face.verts[0] as usize) {
                Some(&p) => p,
                None => continue,
            };
            let v1 = match mesh.positions.get(face.verts[i] as usize) {
                Some(&p) => p,
                None => continue,
            };
            let v2 = match mesh.positions.get(face.verts[i + 1] as usize) {
                Some(&p) => p,
                None => continue,
            };

            if let Some(t) = ray_triangle_intersect(ray, v0, v1, v2).filter(|&t| t > 1e-5) {
                let is_closer = match closest_hit {
                    Some(ref prev) => t < prev.distance,
                    None => true,
                };
                if is_closer {
                    let hit_point = ray.at(t);
                    closest_hit = Some(FaceRayHit {
                        face_index: face_idx,
                        distance: t,
                        hit_point,
                        face_normal: normal,
                    });
                }
            }
        }
    }

    closest_hit
}

/// Möller–Trumbore algorithm: intersects `ray` with triangle `(v0, v1, v2)`.
fn ray_triangle_intersect(ray: Ray3D, v0: Vec3, v1: Vec3, v2: Vec3) -> Option<f32> {
    let edge1 = v1 - v0;
    let edge2 = v2 - v0;
    let h = ray.direction.cross(edge2);
    let a = edge1.dot(h);

    if a.abs() < 1e-7 {
        return None; // Ray is parallel to triangle
    }

    let f = 1.0 / a;
    let s = ray.origin - v0;
    let u = f * s.dot(h);

    if !(0.0..=1.0).contains(&u) {
        return None;
    }

    let q = s.cross(edge1);
    let v = f * ray.direction.dot(q);

    if v < 0.0 || (u + v) > 1.0 {
        return None;
    }

    let t = f * edge2.dot(q);
    if t > 1e-6 {
        Some(t)
    } else {
        None
    }
}

/// Picks the closest edge within `max_world_radius` of `ray`.
pub fn pick_edge_ray(mesh: &PolyMesh, ray: Ray3D, max_world_radius: f32) -> Option<EdgeRayHit> {
    let mut unique_edges = BTreeSet::new();

    for face in &mesh.faces {
        let n = face.verts.len();
        for i in 0..n {
            let next = (i + 1) % n;
            unique_edges.insert(canonical_edge(face.verts[i], face.verts[next]));
        }
    }
    for &edge in &mesh.loose_edges {
        unique_edges.insert(canonical_edge(edge[0], edge[1]));
    }

    let mut closest_hit: Option<EdgeRayHit> = None;

    for edge in unique_edges {
        let p0 = match mesh.positions.get(edge[0] as usize) {
            Some(&p) => p,
            None => continue,
        };
        let p1 = match mesh.positions.get(edge[1] as usize) {
            Some(&p) => p,
            None => continue,
        };

        let (t_ray, s_seg, dist) = ray_to_segment_distance(ray, p0, p1);
        if dist <= max_world_radius && t_ray >= 0.0 {
            let is_closer = match closest_hit {
                Some(ref prev) => t_ray < prev.distance,
                None => true,
            };
            if is_closer {
                let hit_point = ray.at(t_ray);
                let edge_point = p0 + (p1 - p0) * s_seg;
                closest_hit = Some(EdgeRayHit {
                    edge,
                    distance: t_ray,
                    hit_point,
                    edge_point,
                });
            }
        }
    }

    closest_hit
}

/// Computes the closest distance between a ray `O + t * D` (`t >= 0`) and segment `A + s * (B - A)` (`s in [0, 1]`).
/// Returns `(t_ray, s_seg, distance)`.
fn ray_to_segment_distance(ray: Ray3D, a: Vec3, b: Vec3) -> (f32, f32, f32) {
    let u = ray.direction;
    let v = b - a;
    let w = ray.origin - a;

    let a_dot = u.dot(u);
    let b_dot = u.dot(v);
    let c_dot = v.dot(v);
    let d_dot = u.dot(w);
    let e_dot = v.dot(w);

    if c_dot < 1e-8 {
        // Degenerate segment A == B
        let t = (-d_dot / a_dot).max(0.0);
        let p_ray = ray.at(t);
        return (t, 0.0, (p_ray - a).length());
    }

    let denom = a_dot * c_dot - b_dot * b_dot;

    let mut s = if denom.abs() < 1e-7 {
        0.0
    } else {
        ((a_dot * e_dot - b_dot * d_dot) / denom).clamp(0.0, 1.0)
    };

    let mut t = (b_dot * s - d_dot) / a_dot;
    if t < 0.0 {
        t = 0.0;
        s = (e_dot / c_dot).clamp(0.0, 1.0);
    }

    let p_ray = ray.at(t);
    let p_seg = a + v * s;
    let dist = (p_ray - p_seg).length();

    (t, s, dist)
}

/// Picks the closest vertex within `max_world_radius` of `ray`.
pub fn pick_vertex_ray(mesh: &PolyMesh, ray: Ray3D, max_world_radius: f32) -> Option<VertexRayHit> {
    let mut closest_hit: Option<VertexRayHit> = None;

    for (v_idx, &pos) in mesh.positions.iter().enumerate() {
        let diff = pos - ray.origin;
        let t = diff.dot(ray.direction);
        if t < 0.0 {
            continue;
        }

        let p_ray = ray.at(t);
        let dist = (p_ray - pos).length();

        if dist <= max_world_radius {
            let is_closer = match closest_hit {
                Some(ref prev) => t < prev.distance,
                None => true,
            };
            if is_closer {
                closest_hit = Some(VertexRayHit {
                    vertex_index: v_idx as u32,
                    distance: t,
                    hit_point: p_ray,
                    vertex_pos: pos,
                });
            }
        }
    }

    closest_hit
}

/// Traverses an edge loop starting from `seed_edge` using topological half-edge quad walking.
pub fn select_edge_loop(mesh: &PolyMesh, seed_edge: [u32; 2]) -> Vec<[u32; 2]> {
    let he = HalfEdgeMesh::from_polymesh(mesh);
    let edges = he.edge_loop(seed_edge);
    if edges.is_empty() {
        vec![canonical_edge(seed_edge[0], seed_edge[1])]
    } else {
        edges
    }
}

/// Traverses a face loop starting from `seed_face` and crossing `seed_edge`.
pub fn select_face_loop(mesh: &PolyMesh, seed_face: u32, seed_edge: [u32; 2]) -> Vec<u32> {
    let canon_edge = canonical_edge(seed_edge[0], seed_edge[1]);
    let mut loop_faces = Vec::new();
    let mut visited = BTreeSet::new();

    if (seed_face as usize) >= mesh.faces.len() {
        return loop_faces;
    }

    let mut current_face = seed_face as usize;
    let mut current_edge = canon_edge;

    // Walk forward
    while visited.insert(current_face) {
        loop_faces.push(current_face as u32);

        let face = &mesh.faces[current_face];
        let n = face.verts.len();
        if n != 4 {
            // Face loops are topological across quad strips
            break;
        }

        // Find edge opposite to current_edge in quad
        let mut opp_edge = None;
        for i in 0..4 {
            let e = canonical_edge(face.verts[i], face.verts[(i + 1) % 4]);
            if e == current_edge {
                let opp = canonical_edge(face.verts[(i + 2) % 4], face.verts[(i + 3) % 4]);
                opp_edge = Some(opp);
                break;
            }
        }

        let Some(next_edge) = opp_edge else {
            break;
        };

        // Find neighboring quad that shares next_edge
        let mut next_face = None;
        for (f_idx, other_face) in mesh.faces.iter().enumerate() {
            if f_idx == current_face || other_face.verts.len() != 4 {
                continue;
            }
            for i in 0..4 {
                let e = canonical_edge(other_face.verts[i], other_face.verts[(i + 1) % 4]);
                if e == next_edge {
                    next_face = Some(f_idx);
                    break;
                }
            }
            if next_face.is_some() {
                break;
            }
        }

        match next_face {
            Some(nf) => {
                current_face = nf;
                current_edge = next_edge;
            }
            None => break,
        }
    }

    loop_faces
}

/// Selects sub-objects located within an NDC box `[min_ndc, max_ndc]` given a `view_proj` 4x4 matrix.
pub fn box_select_ndc(
    mesh: &PolyMesh,
    view_proj: [[f32; 4]; 4],
    min_ndc: [f32; 2],
    max_ndc: [f32; 2],
    mode: SubObjectMode,
) -> SubObjectSelection {
    let mut selection = SubObjectSelection::new();

    let project_point = |pos: Vec3| -> Option<[f32; 2]> {
        let x = pos.x * view_proj[0][0]
            + pos.y * view_proj[1][0]
            + pos.z * view_proj[2][0]
            + view_proj[3][0];
        let y = pos.x * view_proj[0][1]
            + pos.y * view_proj[1][1]
            + pos.z * view_proj[2][1]
            + view_proj[3][1];
        let z = pos.x * view_proj[0][2]
            + pos.y * view_proj[1][2]
            + pos.z * view_proj[2][2]
            + view_proj[3][2];
        let w = pos.x * view_proj[0][3]
            + pos.y * view_proj[1][3]
            + pos.z * view_proj[2][3]
            + view_proj[3][3];

        if w <= 1e-6 || z < -w || z > w {
            return None;
        }

        Some([x / w, y / w])
    };

    let in_box = |ndc: [f32; 2]| -> bool {
        ndc[0] >= min_ndc[0]
            && ndc[0] <= max_ndc[0]
            && ndc[1] >= min_ndc[1]
            && ndc[1] <= max_ndc[1]
    };

    match mode {
        SubObjectMode::Vertex => {
            for (idx, &pos) in mesh.positions.iter().enumerate() {
                if let Some(_ndc) = project_point(pos).filter(|&ndc| in_box(ndc)) {
                    selection.select_vertex(idx as u32);
                }
            }
        }
        SubObjectMode::Edge => {
            let mut unique_edges = BTreeSet::new();
            for face in &mesh.faces {
                let n = face.verts.len();
                for i in 0..n {
                    unique_edges.insert(canonical_edge(face.verts[i], face.verts[(i + 1) % n]));
                }
            }
            for &e in &mesh.loose_edges {
                unique_edges.insert(canonical_edge(e[0], e[1]));
            }

            for edge in unique_edges {
                let p0 = mesh.positions.get(edge[0] as usize).copied();
                let p1 = mesh.positions.get(edge[1] as usize).copied();
                if let (Some(a), Some(b)) = (p0, p1) {
                    let both_in_box = match (project_point(a), project_point(b)) {
                        (Some(ndc_a), Some(ndc_b)) => in_box(ndc_a) && in_box(ndc_b),
                        _ => false,
                    };
                    if both_in_box {
                        selection.select_edge(edge[0], edge[1]);
                    }
                }
            }
        }
        SubObjectMode::Face => {
            for (f_idx, face) in mesh.faces.iter().enumerate() {
                let mut all_inside = !face.verts.is_empty();
                for &v in &face.verts {
                    let pos = match mesh.positions.get(v as usize) {
                        Some(&p) => p,
                        None => {
                            all_inside = false;
                            break;
                        }
                    };
                    let ndc = match project_point(pos) {
                        Some(p) => p,
                        None => {
                            all_inside = false;
                            break;
                        }
                    };
                    if !in_box(ndc) {
                        all_inside = false;
                        break;
                    }
                }
                if all_inside {
                    selection.select_face(f_idx as u32);
                }
            }
        }
    }

    selection
}

/// Falloff curves for proportional editing (`O` in Blender).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProportionalFalloff {
    Smooth,
    Sphere,
    Root,
    Sharp,
    Linear,
}

impl ProportionalFalloff {
    /// Evaluates the falloff weight for normalized distance `t = distance / radius in [0, 1]`.
    #[inline]
    pub fn evaluate(&self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Linear => 1.0 - t,
            Self::Smooth => {
                // Cosine smoothstep falloff: 0.5 * (1 + cos(pi * t))
                0.5 * (1.0 + (t * std::f32::consts::PI).cos())
            }
            Self::Sphere => (1.0 - t * t).max(0.0).sqrt(),
            Self::Root => (1.0 - t).max(0.0).sqrt(),
            Self::Sharp => (1.0 - t).powi(2),
        }
    }
}

/// Applies a proportional translation to `mesh` vertices based on `selection` and falloff radius.
pub fn transform_selection_proportional(
    mesh: &mut PolyMesh,
    selection: &SubObjectSelection,
    translation: Vec3,
    radius: f32,
    falloff: ProportionalFalloff,
) {
    let selected_verts = selection.all_selected_vertex_indices(mesh);
    if selected_verts.is_empty() || radius <= 1e-6 {
        return;
    }

    let selected_positions: Vec<Vec3> = selected_verts
        .iter()
        .filter_map(|&vi| mesh.positions.get(vi as usize).copied())
        .collect();

    for pos in &mut mesh.positions {
        let mut min_dist = f32::INFINITY;
        for &s_pos in &selected_positions {
            let d = (*pos - s_pos).length();
            if d < min_dist {
                min_dist = d;
            }
        }

        if min_dist <= radius {
            let t = min_dist / radius;
            let weight = falloff.evaluate(t);
            *pos += translation * weight;
        }
    }
}

/// Step in the `polyops 2` parameterized modeling transcript.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum PolyOpStep {
    SelectVertex {
        indices: Vec<u32>,
    },
    SelectEdge {
        edges: Vec<[u32; 2]>,
    },
    SelectFace {
        indices: Vec<u32>,
    },
    SelectLoop {
        edge: [u32; 2],
    },
    ExtrudeFaces {
        face_indices: Vec<u32>,
        distance: f32,
    },
    InsetFaces {
        face_indices: Vec<u32>,
        thickness: f32,
        depth: f32,
    },
    BevelEdges {
        edges: Vec<[u32; 2]>,
        offset: f32,
        segments: u32,
    },
    LoopCut {
        axis: u8,
        factor: f32,
    },
    TranslateProportional {
        translation: [f32; 3],
        radius: f32,
        falloff: ProportionalFalloff,
    },
    TagEdge {
        edge: [u32; 2],
        tag: String,
    },
    TagFace {
        face_index: u32,
        tag: String,
    },
    ClearSelection,
}

/// Error returned when parsing a `polyops 2` script.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PolyopsParseError {
    #[error("Missing polyops 2 header, found: {0}")]
    MissingHeader(String),
    #[error("Invalid command at line {line}: {msg}")]
    InvalidCommand { line: usize, msg: String },
}

/// Error returned when replaying a `polyops 2` script.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PolyopsReplayError {
    #[error("Replay operation failed: {0}")]
    Failed(String),
}

/// Parses a `polyops 2` transcript into steps.
pub fn parse_polyops_v2(input: &str) -> Result<Vec<PolyOpStep>, PolyopsParseError> {
    let mut lines = input.lines().enumerate().peekable();
    let mut steps = Vec::new();

    // Find and check header
    let mut header_found = false;
    while let Some(&(_, line)) = lines.peek() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            lines.next();
            continue;
        }
        if trimmed == "polyops 2" {
            header_found = true;
            lines.next();
            break;
        } else {
            return Err(PolyopsParseError::MissingHeader(trimmed.into()));
        }
    }

    if !header_found {
        return Err(PolyopsParseError::MissingHeader("EOF before header".into()));
    }

    for (line_idx, line) in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        let cmd = parts[0];
        match cmd {
            "select_vertex" => {
                let list_str = parts.get(1).unwrap_or(&"");
                let indices = parse_u32_list(list_str).map_err(|e| PolyopsParseError::InvalidCommand {
                    line: line_idx + 1,
                    msg: format!("select_vertex indices parse error: {e}"),
                })?;
                steps.push(PolyOpStep::SelectVertex { indices });
            }
            "select_edge" => {
                let list_str = parts.get(1).unwrap_or(&"");
                let edges = parse_edge_list(list_str).map_err(|e| PolyopsParseError::InvalidCommand {
                    line: line_idx + 1,
                    msg: format!("select_edge edges parse error: {e}"),
                })?;
                steps.push(PolyOpStep::SelectEdge { edges });
            }
            "select_face" => {
                let list_str = parts.get(1).unwrap_or(&"");
                let indices = parse_u32_list(list_str).map_err(|e| PolyopsParseError::InvalidCommand {
                    line: line_idx + 1,
                    msg: format!("select_face indices parse error: {e}"),
                })?;
                steps.push(PolyOpStep::SelectFace { indices });
            }
            "select_loop" => {
                let edge_str = parts.get(1).unwrap_or(&"");
                let edge = parse_single_edge(edge_str).map_err(|e| PolyopsParseError::InvalidCommand {
                    line: line_idx + 1,
                    msg: format!("select_loop edge parse error: {e}"),
                })?;
                steps.push(PolyOpStep::SelectLoop { edge });
            }
            "extrude_faces" => {
                let list_str = parts.get(1).unwrap_or(&"");
                let face_indices = parse_u32_list(list_str).map_err(|e| PolyopsParseError::InvalidCommand {
                    line: line_idx + 1,
                    msg: format!("extrude_faces indices parse error: {e}"),
                })?;
                let mut dist = 1.0;
                for part in &parts[2..] {
                    if let Some(val) = part.strip_prefix("dist=") {
                        dist = val.parse().unwrap_or(1.0);
                    }
                }
                steps.push(PolyOpStep::ExtrudeFaces {
                    face_indices,
                    distance: dist,
                });
            }
            "inset_faces" => {
                let list_str = parts.get(1).unwrap_or(&"");
                let face_indices = parse_u32_list(list_str).map_err(|e| PolyopsParseError::InvalidCommand {
                    line: line_idx + 1,
                    msg: format!("inset_faces indices parse error: {e}"),
                })?;
                let mut thickness = 0.1;
                let mut depth = 0.0;
                for part in &parts[2..] {
                    if let Some(val) = part.strip_prefix("thickness=") {
                        thickness = val.parse().unwrap_or(0.1);
                    } else if let Some(val) = part.strip_prefix("depth=") {
                        depth = val.parse().unwrap_or(0.0);
                    }
                }
                steps.push(PolyOpStep::InsetFaces {
                    face_indices,
                    thickness,
                    depth,
                });
            }
            "bevel_edges" => {
                let list_str = parts.get(1).unwrap_or(&"");
                let edges = parse_edge_list(list_str).map_err(|e| PolyopsParseError::InvalidCommand {
                    line: line_idx + 1,
                    msg: format!("bevel_edges edges parse error: {e}"),
                })?;
                let mut offset = 0.1;
                let mut segments = 1;
                for part in &parts[2..] {
                    if let Some(val) = part.strip_prefix("offset=") {
                        offset = val.parse().unwrap_or(0.1);
                    } else if let Some(val) = part.strip_prefix("segments=") {
                        segments = val.parse().unwrap_or(1);
                    }
                }
                steps.push(PolyOpStep::BevelEdges {
                    edges,
                    offset,
                    segments,
                });
            }
            "loop_cut" => {
                let mut axis = 0;
                let mut factor = 0.5;
                for part in &parts[1..] {
                    if let Some(val) = part.strip_prefix("axis=") {
                        axis = val.parse().unwrap_or(0);
                    } else if let Some(val) = part.strip_prefix("factor=") {
                        factor = val.parse().unwrap_or(0.5);
                    }
                }
                steps.push(PolyOpStep::LoopCut { axis, factor });
            }
            "translate_proportional" => {
                let mut tx = 0.0f32;
                let mut ty = 0.0f32;
                let mut tz = 0.0f32;
                let mut radius = 1.0f32;
                let mut falloff = ProportionalFalloff::Smooth;

                for part in &parts[1..] {
                    if let Some(val) = part.strip_prefix("x=") {
                        tx = val.parse().unwrap_or(0.0);
                    } else if let Some(val) = part.strip_prefix("y=") {
                        ty = val.parse().unwrap_or(0.0);
                    } else if let Some(val) = part.strip_prefix("z=") {
                        tz = val.parse().unwrap_or(0.0);
                    } else if let Some(val) = part.strip_prefix("radius=") {
                        radius = val.parse().unwrap_or(1.0);
                    } else if let Some(val) = part.strip_prefix("falloff=") {
                        falloff = match val.to_lowercase().as_str() {
                            "linear" => ProportionalFalloff::Linear,
                            "sphere" => ProportionalFalloff::Sphere,
                            "root" => ProportionalFalloff::Root,
                            "sharp" => ProportionalFalloff::Sharp,
                            _ => ProportionalFalloff::Smooth,
                        };
                    }
                }
                steps.push(PolyOpStep::TranslateProportional {
                    translation: [tx, ty, tz],
                    radius,
                    falloff,
                });
            }
            "tag_edge" => {
                let edge_str = parts.get(1).unwrap_or(&"");
                let edge = parse_single_edge(edge_str).map_err(|e| PolyopsParseError::InvalidCommand {
                    line: line_idx + 1,
                    msg: format!("tag_edge edge parse error: {e}"),
                })?;
                let mut tag = "grind_edge".to_string();
                for part in &parts[2..] {
                    if let Some(val) = part.strip_prefix("tag=") {
                        tag = val.trim_matches('"').to_string();
                    }
                }
                steps.push(PolyOpStep::TagEdge { edge, tag });
            }
            "tag_face" => {
                let f_str = parts.get(1).unwrap_or(&"0");
                let face_index = f_str.parse().unwrap_or(0);
                let mut tag = "collision_convex".to_string();
                for part in &parts[2..] {
                    if let Some(val) = part.strip_prefix("tag=") {
                        tag = val.trim_matches('"').to_string();
                    }
                }
                steps.push(PolyOpStep::TagFace { face_index, tag });
            }
            "clear_selection" => {
                steps.push(PolyOpStep::ClearSelection);
            }
            other => {
                return Err(PolyopsParseError::InvalidCommand {
                    line: line_idx + 1,
                    msg: format!("unknown command '{other}'"),
                });
            }
        }
    }

    Ok(steps)
}

fn parse_u32_list(s: &str) -> Result<Vec<u32>, String> {
    if s.is_empty() {
        return Ok(Vec::new());
    }
    s.split(',')
        .map(|item| item.trim().parse::<u32>().map_err(|e| e.to_string()))
        .collect()
}

fn parse_single_edge(s: &str) -> Result<[u32; 2], String> {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 2 {
        return Err(format!("expected 'v0-v1', got '{s}'"));
    }
    let v0 = parts[0].trim().parse::<u32>().map_err(|e| e.to_string())?;
    let v1 = parts[1].trim().parse::<u32>().map_err(|e| e.to_string())?;
    Ok(canonical_edge(v0, v1))
}

fn parse_edge_list(s: &str) -> Result<Vec<[u32; 2]>, String> {
    if s.is_empty() {
        return Ok(Vec::new());
    }
    s.split(',')
        .map(|item| parse_single_edge(item.trim()))
        .collect()
}

/// Serializes steps into a `polyops 2` formatted transcript.
pub fn emit_polyops_v2(steps: &[PolyOpStep]) -> String {
    let mut out = String::from("polyops 2\n");
    for step in steps {
        match step {
            PolyOpStep::SelectVertex { indices } => {
                let list = indices
                    .iter()
                    .map(|i| i.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                out.push_str(&format!("select_vertex {list}\n"));
            }
            PolyOpStep::SelectEdge { edges } => {
                let list = edges
                    .iter()
                    .map(|e| format!("{}-{}", e[0], e[1]))
                    .collect::<Vec<_>>()
                    .join(",");
                out.push_str(&format!("select_edge {list}\n"));
            }
            PolyOpStep::SelectFace { indices } => {
                let list = indices
                    .iter()
                    .map(|i| i.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                out.push_str(&format!("select_face {list}\n"));
            }
            PolyOpStep::SelectLoop { edge } => {
                out.push_str(&format!("select_loop {}-{}\n", edge[0], edge[1]));
            }
            PolyOpStep::ExtrudeFaces {
                face_indices,
                distance,
            } => {
                let list = face_indices
                    .iter()
                    .map(|i| i.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                out.push_str(&format!("extrude_faces {list} dist={distance}\n"));
            }
            PolyOpStep::InsetFaces {
                face_indices,
                thickness,
                depth,
            } => {
                let list = face_indices
                    .iter()
                    .map(|i| i.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                out.push_str(&format!(
                    "inset_faces {list} thickness={thickness} depth={depth}\n"
                ));
            }
            PolyOpStep::BevelEdges {
                edges,
                offset,
                segments,
            } => {
                let list = edges
                    .iter()
                    .map(|e| format!("{}-{}", e[0], e[1]))
                    .collect::<Vec<_>>()
                    .join(",");
                out.push_str(&format!(
                    "bevel_edges {list} offset={offset} segments={segments}\n"
                ));
            }
            PolyOpStep::LoopCut { axis, factor } => {
                out.push_str(&format!("loop_cut axis={axis} factor={factor}\n"));
            }
            PolyOpStep::TranslateProportional {
                translation,
                radius,
                falloff,
            } => {
                let falloff_str = match falloff {
                    ProportionalFalloff::Smooth => "smooth",
                    ProportionalFalloff::Sphere => "sphere",
                    ProportionalFalloff::Root => "root",
                    ProportionalFalloff::Sharp => "sharp",
                    ProportionalFalloff::Linear => "linear",
                };
                out.push_str(&format!(
                    "translate_proportional x={} y={} z={} radius={} falloff={}\n",
                    translation[0], translation[1], translation[2], radius, falloff_str
                ));
            }
            PolyOpStep::TagEdge { edge, tag } => {
                out.push_str(&format!("tag_edge {}-{} tag=\"{}\"\n", edge[0], edge[1], tag));
            }
            PolyOpStep::TagFace { face_index, tag } => {
                out.push_str(&format!("tag_face {} tag=\"{}\"\n", face_index, tag));
            }
            PolyOpStep::ClearSelection => {
                out.push_str("clear_selection\n");
            }
        }
    }
    out
}

/// Replays a `polyops 2` transcript against a mesh and semantic attribute set.
pub fn replay_polyops_v2(
    mesh: &mut PolyMesh,
    attrs: &mut PolyamourSemanticAttributes,
    steps: &[PolyOpStep],
) -> Result<(), PolyopsReplayError> {
    let mut current_selection = SubObjectSelection::new();

    for step in steps {
        match step {
            PolyOpStep::SelectVertex { indices } => {
                for &v in indices {
                    current_selection.select_vertex(v);
                }
            }
            PolyOpStep::SelectEdge { edges } => {
                for &e in edges {
                    current_selection.select_edge(e[0], e[1]);
                }
            }
            PolyOpStep::SelectFace { indices } => {
                for &f in indices {
                    current_selection.select_face(f);
                }
            }
            PolyOpStep::SelectLoop { edge } => {
                let loop_edges = select_edge_loop(mesh, *edge);
                for e in loop_edges {
                    current_selection.select_edge(e[0], e[1]);
                }
            }
            PolyOpStep::ExtrudeFaces {
                face_indices,
                distance,
            } => {
                let indices: Vec<usize> = face_indices.iter().map(|&i| i as usize).collect();
                extrude_faces(mesh, &indices, *distance);
            }
            PolyOpStep::InsetFaces {
                face_indices,
                thickness,
                depth,
            } => {
                let indices: Vec<usize> = face_indices.iter().map(|&i| i as usize).collect();
                inset_faces(mesh, &indices, *thickness, *depth);
            }
            PolyOpStep::BevelEdges {
                offset,
                segments,
                ..
            } => {
                bevel_edges(mesh, *offset, *segments);
            }
            PolyOpStep::LoopCut { axis, factor } => {
                loop_cut(mesh, *axis as usize, *factor);
            }
            PolyOpStep::TranslateProportional {
                translation,
                radius,
                falloff,
            } => {
                let t_vec = Vec3::new(translation[0], translation[1], translation[2]);
                transform_selection_proportional(
                    mesh,
                    &current_selection,
                    t_vec,
                    *radius,
                    *falloff,
                );
            }
            PolyOpStep::TagEdge { edge, tag } => {
                attrs.tag_edge(*edge, tag.clone());
            }
            PolyOpStep::TagFace { face_index, tag } => {
                attrs.tag_face(*face_index, tag.clone());
            }
            PolyOpStep::ClearSelection => {
                current_selection.clear();
            }
        }
    }

    Ok(())
}
