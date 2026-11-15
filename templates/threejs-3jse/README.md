# Foreplay Track D: 3JSE / Three.js WebGPU + Vite + TypeScript Template

This template demonstrates Foreplay's **3JSE-compatible Three.js pipeline**, uniting:
- **`WebGPURenderer` + `WebGLRenderer` fallback** with `AgXToneMapping`
- **Vite 6 + TypeScript 5.7 + Three.js (`^0.174.0`)** zero-placeholder project compilation
- **Bidirectional scene conversion**: `THREE.Object3D.toJSON()` & `3jse.scene.json` <-> `foreplay_ir::IrScene`
- **Live 3JSE `FeelSpec` DevHUD**: Real-time gameplay physics tuning in Chrome and Safari
- **Signal Bus & Transpiler**: Godot GDScript & TypeScript scripts compiled to idiomatic `Behavior` classes

---

## 1. Opening a 3JSE Scene in Foreplay

You can import any Three.js or 3JSE scene into Foreplay's Intermediate Representation (`IrScene`) programmatically or via JSON-RPC 2.0:

### Via Rust API:
```rust
use foreplay_threejs::importer::import_threejs_json_to_ir;

let scene_json = include_str!("neon_skate_3jse.json");
let scene = import_threejs_json_to_ir(scene_json).expect("valid 3jse scene");

println!("Loaded scene '{}' with {} nodes", scene.name, scene.nodes.len());
```

### Via MCP / JSON-RPC:
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "foreplay/import_threejs",
  "params": {
    "source": "{ \"format\": \"3jse.scene\", \"name\": \"NeonSkatePark\", ... }"
  }
}
```

---

## 2. Compiling a Foreplay Scene to Three.js + Vite Web Game

Compile any Foreplay scene into a complete, 100% runnable web project with fixed 60Hz physics, typed signals, and live DevHUD:

### Via Rust API:
```rust
use foreplay_threejs::compiler::{compile_ir_to_threejs_project, ThreeJsExportOptions, ThreeToneMapping};
use std::collections::BTreeMap;

let opts = ThreeJsExportOptions {
    project_name: "Neon Skate Park".into(),
    use_webgpu_renderer: true,
    include_feel_dev_hud: true,
    include_atlas_json: true,
    tone_mapping: ThreeToneMapping::AgX,
};

let bundle = compile_ir_to_threejs_project(&scene, &gd_scripts, &ts_scripts, &opts)?;

// bundle.files contains:
// - package.json
// - tsconfig.json
// - vite.config.ts
// - index.html
// - src/main.ts
// - src/scene.ts
// - src/feel.ts
// - src/signals.ts
// - src/scripts/behaviors.ts
// - src/atlas.json
```

### Running the Exported Project:
```bash
npm install
npm run dev
# Launches Vite dev server at http://localhost:3000
```

---

## 3. Features Included in Generated Projects

| Feature | Description |
|---|---|
| **WebGPU Renderer** | Bootstraps `WebGPURenderer` with seamless fallback to `WebGLRenderer` |
| **AgX Tone Mapping** | Film-grade color transforms preserving highlight details |
| **Collapsible FeelHUD** | Live range sliders bound to `window.__FOREPLAY_FEEL__` for tuning physics knobs on the fly |
| **Typed Signal Bus** | Pre-wired `SignalConnection` graph matching Godot signal semantics |
| **Kinematic Transpiler** | GDScript `_physics_process(delta)` & `move_and_slide()` mapped to Three.js Euler/Vector3 transforms |
| **Atlas Semantic Graph** | `@3jse/atlas` semantic graph JSON (`src/atlas.json`) with layer layouts and blast-radius edges |
