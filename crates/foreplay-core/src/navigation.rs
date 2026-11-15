//! TileMapLayer, GridMap, and 2D/3D Navigation Servers (AStarGrid2D & NavMesh3D).

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

// -----------------------------------------------------------------------------
// 2D TileMapLayer & 3D GridMap Sparse Storage
// -----------------------------------------------------------------------------

/// Single tile cell placed inside a `TileMapLayer`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TileCell {
    pub source_id: i32,
    pub atlas_coords: [i32; 2],
    pub alternative_tile: i32,
    pub terrain_mask: u16,
}

impl TileCell {
    pub fn new(source_id: i32, atlas_coords: [i32; 2]) -> Self {
        Self {
            source_id,
            atlas_coords,
            alternative_tile: 0,
            terrain_mask: 0,
        }
    }
}

/// Sparse 2D TileMap Layer storing cell data and terrain autotile matching.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct TileMapLayer {
    pub cells: BTreeMap<[i32; 2], TileCell>,
}

impl TileMapLayer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_cell(&mut self, coords: [i32; 2], cell: TileCell) {
        self.cells.insert(coords, cell);
    }

    pub fn get_cell(&self, coords: [i32; 2]) -> Option<&TileCell> {
        self.cells.get(&coords)
    }

    pub fn remove_cell(&mut self, coords: [i32; 2]) -> Option<TileCell> {
        self.cells.remove(&coords)
    }

    pub fn len(&self) -> usize {
        self.cells.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// Resolves an autotile atlas coordinate from a 4-bit cardinal or 8-bit surrounding terrain bitmask.
    /// Bit flags: 1 = North, 2 = East, 4 = South, 8 = West.
    pub fn autotile_lookup(surrounding_cardinal_mask: u8) -> [i32; 2] {
        match surrounding_cardinal_mask & 0x0F {
            0b0000 => [0, 0], // Isolated tile
            0b0001 => [0, 1], // N
            0b0010 => [1, 0], // E
            0b0011 => [1, 1], // NE corner
            0b0100 => [0, 2], // S
            0b0101 => [0, 3], // N-S vertical corridor
            0b0110 => [1, 2], // SE corner
            0b0111 => [1, 3], // E-T-junction
            0b1000 => [2, 0], // W
            0b1001 => [2, 1], // NW corner
            0b1010 => [3, 0], // E-W horizontal corridor
            0b1011 => [3, 1], // N-T-junction
            0b1100 => [2, 2], // SW corner
            0b1101 => [2, 3], // W-T-junction
            0b1110 => [3, 2], // S-T-junction
            0b1111 => [3, 3], // Full center 4-way cross
            _ => [0, 0],
        }
    }
}

/// Single 3D voxel block cell placed inside a `GridMap`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct GridCell {
    pub item: i32,
    pub orientation: u8,
}

impl GridCell {
    pub fn new(item: i32) -> Self {
        Self {
            item,
            orientation: 0,
        }
    }
}

/// Sparse 3D GridMap storing block cell placements.
#[derive(Clone, PartialEq, Debug)]
pub struct GridMap {
    pub cell_size: [f32; 3],
    pub cells: BTreeMap<[i32; 3], GridCell>,
}

impl Default for GridMap {
    fn default() -> Self {
        Self {
            cell_size: [2.0, 2.0, 2.0],
            cells: BTreeMap::new(),
        }
    }
}

impl GridMap {
    pub fn new(cell_size: [f32; 3]) -> Self {
        Self {
            cell_size,
            cells: BTreeMap::new(),
        }
    }

    pub fn set_cell(&mut self, coords: [i32; 3], cell: GridCell) {
        self.cells.insert(coords, cell);
    }

    pub fn get_cell(&self, coords: [i32; 3]) -> Option<&GridCell> {
        self.cells.get(&coords)
    }

    pub fn remove_cell(&mut self, coords: [i32; 3]) -> Option<GridCell> {
        self.cells.remove(&coords)
    }

    /// Converts integer block coordinates into world 3D position (cell center).
    pub fn map_to_world(&self, coords: [i32; 3]) -> [f32; 3] {
        [
            (coords[0] as f32 + 0.5) * self.cell_size[0],
            (coords[1] as f32 + 0.5) * self.cell_size[1],
            (coords[2] as f32 + 0.5) * self.cell_size[2],
        ]
    }

    /// Converts world 3D position into integer block grid coordinates.
    pub fn world_to_map(&self, pos: [f32; 3]) -> [i32; 3] {
        [
            (pos[0] / self.cell_size[0]).floor() as i32,
            (pos[1] / self.cell_size[1]).floor() as i32,
            (pos[2] / self.cell_size[2]).floor() as i32,
        ]
    }
}

// -----------------------------------------------------------------------------
// NavigationServer2D (AStarGrid2D)
// -----------------------------------------------------------------------------

/// Octile diagonal pathfinding movement policy.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum DiagonalMode {
    #[default]
    Always,
    Never,
    OnlyIfNoObstacles,
}

/// 2D Grid-based pathfinder matching Godot 4's `AStarGrid2D`.
#[derive(Clone, PartialEq, Debug)]
pub struct AStarGrid2D {
    pub region_min: [i32; 2],
    pub region_max: [i32; 2],
    pub cell_size: [f32; 2],
    pub solid_cells: BTreeSet<[i32; 2]>,
    pub weight_scale: BTreeMap<[i32; 2], f32>,
    pub diagonal_mode: DiagonalMode,
}

impl Default for AStarGrid2D {
    fn default() -> Self {
        Self {
            region_min: [0, 0],
            region_max: [64, 64],
            cell_size: [32.0, 32.0],
            solid_cells: BTreeSet::new(),
            weight_scale: BTreeMap::new(),
            diagonal_mode: DiagonalMode::Always,
        }
    }
}

impl AStarGrid2D {
    pub fn new(min: [i32; 2], max: [i32; 2], cell_size: [f32; 2]) -> Self {
        Self {
            region_min: min,
            region_max: max,
            cell_size,
            solid_cells: BTreeSet::new(),
            weight_scale: BTreeMap::new(),
            diagonal_mode: DiagonalMode::Always,
        }
    }

    pub fn set_point_solid(&mut self, point: [i32; 2], solid: bool) {
        if solid {
            self.solid_cells.insert(point);
        } else {
            self.solid_cells.remove(&point);
        }
    }

    pub fn is_point_solid(&self, point: [i32; 2]) -> bool {
        self.solid_cells.contains(&point)
    }

    pub fn set_point_weight(&mut self, point: [i32; 2], weight: f32) {
        self.weight_scale.insert(point, weight.max(1.0));
    }

    pub fn is_in_bounds(&self, p: [i32; 2]) -> bool {
        p[0] >= self.region_min[0]
            && p[0] <= self.region_max[0]
            && p[1] >= self.region_min[1]
            && p[1] <= self.region_max[1]
    }

    /// Finds the shortest walkable path in grid coordinates using A* with octile distance heuristic.
    pub fn find_path(&self, start: [i32; 2], target: [i32; 2]) -> Option<Vec<[i32; 2]>> {
        if !self.is_in_bounds(start) || !self.is_in_bounds(target) {
            return None;
        }
        if self.is_point_solid(start) || self.is_point_solid(target) {
            return None;
        }
        if start == target {
            return Some(vec![start]);
        }

        #[derive(Copy, Clone, PartialEq)]
        struct NodeState {
            f_score: f32,
            g_score: f32,
            pos: [i32; 2],
        }

        impl Eq for NodeState {}

        impl Ord for NodeState {
            fn cmp(&self, other: &Self) -> Ordering {
                other.f_score.partial_cmp(&self.f_score).unwrap_or(Ordering::Equal)
            }
        }

        impl PartialOrd for NodeState {
            fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
                Some(self.cmp(other))
            }
        }

        let mut dist: BTreeMap<[i32; 2], f32> = BTreeMap::new();
        let mut parent: BTreeMap<[i32; 2], [i32; 2]> = BTreeMap::new();
        let mut heap: BinaryHeap<NodeState> = BinaryHeap::new();

        dist.insert(start, 0.0);
        heap.push(NodeState {
            f_score: 0.0,
            g_score: 0.0,
            pos: start,
        });

        let neighbors_deltas: &[([i32; 2], f32)] = match self.diagonal_mode {
            DiagonalMode::Never => &[
                ([1, 0], 1.0),
                ([-1, 0], 1.0),
                ([0, 1], 1.0),
                ([0, -1], 1.0),
            ],
            DiagonalMode::Always | DiagonalMode::OnlyIfNoObstacles => &[
                ([1, 0], 1.0),
                ([-1, 0], 1.0),
                ([0, 1], 1.0),
                ([0, -1], 1.0),
                ([1, 1], std::f32::consts::SQRT_2),
                ([-1, 1], std::f32::consts::SQRT_2),
                ([1, -1], std::f32::consts::SQRT_2),
                ([-1, -1], std::f32::consts::SQRT_2),
            ],
        };

        while let Some(NodeState {
            f_score: _,
            g_score,
            pos,
        }) = heap.pop()
        {
            if pos == target {
                let mut path = Vec::new();
                let mut curr = target;
                path.push(curr);
                while let Some(&p) = parent.get(&curr) {
                    path.push(p);
                    curr = p;
                }
                path.reverse();
                return Some(path);
            }

            if let Some(&best) = dist.get(&pos)
                && g_score > best
            {
                continue;
            }

            for &(delta, base_step_cost) in neighbors_deltas {
                let next_pos = [pos[0] + delta[0], pos[1] + delta[1]];
                if !self.is_in_bounds(next_pos) || self.is_point_solid(next_pos) {
                    continue;
                }

                // If OnlyIfNoObstacles, verify adjacent cardinals aren't solid
                if self.diagonal_mode == DiagonalMode::OnlyIfNoObstacles
                    && delta[0] != 0
                    && delta[1] != 0
                {
                    let side1 = [pos[0] + delta[0], pos[1]];
                    let side2 = [pos[0], pos[1] + delta[1]];
                    if self.is_point_solid(side1) || self.is_point_solid(side2) {
                        continue;
                    }
                }

                let weight = self.weight_scale.get(&next_pos).copied().unwrap_or(1.0);
                let next_cost = g_score + base_step_cost * weight;

                if next_cost < dist.get(&next_pos).copied().unwrap_or(f32::INFINITY) {
                    dist.insert(next_pos, next_cost);
                    parent.insert(next_pos, pos);

                    // Octile heuristic
                    let dx = (target[0] - next_pos[0]).abs() as f32;
                    let dy = (target[1] - next_pos[1]).abs() as f32;
                    let h = (dx + dy) + (std::f32::consts::SQRT_2 - 2.0) * dx.min(dy);

                    heap.push(NodeState {
                        f_score: next_cost + h,
                        g_score: next_cost,
                        pos: next_pos,
                    });
                }
            }
        }

        None
    }

    /// World-space path lookup mapping cell centers to [f32; 2].
    pub fn find_path_world(&self, start: [f32; 2], target: [f32; 2]) -> Option<Vec<[f32; 2]>> {
        let grid_start = [
            (start[0] / self.cell_size[0]).floor() as i32,
            (start[1] / self.cell_size[1]).floor() as i32,
        ];
        let grid_target = [
            (target[0] / self.cell_size[0]).floor() as i32,
            (target[1] / self.cell_size[1]).floor() as i32,
        ];

        let path = self.find_path(grid_start, grid_target)?;
        let mut world_path = Vec::with_capacity(path.len());
        for p in path {
            world_path.push([
                (p[0] as f32 + 0.5) * self.cell_size[0],
                (p[1] as f32 + 0.5) * self.cell_size[1],
            ]);
        }
        Some(world_path)
    }
}

// -----------------------------------------------------------------------------
// NavigationServer3D (NavMesh3D)
// -----------------------------------------------------------------------------

/// 3D Navigation Mesh consisting of connected convex polygons with A* centroid pathfinding.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct NavMesh3D {
    pub vertices: Vec<[f32; 3]>,
    pub polygons: Vec<Vec<usize>>,
}

impl NavMesh3D {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_polygon(&mut self, polygon_vertices: Vec<[f32; 3]>) -> usize {
        let mut indices = Vec::with_capacity(polygon_vertices.len());
        for v in polygon_vertices {
            // Find existing matching vertex or insert
            let mut found_idx = None;
            for (i, existing) in self.vertices.iter().enumerate() {
                let d2 = (existing[0] - v[0]).powi(2)
                    + (existing[1] - v[1]).powi(2)
                    + (existing[2] - v[2]).powi(2);
                if d2 < 1e-6 {
                    found_idx = Some(i);
                    break;
                }
            }

            let idx = match found_idx {
                Some(i) => i,
                None => {
                    self.vertices.push(v);
                    self.vertices.len() - 1
                }
            };
            indices.push(idx);
        }

        self.polygons.push(indices);
        self.polygons.len() - 1
    }

    pub fn polygon_centroid(&self, poly_idx: usize) -> Option<[f32; 3]> {
        let indices = self.polygons.get(poly_idx)?;
        if indices.is_empty() {
            return None;
        }

        let mut sum = [0.0f32; 3];
        for &idx in indices {
            let v = self.vertices[idx];
            sum[0] += v[0];
            sum[1] += v[1];
            sum[2] += v[2];
        }
        let count = indices.len() as f32;
        Some([sum[0] / count, sum[1] / count, sum[2] / count])
    }

    /// Finds the closest polygon centroid to a given 3D point.
    pub fn find_closest_polygon(&self, point: [f32; 3]) -> Option<usize> {
        let mut closest = None;
        let mut min_d2 = f32::INFINITY;

        for (i, _) in self.polygons.iter().enumerate() {
            if let Some(c) = self.polygon_centroid(i) {
                let d2 = (c[0] - point[0]).powi(2)
                    + (c[1] - point[1]).powi(2)
                    + (c[2] - point[2]).powi(2);
                if d2 < min_d2 {
                    min_d2 = d2;
                    closest = Some(i);
                }
            }
        }

        closest
    }

    /// Returns whether two polygons share at least one edge (2 shared vertices).
    pub fn are_polygons_adjacent(&self, p0: usize, p1: usize) -> bool {
        let Some(idx0) = self.polygons.get(p0) else {
            return false;
        };
        let Some(idx1) = self.polygons.get(p1) else {
            return false;
        };

        let mut shared = 0;
        for &i0 in idx0 {
            if idx1.contains(&i0) {
                shared += 1;
            }
        }
        shared >= 2
    }

    /// Computes smooth 3D waypoints path between `start` and `target` using polygon-graph A*.
    pub fn find_path(&self, start: [f32; 3], target: [f32; 3]) -> Option<Vec<[f32; 3]>> {
        let start_poly = self.find_closest_polygon(start)?;
        let target_poly = self.find_closest_polygon(target)?;

        if start_poly == target_poly {
            return Some(vec![start, target]);
        }

        #[derive(Copy, Clone, PartialEq)]
        struct PolyNode {
            f_score: f32,
            g_score: f32,
            poly: usize,
        }

        impl Eq for PolyNode {}

        impl Ord for PolyNode {
            fn cmp(&self, other: &Self) -> Ordering {
                other.f_score.partial_cmp(&self.f_score).unwrap_or(Ordering::Equal)
            }
        }

        impl PartialOrd for PolyNode {
            fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
                Some(self.cmp(other))
            }
        }

        let mut dist: BTreeMap<usize, f32> = BTreeMap::new();
        let mut parent: BTreeMap<usize, usize> = BTreeMap::new();
        let mut heap = BinaryHeap::new();

        dist.insert(start_poly, 0.0);
        heap.push(PolyNode {
            f_score: 0.0,
            g_score: 0.0,
            poly: start_poly,
        });

        while let Some(PolyNode {
            f_score: _,
            g_score,
            poly,
        }) = heap.pop()
        {
            if poly == target_poly {
                // Reconstruct polygon centroids path
                let mut poly_path = Vec::new();
                let mut curr = target_poly;
                poly_path.push(curr);
                while let Some(&p) = parent.get(&curr) {
                    poly_path.push(p);
                    curr = p;
                }
                poly_path.reverse();

                let mut waypoints = Vec::with_capacity(poly_path.len() + 2);
                waypoints.push(start);
                for p in poly_path {
                    if let Some(c) = self.polygon_centroid(p) {
                        waypoints.push(c);
                    }
                }
                waypoints.push(target);
                return Some(waypoints);
            }

            if let Some(&best) = dist.get(&poly)
                && g_score > best
            {
                continue;
            }

            let c_curr = self.polygon_centroid(poly).unwrap_or([0.0, 0.0, 0.0]);

            for next_poly in 0..self.polygons.len() {
                if next_poly != poly && self.are_polygons_adjacent(poly, next_poly) {
                    let c_next = self.polygon_centroid(next_poly).unwrap_or([0.0, 0.0, 0.0]);
                    let step_cost = ((c_next[0] - c_curr[0]).powi(2)
                        + (c_next[1] - c_curr[1]).powi(2)
                        + (c_next[2] - c_curr[2]).powi(2))
                    .sqrt();

                    let next_cost = g_score + step_cost;
                    if next_cost < dist.get(&next_poly).copied().unwrap_or(f32::INFINITY) {
                        dist.insert(next_poly, next_cost);
                        parent.insert(next_poly, poly);

                        let c_target = self.polygon_centroid(target_poly).unwrap_or([0.0, 0.0, 0.0]);
                        let h = ((c_target[0] - c_next[0]).powi(2)
                            + (c_target[1] - c_next[1]).powi(2)
                            + (c_target[2] - c_next[2]).powi(2))
                        .sqrt();

                        heap.push(PolyNode {
                            f_score: next_cost + h,
                            g_score: next_cost,
                            poly: next_poly,
                        });
                    }
                }
            }
        }

        None
    }
}
