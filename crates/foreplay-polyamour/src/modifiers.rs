//! Non-Destructive 9-Modifier Stack (`foreplay-polyamour::modifiers`).
//!
//! Provides the canonical 9 Polyamour / Blender modifiers:
//! 1. `Mirror`
//! 2. `Subdivision` (Catmull-Clark & Simple linear midpoint)
//! 3. `Array`
//! 4. `Solidify`
//! 5. `Bevel`
//! 6. `Triangulate`
//! 7. `Weld`
//! 8. `Displace`
//! 9. `Smooth` (Laplacian smoothing)
//!
//! Evaluated sequentially via `evaluate_modifier_stack`.

use crate::mesh::{HalfEdgeMesh, PolyMesh};
use crate::ops::{bevel_edges, recalc_normals_outward};
use bevy_math::Vec3;
use std::collections::BTreeMap;

/// Non-destructive modifier variants.
#[derive(Clone, Debug, PartialEq)]
pub enum PolyModifier {
    /// Mirrors mesh geometry across specified Cartesian axes and merges center vertices.
    Mirror {
        axis_x: bool,
        axis_y: bool,
        axis_z: bool,
        merge_distance: f32,
    },
    /// Subdivides polygons using Catmull-Clark or Simple linear midpoint smoothing.
    Subdivision { levels: u8, simple: bool },
    /// Duplicates mesh across linear relative and constant offsets.
    Array {
        count: u32,
        relative_offset: Vec3,
        constant_offset: Vec3,
    },
    /// Gives solid shell thickness to thin surfaces.
    Solidify { thickness: f32 },
    /// Chamfers/bevels edges.
    Bevel { width: f32 },
    /// Converts all n-gon polygons into triangles.
    Triangulate,
    /// Welds vertices within distance threshold.
    Weld { distance: f32 },
    /// Displaces vertices along their surface normal.
    Displace { strength: f32, frequency: f32 },
    /// Applies Laplacian smoothing iterations to vertex positions.
    Smooth { factor: f32, iterations: u32 },
}

/// Evaluates a stack of non-destructive modifiers sequentially on a base mesh.
pub fn evaluate_modifier_stack(base: &PolyMesh, stack: &[PolyModifier]) -> PolyMesh {
    let mut current = base.clone();

    for modifier in stack {
        current = apply_modifier(&current, modifier);
    }

    current
}

/// Dispatches a single modifier transformation.
pub fn apply_modifier(mesh: &PolyMesh, modifier: &PolyModifier) -> PolyMesh {
    match modifier {
        PolyModifier::Mirror {
            axis_x,
            axis_y,
            axis_z,
            merge_distance,
        } => apply_mirror(mesh, *axis_x, *axis_y, *axis_z, *merge_distance),
        PolyModifier::Subdivision { levels, simple } => apply_subdivision(mesh, *levels, *simple),
        PolyModifier::Array {
            count,
            relative_offset,
            constant_offset,
        } => apply_array(mesh, *count, *relative_offset, *constant_offset),
        PolyModifier::Solidify { thickness } => apply_solidify(mesh, *thickness),
        PolyModifier::Bevel { width } => apply_bevel(mesh, *width),
        PolyModifier::Triangulate => apply_triangulate(mesh),
        PolyModifier::Weld { distance } => apply_weld(mesh, *distance),
        PolyModifier::Displace {
            strength,
            frequency,
        } => apply_displace(mesh, *strength, *frequency),
        PolyModifier::Smooth { factor, iterations } => apply_smooth(mesh, *factor, *iterations),
    }
}

fn apply_mirror(
    mesh: &PolyMesh,
    axis_x: bool,
    axis_y: bool,
    axis_z: bool,
    merge_distance: f32,
) -> PolyMesh {
    if !axis_x && !axis_y && !axis_z {
        return mesh.clone();
    }

    let mut out = mesh.clone();
    let num_orig_verts = mesh.positions.len();

    // Reflection scale factor
    let scale = Vec3::new(
        if axis_x { -1.0 } else { 1.0 },
        if axis_y { -1.0 } else { 1.0 },
        if axis_z { -1.0 } else { 1.0 },
    );

    // Add reflected vertices
    let mut mirror_vert_map: Vec<u32> = Vec::with_capacity(num_orig_verts);
    for &p in &mesh.positions {
        let mirrored_p = p * scale;
        let v_idx = out.add_vertex(mirrored_p);
        mirror_vert_map.push(v_idx);
    }

    // Add mirrored faces with reversed winding to preserve outward normals
    let num_inversions = (if axis_x { 1 } else { 0 })
        + (if axis_y { 1 } else { 0 })
        + (if axis_z { 1 } else { 0 });
    let reverse_winding = num_inversions % 2 != 0;

    for face in &mesh.faces {
        let mut mirrored_verts: Vec<u32> = face
            .verts
            .iter()
            .map(|&v| mirror_vert_map[v as usize])
            .collect();
        let mut mirrored_uvs = face.uvs.clone();

        if reverse_winding {
            mirrored_verts.reverse();
            mirrored_uvs.reverse();
        }

        out.add_face(
            mirrored_verts,
            mirrored_uvs,
            face.smooth,
            face.material_slot,
        );
    }

    if merge_distance > 1e-6 {
        out.weld_vertices(merge_distance);
    }

    out
}

fn apply_subdivision(mesh: &PolyMesh, levels: u8, simple: bool) -> PolyMesh {
    let iters = levels.clamp(1, 4);
    let mut current = mesh.clone();

    for _ in 0..iters {
        current = subdivide_single_level(&current, simple);
    }

    current
}

/// Executes one level of Catmull-Clark or Simple midpoint subdivision.
fn subdivide_single_level(mesh: &PolyMesh, simple: bool) -> PolyMesh {
    let mut out = PolyMesh::new();

    // 1. Compute Face Points
    let mut face_points = Vec::with_capacity(mesh.faces.len());
    for face in &mesh.faces {
        face_points.push(mesh.compute_face_center(face));
    }

    // 2. Compute Edge Points & Half-edge adjacency
    let mut edge_to_face_pts: BTreeMap<[u32; 2], Vec<Vec3>> = BTreeMap::new();
    let mut vertex_incident_edges: BTreeMap<u32, Vec<[u32; 2]>> = BTreeMap::new();
    let mut vertex_incident_face_pts: BTreeMap<u32, Vec<Vec3>> = BTreeMap::new();

    for (f_idx, face) in mesh.faces.iter().enumerate() {
        let f_pt = face_points[f_idx];
        let n = face.verts.len();
        for i in 0..n {
            let v0 = face.verts[i];
            let v1 = face.verts[(i + 1) % n];
            let e = if v0 < v1 { [v0, v1] } else { [v1, v0] };

            edge_to_face_pts.entry(e).or_default().push(f_pt);
            vertex_incident_edges.entry(v0).or_default().push(e);
            vertex_incident_face_pts.entry(v0).or_default().push(f_pt);
        }
    }

    let mut edge_point_map: BTreeMap<[u32; 2], u32> = BTreeMap::new();
    for (&e, f_pts) in &edge_to_face_pts {
        let p0 = mesh.positions[e[0] as usize];
        let p1 = mesh.positions[e[1] as usize];

        let edge_pos = if simple || f_pts.len() < 2 {
            // Midpoint on boundary or simple mode
            (p0 + p1) * 0.5
        } else {
            // Catmull-Clark edge point: (p0 + p1 + f1 + f2) / 4
            (p0 + p1 + f_pts[0] + f_pts[1]) * 0.25
        };

        let new_v = out.add_vertex(edge_pos);
        edge_point_map.insert(e, new_v);
    }

    // 3. Compute Updated Vertex Points
    let mut vertex_point_map: Vec<u32> = Vec::with_capacity(mesh.positions.len());
    for (v_idx, &orig_p) in mesh.positions.iter().enumerate() {
        let v_u32 = v_idx as u32;

        let new_pos = if simple {
            orig_p
        } else if let (Some(f_pts), Some(edges)) = (
            vertex_incident_face_pts.get(&v_u32),
            vertex_incident_edges.get(&v_u32),
        ) {
            let n = edges.len() as f32;
            if n < 3.0 {
                orig_p
            } else {
                let f_avg: Vec3 = f_pts.iter().copied().sum::<Vec3>() / (f_pts.len() as f32);
                let edge_mid_avg: Vec3 = edges
                    .iter()
                    .map(|e| (mesh.positions[e[0] as usize] + mesh.positions[e[1] as usize]) * 0.5)
                    .sum::<Vec3>()
                    / n;

                (f_avg + (2.0 * edge_mid_avg) + ((n - 3.0) * orig_p)) / n
            }
        } else {
            orig_p
        };

        let mapped_v = out.add_vertex(new_pos);
        vertex_point_map.push(mapped_v);
    }

    // 4. Register Face Points as vertices in out mesh
    let mut face_point_vert_map = Vec::with_capacity(face_points.len());
    for &f_pt in &face_points {
        face_point_vert_map.push(out.add_vertex(f_pt));
    }

    // 5. Connect new quad faces
    for (f_idx, face) in mesh.faces.iter().enumerate() {
        let f_pt_idx = face_point_vert_map[f_idx];
        let n = face.verts.len();

        for i in 0..n {
            let prev_i = if i == 0 { n - 1 } else { i - 1 };
            let v_curr = face.verts[i];
            let v_prev = face.verts[prev_i];
            let next_i = (i + 1) % n;
            let v_next = face.verts[next_i];

            let e_prev = if v_prev < v_curr {
                [v_prev, v_curr]
            } else {
                [v_curr, v_prev]
            };
            let e_next = if v_curr < v_next {
                [v_curr, v_next]
            } else {
                [v_next, v_curr]
            };

            let v_corner = vertex_point_map[v_curr as usize];
            let edge_prev_pt = edge_point_map[&e_prev];
            let edge_next_pt = edge_point_map[&e_next];

            // Quad connecting: edge_prev -> corner -> edge_next -> face_center
            let quad_verts = vec![edge_prev_pt, v_corner, edge_next_pt, f_pt_idx];
            let uvs = vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];

            out.add_face(quad_verts, uvs, face.smooth, face.material_slot);
        }
    }

    out
}

fn apply_array(
    mesh: &PolyMesh,
    count: u32,
    relative_offset: Vec3,
    constant_offset: Vec3,
) -> PolyMesh {
    if count <= 1 {
        return mesh.clone();
    }

    let (min, max) = mesh.bounding_box();
    let bbox_size = max - min;
    let step_offset = constant_offset + (bbox_size * relative_offset);

    let mut out = PolyMesh::new();

    for c in 0..count {
        let shift = step_offset * (c as f32);
        let base_vert_idx = out.positions.len() as u32;

        for &p in &mesh.positions {
            out.add_vertex(p + shift);
        }

        for face in &mesh.faces {
            let remapped_verts: Vec<u32> = face.verts.iter().map(|&v| base_vert_idx + v).collect();
            out.add_face(
                remapped_verts,
                face.uvs.clone(),
                face.smooth,
                face.material_slot,
            );
        }
    }

    out
}

fn apply_solidify(mesh: &PolyMesh, thickness: f32) -> PolyMesh {
    if thickness.abs() <= 1e-6 {
        return mesh.clone();
    }

    let mut out = mesh.clone();
    let num_orig_verts = mesh.positions.len();

    // Compute vertex normals from incident faces
    let mut vert_normals = vec![Vec3::ZERO; num_orig_verts];
    for face in &mesh.faces {
        let n = mesh.compute_face_normal(face);
        for &v in &face.verts {
            vert_normals[v as usize] += n;
        }
    }
    for n in &mut vert_normals {
        *n = n.normalize_or_zero();
    }

    // Add inner shell vertices offset along -normal
    let mut inner_vert_map = Vec::with_capacity(num_orig_verts);
    for (i, &norm) in vert_normals.iter().enumerate().take(num_orig_verts) {
        let inner_p = mesh.positions[i] - (norm * thickness);
        let new_v = out.add_vertex(inner_p);
        inner_vert_map.push(new_v);
    }

    // Add inner faces with inverted winding
    for face in &mesh.faces {
        let mut inner_verts: Vec<u32> = face
            .verts
            .iter()
            .map(|&v| inner_vert_map[v as usize])
            .collect();
        inner_verts.reverse();
        let mut inner_uvs = face.uvs.clone();
        inner_uvs.reverse();

        out.add_face(inner_verts, inner_uvs, face.smooth, face.material_slot);
    }

    // Connect boundary edges with quad rims
    let half_edge = HalfEdgeMesh::from_polymesh(mesh);
    for &edge in half_edge.boundary_edges() {
        let v0_out = edge[0];
        let v1_out = edge[1];
        let v1_in = inner_vert_map[v1_out as usize];
        let v0_in = inner_vert_map[v0_out as usize];

        out.add_face(
            vec![v0_out, v1_out, v1_in, v0_in],
            vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            false,
            0,
        );
    }

    out
}

fn apply_bevel(mesh: &PolyMesh, width: f32) -> PolyMesh {
    let mut out = mesh.clone();
    bevel_edges(&mut out, width, 1);
    out
}

fn apply_triangulate(mesh: &PolyMesh) -> PolyMesh {
    let tri_data = mesh.triangulate();
    let mut out = PolyMesh::new();

    let num_tris = tri_data.indices.len() / 3;
    for t in 0..num_tris {
        let i0 = tri_data.indices[t * 3] as usize;
        let i1 = tri_data.indices[t * 3 + 1] as usize;
        let i2 = tri_data.indices[t * 3 + 2] as usize;

        let v0 = out.add_vertex(tri_data.positions[i0]);
        let v1 = out.add_vertex(tri_data.positions[i1]);
        let v2 = out.add_vertex(tri_data.positions[i2]);

        let uv0 = tri_data.uvs[i0];
        let uv1 = tri_data.uvs[i1];
        let uv2 = tri_data.uvs[i2];

        let slot = tri_data.material_slots.get(t * 3).copied().unwrap_or(0);

        out.add_face(vec![v0, v1, v2], vec![uv0, uv1, uv2], false, slot);
    }

    out.weld_vertices(1e-5);
    out
}

fn apply_weld(mesh: &PolyMesh, distance: f32) -> PolyMesh {
    let mut out = mesh.clone();
    out.weld_vertices(distance);
    out
}

fn apply_displace(mesh: &PolyMesh, strength: f32, frequency: f32) -> PolyMesh {
    let mut out = mesh.clone();

    // Compute smooth vertex normals
    let mut normals = vec![Vec3::ZERO; out.positions.len()];
    for face in &out.faces {
        let fnorm = out.compute_face_normal(face);
        for &v in &face.verts {
            normals[v as usize] += fnorm;
        }
    }
    for n in &mut normals {
        *n = n.normalize_or_zero();
    }

    for (i, p) in out.positions.iter_mut().enumerate() {
        let disp = strength * (p.x * frequency).sin() * (p.z * frequency).cos();
        *p += normals[i] * disp;
    }

    out
}

fn apply_smooth(mesh: &PolyMesh, factor: f32, iterations: u32) -> PolyMesh {
    let mut out = mesh.clone();
    let iters = iterations.clamp(1, 10);
    let f = factor.clamp(0.0, 1.0);

    for _ in 0..iters {
        let half_edge = HalfEdgeMesh::from_polymesh(&out);
        let mut new_positions = out.positions.clone();

        for (i, pos_slot) in new_positions.iter_mut().enumerate() {
            let neighbors = half_edge.vertex_neighbors(i as u32);
            if !neighbors.is_empty() {
                let neighbor_avg: Vec3 = neighbors
                    .iter()
                    .map(|&nb| out.positions[nb as usize])
                    .sum::<Vec3>()
                    / (neighbors.len() as f32);

                *pos_slot = out.positions[i].lerp(neighbor_avg, f);
            }
        }

        out.positions = new_positions;
    }

    recalc_normals_outward(&mut out);
    out
}
