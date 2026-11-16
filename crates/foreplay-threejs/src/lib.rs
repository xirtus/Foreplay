//! `foreplay-threejs` — 3JSE-compatible Three.js (`WebGPURenderer` + `WebGLRenderer` + Vite + TypeScript)
//! game project compiler and bidirectional `THREE.Object3D` / `3jse.scene.json` scene importer.

pub mod compiler;
pub mod error;
pub mod feel;
pub mod importer;
pub mod parametric_polyamour;
pub mod transpiler;

pub use compiler::{
    ThreeJsExportOptions, ThreeJsProjectBundle, ThreeToneMapping, compile_ir_to_threejs_project,
};
pub use error::ThreeJsError;
pub use feel::{collect_feel_knobs, generate_feel_ts};
pub use importer::{export_ir_to_threejs_json, import_threejs_json_to_ir};
pub use parametric_polyamour::{
    BakedPolyamourAsset, ThreeJsParametricPolyamourAsset, extract_polyamour_assets_from_scene,
    generate_polyamour_runtime_ts,
};
pub use transpiler::{generate_behaviors_ts, transpile_gdscript_to_ts};

/// Default Three.js npm semver targeted by generated 3JSE web projects.
pub const THREEJS_TARGET_VERSION: &str = "^0.174.0";
