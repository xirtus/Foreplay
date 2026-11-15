# Foreplay Game Engine (`v1.0.0` — Studio & Full Godot 4 Parity Edition)

**The world-class, AI-native Rust & Three.js (`3JSE`) game engine, Godot 4-compatible studio, 3D modeler, and clean-room port host for desktop, WebGPU, and the SexOS `sexlife` suite.**

Foreplay (`foreplay`) unites **19 Rust crates (`82,166 LoC`)** into one cohesive creative studio:

1. **Virtually Complete Godot 4 Rust Runtime (`foreplay-core`, `foreplay-render`, `foreplay-physics`, `foreplay-audio`, `foreplay-platform`, `foreplay-gdscript`, `foreplay-gdext`, `foreplay-player`):**
   - **Hardware `wgpu` `30.0.2` GPU Backend:** Real Metal / Vulkan / DX12 / WebGPU Clustered Forward+ PBR, 1024x1024 PCF directional shadow maps, 64-bone GPU skeletal skinning, WGSL GPU compute particles, **AgX** filmic tonemapping, `image` trilinear mipmap chains, and 2D `CanvasItem` batching with **2D Signed Distance Field (`SdfLightField2D`) lights & occluder shadows**.
   - **Production `rapier2d` & `rapier3d` `0.36.1` Physics + Live Port Hotswap:** Rigid/Kinematic/Fixed bodies, CCD, island sleeping, 6-DOF/hinge/slider motor joints, raycasts/shapecasts (`PhysicsDirectSpaceState`), live mid-air `Rapier3D <-> PortFaithful` state hotswapping, and Godot 4-faithful `CharacterBody2D/3D::move_and_slide` (stair-stepping, slope constant speed, floor snap, moving platform carryover).
   - **Real-Time `cpal` OS Audio, `symphonia` Multi-Codec Decoding & `gilrs` InputMap:** Lock-free atomic SPSC ringbuffer hardware audio (`cpal 0.18.2`), WAV/Ogg Vorbis/FLAC/MP3/PCM decoding (`symphonia 0.6.1`), 3D Doppler/equal-power spatial acoustics, `gilrs 0.11.2` gamepads + rumble, and Godot 4 `InputMap`.
   - **471-Class Godot 4.4 `ClassDB`, Rigged `.glb`, `.tres` & `cosmic-text` UI:** Canonical 471-class hierarchy, binary glTF 2.0 (`.glb`) skinned character & animation importer (`gltf 1.4.1`), `Skeleton3D` + `AnimationTree` (`StateMachine`, `BlendSpace1D/2D`), `cosmic-text` `0.19.0` Unicode shaping + `RichTextLabel` BBCode (`[wave]`, `[shake]`), `TileMapLayer` Wang/bitmask terrain autotiling, `.tscn`/`.tres` codecs, and threaded `ResourceLoader`.
   - **Full GDScript 2.0 + LSP + `GDExtension` C ABI:** Suspendable `await` coroutines, lambdas/`Callable`, `super()`, structural `match` guards, typed `Array[T]`/`Dictionary[K,V]`, **444 built-in methods**, real-time `GdLspAnalyzer` autocomplete/hover/goto, and **82 Godot 4.4 `GDExtension` C-ABI functions** (`foreplay-gdext`).
   - **Standalone `foreplay-player` & Self-Contained `FPCKEXEC` Native Exports:** Standalone runtime (`winit` + `wgpu` + `cpal` + `rapier`), Godot-style `pck_embed` self-contained native executables (`FPCKEXEC`), and loopback TCP JSON-Lines **Remote Play-In-Editor (PIE) IPC Debugger**.
2. **Dual Native Game Targets — Rust (`Bevy 0.20 + wgpu + SexOS`) AND Three.js (`3JSE` WebGPU + Vite + TypeScript) (`foreplay-threejs`):** Compile any `IrScene` to native desktop/SexOS binaries OR a runnable **Three.js + Vite + TypeScript (`3JSE`)** web project with bidirectional `3jse.scene.json` / `THREE.Object3D` round-tripping.
3. **Built-In Blender-Class 3D Modeling & Path Tracer (`foreplay-polyamour`):** Topological `HalfEdgeMesh`, Blender Edit-Mode ops, 9-modifier stack (`Mirror`, `Catmull-Clark`, `Boolean`, etc.), `.blend`/glTF/OBJ/STL/PLY I/O, `polyops 1` (`import bpy`) atomic scripting, and SAH BVH GGX + AgX path tracing.
4. **Apple Logic Pro & Final Cut Pro "Studio Obsidian & Chroma" UX (`foreplay-editor`):** 8 workspaces (`3D`, `2D`, `Model`, `Script`, `Three.js`, `Atlas`, `Port Diff`, `AssetLib`), hardware `wgpu` viewport rendering, smoked-glass Studio LCD HUD, 270° rotary `FeelSpec` Smart Controls, 32-band FFT EQ, FCP Magnetic Director Timeline, GDScript LSP gutter breakpoints, and Remote PIE Debugger.
5. **3JSE AI Harness & `rustports.org` Clean-Room Porting (`foreplay-atlas`, `foreplay-harness`, `foreplay-porthost`, `foreplay-net`, `foreplay-verifier`):** Invertible `EditorCommand` Undo/Redo, 600-frame Time-Travel Atlas scrubbing, JSON-RPC 2.0 `mcp-stdio` agent server, `port://` BYO-ROM mounts, 1,000-frame Differential Port Oracle, `foreplay-server` multiplayer, and **42 independent adversarial `proptest` suites**.

---

## Copyleft Licensing Model (No MIT Giveaway, Commercial Game Friendly)

- **Studio IDE, 3JSE Compiler, AI Harness, Atlas, Exporter, Verifier & Porting Suite (`GPL-3.0-only`):** `foreplay-editor`, `foreplay-threejs`, `foreplay-harness`, `foreplay-atlas`, `foreplay-porthost`, `foreplay-export`, and `foreplay-verifier`.
- **Engine Runtime, Player, GDExtension & Procedural 3D Crates (`MPL-2.0`):** `foreplay-ir`, `foreplay-core`, `foreplay-polyamour`, `foreplay-platform`, `foreplay-render`, `foreplay-physics`, `foreplay-audio`, `foreplay-net`, `foreplay-script`, `foreplay-gdscript`, `foreplay-gdext`, and `foreplay-player` (file-level strong copyleft compatible with GPL-3.0 while allowing commercial closed-source games to statically link unmodified runtime crates).

---

## Quick Start

```bash
# Run all 438 unit, integration & 42 adversarial proptest suites across all 19 crates
cargo test --workspace

# Run all 8 automated end-to-end verification gates (~380ms)
cargo run -p foreplay-harness -- verify-all-gates

# Launch the interactive desktop Studio Obsidian & Chroma IDE (egui + egui_dock + hardware wgpu)
cargo run -p foreplay-editor -- --gui

# Run headless smoke verification across all 8 IDE workspaces, 7 bottom docks & 5 modals
cargo run -p foreplay-editor

# Run standalone native game player comprehensive hardware + TCP PIE IPC + FPCKEXEC self-test
cargo run -p foreplay-player -- --self-test

# Start JSON-RPC 2.0 MCP stdio server for AI coding agents
cargo run -p foreplay-harness -- mcp-stdio

# Run dedicated headless multiplayer server self-test
cargo run -p foreplay-net --bin foreplay-server -- --self-test
```

## Key Documents

- [`PLAN.md`](PLAN.md) — Phases 0–16 master roadmap, crate DAG, C++-to-Rust idiom map, and verified exit gates.
- [`HARNESS.md`](HARNESS.md) — 3JSE-style Agent API surface, `FeelSpec` specification, and Carbon/Hydrogen verification gates.
- [`AGENTS.md`](AGENTS.md) — Multi-agent operating rules (Carbon vs. Hydrogen track boundaries).
