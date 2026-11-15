//! Godot 4 TileMap Terrain Autotiler and Polygon2D Multi-Bone Skeletal Skinner.
//!
//! Implements Godot 4 TileSet terrains with 3 peering modes:
//! - `MatchCornersAndSides` (3x3 minimal 8-bit peering bitmasks)
//! - `MatchCorners` (diagonal corners)
//! - `MatchSides` (orthogonal sides)
//!
//! Deterministic neighbor tile connection and path painting (`paint_terrain_connect`, `paint_terrain_path`).
//! Also provides `Polygon2DSkinner` 2D Linear Blend Skinning (LBS) for skeletal deformation.

use std::collections::{BTreeMap, BTreeSet};

// ============================================================================
// TileMap Terrain Peering & Catalog
// ============================================================================

/// Godot 4 terrain matching mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TerrainMode {
    #[default]
    MatchCornersAndSides, // 3x3 minimal 8-neighbor peering
    MatchCorners,          // 4 diagonal corners (TL, TR, BR, BL)
    MatchSides,            // 4 orthogonal sides (T, R, B, L)
}

/// 8 peering bitmask directions around a central tile cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PeeringDirection {
    TopLeft = 0,
    Top = 1,
    TopRight = 2,
    Right = 3,
    BottomRight = 4,
    Bottom = 5,
    BottomLeft = 6,
    Left = 7,
}

impl PeeringDirection {
    pub const ALL: [PeeringDirection; 8] = [
        Self::TopLeft,
        Self::Top,
        Self::TopRight,
        Self::Right,
        Self::BottomRight,
        Self::Bottom,
        Self::BottomLeft,
        Self::Left,
    ];

    pub const CORNERS: [PeeringDirection; 4] = [
        Self::TopLeft,
        Self::TopRight,
        Self::BottomRight,
        Self::BottomLeft,
    ];

    pub const SIDES: [PeeringDirection; 4] = [
        Self::Top,
        Self::Right,
        Self::Bottom,
        Self::Left,
    ];

    pub fn offset(self) -> (i32, i32) {
        match self {
            Self::TopLeft => (-1, -1),
            Self::Top => (0, -1),
            Self::TopRight => (1, -1),
            Self::Right => (1, 0),
            Self::BottomRight => (1, 1),
            Self::Bottom => (0, 1),
            Self::BottomLeft => (-1, 1),
            Self::Left => (-1, 0),
        }
    }
}

/// A registered tile definition in a terrain set catalog.
#[derive(Debug, Clone, PartialEq)]
pub struct TerrainTileDef {
    pub tile_id: u32,
    pub terrain_id: u32,
    pub peering_bits: [Option<u32>; 8],
    pub probability: f32,
}

impl TerrainTileDef {
    pub fn new(tile_id: u32, terrain_id: u32) -> Self {
        Self {
            tile_id,
            terrain_id,
            peering_bits: [None; 8],
            probability: 1.0,
        }
    }

    pub fn with_peering(mut self, dir: PeeringDirection, neighbor_terrain: u32) -> Self {
        self.peering_bits[dir as usize] = Some(neighbor_terrain);
        self
    }
}

/// Configuration for a group of related terrains sharing a matching mode.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TerrainSet {
    pub mode: TerrainMode,
    pub terrain_names: Vec<String>,
    pub tiles: Vec<TerrainTileDef>,
}

impl TerrainSet {
    pub fn new(mode: TerrainMode) -> Self {
        Self {
            mode,
            terrain_names: Vec::new(),
            tiles: Vec::new(),
        }
    }

    pub fn add_terrain(&mut self, name: impl Into<String>) -> u32 {
        let id = self.terrain_names.len() as u32;
        self.terrain_names.push(name.into());
        id
    }

    pub fn add_tile(&mut self, tile: TerrainTileDef) {
        self.tiles.push(tile);
    }
}

/// Registry catalog holding all terrain sets for a TileSet.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TileSetTerrainCatalog {
    pub terrain_sets: BTreeMap<u32, TerrainSet>,
}

impl TileSetTerrainCatalog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_set(&mut self, set_id: u32, set: TerrainSet) {
        self.terrain_sets.insert(set_id, set);
    }

    pub fn get_set(&self, set_id: u32) -> Option<&TerrainSet> {
        self.terrain_sets.get(&set_id)
    }
}

// ============================================================================
// TileMap Terrain Autotiler
// ============================================================================

/// Godot 4 TileMap autotiler placing deterministic matching tiles for painted terrain cells.
#[derive(Debug, Clone, PartialEq)]
pub struct TileMapTerrainAutotiler {
    pub catalog: TileSetTerrainCatalog,
    pub painted_cells: BTreeMap<(i32, i32), (u32, u32)>, // (x, y) -> (terrain_set, terrain_id)
    pub placed_tiles: BTreeMap<(i32, i32), u32>,         // (x, y) -> tile_id
}

impl TileMapTerrainAutotiler {
    pub fn new(catalog: TileSetTerrainCatalog) -> Self {
        Self {
            catalog,
            painted_cells: BTreeMap::new(),
            placed_tiles: BTreeMap::new(),
        }
    }

    /// Paints terrain onto the specified cells and automatically updates neighbor peering tiles.
    pub fn paint_terrain_connect(
        &mut self,
        coords: &[(i32, i32)],
        terrain_set_id: u32,
        terrain_id: u32,
    ) {
        for &coord in coords {
            self.painted_cells.insert(coord, (terrain_set_id, terrain_id));
        }

        // Identify all affected cells (the painted cells + 1-tile ring of neighbors)
        let mut affected: BTreeSet<(i32, i32)> = BTreeSet::new();
        for &coord in coords {
            affected.insert(coord);
            for dir in PeeringDirection::ALL {
                let off = dir.offset();
                affected.insert((coord.0 + off.0, coord.1 + off.1));
            }
        }

        // Update each affected cell that has a painted terrain
        for coord in affected {
            if let Some(&(set_id, cell_terrain)) = self.painted_cells.get(&coord)
                && let Some(tile_id) = self.select_best_tile(coord, set_id, cell_terrain)
            {
                self.placed_tiles.insert(coord, tile_id);
            }
        }
    }

    /// Connects sequential coordinates in a path, painting and updating peering tiles along the route.
    pub fn paint_terrain_path(
        &mut self,
        path: &[(i32, i32)],
        terrain_set_id: u32,
        terrain_id: u32,
    ) {
        if path.is_empty() {
            return;
        }

        let mut interpolated_cells = Vec::new();
        for i in 0..path.len() {
            interpolated_cells.push(path[i]);
            if i + 1 < path.len() {
                let (x0, y0) = path[i];
                let (x1, y1) = path[i + 1];
                let line = bresenham_line(x0, y0, x1, y1);
                interpolated_cells.extend(line);
            }
        }

        self.paint_terrain_connect(&interpolated_cells, terrain_set_id, terrain_id);
    }

    /// Finds the best matching tile from the catalog based on peering bits.
    fn select_best_tile(&self, coord: (i32, i32), set_id: u32, terrain_id: u32) -> Option<u32> {
        let set = self.catalog.get_set(set_id)?;

        let active_dirs: &[PeeringDirection] = match set.mode {
            TerrainMode::MatchCornersAndSides => &PeeringDirection::ALL,
            TerrainMode::MatchCorners => &PeeringDirection::CORNERS,
            TerrainMode::MatchSides => &PeeringDirection::SIDES,
        };

        let mut best_tile_id = None;
        let mut best_score = -1i32;

        for tile in &set.tiles {
            if tile.terrain_id != terrain_id {
                continue;
            }

            let mut valid = true;
            let mut score = 0i32;

            for &dir in active_dirs {
                let off = dir.offset();
                let neighbor_coord = (coord.0 + off.0, coord.1 + off.1);
                let neighbor_terrain = self
                    .painted_cells
                    .get(&neighbor_coord)
                    .and_then(|(s, t)| if *s == set_id { Some(*t) } else { None });

                let expected = tile.peering_bits[dir as usize];

                match (expected, neighbor_terrain) {
                    (Some(exp), Some(act)) => {
                        if exp == act {
                            score += 10;
                        } else {
                            valid = false;
                            break;
                        }
                    }
                    (Some(_), None) => {
                        // Expected a neighbor, but empty
                        valid = false;
                        break;
                    }
                    (None, Some(_)) => {
                        // Tile has no peering bit here, but neighbor exists
                        score += 1;
                    }
                    (None, None) => {
                        // Both empty
                        score += 5;
                    }
                }
            }

            if valid && score > best_score {
                best_score = score;
                best_tile_id = Some(tile.tile_id);
            }
        }

        // If no strict match, fallback to any tile matching this terrain
        best_tile_id.or_else(|| {
            set.tiles
                .iter()
                .find(|t| t.terrain_id == terrain_id)
                .map(|t| t.tile_id)
        })
    }

    pub fn get_tile(&self, x: i32, y: i32) -> Option<u32> {
        self.placed_tiles.get(&(x, y)).copied()
    }

    pub fn get_terrain(&self, x: i32, y: i32) -> Option<(u32, u32)> {
        self.painted_cells.get(&(x, y)).copied()
    }
}

fn bresenham_line(mut x0: i32, mut y0: i32, x1: i32, y1: i32) -> Vec<(i32, i32)> {
    let mut line = Vec::new();
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;

    loop {
        line.push((x0, y0));
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }

    line
}

// ============================================================================
// Polygon2DSkinner (2D Linear Blend Skinning)
// ============================================================================

/// Affine 2D transformation matrix `[a, c, tx; b, d, ty]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform2D {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub tx: f32,
    pub ty: f32,
}

impl Default for Transform2D {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Transform2D {
    pub const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        tx: 0.0,
        ty: 0.0,
    };

    pub fn from_translation(tx: f32, ty: f32) -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            tx,
            ty,
        }
    }

    pub fn from_rotation(angle: f32) -> Self {
        let (sin, cos) = angle.sin_cos();
        Self {
            a: cos,
            b: sin,
            c: -sin,
            d: cos,
            tx: 0.0,
            ty: 0.0,
        }
    }

    pub fn from_scale(sx: f32, sy: f32) -> Self {
        Self {
            a: sx,
            b: 0.0,
            c: 0.0,
            d: sy,
            tx: 0.0,
            ty: 0.0,
        }
    }

    pub fn mul(&self, other: &Self) -> Self {
        Self {
            a: self.a * other.a + self.c * other.b,
            b: self.b * other.a + self.d * other.b,
            c: self.a * other.c + self.c * other.d,
            d: self.b * other.c + self.d * other.d,
            tx: self.a * other.tx + self.c * other.ty + self.tx,
            ty: self.b * other.tx + self.d * other.ty + self.ty,
        }
    }

    pub fn transform_point(&self, p: [f32; 2]) -> [f32; 2] {
        [
            self.a * p[0] + self.c * p[1] + self.tx,
            self.b * p[0] + self.d * p[1] + self.ty,
        ]
    }

    pub fn inverse(&self) -> Option<Self> {
        let det = self.a * self.d - self.b * self.c;
        if det.abs() < 1e-6 {
            return None;
        }
        let inv_det = 1.0 / det;
        Some(Self {
            a: self.d * inv_det,
            b: -self.b * inv_det,
            c: -self.c * inv_det,
            d: self.a * inv_det,
            tx: (self.c * self.ty - self.d * self.tx) * inv_det,
            ty: (self.b * self.tx - self.a * self.ty) * inv_det,
        })
    }
}

/// A 2D skeletal bone definition within `Polygon2DSkinner`.
#[derive(Debug, Clone, PartialEq)]
pub struct Bone2DDef {
    pub name: String,
    pub parent_idx: Option<usize>,
    pub rest_transform: Transform2D,
    pub pose_transform: Transform2D,
    pub inverse_bind: Transform2D,
    pub global_transform: Transform2D,
}

impl Bone2DDef {
    pub fn new(name: impl Into<String>, parent_idx: Option<usize>) -> Self {
        Self {
            name: name.into(),
            parent_idx,
            rest_transform: Transform2D::IDENTITY,
            pose_transform: Transform2D::IDENTITY,
            inverse_bind: Transform2D::IDENTITY,
            global_transform: Transform2D::IDENTITY,
        }
    }

    pub fn with_rest(mut self, rest: Transform2D) -> Self {
        self.rest_transform = rest;
        self
    }

    pub fn with_inverse_bind(mut self, inv: Transform2D) -> Self {
        self.inverse_bind = inv;
        self
    }
}

/// Godot-parity `Polygon2D` multi-bone weighted skeletal deformation solver.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Polygon2DSkinner {
    pub bones: Vec<Bone2DDef>,
    pub rest_vertices: Vec<[f32; 2]>,
    pub vertex_weights: Vec<Vec<(usize, f32)>>,
}

impl Polygon2DSkinner {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_bone(&mut self, bone: Bone2DDef) -> usize {
        let idx = self.bones.len();
        self.bones.push(bone);
        idx
    }

    pub fn add_vertex(&mut self, rest_pos: [f32; 2], weights: Vec<(usize, f32)>) {
        self.rest_vertices.push(rest_pos);
        self.vertex_weights.push(weights);
    }

    pub fn set_bone_pose(&mut self, idx: usize, pose: Transform2D) {
        if let Some(b) = self.bones.get_mut(idx) {
            b.pose_transform = pose;
        }
    }

    /// Evaluates global transforms down the 2D bone hierarchy.
    pub fn evaluate_global_transforms(&mut self) {
        for i in 0..self.bones.len() {
            let parent_idx = self.bones[i].parent_idx;
            let local = self.bones[i].rest_transform.mul(&self.bones[i].pose_transform);

            let global = match parent_idx {
                Some(p) if p < i => self.bones[p].global_transform.mul(&local),
                Some(p) if p < self.bones.len() => {
                    let p_global = self.compute_bone_global(p);
                    p_global.mul(&local)
                }
                _ => local,
            };

            self.bones[i].global_transform = global;
        }
    }

    fn compute_bone_global(&self, idx: usize) -> Transform2D {
        let mut curr = idx;
        let mut chain = Vec::new();
        let mut visited = 0;
        while visited < self.bones.len() {
            chain.push(self.bones[curr].rest_transform.mul(&self.bones[curr].pose_transform));
            if let Some(p) = self.bones[curr].parent_idx {
                curr = p;
                visited += 1;
            } else {
                break;
            }
        }
        let mut result = Transform2D::IDENTITY;
        for t in chain.into_iter().rev() {
            result = result.mul(&t);
        }
        result
    }

    /// Evaluates deformed vertex positions using Multi-Bone Linear Blend Skinning (LBS).
    pub fn deform(&mut self) -> Vec<[f32; 2]> {
        self.evaluate_global_transforms();

        let mut deformed = Vec::with_capacity(self.rest_vertices.len());

        for (v_idx, &rest_pos) in self.rest_vertices.iter().enumerate() {
            let weights = &self.vertex_weights[v_idx];
            if weights.is_empty() {
                deformed.push(rest_pos);
                continue;
            }

            let mut out_x = 0.0f32;
            let mut out_y = 0.0f32;
            let mut total_w = 0.0f32;

            for &(b_idx, w) in weights {
                if let Some(bone) = self.bones.get(b_idx) {
                    let skin_mat = bone.global_transform.mul(&bone.inverse_bind);
                    let p = skin_mat.transform_point(rest_pos);
                    out_x += w * p[0];
                    out_y += w * p[1];
                    total_w += w;
                }
            }

            if total_w > 0.0 {
                deformed.push([out_x / total_w, out_y / total_w]);
            } else {
                deformed.push(rest_pos);
            }
        }

        deformed
    }
}
