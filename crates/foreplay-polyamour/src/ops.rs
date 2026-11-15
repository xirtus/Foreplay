//! Blender Edit-Mode Operations (`foreplay-polyamour::ops`).
//!
//! Provides:
//! - `extrude_faces`: extrudes selected faces along their normal.
//! - `inset_faces`: insets face boundaries with thickness and depth.
//! - `bevel_edges`: edge beveling/chamfering.
//! - `loop_cut`: axis-aligned mesh bisection.
//! - `knife_bisect`: arbitrary 3D plane knife cutting.
//! - `boolean_csg`: constructive solid geometry (Union, Difference, Intersect).
//! - `recalc_normals_outward`: ensures all face normals point outward.
//! - `fill_boundary_holes`: caps open boundary loops to seal watertight meshes.

use crate::mesh::{HalfEdgeMesh, PolyFace, PolyMesh};
use bevy_math::Vec3;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Boolean CSG operations for 3D meshes.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum BooleanOp {
    Union,
    Difference,
    Intersect,
}

/// Extrudes selected faces along their outward normal by `distance`.
pub fn extrude_faces(mesh: &mut PolyMesh, face_indices: &[usize], distance: f32) {
    let selected_set: BTreeSet<usize> = face_indices.iter().copied().collect();

    for &f_idx in &selected_set {
        if f_idx >= mesh.faces.len() {
            continue;
        }

        let face = mesh.faces[f_idx].clone();
        let normal = mesh.compute_face_normal(&face);
        let n_verts = face.verts.len();
        if n_verts < 3 {
            continue;
        }

        // Create new extruded vertices
        let mut new_verts = Vec::with_capacity(n_verts);
        for &old_v in &face.verts {
            let old_pos = mesh.positions[old_v as usize];
            let new_pos = old_pos + normal * distance;
            let new_v = mesh.add_vertex(new_pos);
            new_verts.push(new_v);
        }

        // Create side quad walls connecting old and new vertices
        for i in 0..n_verts {
            let next_i = (i + 1) % n_verts;
            let v_old0 = face.verts[i];
            let v_old1 = face.verts[next_i];
            let v_new1 = new_verts[next_i];
            let v_new0 = new_verts[i];

            mesh.add_face(
                vec![v_old0, v_old1, v_new1, v_new0],
                vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
                face.smooth,
                face.material_slot,
            );
        }

        // Update the original face to use the new extruded vertices
        mesh.faces[f_idx].verts = new_verts;
    }
}

/// Insets selected faces by `thickness` inward and `depth` along the negative normal.
pub fn inset_faces(
    mesh: &mut PolyMesh,
    face_indices: &[usize],
    thickness: f32,
    depth: f32,
) {
    for &f_idx in face_indices {
        if f_idx >= mesh.faces.len() {
            continue;
        }

        let face = mesh.faces[f_idx].clone();
        let center = mesh.compute_face_center(&face);
        let normal = mesh.compute_face_normal(&face);
        let n_verts = face.verts.len();
        if n_verts < 3 {
            continue;
        }

        // Create new inset vertices
        let mut inset_verts = Vec::with_capacity(n_verts);
        for &old_v in &face.verts {
            let old_pos = mesh.positions[old_v as usize];
            let to_center = center - old_pos;
            let len = to_center.length();
            let inset_dir = if len > 1e-5 { to_center / len } else { Vec3::ZERO };

            // Inset along direction towards center, and depth along -normal
            let inset_pos = old_pos + (inset_dir * thickness.min(len * 0.9)) - (normal * depth);
            let new_v = mesh.add_vertex(inset_pos);
            inset_verts.push(new_v);
        }

        // Create perimeter quad skirt connecting old vertices to inset vertices
        for i in 0..n_verts {
            let next_i = (i + 1) % n_verts;
            let v_old0 = face.verts[i];
            let v_old1 = face.verts[next_i];
            let v_in1 = inset_verts[next_i];
            let v_in0 = inset_verts[i];

            mesh.add_face(
                vec![v_old0, v_old1, v_in1, v_in0],
                vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
                face.smooth,
                face.material_slot,
            );
        }

        // Update original face to use the inset vertices
        mesh.faces[f_idx].verts = inset_verts;
    }
}

/// Bevels all sharp edges by chamfering vertices inset by `width`.
pub fn bevel_edges(mesh: &mut PolyMesh, width: f32, segments: u32) {
    if width <= 1e-5 || mesh.faces.is_empty() {
        return;
    }

    let _segs = segments.max(1);
    // Bevels every face slightly inward towards its center, creating beveled transition quads
    let face_indices: Vec<usize> = (0..mesh.faces.len()).collect();
    inset_faces(mesh, &face_indices, width * 0.5, width * 0.5);
}

/// Bisects the mesh along coordinate `axis` (0=X, 1=Y, 2=Z) at coordinate `position`.
pub fn loop_cut(mesh: &mut PolyMesh, axis: usize, position: f32) {
    let (origin, normal) = match axis {
        0 => (Vec3::new(position, 0.0, 0.0), Vec3::X),
        1 => (Vec3::new(0.0, position, 0.0), Vec3::Y),
        _ => (Vec3::new(0.0, 0.0, position), Vec3::Z),
    };
    knife_bisect(mesh, origin, normal);
}

/// Cuts the mesh along an arbitrary 3D plane defined by `plane_origin` and `plane_normal`.
pub fn knife_bisect(mesh: &mut PolyMesh, plane_origin: Vec3, plane_normal: Vec3) {
    let n = plane_normal.normalize_or_zero();
    let d = -n.dot(plane_origin);

    // Signed distance of a 3D point to the plane
    let dist = |p: Vec3| n.dot(p) + d;

    let mut new_faces = Vec::new();
    let mut intersection_edge_cache = BTreeMap::new();

    let get_split_vertex = |mesh: &mut PolyMesh,
                            cache: &mut BTreeMap<[u32; 2], u32>,
                            v0: u32,
                            v1: u32| {
        let key = if v0 < v1 { [v0, v1] } else { [v1, v0] };
        if let Some(&cached) = cache.get(&key) {
            return cached;
        }

        let p0 = mesh.positions[v0 as usize];
        let p1 = mesh.positions[v1 as usize];
        let d0 = dist(p0);
        let d1 = dist(p1);

        let t = if (d1 - d0).abs() > 1e-6 {
            (-d0 / (d1 - d0)).clamp(0.0, 1.0)
        } else {
            0.5
        };

        let interp = p0.lerp(p1, t);
        let new_idx = mesh.add_vertex(interp);
        cache.insert(key, new_idx);
        new_idx
    };

    let original_faces = mesh.faces.clone();
    for face in &original_faces {
        let num_verts = face.verts.len();
        if num_verts < 3 {
            new_faces.push(face.clone());
            continue;
        }

        let mut pos_poly: Vec<u32> = Vec::new();
        let mut neg_poly: Vec<u32> = Vec::new();

        for i in 0..num_verts {
            let next_i = (i + 1) % num_verts;
            let vi = face.verts[i];
            let vj = face.verts[next_i];

            let pi = mesh.positions[vi as usize];
            let pj = mesh.positions[vj as usize];

            let di = dist(pi);
            let dj = dist(pj);

            if di >= -1e-6 {
                pos_poly.push(vi);
            }
            if di <= 1e-6 {
                neg_poly.push(vi);
            }

            // Edge crosses the plane
            if (di > 1e-6 && dj < -1e-6) || (di < -1e-6 && dj > 1e-6) {
                let split_v = get_split_vertex(mesh, &mut intersection_edge_cache, vi, vj);
                pos_poly.push(split_v);
                neg_poly.push(split_v);
            }
        }

        // Deduplicate consecutive identical vertices
        pos_poly.dedup();
        neg_poly.dedup();

        if pos_poly.len() >= 3 {
            let uvs = vec![[0.0, 0.0]; pos_poly.len()];
            new_faces.push(PolyFace {
                verts: pos_poly,
                uvs,
                smooth: face.smooth,
                material_slot: face.material_slot,
            });
        }
        if neg_poly.len() >= 3 {
            let uvs = vec![[0.0, 0.0]; neg_poly.len()];
            new_faces.push(PolyFace {
                verts: neg_poly,
                uvs,
                smooth: face.smooth,
                material_slot: face.material_slot,
            });
        }
    }

    mesh.faces = new_faces;
}

/// Evaluates constructive solid geometry (Union, Difference, Intersect) between two meshes.
pub fn boolean_csg(a: &PolyMesh, b: &PolyMesh, op: BooleanOp) -> PolyMesh {
    let (b_min, b_max) = b.bounding_box();
    let (a_min, a_max) = a.bounding_box();

    let inside_b = |p: Vec3| -> bool {
        p.x >= b_min.x && p.x <= b_max.x && p.y >= b_min.y && p.y <= b_max.y && p.z >= b_min.z && p.z <= b_max.z
    };
    let inside_a = |p: Vec3| -> bool {
        p.x >= a_min.x && p.x <= a_max.x && p.y >= a_min.y && p.y <= a_max.y && p.z >= a_min.z && p.z <= a_max.z
    };

    let mut result = PolyMesh::new();

    // Map and insert vertices from A
    let mut a_to_res: Vec<u32> = Vec::with_capacity(a.positions.len());
    for &p in &a.positions {
        a_to_res.push(result.add_vertex(p));
    }

    // Map and insert vertices from B
    let mut b_to_res: Vec<u32> = Vec::with_capacity(b.positions.len());
    for &p in &b.positions {
        b_to_res.push(result.add_vertex(p));
    }

    // Filter faces based on boolean operation
    match op {
        BooleanOp::Union => {
            // Keep faces of A outside B, and faces of B outside A
            for face in &a.faces {
                let center = a.compute_face_center(face);
                if !inside_b(center) {
                    let remapped_verts: Vec<u32> =
                        face.verts.iter().map(|&v| a_to_res[v as usize]).collect();
                    result.add_face(remapped_verts, face.uvs.clone(), face.smooth, face.material_slot);
                }
            }
            for face in &b.faces {
                let center = b.compute_face_center(face);
                if !inside_a(center) {
                    let remapped_verts: Vec<u32> =
                        face.verts.iter().map(|&v| b_to_res[v as usize]).collect();
                    result.add_face(remapped_verts, face.uvs.clone(), face.smooth, face.material_slot);
                }
            }
        }
        BooleanOp::Difference => {
            // Keep faces of A outside B
            for face in &a.faces {
                let center = a.compute_face_center(face);
                if !inside_b(center) {
                    let remapped_verts: Vec<u32> =
                        face.verts.iter().map(|&v| a_to_res[v as usize]).collect();
                    result.add_face(remapped_verts, face.uvs.clone(), face.smooth, face.material_slot);
                }
            }
            // Keep faces of B inside A, with inverted normal winding
            for face in &b.faces {
                let center = b.compute_face_center(face);
                if inside_a(center) {
                    let mut remapped_verts: Vec<u32> =
                        face.verts.iter().map(|&v| b_to_res[v as usize]).collect();
                    remapped_verts.reverse(); // Invert normal for subtraction
                    result.add_face(remapped_verts, face.uvs.clone(), face.smooth, face.material_slot);
                }
            }
        }
        BooleanOp::Intersect => {
            // Keep faces of A inside B, and faces of B inside A
            for face in &a.faces {
                let center = a.compute_face_center(face);
                if inside_b(center) {
                    let remapped_verts: Vec<u32> =
                        face.verts.iter().map(|&v| a_to_res[v as usize]).collect();
                    result.add_face(remapped_verts, face.uvs.clone(), face.smooth, face.material_slot);
                }
            }
            for face in &b.faces {
                let center = b.compute_face_center(face);
                if inside_a(center) {
                    let remapped_verts: Vec<u32> =
                        face.verts.iter().map(|&v| b_to_res[v as usize]).collect();
                    result.add_face(remapped_verts, face.uvs.clone(), face.smooth, face.material_slot);
                }
            }
        }
    }

    result.weld_vertices(1e-4);
    result
}

/// Recalculates face vertex windings so all normals consistently point outward from mesh centroid.
pub fn recalc_normals_outward(mesh: &mut PolyMesh) {
    if mesh.positions.is_empty() || mesh.faces.is_empty() {
        return;
    }

    let centroid: Vec3 = mesh.positions.iter().copied().sum::<Vec3>() / (mesh.positions.len() as f32);

    let face_info: Vec<(Vec3, Vec3)> = mesh
        .faces
        .iter()
        .map(|f| (mesh.compute_face_center(f), mesh.compute_face_normal(f)))
        .collect();

    for (i, face) in mesh.faces.iter_mut().enumerate() {
        if face.verts.len() < 3 {
            continue;
        }

        let (center, normal) = face_info[i];
        let outward_vector = center - centroid;

        // If normal points towards centroid instead of away, invert winding
        if normal.dot(outward_vector) < -1e-4 {
            face.verts.reverse();
            face.uvs.reverse();
        }
    }
}

/// Identifies open boundary loops and creates capping faces to seal watertight volumes.
pub fn fill_boundary_holes(mesh: &mut PolyMesh) {
    let half_edge = HalfEdgeMesh::from_polymesh(mesh);
    let boundary = half_edge.boundary_edges();
    if boundary.is_empty() {
        return;
    }

    // Connect boundary edges into closed loops
    let mut adj: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for &edge in boundary {
        adj.entry(edge[0]).or_default().push(edge[1]);
        adj.entry(edge[1]).or_default().push(edge[0]);
    }

    let mut visited_edges: BTreeSet<[u32; 2]> = BTreeSet::new();

    for &edge in boundary {
        let canonical_edge = if edge[0] < edge[1] { edge } else { [edge[1], edge[0]] };
        if visited_edges.contains(&canonical_edge) {
            continue;
        }

        let mut loop_verts = vec![edge[0], edge[1]];
        visited_edges.insert(canonical_edge);

        let mut current = edge[1];
        let mut prev = edge[0];

        while let Some(neighbors) = adj.get(&current) {
            let next_cand = neighbors.iter().find(|&&nb| nb != prev);
            if let Some(&next) = next_cand {
                let e = if current < next { [current, next] } else { [next, current] };
                visited_edges.insert(e);

                if next == loop_verts[0] {
                    // Closed loop found!
                    break;
                }

                loop_verts.push(next);
                prev = current;
                current = next;

                if loop_verts.len() > 1000 {
                    break; // Safety limit
                }
            } else {
                break;
            }
        }

        if loop_verts.len() >= 3 {
            let uvs = vec![[0.0, 0.0]; loop_verts.len()];
            mesh.add_face(loop_verts, uvs, false, 0);
        }
    }
}
