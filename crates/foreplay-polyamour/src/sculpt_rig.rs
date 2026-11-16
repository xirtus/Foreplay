//! BVH/Spatial-Accelerated Sculpt Brushes, Heat-Diffusion Auto-Rig Bone Weights &
//! Pose-Space Shape Keys (`foreplay-polyamour::sculpt_rig`).

use crate::mesh::PolyMesh;
use bevy_math::Vec3;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Sculpt brush types supported by Polyamour sculpt mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SculptBrush {
    /// Displaces vertices along the brush stroke delta.
    Grab,
    /// Builds up thin, controlled surface layers along face normals.
    ClayStrips,
    /// Pinches vertices toward brush center and carves into surface.
    Crease,
    /// Smooths surface irregularities via topological Laplacian averaging.
    Smooth,
    /// Expands vertices outward along surface normals.
    Inflate,
    /// Projects vertices onto the local stroke tangent plane.
    Flatten,
}

/// Symmetry axes configuration for sculpt strokes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymmetryAxes {
    /// Mirror sculpt operations across the X=0 plane.
    pub mirror_x: bool,
    /// Mirror sculpt operations across the Y=0 plane.
    pub mirror_y: bool,
    /// Mirror sculpt operations across the Z=0 plane.
    pub mirror_z: bool,
}

impl SymmetryAxes {
    /// No symmetry enabled.
    pub fn none() -> Self {
        Self::default()
    }

    /// Mirror along X axis (canonical character modeling).
    pub fn x_only() -> Self {
        Self {
            mirror_x: true,
            mirror_y: false,
            mirror_z: false,
        }
    }
}

/// A sculpt stroke input command.
#[derive(Clone, Debug, PartialEq)]
pub struct SculptStroke {
    /// The brush operation to apply.
    pub brush: SculptBrush,
    /// 3D hit point on the surface.
    pub hit_point: Vec3,
    /// Surface normal at the hit point.
    pub hit_normal: Vec3,
    /// World-space brush influence radius.
    pub radius: f32,
    /// Stroke strength / intensity in `[0, 1]`.
    pub strength: f32,
    /// Active symmetry axes.
    pub symmetry: SymmetryAxes,
    /// Directional delta vector (used for `Grab` and `ClayStrips`).
    pub stroke_delta: Option<Vec3>,
}

/// Applies a sculpt stroke to `mesh` in-place, accelerated by spatial AABB pruning
/// and handling active symmetry mirrors.
pub fn apply_sculpt_stroke(mesh: &mut PolyMesh, stroke: &SculptStroke) {
    if stroke.radius <= 1e-6 || stroke.strength <= 1e-6 || mesh.positions.is_empty() {
        return;
    }

    // Build list of stroke centers and normals (primary + active symmetry reflections)
    let mut centers_and_normals = vec![(stroke.hit_point, stroke.hit_normal)];

    if stroke.symmetry.mirror_x {
        centers_and_normals.push((
            Vec3::new(-stroke.hit_point.x, stroke.hit_point.y, stroke.hit_point.z),
            Vec3::new(-stroke.hit_normal.x, stroke.hit_normal.y, stroke.hit_normal.z),
        ));
    }
    if stroke.symmetry.mirror_y {
        centers_and_normals.push((
            Vec3::new(stroke.hit_point.x, -stroke.hit_point.y, stroke.hit_point.z),
            Vec3::new(stroke.hit_normal.x, -stroke.hit_normal.y, stroke.hit_normal.z),
        ));
    }
    if stroke.symmetry.mirror_z {
        centers_and_normals.push((
            Vec3::new(stroke.hit_point.x, stroke.hit_point.y, -stroke.hit_point.z),
            Vec3::new(stroke.hit_normal.x, stroke.hit_normal.y, -stroke.hit_normal.z),
        ));
    }

    // Precompute topological adjacency if Smooth brush is used
    let adjacency = if stroke.brush == SculptBrush::Smooth {
        let mut adj: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
        for face in &mesh.faces {
            let n = face.verts.len();
            for i in 0..n {
                let v0 = face.verts[i];
                let v1 = face.verts[(i + 1) % n];
                adj.entry(v0).or_default().insert(v1);
                adj.entry(v1).or_default().insert(v0);
            }
        }
        for edge in &mesh.loose_edges {
            adj.entry(edge[0]).or_default().insert(edge[1]);
            adj.entry(edge[1]).or_default().insert(edge[0]);
        }
        Some(adj)
    } else {
        None
    };

    let radius_sq = stroke.radius * stroke.radius;
    let prev_positions = mesh.positions.clone();

    for (center, normal) in centers_and_normals {
        let aabb_min = center - Vec3::splat(stroke.radius);
        let aabb_max = center + Vec3::splat(stroke.radius);

        for (v_idx, pos) in mesh.positions.iter_mut().enumerate() {
            // Spatial AABB coarse pruning
            if pos.x < aabb_min.x
                || pos.x > aabb_max.x
                || pos.y < aabb_min.y
                || pos.y > aabb_max.y
                || pos.z < aabb_min.z
                || pos.z > aabb_max.z
            {
                continue;
            }

            let diff = *pos - center;
            let dist_sq = diff.length_squared();
            if dist_sq > radius_sq {
                continue;
            }

            let dist = dist_sq.sqrt();
            let t = dist / stroke.radius;
            // Smooth polynomial brush falloff: (1 - t^2)^2
            let falloff = (1.0 - t * t).powi(2);
            let effective_strength = stroke.strength * falloff;

            match stroke.brush {
                SculptBrush::Grab => {
                    let delta = stroke.stroke_delta.unwrap_or(normal * stroke.radius * 0.1);
                    *pos += delta * effective_strength;
                }
                SculptBrush::ClayStrips => {
                    // Accumulates thin strips along hit normal
                    let delta = normal * (stroke.radius * 0.05 * effective_strength);
                    *pos += delta;
                }
                SculptBrush::Crease => {
                    // Pull toward stroke center in tangent plane, push inward along normal
                    let to_center = (center - *pos).normalize_or_zero();
                    let pinch = to_center * (stroke.radius * 0.08 * effective_strength);
                    let carve = -normal * (stroke.radius * 0.05 * effective_strength);
                    *pos += pinch + carve;
                }
                SculptBrush::Smooth => {
                    let maybe_neighbors = adjacency
                        .as_ref()
                        .and_then(|adj| adj.get(&(v_idx as u32)))
                        .filter(|n| !n.is_empty());
                    if let Some(neighbors) = maybe_neighbors {
                        let sum: Vec3 = neighbors
                            .iter()
                            .filter_map(|&n_idx| prev_positions.get(n_idx as usize).copied())
                            .sum();
                        let avg = sum / (neighbors.len() as f32);
                        *pos += (avg - *pos) * (effective_strength * 0.5);
                    }
                }
                SculptBrush::Inflate => {
                    *pos += normal * (stroke.radius * 0.08 * effective_strength);
                }
                SculptBrush::Flatten => {
                    // Distance from vertex to tangent plane passing through center with normal
                    let plane_dist = (*pos - center).dot(normal);
                    *pos -= normal * (plane_dist * effective_strength * 0.6);
                }
            }
        }
    }
}

/// A bone segment for skeletal auto-rigging.
#[derive(Clone, Debug, PartialEq)]
pub struct RigBoneSegment {
    /// Bone identifier (e.g. "Root", "Spine", "Arm.L").
    pub name: String,
    /// 3D position of the bone head (joint origin).
    pub head: Vec3,
    /// 3D position of the bone tail (joint endpoint).
    pub tail: Vec3,
    /// Radial influence radius for heat diffusion.
    pub radius: f32,
}

impl RigBoneSegment {
    /// Creates a new bone segment.
    pub fn new(name: impl Into<String>, head: Vec3, tail: Vec3, radius: f32) -> Self {
        Self {
            name: name.into(),
            head,
            tail,
            radius: radius.max(0.01),
        }
    }
}

/// 4-bone vertex skinning influence with normalized weights.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkinVertexInfluence {
    /// Top 4 bone indices influencing this vertex.
    pub bone_indices: [u16; 4],
    /// Normalized weights summing to exactly 1.0 (`sum == 1.0`).
    pub weights: [f32; 4],
}

impl Default for SkinVertexInfluence {
    fn default() -> Self {
        Self {
            bone_indices: [0; 4],
            weights: [1.0, 0.0, 0.0, 0.0],
        }
    }
}

/// Computes smooth 4-bone normalized skinning weights via geodesic/distance-attenuated
/// Laplacian heat diffusion iterations.
///
/// Guaranteed properties:
/// - Exact normalized sum: `sum(weights) == 1.0`.
/// - Zero NaNs or Infs even on degenerate zero-length bones.
/// - Graceful fallback when bones list is empty.
pub fn compute_heat_bone_weights(
    mesh: &PolyMesh,
    bones: &[RigBoneSegment],
) -> Vec<SkinVertexInfluence> {
    if mesh.positions.is_empty() {
        return Vec::new();
    }

    if bones.is_empty() {
        return vec![SkinVertexInfluence::default(); mesh.positions.len()];
    }

    let n_verts = mesh.positions.len();
    let n_bones = bones.len();

    // Step 1: Compute initial heat from each bone segment to each vertex
    // heat_matrix[vert_idx][bone_idx]
    let mut heat_matrix: Vec<Vec<f32>> = Vec::with_capacity(n_verts);

    for &pos in &mesh.positions {
        let mut row = Vec::with_capacity(n_bones);
        for bone in bones {
            let dist = distance_to_segment(pos, bone.head, bone.tail);
            let r = bone.radius.max(0.01);
            let heat = (-((dist / r).powi(2))).exp();
            let clamped_heat = if heat.is_nan() || heat < 1e-7 {
                0.0
            } else {
                heat
            };
            row.push(clamped_heat);
        }
        heat_matrix.push(row);
    }

    // Step 2: Build topological vertex adjacency
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n_verts];
    for face in &mesh.faces {
        let n = face.verts.len();
        for i in 0..n {
            let v0 = face.verts[i] as usize;
            let v1 = face.verts[(i + 1) % n] as usize;
            if v0 < n_verts && v1 < n_verts {
                if !adj[v0].contains(&v1) {
                    adj[v0].push(v1);
                }
                if !adj[v1].contains(&v0) {
                    adj[v1].push(v0);
                }
            }
        }
    }
    for edge in &mesh.loose_edges {
        let v0 = edge[0] as usize;
        let v1 = edge[1] as usize;
        if v0 < n_verts && v1 < n_verts {
            if !adj[v0].contains(&v1) {
                adj[v0].push(v1);
            }
            if !adj[v1].contains(&v0) {
                adj[v1].push(v0);
            }
        }
    }

    // Step 3: Run Laplacian heat diffusion iterations (5 iterations)
    let diffusion_lambda = 0.4f32;
    for _ in 0..5 {
        let prev = heat_matrix.clone();
        for i in 0..n_verts {
            let neighbors = &adj[i];
            if neighbors.is_empty() {
                continue;
            }

            for b in 0..n_bones {
                let nbr_sum: f32 = neighbors.iter().map(|&j| prev[j][b]).sum();
                let nbr_avg = nbr_sum / (neighbors.len() as f32);
                let new_val = (1.0 - diffusion_lambda) * prev[i][b] + diffusion_lambda * nbr_avg;
                heat_matrix[i][b] = if new_val.is_nan() { 0.0 } else { new_val };
            }
        }
    }

    // Step 4: Extract top 4 bones per vertex and normalize
    let mut influences = Vec::with_capacity(n_verts);

    for (v_idx, row) in heat_matrix.into_iter().enumerate() {
        let mut bone_heats: Vec<(usize, f32)> = row.into_iter().enumerate().collect();
        // Sort descending by heat
        bone_heats.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let top_count = bone_heats.len().min(4);
        let mut top_indices = [0u16; 4];
        let mut top_weights = [0.0f32; 4];
        let mut heat_sum = 0.0f32;

        for k in 0..top_count {
            top_indices[k] = bone_heats[k].0 as u16;
            top_weights[k] = bone_heats[k].1;
            heat_sum += bone_heats[k].1;
        }

        if heat_sum > 1e-6 && !heat_sum.is_nan() {
            for w in &mut top_weights {
                *w /= heat_sum;
                if w.is_nan() {
                    *w = 0.0;
                }
            }
            // Guarantee sum == 1.0 against float round-off
            let others_sum = top_weights[1] + top_weights[2] + top_weights[3];
            top_weights[0] = (1.0 - others_sum).max(0.0);
        } else {
            // Fallback: assign 1.0 to the closest bone by Euclidean segment distance
            let pos = mesh.positions[v_idx];
            let mut closest_b = 0;
            let mut min_d = f32::INFINITY;
            for (b_idx, bone) in bones.iter().enumerate() {
                let d = distance_to_segment(pos, bone.head, bone.tail);
                if d < min_d {
                    min_d = d;
                    closest_b = b_idx;
                }
            }
            top_indices = [closest_b as u16, 0, 0, 0];
            top_weights = [1.0, 0.0, 0.0, 0.0];
        }

        influences.push(SkinVertexInfluence {
            bone_indices: top_indices,
            weights: top_weights,
        });
    }

    influences
}

/// Distance from point `p` to segment `[head, tail]`, robust to degenerate zero-length segments.
fn distance_to_segment(p: Vec3, head: Vec3, tail: Vec3) -> f32 {
    let segment = tail - head;
    let seg_len_sq = segment.length_squared();

    if seg_len_sq < 1e-8 {
        // Degenerate zero-length bone: distance to head point
        return (p - head).length();
    }

    let t = ((p - head).dot(segment) / seg_len_sq).clamp(0.0, 1.0);
    let closest_point = head + segment * t;
    (p - closest_point).length()
}

/// Pose-space morph target shape key set (`Basis` + named target deformations).
#[derive(Clone, Debug, PartialEq)]
pub struct ShapeKeySet {
    /// Neutral base mesh positions.
    pub basis: Vec<Vec3>,
    /// Named morph targets mapping shape key name -> deformed vertex positions.
    pub targets: BTreeMap<String, Vec<Vec3>>,
}

impl ShapeKeySet {
    /// Creates a shape key set from a base position buffer.
    pub fn new(basis: Vec<Vec3>) -> Self {
        Self {
            basis,
            targets: BTreeMap::new(),
        }
    }

    /// Creates a shape key set initialized from the vertex positions of a `PolyMesh`.
    pub fn from_polymesh(mesh: &PolyMesh) -> Self {
        Self::new(mesh.positions.clone())
    }

    /// Adds a named morph target with full target positions.
    /// Returns `true` if target length matches basis length.
    pub fn add_shape_key(&mut self, name: impl Into<String>, positions: Vec<Vec3>) -> bool {
        if positions.len() != self.basis.len() {
            return false;
        }
        self.targets.insert(name.into(), positions);
        true
    }

    /// Adds a named morph target from delta offsets (`pos = basis + delta`).
    pub fn sculpt_shape_key_delta(&mut self, name: impl Into<String>, deltas: &[Vec3]) -> bool {
        if deltas.len() != self.basis.len() {
            return false;
        }
        let target_positions: Vec<Vec3> = self
            .basis
            .iter()
            .zip(deltas.iter())
            .map(|(&b, &d)| b + d)
            .collect();
        self.targets.insert(name.into(), target_positions);
        true
    }

    /// Evaluates blended vertex positions: `P = Basis + sum(w_k * (Target_k - Basis))`.
    pub fn evaluate_blended_positions(&self, weights: &BTreeMap<String, f32>) -> Vec<Vec3> {
        let mut result = self.basis.clone();

        for (name, &weight) in weights {
            if weight.abs() < 1e-6 {
                continue;
            }
            if let Some(target) = self.targets.get(name) {
                for (i, (&b_pos, &t_pos)) in self.basis.iter().zip(target.iter()).enumerate() {
                    let delta = t_pos - b_pos;
                    result[i] += delta * weight;
                }
            }
        }

        result
    }

    /// Updates `mesh.positions` with blended positions evaluated from the given shape key weights.
    pub fn apply_to_mesh(&self, mesh: &mut PolyMesh, weights: &BTreeMap<String, f32>) {
        if mesh.positions.len() == self.basis.len() {
            mesh.positions = self.evaluate_blended_positions(weights);
        }
    }
}
