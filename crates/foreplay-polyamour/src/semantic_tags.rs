//! Game-Semantic Edge & Face Compiler (`foreplay-polyamour::semantic_tags`).
//!
//! Provides:
//! - `PolyamourSemanticAttributes`: edge tags (`grind_edge`, `sharp_crease`, `uv_seam`) and face tags
//!   (`collision_convex`, `collision_trimesh`, `collision_none`, `walkable_nav`).
//! - `compile_polyamour_game_bundle`: compiles a mesh into continuous skate grind polylines,
//!   physics colliders (convex hull & trimesh), walkable navigation meshes, and non-overlapping
//!   smart-projected UV2 lightmap atlas coordinates.

use crate::mesh::PolyMesh;
use crate::selection_raycast::canonical_edge;
use bevy_math::Vec3;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Standard semantic edge tags for game design in Foreplay / sex-3d.
pub const TAG_GRIND_EDGE: &str = "grind_edge";
pub const TAG_SHARP_CREASE: &str = "sharp_crease";
pub const TAG_UV_SEAM: &str = "uv_seam";

/// Standard semantic face tags for collision and navigation.
pub const TAG_COLLISION_CONVEX: &str = "collision_convex";
pub const TAG_COLLISION_TRIMESH: &str = "collision_trimesh";
pub const TAG_COLLISION_NONE: &str = "collision_none";
pub const TAG_WALKABLE_NAV: &str = "walkable_nav";

/// Semantic edge and face annotations attached to a `PolyMesh`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolyamourSemanticAttributes {
    /// Canonical edge `[min, max]` -> set of tag strings.
    pub edge_tags: BTreeMap<[u32; 2], BTreeSet<String>>,
    /// Face index -> set of tag strings.
    pub face_tags: BTreeMap<u32, BTreeSet<String>>,
}

impl PolyamourSemanticAttributes {
    /// Creates an empty set of semantic attributes.
    pub fn new() -> Self {
        Self::default()
    }

    /// Tags an edge with a semantic label.
    pub fn tag_edge(&mut self, edge: [u32; 2], tag: impl Into<String>) {
        let canon = canonical_edge(edge[0], edge[1]);
        self.edge_tags.entry(canon).or_default().insert(tag.into());
    }

    /// Removes a tag from an edge.
    pub fn untag_edge(&mut self, edge: [u32; 2], tag: &str) {
        let canon = canonical_edge(edge[0], edge[1]);
        if let Some(tags) = self.edge_tags.get_mut(&canon) {
            tags.remove(tag);
            if tags.is_empty() {
                self.edge_tags.remove(&canon);
            }
        }
    }

    /// Checks if an edge has a specific tag.
    pub fn has_edge_tag(&self, edge: [u32; 2], tag: &str) -> bool {
        let canon = canonical_edge(edge[0], edge[1]);
        self.edge_tags
            .get(&canon)
            .map(|tags| tags.contains(tag))
            .unwrap_or(false)
    }

    /// Tags a face with a semantic label.
    pub fn tag_face(&mut self, face_idx: u32, tag: impl Into<String>) {
        self.face_tags
            .entry(face_idx)
            .or_default()
            .insert(tag.into());
    }

    /// Removes a tag from a face.
    pub fn untag_face(&mut self, face_idx: u32, tag: &str) {
        if let Some(tags) = self.face_tags.get_mut(&face_idx) {
            tags.remove(tag);
            if tags.is_empty() {
                self.face_tags.remove(&face_idx);
            }
        }
    }

    /// Checks if a face has a specific tag.
    pub fn has_face_tag(&self, face_idx: u32, tag: &str) -> bool {
        self.face_tags
            .get(&face_idx)
            .map(|tags| tags.contains(tag))
            .unwrap_or(false)
    }

    /// Returns all canonical edges that have a given tag.
    pub fn get_edges_with_tag(&self, tag: &str) -> Vec<[u32; 2]> {
        self.edge_tags
            .iter()
            .filter(|(_, tags)| tags.contains(tag))
            .map(|(&e, _)| e)
            .collect()
    }

    /// Returns all face indices that have a given tag.
    pub fn get_faces_with_tag(&self, tag: &str) -> Vec<u32> {
        self.face_tags
            .iter()
            .filter(|(_, tags)| tags.contains(tag))
            .map(|(&f, _)| f)
            .collect()
    }

    /// Clears all semantic tags.
    pub fn clear(&mut self) {
        self.edge_tags.clear();
        self.face_tags.clear();
    }
}

/// Metadata describing a UV2 lightmap atlas island.
#[derive(Clone, Debug, PartialEq)]
pub struct UvIsland {
    pub island_index: usize,
    pub face_index: u32,
    pub min_uv: [f32; 2],
    pub max_uv: [f32; 2],
}

/// Compiled game assets bundle ready for game engines (Foreplay, Three.js, Rapier/PhysX).
#[derive(Clone, Debug, PartialEq)]
pub struct CompiledPolyamourGameBundle {
    /// Ordered continuous 3D polylines chained from `"grind_edge"` tags (closed-loop aware).
    pub grind_rail_splines: Vec<Vec<Vec3>>,
    /// Point cloud extracted from `"collision_convex"` tagged faces.
    pub convex_hull_points: Vec<Vec3>,
    /// Triangulated trimesh collider `(vertices, triangle_indices)` for `"collision_trimesh"` faces.
    pub trimesh_collider: (Vec<Vec3>, Vec<[u32; 3]>),
    /// Walkable navigation face indices (`"walkable_nav"`).
    pub walkable_nav_faces: Vec<u32>,
    /// Flattened non-overlapping Smart UV2 lightmap coordinates for every vertex in every face.
    pub uv2_coords: Vec<[f32; 2]>,
    /// Per-face structured UV2 coordinates: `uv2_by_face[face_idx][vertex_in_face]`.
    pub uv2_by_face: Vec<Vec<[f32; 2]>>,
    /// UV2 lightmap island bounds in atlas `[0, 1]^2`.
    pub uv2_islands: Vec<UvIsland>,
}

/// Compiles a `PolyMesh` and its `PolyamourSemanticAttributes` into game runtime bundles.
pub fn compile_polyamour_game_bundle(
    mesh: &PolyMesh,
    attrs: &PolyamourSemanticAttributes,
) -> CompiledPolyamourGameBundle {
    // 1. Chain grind edge segments into continuous 3D polylines
    let grind_rail_splines = extract_grind_rail_splines(mesh, attrs);

    // 2. Partition physics collision geometry
    let convex_hull_points = extract_convex_hull_points(mesh, attrs);
    let trimesh_collider = extract_trimesh_collider(mesh, attrs);

    // 3. Extract walkable navigation faces
    let walkable_nav_faces = extract_walkable_nav_faces(mesh, attrs);

    // 4. Generate Smart UV2 Lightmap atlas
    let (uv2_coords, uv2_by_face, uv2_islands) = generate_smart_uv2_atlas(mesh);

    CompiledPolyamourGameBundle {
        grind_rail_splines,
        convex_hull_points,
        trimesh_collider,
        walkable_nav_faces,
        uv2_coords,
        uv2_by_face,
        uv2_islands,
    }
}

/// Chaining algorithm: collects all `"grind_edge"` segments and chains them into continuous 3D polylines.
fn extract_grind_rail_splines(
    mesh: &PolyMesh,
    attrs: &PolyamourSemanticAttributes,
) -> Vec<Vec<Vec3>> {
    let grind_edges = attrs.get_edges_with_tag(TAG_GRIND_EDGE);
    if grind_edges.is_empty() {
        return Vec::new();
    }

    // Build adjacency graph
    let mut adj: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
    for edge in &grind_edges {
        adj.entry(edge[0]).or_default().insert(edge[1]);
        adj.entry(edge[1]).or_default().insert(edge[0]);
    }

    let mut remaining_edges: BTreeSet<[u32; 2]> = grind_edges.into_iter().collect();
    let mut polylines = Vec::new();

    // Pass 1: Trace all open chains starting from vertices with degree == 1
    let degree_one_verts: Vec<u32> = adj
        .iter()
        .filter(|(_, neighbors)| neighbors.len() == 1)
        .map(|(&v, _)| v)
        .collect();

    for start_vert in degree_one_verts {
        if !adj.contains_key(&start_vert) {
            continue;
        }

        let mut chain = vec![start_vert];
        let mut curr = start_vert;

        while let Some(&next) = adj.get(&curr).and_then(|nbrs| nbrs.iter().next()) {
            let edge = canonical_edge(curr, next);
            if remaining_edges.remove(&edge) {
                // Remove edge from graph
                adj.entry(curr).or_default().remove(&next);
                adj.entry(next).or_default().remove(&curr);

                chain.push(next);
                curr = next;

                // Stop if this vertex had other branches or is now dead end
                if adj.get(&curr).map(|n| n.is_empty()).unwrap_or(true) {
                    break;
                }
            } else {
                break;
            }
        }

        if chain.len() >= 2 {
            let pts = chain
                .iter()
                .filter_map(|&vi| mesh.positions.get(vi as usize).copied())
                .collect::<Vec<_>>();
            if pts.len() >= 2 {
                polylines.push(pts);
            }
        }
    }

    // Pass 2: Trace remaining closed loops (all vertices degree 2)
    while let Some(&edge) = remaining_edges.iter().next() {
        remaining_edges.remove(&edge);
        let start_v = edge[0];
        let mut curr = edge[1];
        let mut chain = vec![start_v, curr];

        adj.entry(edge[0]).or_default().remove(&edge[1]);
        adj.entry(edge[1]).or_default().remove(&edge[0]);

        while curr != start_v {
            let next_opt = adj.get(&curr).and_then(|nbrs| nbrs.iter().copied().next());
            match next_opt {
                Some(next) => {
                    let e = canonical_edge(curr, next);
                    remaining_edges.remove(&e);
                    adj.entry(curr).or_default().remove(&next);
                    adj.entry(next).or_default().remove(&curr);
                    chain.push(next);
                    curr = next;
                }
                None => break,
            }
        }

        if chain.len() >= 3 {
            let pts = chain
                .iter()
                .filter_map(|&vi| mesh.positions.get(vi as usize).copied())
                .collect::<Vec<_>>();
            if pts.len() >= 3 {
                polylines.push(pts);
            }
        }
    }

    polylines
}

/// Extracts vertices from faces tagged with `"collision_convex"`.
fn extract_convex_hull_points(
    mesh: &PolyMesh,
    attrs: &PolyamourSemanticAttributes,
) -> Vec<Vec3> {
    let convex_faces = attrs.get_faces_with_tag(TAG_COLLISION_CONVEX);
    if convex_faces.is_empty() {
        return Vec::new();
    }

    let mut points = Vec::new();
    let mut seen = BTreeSet::new();

    for f_idx in convex_faces {
        if let Some(face) = mesh.faces.get(f_idx as usize) {
            for &v in &face.verts {
                if let Some(&pos) = mesh.positions.get(v as usize) {
                    let key = [
                        (pos.x * 1000.0).round() as i32,
                        (pos.y * 1000.0).round() as i32,
                        (pos.z * 1000.0).round() as i32,
                    ];
                    if seen.insert(key) {
                        points.push(pos);
                    }
                }
            }
        }
    }

    points
}

/// Triangulates faces tagged with `"collision_trimesh"` into a collider vertex & index buffer.
fn extract_trimesh_collider(
    mesh: &PolyMesh,
    attrs: &PolyamourSemanticAttributes,
) -> (Vec<Vec3>, Vec<[u32; 3]>) {
    let mut target_faces: Vec<usize> = Vec::new();

    let has_explicit_trimesh = !attrs.get_faces_with_tag(TAG_COLLISION_TRIMESH).is_empty();
    for (f_idx, _) in mesh.faces.iter().enumerate() {
        let f_u32 = f_idx as u32;
        if attrs.has_face_tag(f_u32, TAG_COLLISION_NONE) {
            continue;
        }

        if has_explicit_trimesh {
            if attrs.has_face_tag(f_u32, TAG_COLLISION_TRIMESH) {
                target_faces.push(f_idx);
            }
        } else if !attrs.has_face_tag(f_u32, TAG_COLLISION_CONVEX) {
            target_faces.push(f_idx);
        }
    }

    let mut collider_positions = Vec::new();
    let mut collider_indices = Vec::new();

    for &f_idx in &target_faces {
        let face = &mesh.faces[f_idx];
        let n = face.verts.len();
        if n < 3 {
            continue;
        }

        // Fan triangulate face into triangle triplets
        for i in 1..(n - 1) {
            let v0 = face.verts[0];
            let v1 = face.verts[i];
            let v2 = face.verts[i + 1];

            let p0 = mesh.positions.get(v0 as usize).copied().unwrap_or(Vec3::ZERO);
            let p1 = mesh.positions.get(v1 as usize).copied().unwrap_or(Vec3::ZERO);
            let p2 = mesh.positions.get(v2 as usize).copied().unwrap_or(Vec3::ZERO);

            let base = collider_positions.len() as u32;
            collider_positions.push(p0);
            collider_positions.push(p1);
            collider_positions.push(p2);
            collider_indices.push([base, base + 1, base + 2]);
        }
    }

    (collider_positions, collider_indices)
}

/// Identifies walkable navigation faces based on `"walkable_nav"` tags or upward normal.
fn extract_walkable_nav_faces(
    mesh: &PolyMesh,
    attrs: &PolyamourSemanticAttributes,
) -> Vec<u32> {
    let mut walkable = BTreeSet::new();

    for &f_idx in &attrs.get_faces_with_tag(TAG_WALKABLE_NAV) {
        if (f_idx as usize) < mesh.faces.len() {
            walkable.insert(f_idx);
        }
    }

    // Also include any non-collision-none faces with normal.y >= 0.707 (steepness < 45 degrees)
    for (f_idx, face) in mesh.faces.iter().enumerate() {
        let f_u32 = f_idx as u32;
        if attrs.has_face_tag(f_u32, TAG_COLLISION_NONE) {
            continue;
        }
        let normal = mesh.compute_face_normal(face);
        if normal.y >= 0.707 {
            walkable.insert(f_u32);
        }
    }

    walkable.into_iter().collect()
}

/// Tuple containing flattened UV2 coords, per-face UV2 coords, and island metadata.
pub type SmartUv2Atlas = (Vec<[f32; 2]>, Vec<Vec<[f32; 2]>>, Vec<UvIsland>);

/// Smart-Projects face geometry into local 2D planes, and packs all islands into a non-overlapping
/// lightmap atlas shelf layout in `[0, 1]^2` with texel padding.
fn generate_smart_uv2_atlas(mesh: &PolyMesh) -> SmartUv2Atlas {
    if mesh.faces.is_empty() {
        return (Vec::new(), Vec::new(), Vec::new());
    }

    // Step 1: Project each face into its local 2D tangent plane
    struct LocalFaceIsland {
        face_index: usize,
        local_uvs: Vec<[f32; 2]>,
        width: f32,
        height: f32,
    }

    let mut islands = Vec::with_capacity(mesh.faces.len());

    for (f_idx, face) in mesh.faces.iter().enumerate() {
        let n_verts = face.verts.len();
        if n_verts < 3 {
            islands.push(LocalFaceIsland {
                face_index: f_idx,
                local_uvs: vec![[0.0, 0.0]; n_verts],
                width: 0.01,
                height: 0.01,
            });
            continue;
        }

        let normal = mesh.compute_face_normal(face);
        let center = mesh.compute_face_center(face);

        let tangent = if normal.y.abs() > 0.99 {
            Vec3::X
        } else {
            Vec3::Y.cross(normal).normalize_or_zero()
        };
        let bitangent = normal.cross(tangent).normalize_or_zero();

        let mut pts_2d = Vec::with_capacity(n_verts);
        let mut min_u = f32::INFINITY;
        let mut min_v = f32::INFINITY;
        let mut max_u = f32::NEG_INFINITY;
        let mut max_v = f32::NEG_INFINITY;

        for &v in &face.verts {
            let pos = mesh.positions.get(v as usize).copied().unwrap_or(Vec3::ZERO);
            let diff = pos - center;
            let u = diff.dot(tangent);
            let v_coord = diff.dot(bitangent);

            min_u = min_u.min(u);
            max_u = max_u.max(u);
            min_v = min_v.min(v_coord);
            max_v = max_v.max(v_coord);

            pts_2d.push([u, v_coord]);
        }

        let width = (max_u - min_u).max(0.001);
        let height = (max_v - min_v).max(0.001);

        // Normalize local island coordinates to start at (0, 0)
        let normalized_pts: Vec<[f32; 2]> = pts_2d
            .into_iter()
            .map(|[u, v_coord]| [u - min_u, v_coord - min_v])
            .collect();

        islands.push(LocalFaceIsland {
            face_index: f_idx,
            local_uvs: normalized_pts,
            width,
            height,
        });
    }

    // Step 2: Shelf Bin-Packing into atlas
    // Compute shelf layout with padding
    let padding = 0.05; // 5% island padding
    let total_area: f32 = islands.iter().map(|isl| (isl.width + padding) * (isl.height + padding)).sum();
    let target_row_width = (total_area.sqrt() * 1.2).max(0.5);

    struct PlacedIsland {
        face_index: usize,
        local_uvs: Vec<[f32; 2]>,
        rect_x: f32,
        rect_y: f32,
        width: f32,
        height: f32,
    }

    let mut placed = Vec::with_capacity(islands.len());
    let mut current_x = padding;
    let mut current_y = padding;
    let mut current_shelf_height = 0.0f32;

    for island in islands {
        let island_w = island.width + padding;
        let island_h = island.height + padding;

        if current_x + island_w > target_row_width && current_x > padding {
            // New shelf
            current_x = padding;
            current_y += current_shelf_height;
            current_shelf_height = 0.0;
        }

        placed.push(PlacedIsland {
            face_index: island.face_index,
            local_uvs: island.local_uvs,
            rect_x: current_x,
            rect_y: current_y,
            width: island.width,
            height: island.height,
        });

        current_x += island_w;
        current_shelf_height = current_shelf_height.max(island_h);
    }

    let total_atlas_w = (target_row_width + padding).max(current_x);
    let total_atlas_h = current_y + current_shelf_height + padding;

    // Step 3: Normalize all coordinates to [0, 1]^2
    let mut uv2_by_face = vec![Vec::new(); mesh.faces.len()];
    let mut uv2_coords = Vec::new();
    let mut uv2_islands = Vec::new();

    for (island_idx, item) in placed.into_iter().enumerate() {
        let mut face_uvs = Vec::with_capacity(item.local_uvs.len());

        let min_u = item.rect_x / total_atlas_w;
        let min_v = item.rect_y / total_atlas_h;
        let max_u = (item.rect_x + item.width) / total_atlas_w;
        let max_v = (item.rect_y + item.height) / total_atlas_h;

        for local in item.local_uvs {
            let u_norm = (item.rect_x + local[0]) / total_atlas_w;
            let v_norm = (item.rect_y + local[1]) / total_atlas_h;
            let uv = [u_norm.clamp(0.0, 1.0), v_norm.clamp(0.0, 1.0)];
            face_uvs.push(uv);
            uv2_coords.push(uv);
        }

        uv2_islands.push(UvIsland {
            island_index: island_idx,
            face_index: item.face_index as u32,
            min_uv: [min_u, min_v],
            max_uv: [max_u, max_v],
        });

        uv2_by_face[item.face_index] = face_uvs;
    }

    (uv2_coords, uv2_by_face, uv2_islands)
}
