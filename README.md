# Foreplay Game Engine (`v0.2.0` — Studio Edition)

**The world-class, AI-native Rust & Three.js (`3JSE`) game engine, 3D modeler, and creative studio for desktop, WebGPU, clean-room ports, and the SexOS `sexlife` suite.**

Foreplay (`foreplay`) fuses six capabilities into one cohesive 17-crate Rust workspace:
1. **Dual Native Game Targets — Rust (`Bevy 0.20 + wgpu + SexOS`) AND Three.js (`3JSE` WebGPU + Vite + TypeScript):** Author a game once in Foreplay's canonical **Gameplay IR** (`foreplay-ir`) and compile it either to a high-FPS native Rust binary (`Linux/Steam Deck`, `macOS .app` with Game Mode, `Windows`, `WebGPU Wasm`, `SexOS .sexpkg`) **OR** to a clean, zero-lock-in **Three.js + Vite + TypeScript (`3JSE`-compatible)** web project (`foreplay-threejs`) complete with `WebGPURenderer` + `WebGLRenderer` fallback, Polyamour `BufferGeometry` meshes, reactive `FeelSpec` browser DevHUD, `@3jse/atlas` semantic graph, and transpiled GDScript/TS behaviors—plus bidirectional `THREE.Object3D` / `3jse.scene.json` import!
2. **Built-In Blender-Class 3D Modeling & Path Tracer (`foreplay-polyamour`):** Native `HalfEdgeMesh` n-gon topology, Blender Edit-Mode ops (`extrude`, `inset`, `bevel`, `loop_cut`, `knife`, `boolean_csg`), non-destructive 9-modifier stack (`Mirror`, `Catmull-Clark Subdivision`, `Array`, `Solidify`, `Bevel`, `Triangulate`, `Weld`, `Displace`, `Smooth`), `.blend` / glTF / OBJ / STL / PLY I/O, `polyops 1` (`import bpy`) atomic scripting, and a deterministic SAH BVH GGX + **AgX** path-traced lightmapper right inside the `Model` workspace.
3. **Apple Logic Pro & Final Cut Pro "Studio Obsidian & Chroma" UX (`foreplay-editor`):** 8 dockable workspaces (`2D`, `3D`, `Model`, `Script`, `Three.js`, `Atlas`, `Port Diff`, `AssetLib`), deep OLED obsidian + jewel-toned semantic track colors, smoked-glass center **Studio LCD Transport HUD**, **Logic Pro 270° Rotary Arc Knobs** (`FeelSpec` Smart Controls) + interactive 3-band EQ & 32-band FFT spectrum analyzer, **Final Cut Pro Magnetic Director Timeline** with live optical hover-skimming and connected audio/camera/signal clips (`moan-core`-inspired 48kHz non-destructive audio timeline), and macOS 120Hz trackpad gestures.
4. **Built *Around* Modular Bevy 0.20 (`Bevy is a dependency, never a fork`) + UE6 Capabilities:** Wraps `bevy_ecs`, `bevy_reflect`, and `bevy_math` `0.20.1` (`ChildOf`/`Children`, `RequiredComponents`, `EntityEvent` Observers) enhanced with **UE6/Verse Speculative Frame Rollback**, **64-vert Meshlet LODs ("Micro-Nanite")**, **Radiance Cascades + Voxel Global Illumination ("Micro-Lumen")**, **Two-Bone/FABRIK IK & Motion Matching (`PoseSearchDatabase`)**, **3D Spline PCG**, and **MetaSounds-lite Procedural Foley Synthesis**.
5. **3JSE Unified Architecture (`One Game, Four Frontends`):** Visual Editor, Rust / TypeScript / GDScript VMs, Visual Gameplay Graphs, and AI Agents (`mcp-stdio`) all operate over invertible `EditorCommand` transactions, 600-frame **Time-Travel Causality Scrubbing**, and **Multi-Agent Subtree Leases** (`foreplay-atlas`).
6. **`rustports.org` & Clean-Room Port Hosting (`skate2-rust-port` Model):** `port://` BYO-ROM VFS mounts (`foreplay-porthost`), live mid-frame A/B physics hotswapping, 1,000-frame Differential Port Oracle, and replay-frame multiplayer (`foreplay-server`).

---

## Copyleft Licensing Model (No MIT Giveaway, Commercial Game Friendly)

Foreplay rejects permissive MIT giveaways while allowing studios and indie developers to ship commercial closed-source games:

- **Studio IDE, 3JSE Compiler, AI Harness, Atlas, Exporter, Verifier & Porting Suite (`GPL-3.0-only`):** `foreplay-editor`, `foreplay-threejs`, `foreplay-harness`, `foreplay-atlas`, `foreplay-porthost`, `foreplay-export`, and `foreplay-verifier` are licensed under **GNU GPL v3.0 only** (matching SexOS, 3JSE, and `skate2-rust-port`). Any fork of the editor, Three.js compiler, or AI harness must remain free software.
- **Engine Runtime & Procedural 3D Crates (`MPL-2.0`):** `foreplay-ir`, `foreplay-core`, `foreplay-polyamour`, `foreplay-platform`, `foreplay-render`, `foreplay-physics`, `foreplay-audio`, `foreplay-net`, `foreplay-script`, and `foreplay-gdscript` use file-level strong copyleft (**MPL-2.0**, secondary-compatible with **GPL-3.0**).
  - **Why not plain GPL-2.0?** Rust statically links `.rlib` crates by default; plain GPL-2.0 would infect a developer's proprietary game code on static link and is incompatible with GPL-3.0 editor crates.
  - **What MPL-2.0 guarantees:** Any modification to Foreplay's runtime crates *must* be released as open source (zero proprietary engine forks), while game developers can statically link unmodified runtime crates into closed-source commercial games on Steam, consoles, Linux, and SexOS.

---

## Quick Start

```bash
# Run full 15-crate test suite & adversarial proptest fuzzers
cargo test --workspace

# Launch the interactive desktop Godot-parity IDE (egui + egui_dock)
cargo run -p foreplay-editor -- --gui

# Run headless smoke verification across all 6 workspaces, 7 bottom docks & 5 modals
cargo run -p foreplay-editor

# Execute all Phase 1-6 automated exit gates (200 .tscn conformance, 300-tick AI playtest, 1,000-frame port oracle, 5-platform exports)
cargo run -p foreplay-harness -- verify-all-gates

# Convert a Godot 4 .tscn scene into deterministic Foreplay .fscn IR & inspect Atlas
cargo run -p foreplay-harness -- import-tscn templates/godot-migration/skate_spot.tscn

# Start JSON-RPC 2.0 MCP stdio server for Claude Code / Gemini / Codex AI agents
cargo run -p foreplay-harness -- mcp-stdio

# Run dedicated headless multiplayer server self-test
cargo run -p foreplay-net --bin foreplay-server -- --self-test
```

## Key Documents

- [`PLAN.md`](PLAN.md) — RustPorts Phase 0–6 inventory, dependency DAG, `[mechanical]`/`[semantic]` wave tags, C++-to-Rust idiom map, and live checkpoint ledger.
- [`HARNESS.md`](HARNESS.md) — 3JSE-style Agent API surface, `FeelSpec` specification, Reuse Ladder, and Carbon/Hydrogen verification gates.
- [`AGENTS.md`](AGENTS.md) — Auto-loaded multi-agent operating rules (Carbon vs. Hydrogen track boundaries).
