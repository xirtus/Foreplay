//! Clean-room BYO-ROM validator and spatial world cell streamer.
//!
//! Provides signature verification for retail game directories and hysteresis-based
//! 3D spatial cell streaming for open-world partitions.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Expected retail game signature definitions.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct RomSignature {
    pub title: String,
    pub platform: String,
    pub required_files: Vec<String>,
    pub optional_files: Vec<String>,
}

/// Results from BYO-ROM folder validation.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct RomValidationResult {
    pub is_valid: bool,
    pub detected_title: Option<String>,
    pub detected_platform: Option<String>,
    pub found_files: Vec<String>,
    pub missing_files: Vec<String>,
    pub total_bytes: u64,
}

/// Wizard validating retail game extracts and BYO-ROM installations.
#[derive(Clone, Debug, Default)]
pub struct ByoRomWizard {
    signatures: Vec<RomSignature>,
}

impl ByoRomWizard {
    pub fn new() -> Self {
        Self {
            signatures: Self::default_signatures(),
        }
    }

    pub fn default_signatures() -> Vec<RomSignature> {
        vec![
            RomSignature {
                title: "Skate 2".into(),
                platform: "Xbox 360".into(),
                required_files: vec!["default.xex".into()],
                optional_files: vec!["sk82_na_m.xex".into(), "data.pak".into()],
            },
            RomSignature {
                title: "Skate 3".into(),
                platform: "Xbox 360".into(),
                required_files: vec!["default.xex".into(), "big4.big".into()],
                optional_files: vec!["sk83.xex".into()],
            },
            RomSignature {
                title: "Godot 4 Project".into(),
                platform: "PC/Universal".into(),
                required_files: vec!["project.godot".into()],
                optional_files: vec!["icon.svg".into()],
            },
            RomSignature {
                title: "Generic Foreplay PAK".into(),
                platform: "Any".into(),
                required_files: vec!["game.pak".into()],
                optional_files: Vec::new(),
            },
        ]
    }

    /// Inspects a local filesystem directory against known signatures without shipping proprietary assets.
    pub fn validate_directory(&self, dir: &Path) -> RomValidationResult {
        if !dir.is_dir() {
            return RomValidationResult {
                is_valid: false,
                detected_title: None,
                detected_platform: None,
                found_files: Vec::new(),
                missing_files: Vec::new(),
                total_bytes: 0,
            };
        }

        let mut dir_files: BTreeMap<String, u64> = BTreeMap::new();
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file() {
                    let file_name = p
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_lowercase();
                    let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                    dir_files.insert(file_name, size);
                }
            }
        }

        let mut best_match: Option<(&RomSignature, usize, Vec<String>, Vec<String>)> = None;

        for sig in &self.signatures {
            let mut found = Vec::new();
            let mut missing = Vec::new();

            for req in &sig.required_files {
                let req_lower = req.to_lowercase();
                if dir_files.contains_key(&req_lower) {
                    found.push(req.clone());
                } else {
                    missing.push(req.clone());
                }
            }

            for opt in &sig.optional_files {
                let opt_lower = opt.to_lowercase();
                if dir_files.contains_key(&opt_lower) {
                    found.push(opt.clone());
                }
            }

            let score = found.len();
            if missing.is_empty() && (best_match.is_none() || score > best_match.as_ref().unwrap().1) {
                best_match = Some((sig, score, found, missing));
            }
        }

        let total_bytes = dir_files.values().sum();

        if let Some((sig, _, found, missing)) = best_match {
            RomValidationResult {
                is_valid: true,
                detected_title: Some(sig.title.clone()),
                detected_platform: Some(sig.platform.clone()),
                found_files: found,
                missing_files: missing,
                total_bytes,
            }
        } else {
            RomValidationResult {
                is_valid: false,
                detected_title: None,
                detected_platform: None,
                found_files: dir_files.keys().cloned().collect(),
                missing_files: vec!["No recognized retail signature matched".into()],
                total_bytes,
            }
        }
    }
}

/// 2D grid coordinates for a spatial cell.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub struct CellCoord {
    pub x: i32,
    pub z: i32,
}

impl CellCoord {
    pub const fn new(x: i32, z: i32) -> Self {
        Self { x, z }
    }
}

/// Changes in streaming cell state following a player position update.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct CellStreamDelta {
    pub loaded: Vec<CellCoord>,
    pub unloaded: Vec<CellCoord>,
}

/// Spatial cell partition streamer with hysteresis bounds to prevent thrashing.
#[derive(Clone, PartialEq, Debug)]
pub struct WorldCellStreamer {
    pub cell_size: f32,
    pub load_radius: f32,
    pub unload_radius: f32,
    registered_cells: BTreeMap<CellCoord, String>,
    loaded_cells: BTreeSet<CellCoord>,
}

impl WorldCellStreamer {
    pub fn new(cell_size: f32, load_radius: f32, unload_radius: f32) -> Self {
        assert!(
            unload_radius >= load_radius,
            "unload_radius must be >= load_radius for hysteresis"
        );
        Self {
            cell_size,
            load_radius,
            unload_radius,
            registered_cells: BTreeMap::new(),
            loaded_cells: BTreeSet::new(),
        }
    }

    pub fn register_cell(&mut self, coord: CellCoord, scene_path: impl Into<String>) {
        self.registered_cells.insert(coord, scene_path.into());
    }

    pub fn loaded_cells(&self) -> &BTreeSet<CellCoord> {
        &self.loaded_cells
    }

    pub fn registered_count(&self) -> usize {
        self.registered_cells.len()
    }

    /// Evaluates player `[x, y, z]` world position and streams cells with hysteresis.
    pub fn update_player_position(&mut self, player_pos: [f32; 3]) -> CellStreamDelta {
        let px = player_pos[0];
        let pz = player_pos[2];

        let mut to_load = Vec::new();
        let mut to_unload = Vec::new();

        let load_sq = self.load_radius * self.load_radius;
        let unload_sq = self.unload_radius * self.unload_radius;

        // 1. Check registered cells to load
        for &coord in self.registered_cells.keys() {
            let cx = (coord.x as f32 + 0.5) * self.cell_size;
            let cz = (coord.z as f32 + 0.5) * self.cell_size;
            let dx = cx - px;
            let dz = cz - pz;
            let dist_sq = dx * dx + dz * dz;

            if dist_sq <= load_sq && !self.loaded_cells.contains(&coord) {
                to_load.push(coord);
            }
        }

        // 2. Check loaded cells to unload
        for &coord in &self.loaded_cells {
            let cx = (coord.x as f32 + 0.5) * self.cell_size;
            let cz = (coord.z as f32 + 0.5) * self.cell_size;
            let dx = cx - px;
            let dz = cz - pz;
            let dist_sq = dx * dx + dz * dz;

            if dist_sq > unload_sq {
                to_unload.push(coord);
            }
        }

        for &c in &to_load {
            self.loaded_cells.insert(c);
        }
        for &c in &to_unload {
            self.loaded_cells.remove(&c);
        }

        CellStreamDelta {
            loaded: to_load,
            unloaded: to_unload,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_cell_streamer_hysteresis() {
        let mut streamer = WorldCellStreamer::new(100.0, 150.0, 200.0);
        let c0 = CellCoord::new(0, 0); // Center at (50, 50)
        let c1 = CellCoord::new(2, 0); // Center at (250, 50)
        streamer.register_cell(c0, "cell_0_0.tscn");
        streamer.register_cell(c1, "cell_2_0.tscn");

        // Player at origin (0, 0, 0)
        let delta1 = streamer.update_player_position([0.0, 0.0, 0.0]);
        assert_eq!(delta1.loaded, vec![c0]);
        assert!(delta1.unloaded.is_empty());
        assert!(streamer.loaded_cells().contains(&c0));
        assert!(!streamer.loaded_cells().contains(&c1));

        // Move to (120, 0, 50): inside c0, within load radius of c1?
        // c1 center is (250, 50). dist to (120, 50) is 130m <= 150m load radius!
        let delta2 = streamer.update_player_position([120.0, 0.0, 50.0]);
        assert_eq!(delta2.loaded, vec![c1]);
        assert!(delta2.unloaded.is_empty());
        assert_eq!(streamer.loaded_cells().len(), 2);

        // Move back to (0, 0, 0): dist to c1 center (250, 50) is sqrt(250^2 + 50^2) = 254.9m > 200m unload radius
        let delta3 = streamer.update_player_position([0.0, 0.0, 0.0]);
        assert!(delta3.loaded.is_empty());
        assert_eq!(delta3.unloaded, vec![c1]);
        assert_eq!(streamer.loaded_cells().len(), 1);
    }
}
