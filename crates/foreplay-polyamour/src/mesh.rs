//! N-Gon Polygon Mesh & Topological Half-Edge Representation (`foreplay-polyamour::mesh`).
//!
//! Provides:
//! - `PolyMesh`: n-gon polygon mesh (`positions`, `faces` with UVs and material slots, `loose_edges`).
//! - `TriangulatedMesh`: GPU-ready triangle mesh with vertex normals and UV coordinates.
//! - Ear-clipping triangulation and vertex welding.
//! - `HalfEdgeMesh`: topological view offering vertex neighborhood queries, boundary detection,
//!   manifold verification, edge loops, and face areas.

use bevy_math::Vec3;
use std::collections::{BTreeMap, BTreeSet};

/// A single n-gon face with vertex indices, UV coordinates, smoothing, and material assignment.
#[derive(Clone, Debug, PartialEq)]
pub struct PolyFace {
    pub verts: Vec<u32>,
    pub uvs: Vec<[f32; 2]>,
    pub smooth: bool,
    pub material_slot: u16,
}

impl PolyFace {
    pub fn new(verts: Vec<u32>) -> Self {
        let uvs = vec![[0.0, 0.0]; verts.len()];
        Self {
            verts,
            uvs,
            smooth: false,
            material_slot: 0,
        }
    }

    pub fn with_uvs(mut self, uvs: Vec<[f32; 2]>) -> Self {
        self.uvs = uvs;
        self
    }

    pub fn with_smooth(mut self, smooth: bool) -> Self {
        self.smooth = smooth;
        self
    }

    pub fn with_material(mut self, slot: u16) -> Self {
        self.material_slot = slot;
        self
    }
}

/// GPU-ready triangulated mesh buffer.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TriangulatedMesh {
    pub positions: Vec<Vec3>,
    pub normals: Vec<Vec3>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
    pub material_slots: Vec<u16>,
}

/// Arbitrary n-gon polygon mesh.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PolyMesh {
    pub positions: Vec<Vec3>,
    pub faces: Vec<PolyFace>,
    pub loose_edges: Vec<[u32; 2]>,
}

impl PolyMesh {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a vertex position and returns its index.
    pub fn add_vertex(&mut self, pos: Vec3) -> u32 {
        let idx = self.positions.len() as u32;
        self.positions.push(pos);
        idx
    }

    /// Adds an n-gon face.
    pub fn add_face(
        &mut self,
        verts: Vec<u32>,
        uvs: Vec<[f32; 2]>,
        smooth: bool,
        material_slot: u16,
    ) -> usize {
        let idx = self.faces.len();
        let uvs = if uvs.len() == verts.len() {
            uvs
        } else {
            vec![[0.0, 0.0]; verts.len()]
        };
        self.faces.push(PolyFace {
            verts,
            uvs,
            smooth,
            material_slot,
        });
        idx
    }

    /// Computes the face normal using Newell's method for arbitrary planar n-gons.
    pub fn compute_face_normal(&self, face: &PolyFace) -> Vec3 {
        let n = face.verts.len();
        if n < 3 {
            return Vec3::Y;
        }

        let mut normal = Vec3::ZERO;
        for i in 0..n {
            let i_next = (i + 1) % n;
            let vi = self.positions.get(face.verts[i] as usize).copied().unwrap_or(Vec3::ZERO);
            let vj = self
                .positions
                .get(face.verts[i_next] as usize)
                .copied()
                .unwrap_or(Vec3::ZERO);

            normal.x += (vi.y - vj.y) * (vi.z + vj.z);
            normal.y += (vi.z - vj.z) * (vi.x + vj.x);
            normal.z += (vi.x - vj.x) * (vi.y + vj.y);
        }

        if normal.length_squared() > 1e-8 {
            normal.normalize()
        } else {
            Vec3::Y
        }
    }

    /// Computes the centroid of a face.
    pub fn compute_face_center(&self, face: &PolyFace) -> Vec3 {
        if face.verts.is_empty() {
            return Vec3::ZERO;
        }
        let sum: Vec3 = face
            .verts
            .iter()
            .filter_map(|&vi| self.positions.get(vi as usize).copied())
            .sum();
        sum / (face.verts.len() as f32)
    }

    /// Computes the axis-aligned bounding box `(min, max)`.
    pub fn bounding_box(&self) -> (Vec3, Vec3) {
        if self.positions.is_empty() {
            return (Vec3::ZERO, Vec3::ZERO);
        }
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        for &p in &self.positions {
            min = min.min(p);
            max = max.max(p);
        }
        (min, max)
    }

    /// Welds vertices that are within `distance` threshold of each other.
    pub fn weld_vertices(&mut self, distance: f32) {
        if self.positions.is_empty() {
            return;
        }

        let dist_sq = distance * distance;
        let mut remap: Vec<u32> = Vec::with_capacity(self.positions.len());
        let mut new_positions: Vec<Vec3> = Vec::new();

        for &pos in self.positions.iter() {
            let mut welded_to: Option<u32> = None;
            for (new_idx, &existing) in new_positions.iter().enumerate() {
                if pos.distance_squared(existing) <= dist_sq {
                    welded_to = Some(new_idx as u32);
                    break;
                }
            }

            match welded_to {
                Some(target) => remap.push(target),
                None => {
                    let new_idx = new_positions.len() as u32;
                    new_positions.push(pos);
                    remap.push(new_idx);
                }
            }
        }

        self.positions = new_positions;

        // Remap face vertex indices and deduplicate consecutive identical vertices
        for face in &mut self.faces {
            let mut remapped_verts = Vec::with_capacity(face.verts.len());
            let mut remapped_uvs = Vec::with_capacity(face.uvs.len());

            for (i, &v) in face.verts.iter().enumerate() {
                let mapped = remap[v as usize];
                if remapped_verts.last().copied() != Some(mapped) {
                    remapped_verts.push(mapped);
                    if i < face.uvs.len() {
                        remapped_uvs.push(face.uvs[i]);
                    } else {
                        remapped_uvs.push([0.0, 0.0]);
                    }
                }
            }

            // Also check wraparound between last and first
            if remapped_verts.len() > 1 && remapped_verts.first() == remapped_verts.last() {
                remapped_verts.pop();
                remapped_uvs.pop();
            }

            face.verts = remapped_verts;
            face.uvs = remapped_uvs;
        }

        // Drop collapsed faces with fewer than 3 vertices
        self.faces.retain(|f| f.verts.len() >= 3);

        // Remap loose edges
        for edge in &mut self.loose_edges {
            edge[0] = remap[edge[0] as usize];
            edge[1] = remap[edge[1] as usize];
        }
        self.loose_edges.retain(|e| e[0] != e[1]);
    }

    /// Triangulates all n-gons into GPU-ready triangle buffers using ear-clipping.
    pub fn triangulate(&self) -> TriangulatedMesh {
        let mut out = TriangulatedMesh::default();

        // Precompute smooth vertex normals if any face uses smooth shading
        let mut vertex_smooth_normals = vec![Vec3::ZERO; self.positions.len()];
        let mut vertex_normal_counts = vec![0u32; self.positions.len()];

        for face in &self.faces {
            if face.smooth && face.verts.len() >= 3 {
                let n = self.compute_face_normal(face);
                for &v in &face.verts {
                    let v_idx = v as usize;
                    if v_idx < vertex_smooth_normals.len() {
                        vertex_smooth_normals[v_idx] += n;
                        vertex_normal_counts[v_idx] += 1;
                    }
                }
            }
        }

        for (i, count) in vertex_normal_counts.iter().enumerate() {
            if *count > 0 {
                vertex_smooth_normals[i] = vertex_smooth_normals[i].normalize_or_zero();
            }
        }

        for face in &self.faces {
            let n_verts = face.verts.len();
            if n_verts < 3 {
                continue;
            }

            let face_normal = self.compute_face_normal(face);

            // Triangulate face into index triples relative to face.verts
            let tri_indices = Self::ear_clip_polygon(&face.verts, &self.positions, face_normal);

            for &[i0, i1, i2] in &tri_indices {
                let v0_idx = face.verts[i0] as usize;
                let v1_idx = face.verts[i1] as usize;
                let v2_idx = face.verts[i2] as usize;

                let p0 = self.positions.get(v0_idx).copied().unwrap_or(Vec3::ZERO);
                let p1 = self.positions.get(v1_idx).copied().unwrap_or(Vec3::ZERO);
                let p2 = self.positions.get(v2_idx).copied().unwrap_or(Vec3::ZERO);

                let uv0 = face.uvs.get(i0).copied().unwrap_or([0.0, 0.0]);
                let uv1 = face.uvs.get(i1).copied().unwrap_or([0.0, 0.0]);
                let uv2 = face.uvs.get(i2).copied().unwrap_or([0.0, 0.0]);

                let (n0, n1, n2) = if face.smooth {
                    (
                        vertex_smooth_normals[v0_idx],
                        vertex_smooth_normals[v1_idx],
                        vertex_smooth_normals[v2_idx],
                    )
                } else {
                    (face_normal, face_normal, face_normal)
                };

                let base_idx = out.positions.len() as u32;
                out.positions.extend_from_slice(&[p0, p1, p2]);
                out.normals.extend_from_slice(&[n0, n1, n2]);
                out.uvs.extend_from_slice(&[uv0, uv1, uv2]);
                out.indices.extend_from_slice(&[base_idx, base_idx + 1, base_idx + 2]);
                out.material_slots.extend_from_slice(&[
                    face.material_slot,
                    face.material_slot,
                    face.material_slot,
                ]);
            }
        }

        out
    }

    /// Ear-clipping triangulation algorithm for an arbitrary planar n-gon.
    ///
    /// Returns a list of `[local_i0, local_i1, local_i2]` triangle vertex indices.
    fn ear_clip_polygon(verts: &[u32], positions: &[Vec3], normal: Vec3) -> Vec<[usize; 3]> {
        let n = verts.len();
        if n == 3 {
            return vec![[0, 1, 2]];
        }
        if n == 4 {
            // Quad split
            return vec![[0, 1, 2], [0, 2, 3]];
        }

        // Project 3D points onto 2D plane perpendicular to normal
        let up = if normal.y.abs() > 0.99 { Vec3::Z } else { Vec3::Y };
        let u_axis = normal.cross(up).normalize_or_zero();
        let v_axis = normal.cross(u_axis).normalize_or_zero();

        let pts_2d: Vec<[f32; 2]> = verts
            .iter()
            .map(|&vi| {
                let p = positions.get(vi as usize).copied().unwrap_or(Vec3::ZERO);
                [p.dot(u_axis), p.dot(v_axis)]
            })
            .collect();

        // Polygon orientation (signed area in 2D)
        let mut signed_area = 0.0;
        for i in 0..n {
            let next = (i + 1) % n;
            signed_area += pts_2d[i][0] * pts_2d[next][1] - pts_2d[next][0] * pts_2d[i][1];
        }

        let is_ccw = signed_area >= 0.0;

        let mut remaining: Vec<usize> = (0..n).collect();
        let mut triangles = Vec::with_capacity(n - 2);

        let mut loop_count = 0;
        let max_loops = n * n;

        while remaining.len() > 3 && loop_count < max_loops {
            loop_count += 1;
            let mut ear_found = false;
            let m = remaining.len();

            for i in 0..m {
                let prev = remaining[(i + m - 1) % m];
                let curr = remaining[i];
                let next = remaining[(i + 1) % m];

                let a = pts_2d[prev];
                let b = pts_2d[curr];
                let c = pts_2d[next];

                // Check convexity: cross product (b - a) x (c - b)
                let cross = (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0]);
                let is_convex = if is_ccw { cross > 1e-7 } else { cross < -1e-7 };

                if !is_convex {
                    continue;
                }

                // Check if any other point lies strictly inside triangle abc
                let mut point_inside = false;
                for &test_idx in &remaining {
                    if test_idx == prev || test_idx == curr || test_idx == next {
                        continue;
                    }
                    if point_in_triangle_2d(pts_2d[test_idx], a, b, c) {
                        point_inside = true;
                        break;
                    }
                }

                if !point_inside {
                    triangles.push([prev, curr, next]);
                    remaining.remove(i);
                    ear_found = true;
                    break;
                }
            }

            if !ear_found {
                // Fallback: fan triangulation for remaining vertices
                break;
            }
        }

        // Fan triangulate any remaining vertices
        if remaining.len() >= 3 {
            for i in 1..(remaining.len() - 1) {
                triangles.push([remaining[0], remaining[i], remaining[i + 1]]);
            }
        }

        triangles
    }
}

/// Tests if point `p` is strictly inside triangle `a`, `b`, `c` in 2D.
fn point_in_triangle_2d(p: [f32; 2], a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> bool {
    let sign = |p1: [f32; 2], p2: [f32; 2], p3: [f32; 2]| {
        (p1[0] - p3[0]) * (p2[1] - p3[1]) - (p2[0] - p3[0]) * (p1[1] - p3[1])
    };

    let d1 = sign(p, a, b);
    let d2 = sign(p, b, c);
    let d3 = sign(p, c, a);

    let has_neg = (d1 < -1e-6) || (d2 < -1e-6) || (d3 < -1e-6);
    let has_pos = (d1 > 1e-6) || (d2 > 1e-6) || (d3 > 1e-6);

    !(has_neg && has_pos)
}

/// Topological Half-Edge view of a `PolyMesh`.
#[derive(Clone, Debug, PartialEq)]
pub struct HalfEdgeMesh {
    /// Adjacency: vertex index -> set of neighbor vertex indices.
    vertex_neighbors: BTreeMap<u32, BTreeSet<u32>>,
    /// Directed edge count: (origin, target) -> number of faces with this directed edge.
    edge_face_counts: BTreeMap<[u32; 2], usize>,
    /// Undirected edges: canonical [min, max] -> count.
    edge_counts: BTreeMap<[u32; 2], usize>,
    /// Boundary edges (edges used by exactly one face).
    boundary_edges: Vec<[u32; 2]>,
    /// Quad edge loops.
    face_areas: Vec<f32>,
    is_manifold: bool,
    faces_as_quads: Vec<Option<[u32; 4]>>,
}

impl HalfEdgeMesh {
    /// Builds a topological half-edge structure from a `PolyMesh`.
    pub fn from_polymesh(mesh: &PolyMesh) -> Self {
        let mut vertex_neighbors: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
        let mut edge_face_counts: BTreeMap<[u32; 2], usize> = BTreeMap::new();
        let mut edge_counts: BTreeMap<[u32; 2], usize> = BTreeMap::new();
        let mut face_areas: Vec<f32> = Vec::with_capacity(mesh.faces.len());
        let mut faces_as_quads = Vec::with_capacity(mesh.faces.len());

        let mut is_manifold = true;

        for face in &mesh.faces {
            let n = face.verts.len();
            if n < 3 {
                face_areas.push(0.0);
                faces_as_quads.push(None);
                continue;
            }

            if n == 4 {
                faces_as_quads.push(Some([
                    face.verts[0],
                    face.verts[1],
                    face.verts[2],
                    face.verts[3],
                ]));
            } else {
                faces_as_quads.push(None);
            }

            // Compute face area using triangle decomposition from centroid
            let center = mesh.compute_face_center(face);
            let mut area = 0.0;
            for i in 0..n {
                let next = (i + 1) % n;
                let v0 = mesh.positions.get(face.verts[i] as usize).copied().unwrap_or(Vec3::ZERO);
                let v1 = mesh.positions.get(face.verts[next] as usize).copied().unwrap_or(Vec3::ZERO);
                let cross = (v0 - center).cross(v1 - center);
                area += 0.5 * cross.length();
            }
            face_areas.push(area);

            // Record directed edges and undirected edge counts
            for i in 0..n {
                let v0 = face.verts[i];
                let v1 = face.verts[(i + 1) % n];

                vertex_neighbors.entry(v0).or_default().insert(v1);
                vertex_neighbors.entry(v1).or_default().insert(v0);

                let dir_edge = [v0, v1];
                let dir_count = edge_face_counts.entry(dir_edge).or_insert(0);
                *dir_count += 1;
                if *dir_count > 1 {
                    // Two faces sharing identical directed orientation -> non-manifold orientation!
                    is_manifold = false;
                }

                let undir_edge = if v0 < v1 { [v0, v1] } else { [v1, v0] };
                let undir_count = edge_counts.entry(undir_edge).or_insert(0);
                *undir_count += 1;
                if *undir_count > 2 {
                    // Edge shared by > 2 faces -> non-manifold!
                    is_manifold = false;
                }
            }
        }

        // Identify boundary edges (edges with undirected count == 1)
        let mut boundary_edges = Vec::new();
        for (&edge, &count) in &edge_counts {
            if count == 1 {
                boundary_edges.push(edge);
            }
        }

        Self {
            vertex_neighbors,
            edge_face_counts,
            edge_counts,
            boundary_edges,
            face_areas,
            is_manifold,
            faces_as_quads,
        }
    }

    /// Returns direct neighbor vertex indices connected to `vertex`.
    pub fn vertex_neighbors(&self, vertex: u32) -> Vec<u32> {
        self.vertex_neighbors
            .get(&vertex)
            .map(|set| set.iter().copied().collect())
            .unwrap_or_default()
    }

    /// Returns all boundary edges `[v0, v1]`.
    pub fn boundary_edges(&self) -> &[[u32; 2]] {
        &self.boundary_edges
    }

    /// Checks whether the mesh is a 2-manifold surface.
    pub fn is_manifold(&self) -> bool {
        self.is_manifold
    }

    /// Computes the area of each face.
    pub fn face_areas(&self) -> &[f32] {
        &self.face_areas
    }

    /// Traverses a quad edge loop starting from edge `start_edge`.
    ///
    /// Walks opposite edges across quad faces until hitting a boundary or loop closure.
    pub fn edge_loop(&self, start_edge: [u32; 2]) -> Vec<[u32; 2]> {
        let mut loop_edges = Vec::new();
        let mut visited = BTreeSet::new();

        let canon_start = if start_edge[0] < start_edge[1] {
            start_edge
        } else {
            [start_edge[1], start_edge[0]]
        };

        // Find the first quad face containing start_edge
        let mut current_face_idx = None;
        for (f_idx, quad_opt) in self.faces_as_quads.iter().enumerate() {
            if let Some(quad) = quad_opt {
                for i in 0..4 {
                    let e = [quad[i], quad[(i + 1) % 4]];
                    let c = if e[0] < e[1] { e } else { [e[1], e[0]] };
                    if c == canon_start {
                        current_face_idx = Some(f_idx);
                        break;
                    }
                }
                if current_face_idx.is_some() {
                    break;
                }
            }
        }

        let Some(mut curr_face) = current_face_idx else {
            return loop_edges;
        };

        let mut curr_edge = canon_start;

        while visited.insert(curr_edge) {
            loop_edges.push(curr_edge);

            // In curr_face, find the opposite edge to curr_edge
            let quad = match self.faces_as_quads.get(curr_face).and_then(|q| q.as_ref()) {
                Some(q) => q,
                None => break,
            };

            let mut opp_edge = None;
            for i in 0..4 {
                let e = [quad[i], quad[(i + 1) % 4]];
                let c = if e[0] < e[1] { e } else { [e[1], e[0]] };
                if c == curr_edge {
                    let opp = [quad[(i + 2) % 4], quad[(i + 3) % 4]];
                    opp_edge = Some(if opp[0] < opp[1] { opp } else { [opp[1], opp[0]] });
                    break;
                }
            }

            let Some(next_edge) = opp_edge else {
                break;
            };

            if visited.contains(&next_edge) {
                break;
            }

            // Find the neighboring quad face that shares next_edge and is NOT curr_face
            let mut next_face_opt = None;
            for (f_idx, quad_opt) in self.faces_as_quads.iter().enumerate() {
                if f_idx == curr_face {
                    continue;
                }
                if let Some(q) = quad_opt {
                    for i in 0..4 {
                        let e = [q[i], q[(i + 1) % 4]];
                        let c = if e[0] < e[1] { e } else { [e[1], e[0]] };
                        if c == next_edge {
                            next_face_opt = Some(f_idx);
                            break;
                        }
                    }
                    if next_face_opt.is_some() {
                        break;
                    }
                }
            }

            match next_face_opt {
                Some(next_face) => {
                    curr_face = next_face;
                    curr_edge = next_edge;
                }
                None => {
                    break;
                }
            }
        }

        loop_edges
    }
}
