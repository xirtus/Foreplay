//! "Micro-Nanite" GPU-Style Meshlet DAG Clusterer.
//!
//! Partitions arbitrary triangle meshes into clusters of at most 64 unique vertices
//! and 124 triangles, computing bounding spheres and normal cones for hierarchical
//! frustum and backface cluster culling.

use bevy_math::Vec3;
use crate::pipeline::Frustum;

/// Maximum unique vertices per meshlet (DirectX 12 / Vulkan Mesh Shader hardware constraint).
pub const MAX_MESHLET_VERTICES: usize = 64;

/// Maximum triangles per meshlet.
pub const MAX_MESHLET_TRIANGLES: usize = 124;

/// Micro-Nanite GPU cluster representing a localized patch of triangles.
#[derive(Clone, PartialEq, Debug)]
pub struct Meshlet {
    /// Indices into the original vertex buffer (length <= 64).
    pub vertices: Vec<u32>,
    /// Local triangle index triplets indexing into `vertices` (length <= 124).
    pub triangles: Vec<[u8; 3]>,
    /// Bounding sphere center in local object space.
    pub center: Vec3,
    /// Bounding sphere radius.
    pub radius: f32,
    /// Normal cone axis (average surface normal of the cluster).
    pub axis: Vec3,
    /// Normal cone cutoff: cosine of the half-angle containing all cluster face normals.
    pub cutoff: f32,
}

impl Meshlet {
    /// Create an empty meshlet.
    #[must_use]
    pub fn new() -> Self {
        Self {
            vertices: Vec::with_capacity(MAX_MESHLET_VERTICES),
            triangles: Vec::with_capacity(MAX_MESHLET_TRIANGLES),
            center: Vec3::ZERO,
            radius: 0.0,
            axis: Vec3::Y,
            cutoff: 1.0,
        }
    }
}

impl Default for Meshlet {
    fn default() -> Self {
        Self::new()
    }
}

/// Partition indexed triangle geometry into hardware-ready meshlet clusters.
///
/// Guaranteed invariants:
/// - `meshlet.vertices.len() <= 64`
/// - `meshlet.triangles.len() <= 124`
/// - Every input triangle is preserved.
#[must_use]
pub fn build_meshlets(positions: &[[f32; 3]], indices: &[u32]) -> Vec<Meshlet> {
    if positions.is_empty() || indices.len() < 3 {
        return Vec::new();
    }

    let mut meshlets = Vec::new();
    let mut current = Meshlet::new();

    let tri_count = indices.len() / 3;
    for t in 0..tri_count {
        let i0 = indices[t * 3];
        let i1 = indices[t * 3 + 1];
        let i2 = indices[t * 3 + 2];

        // Check how many new vertices this triangle would introduce
        let has0 = current.vertices.contains(&i0);
        let has1 = current.vertices.contains(&i1);
        let has2 = current.vertices.contains(&i2);

        let new_verts = (!has0 as usize) + (!has1 as usize) + (!has2 as usize);

        // If adding this triangle exceeds max vertices or max triangles, finalize current meshlet
        if (current.vertices.len() + new_verts > MAX_MESHLET_VERTICES
            || current.triangles.len() >= MAX_MESHLET_TRIANGLES)
            && !current.triangles.is_empty()
        {
            finalize_meshlet_bounds(&mut current, positions);
            meshlets.push(current);
            current = Meshlet::new();
        }

        // Add vertices and get local indices
        let l0 = add_or_get_vertex(&mut current.vertices, i0);
        let l1 = add_or_get_vertex(&mut current.vertices, i1);
        let l2 = add_or_get_vertex(&mut current.vertices, i2);

        current.triangles.push([l0, l1, l2]);
    }

    if !current.triangles.is_empty() {
        finalize_meshlet_bounds(&mut current, positions);
        meshlets.push(current);
    }

    meshlets
}

fn add_or_get_vertex(verts: &mut Vec<u32>, v: u32) -> u8 {
    if let Some(pos) = verts.iter().position(|&x| x == v) {
        pos as u8
    } else {
        let idx = verts.len() as u8;
        verts.push(v);
        idx
    }
}

/// Compute bounding sphere and normal cone for a meshlet cluster.
fn finalize_meshlet_bounds(meshlet: &mut Meshlet, positions: &[[f32; 3]]) {
    if meshlet.vertices.is_empty() {
        return;
    }

    // 1. Bounding sphere: center = centroid of vertices
    let mut center = Vec3::ZERO;
    for &vi in &meshlet.vertices {
        let p = positions[vi as usize];
        center += Vec3::new(p[0], p[1], p[2]);
    }
    center /= meshlet.vertices.len() as f32;

    let mut radius_sq = 0.0_f32;
    for &vi in &meshlet.vertices {
        let p = positions[vi as usize];
        let v = Vec3::new(p[0], p[1], p[2]);
        radius_sq = radius_sq.max(center.distance_squared(v));
    }

    meshlet.center = center;
    meshlet.radius = radius_sq.sqrt();

    // 2. Normal cone: average triangle normals
    let mut avg_normal = Vec3::ZERO;
    let mut face_normals = Vec::with_capacity(meshlet.triangles.len());

    for tri in &meshlet.triangles {
        let vi0 = meshlet.vertices[tri[0] as usize] as usize;
        let vi1 = meshlet.vertices[tri[1] as usize] as usize;
        let vi2 = meshlet.vertices[tri[2] as usize] as usize;

        let p0 = Vec3::from_array(positions[vi0]);
        let p1 = Vec3::from_array(positions[vi1]);
        let p2 = Vec3::from_array(positions[vi2]);

        let e1 = p1 - p0;
        let e2 = p2 - p0;
        let fnorm = e1.cross(e2).normalize_or_zero();
        if fnorm.length_squared() > 1e-6 {
            avg_normal += fnorm;
            face_normals.push(fnorm);
        }
    }

    let avg_len = avg_normal.length();
    if avg_len > 1e-6 {
        meshlet.axis = avg_normal / avg_len;
        let mut min_dot = 1.0_f32;
        for fnorm in &face_normals {
            min_dot = min_dot.min(meshlet.axis.dot(*fnorm));
        }
        meshlet.cutoff = min_dot.clamp(-1.0, 1.0);
    } else {
        meshlet.axis = Vec3::Y;
        meshlet.cutoff = -1.0; // omnidirectional
    }
}

/// Cull meshlets using frustum sphere tests and backface normal cone rejection.
///
/// Returns indices of visible meshlets.
#[must_use]
pub fn cull_meshlets(meshlets: &[Meshlet], frustum: &Frustum, camera_pos: Vec3) -> Vec<usize> {
    let mut visible = Vec::with_capacity(meshlets.len());

    for (i, m) in meshlets.iter().enumerate() {
        // 1. Frustum sphere test: d(plane, center) >= -radius
        let mut inside_frustum = true;
        for plane in &frustum.planes {
            if plane.distance_to(m.center) < -m.radius {
                inside_frustum = false;
                break;
            }
        }
        if !inside_frustum {
            continue;
        }

        // 2. Backface cluster culling using normal cone
        // If all face normals in the cluster point away from the camera, cull the entire cluster!
        if m.cutoff > 0.0 {
            let to_cam = camera_pos - m.center;
            let dist = to_cam.length();
            if dist > m.radius {
                let to_cam_dir = to_cam / dist;
                // If dot product with cone axis is less than -sin(angle) * (r/d) - cutoff:
                // Conservatively: if to_cam_dir.dot(axis) + (m.radius / dist) < -sqrt(1 - cutoff^2):
                let sin_cutoff = (1.0 - m.cutoff * m.cutoff).max(0.0).sqrt();
                let backface_threshold = -sin_cutoff * (m.radius / dist) - 0.01;
                if to_cam_dir.dot(m.axis) < backface_threshold && to_cam_dir.dot(m.axis) < 0.0 {
                    // Cluster is backfacing camera
                    continue;
                }
            }
        }

        visible.push(i);
    }

    visible
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meshlet_invariants_hold() {
        // Build a grid of 200 triangles (exceeding single meshlet limits)
        let mut positions = Vec::new();
        let mut indices = Vec::new();

        let grid_size = 12;
        for y in 0..grid_size {
            for x in 0..grid_size {
                positions.push([x as f32, 0.0, y as f32]);
            }
        }

        for y in 0..(grid_size - 1) {
            for x in 0..(grid_size - 1) {
                let top_left = (y * grid_size + x) as u32;
                let top_right = top_left + 1;
                let bot_left = ((y + 1) * grid_size + x) as u32;
                let bot_right = bot_left + 1;

                indices.push(top_left);
                indices.push(bot_left);
                indices.push(top_right);

                indices.push(top_right);
                indices.push(bot_left);
                indices.push(bot_right);
            }
        }

        let meshlets = build_meshlets(&positions, &indices);
        assert!(!meshlets.is_empty());

        let mut total_triangles = 0;
        for m in &meshlets {
            assert!(m.vertices.len() <= MAX_MESHLET_VERTICES);
            assert!(m.triangles.len() <= MAX_MESHLET_TRIANGLES);
            assert!(m.radius >= 0.0);
            total_triangles += m.triangles.len();
        }

        assert_eq!(total_triangles, indices.len() / 3);
    }

    #[test]
    fn meshlet_culling_frustum_and_cone() {
        let positions = vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
        ];
        let indices = vec![0, 1, 2];
        let meshlets = build_meshlets(&positions, &indices);
        assert_eq!(meshlets.len(), 1);

        // Frustum centered at (0, 0, 5) looking towards -Z
        let mut cam = crate::PerspectiveCamera3D::new(Vec3::new(0.0, 0.0, 5.0), 60.0, 1.0);
        cam.look_at(Vec3::ZERO, Vec3::Y);
        let frustum = Frustum::from_view_projection(&cam.view_projection_matrix());

        // Camera in front of triangle (facing +Z)
        let visible = cull_meshlets(&meshlets, &frustum, cam.position);
        assert_eq!(visible.len(), 1);

        // Camera behind triangle (facing away)
        let behind_cam_pos = Vec3::new(0.0, 0.0, -5.0);
        let behind_visible = cull_meshlets(&meshlets, &frustum, behind_cam_pos);
        assert_eq!(behind_visible.len(), 0, "Backfacing meshlet should be culled by normal cone");
    }
}
