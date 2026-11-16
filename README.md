# Foreplay Game Engine (`v1.1.0` — Deep Polyamour Live-Link & Full Godot 4 Parity Edition)

**The world-class, AI-native Rust & Three.js (`3JSE`) game engine, Godot 4-compatible studio, zero-export Polyamour 3D modeler, and clean-room port host for macOS, Linux/SteamOS, WebGPU, and the SexOS `sexlife` suite.**

Foreplay (`foreplay`) unites **19 Rust crates (`97,333 LoC`, `557/557` passing tests, `52` red-team `proptest` suites, `10/10` verification gates)**:

1. **Zero-Export Polyamour (`sex-3d`) Integration (`foreplay-polyamour`, `foreplay-player`, `foreplay-threejs`):**
   - **120Hz `poly://` (`PLNK`) Bidirectional Live-Link:** Stream `HalfEdgeMesh` vertex/topology deltas, `1/2/3` sub-object selections, and semantic tags between standalone **Polyamour** and a running **Foreplay** game session—hot-rebuilding `RapierSpace3D` colliders and skate grind splines mid-frame with **zero player momentum loss** and zero `.glb` file export.
   - **Sub-Object Raycast Modeling, Sculpting & Auto-Rigging:** Exact Möller–Trumbore `[1] Vertex` / `[2] Edge` / `[3] Face` picking, `Alt+Click` quad edge-loop select, proportional editing (`O`), parameterized `polyops 2` transcripts, BVH sculpt brushes (`Grab`, `ClayStrips`, `Crease`, `Smooth`, `Inflate`, `Flatten` + XYZ symmetry), Laplacian heat-diffusion auto-rig bone skinning (`sum(weights) == 1.0`), `ShapeKeySet` blendshapes, and non-overlapping Smart UV2 lightmap packing.
   - **Live Runtime Parametric Modifiers (Rust & Three.js):** Keep Polyamour modifier stacks & morph targets live in shipped native Rust games AND exported Three.js (`src/polyamour_runtime.ts`) web games, compiling `"grind_edge"` loops directly into 3D `CatmullRomSpline3D` / `THREE.CatmullRomCurve3` rails.
2. **Hardware `wgpu` `30.0.2` Rendering, WGSL `@compute` Path-Tracer + SVGF & Zero-Copy 120Hz Viewports (`foreplay-render`):**
   - Clustered Forward+ PBR, 1024x1024 PCF directional shadows, 64-bone GPU skinning, WGSL `@compute` BVH GGX multi-bounce Path-Tracer + 5x5 À-Trous edge-stopping **SVGF denoiser** + AgX tonemapping, WGSL `@compute` Radiance Cascades GI, 2D `CanvasItem` SDF lights/shadows, `ZeroCopyViewportBridge` (`egui_wgpu::Renderer::register_native_texture`), and 120Hz `SurfaceSwapchainPresenter`.
3. **Virtually Complete Godot 4 Runtime, Scripting & `sexlife` Pro-Apps Suite:**
   - **Physics, Audio & Input:** `rapier2d/3d` `0.36.1` + live `PortFaithful` hotswap + Godot 4 `CharacterBody2D/3D::move_and_slide`, lock-free SPSC `cpal 0.18.2` hardware audio + `symphonia 0.6.1` codecs + 3D Doppler acoustics, and `gilrs 0.11.2` + Godot `InputMap`.
   - **471-Class `ClassDB`, Assets & UI:** Binary `.glb` rigged character importer, `.tscn`/`.tres` codecs, threaded `ResourceLoader`, `Skeleton3D` + `AnimationTree`, `cosmic-text 0.19.0` BBCode shaping, and `TileMap` Wang/bitmask terrain autotiling.
   - **Quad-Frontend Scripting & `sexlife` Links:** Sub-second native Rust `.dylib`/`.so` hot-reload (`RustHotReloadHost` with state-preserving `IrValue` migration), full GDScript 2.0 (`await` coroutines, lambdas, `match` guards, 444 builtins, `GdLspAnalyzer`), TypeScript/JS, 82-proc `GDExtension` C ABI (`foreplay-gdext`), plus **`Moan`** adaptive music stem link & **`Splicy`** FCP J/L cutscene EDL bridge.
   - **6+1 One-Click Exports:** Self-contained `FPCKEXEC` native binaries (`macOS .app`, `Linux/SteamDeck`, `Windows`, `SexOS .sexpkg`), spec-compliant **WASM WebGPU** modules with embedded `"foreplay_fpck"` custom sections + PWA workers, and runnable **Three.js / `3JSE` Vite + TypeScript** web projects.

---

## Copyleft Licensing Model (No MIT Giveaway, Commercial Game Friendly)

- **Studio IDE, 3JSE Compiler, AI Harness, Atlas, Exporter, Verifier & Porting Suite (`GPL-3.0-only`):** `foreplay-editor`, `foreplay-threejs`, `foreplay-harness`, `foreplay-atlas`, `foreplay-porthost`, `foreplay-export`, and `foreplay-verifier`.
- **Engine Runtime, Player, GDExtension & Procedural 3D Crates (`MPL-2.0`):** `foreplay-ir`, `foreplay-core`, `foreplay-polyamour`, `foreplay-platform`, `foreplay-render`, `foreplay-physics`, `foreplay-audio`, `foreplay-net`, `foreplay-script`, `foreplay-gdscript`, `foreplay-gdext`, and `foreplay-player`.

---

## Quick Start

```bash
# Run all 557 unit, integration & 52 adversarial proptest suites across all 19 crates
cargo test --workspace

# Run all 10 automated end-to-end verification gates (<1s)
cargo run -p foreplay-harness -- verify-all-gates

# Launch the interactive desktop Studio Obsidian & Chroma IDE (egui + egui_dock + hardware wgpu)
cargo run -p foreplay-editor -- --gui

# Run headless smoke verification across all 8 IDE workspaces, 7 bottom docks & 5 modals
cargo run -p foreplay-editor -- --headless

# Run standalone native game player 6-stage hardware + TCP PIE IPC + FPCKEXEC + poly:// Live-Link self-test
cargo run -p foreplay-player -- --self-test

# Build native macOS Universal2 (arm64 + x86_64) Foreplay.app, .dmg & .zip in dist/macos/
bash scripts/package-macos.sh

# Build native Linux x86_64 (SteamOS / Desktop) & ARM64 .tar.gz AppDirs and .deb packages in dist/linux/
bash scripts/package-linux.sh
```

## Key Documents

- [`PLAN.md`](PLAN.md) — Phases 0–21 master roadmap, crate DAG, C++-to-Rust idiom map, and verified exit gates.
- [`HARNESS.md`](HARNESS.md) — 3JSE-style Agent API surface, `FeelSpec` specification, and Carbon/Hydrogen verification gates.
- [`AGENTS.md`](AGENTS.md) — Multi-agent operating rules (Carbon vs. Hydrogen track boundaries).
