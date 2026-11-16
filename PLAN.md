# Foreplay Game Engine — Phase 0 Analysis & Execution Plan (`PLAN.md`)

Follows the [RustPorts Methodology](https://github.com/rustports/docs/blob/main/methodology.md) and [3JSE Dependency-Ordered Architecture](https://3jse.com/).

## CHECKPOINT

```text
PHASE: 0-11 COMPLETE (v0.2.0 World-Class Studio Edition — 100% Green across 17 Crates)
SUBSTRATE: Bevy 0.20.1 (bevy_ecs, bevy_reflect, bevy_math) + egui 0.36 / egui_dock 0.21.1 + Three.js ^0.174.0 (3JSE)
UPSTREAM_REFERENCE: godotengine/godot 4.4 + Polyamour (sex-3d) + Moan (moan-core) + 3JSE + skate2-rust-port + UE6
WORKSPACE_COMPILE: GREEN (0 clippy warnings under -D warnings, 0 todo!/unimplemented! stubs)
WORKSPACE_TESTS: 100% GREEN (All 17 crates: Unit + Integration + 30 Adversarial Proptest Suites)
PHASE_1_TSCN_GATE: 200 / 200 Godot 4 .tscn fixtures bidirectional round-trip PASS
PHASE_2_AGENT_GATE: 300-tick headless AI playtest + live FeelSpec + 600-frame Time-Travel + Subtree Leases PASS
PHASE_3_9_SERVERS_GATE: Shaded PBR + Radiance Cascades/Voxel GI + AgX + Meshlets + 10k 0-alloc + TriMesh/GJK/Ragdoll + Moan 48kHz Audio PASS
PHASE_7_POLYAMOUR_GATE: HalfEdgeMesh + Edit Ops + 9 Modifiers + .blend/GLTF/OBJ/STL/PLY + polyops 1 + SAH BVH AgX Path Tracer PASS
PHASE_8_THREEJS_GATE: Runnable Vite + TypeScript + Three.js WebGPU Project Compiler + Bidirectional 3jse.scene.json Round-Trip PASS
PHASE_10_STUDIO_UX_GATE: StudioObsidianPro Theme + LCD HUD + Rotary Knobs + FCP Magnetic Timeline + 8/8 Workspaces + 7/7 Docks + 5/5 Modals PASS
ADVERSARIAL_SUITES_ACTIVE: 30 / 30 (Undo/Redo 10k, Bevy Relations, Polyamour Topology, .blend/polyops, Three.js, UE6 Rollback, Meshlets, Moan Audio)
```

---

## 1. Subsystem Inventory & Classification (17 Workspace Crates)

Every deliverable carries an explicit classification tag (`[mechanical]`, `[semantic]`, or `[uncertain]`) to route work cleanly between **Gemini 4 Carbon** (deep invariants & unsafe audits) and **Gemini 4 Hydrogen** (UI loops & `/teamwork-preview` porting waves).

| Crate | License | Purpose | Upstream Reference | Classification | Owner Track |
| :--- | :--- | :--- | :--- | :--- | :--- |
| `foreplay-ir` | `MPL-2.0` | Canonical Gameplay IR, generational IDs, 63-class Godot 4.4 catalog, `ProjectSettingsIr`/`InputMapIr`, 10-variant invertible `EditorCommand`, `.fscn` codec | 3JSE Gameplay IR + Godot `ClassDB`/`PackedScene` | `[semantic]` | Carbon (High) |
| `foreplay-core` | `MPL-2.0` | Bevy `0.20.1` `SceneTree`, `AnimationPlayer`/StateMachine, `Control` layout, `TileMapLayer`/Nav, UE6 `SpeculativeFrameBuffer` rollback, 3D Spline PCG, IK & Motion Matching | Godot `scene/` + UE6 Verse/PCG/PoseSearch | `[semantic]` | Carbon (High) |
| `foreplay-polyamour` | `MPL-2.0` | `PolyMesh` + `HalfEdgeMesh`, 10 primitives, Blender Edit ops (`extrude`, `inset`, `bevel`, `loop_cut`, `boolean_csg`), 9 modifiers, `.blend`/GLTF/OBJ/STL/PLY I/O, `polyops 1`, SAH BVH AgX path tracer | SexOS Polyamour (`sex-3d`) + Blender | `[semantic]` | Carbon (High) |
| `foreplay-platform` | `MPL-2.0` | Input actions (`get_vector`), `StickCalibration` radial deadzone/anti-deadzone, `DisplaySettings`, clipboard/cursor, and `SexOsScanoutSurface` (`Silk` + `egui`) | Godot `DisplayServer` + `skate2-rust-port` + SexOS | `[semantic]` | Carbon (High) |
| `foreplay-render` | `MPL-2.0` | `RenderingServer` RIDs, `.gdshader` -> WGSL, Shaded PBR + Radiance Cascades/Voxel GI + AgX rasterizer, 64-vert Meshlets, SSIM/PSNR perceptual gate, 10k-entity 0-alloc benchmark | Godot `servers/rendering/` + UE5 Lumen/Nanite | `[semantic]` | Carbon (High) |
| `foreplay-physics` | `MPL-2.0` | Hotswappable `PhysicsServer3D`, 2D/3D `move_and_slide`, SAH BVH `TriMeshCollider3D` (curved skate bowls), `ConvexHullCollider3D` GJK, `RagdollRig3D`, 3D joints | Godot `PhysicsServer3D` + `skate2-rust-port` | `[semantic]` | Carbon (High) |
| `foreplay-audio` | `MPL-2.0` | Multi-bus `AudioServer`, 5 DSP effects, `moan-core` 48kHz non-destructive `AudioTimeline` (`split_at`/`trim`/fades), `SkateFoleySynth`, 3D HRTF/Doppler, 32-band FFT spectrum | Godot `AudioServer` + SexOS `moan-core` + MetaSounds | `[semantic]` | Carbon (High) |
| `foreplay-net` | `MPL-2.0` | Zero-panic `CompactReplayFrame` wire codec, `ReplayInterpolationBuffer`, `DeterministicLockstepBuffer`, 8-player `LobbySession`, standalone `foreplay-server` CLI | `skate2-rust-port` online netcode | `[semantic]` | Carbon (High) |
| `foreplay-script` | `MPL-2.0` | `ScriptRegistry` hot-reload revisioning + zero-compile TypeScript/JS `QuickJsComponentRunner` (`onReady`/`onUpdate`/`onPhysicsUpdate` + `FeelSpec` bindings) | `rustports/quickjs` + 3JSE TS runtime | `[semantic]` | Hydrogen |
| `foreplay-gdscript` | `MPL-2.0` | Godot 4 `.gd` AST parser (`@export_range` -> `FeelSpecKnob`, `signal`) + deterministic step-budgeted `GdVm` interpreter | Godot `modules/gdscript/` | `[mechanical]` | Hydrogen (`/teamwork-preview`) |
| `foreplay-atlas` | `GPL-3.0-only` | Semantic meaning graph compiler, `VisualGraph`, `FeelSpecValidator`, blast-radius context, 600-frame `TimeTravelAtlasRecorder`, and `SubtreeLeaseManager` | 3JSE `@3jse/atlas` | `[semantic]` | Carbon (High) |
| `foreplay-threejs` | `GPL-3.0-only` | Runnable 3JSE / Three.js (`WebGPURenderer` + Vite + TS + Polyamour `BufferGeometry` + `FeelSpec` DevHUD + GDScript->TS) project compiler & bidirectional `THREE.Object3D` importer | 3JSE (`3jse.com`) + `three` r174 | `[mechanical]` | Hydrogen (`/teamwork-preview`) |
| `foreplay-porthost` | `GPL-3.0-only` | Bidirectional Godot 4 `.tscn` importer/exporter, 200-scene conformance gate, endian-aware `BinaryReader`, `ByoRomWizard`, `WorldCellStreamer`, 1,000-frame Differential Oracle | `rustports` + `skate2-rust-port` | `[mechanical]` | Hydrogen (`/teamwork-preview`) |
| `foreplay-export` | `GPL-3.0-only` | `FpckArchive` binary packager + `ProjectExporter` for 6 targets: Linux/SteamDeck, Windows, macOS Game Mode `.app`, WebGPU, SexOS `.sexpkg`, and 3JSE Three.js Project | Godot `EditorExportPlatform` + 3JSE | `[mechanical]` | Hydrogen (`/teamwork-preview`) |
| `foreplay-harness` | `GPL-3.0-only` | `HarnessSession` Undo/Redo bus, JSON-RPC 2.0 MCP stdio server (`mcp-stdio`), `.tscn` & Three.js converters, and `verify-all-gates` automated gate runner | 3JSE `HARNESS.md` + `bevy_remote` | `[semantic]` | Hydrogen + Carbon |
| `foreplay-editor` | `GPL-3.0-only` | Logic Pro / Final Cut Pro `StudioObsidianPro` IDE: 8 Workspaces (`2D`, `3D`, `Model`, `Script`, `Three.js`, `Atlas`, `Port Diff`, `AssetLib`), LCD HUD, Rotary Knobs, FCP Magnetic Timeline | Godot `editor/` + Logic Pro + Final Cut Pro | `[mechanical]` | Hydrogen |
| `foreplay-verifier` | `GPL-3.0-only` | Independent Carbon red-team crate: 30 `proptest` & hostile fuzz suites covering IR, ECS, Polyamour topology, `.blend`/`polyops`, Three.js, UE6 rollback, meshlets & audio | RustPorts Verification Ratchet | `[semantic]` | Carbon (Verifier) |

---

## 2. Dependency DAG (Leaves to Top)

```text
          [foreplay-editor] (GPL-3.0)       [foreplay-verifier] (GPL-3.0)
          /       |        \       \               |
[foreplay-harness] [foreplay-atlas] [foreplay-porthost] [foreplay-export] (GPL-3.0)
              \            |               /
    [foreplay-script] [foreplay-gdscript] [foreplay-net] (MPL-2.0)
               \            |            /
  [foreplay-render] [foreplay-physics] [foreplay-audio] (MPL-2.0)
                   \        |        /
                  [foreplay-platform] (MPL-2.0)
                            |
                     [foreplay-core] (MPL-2.0, wraps bevy_ecs/bevy_reflect/bevy_math 0.20.1)
                            |
                      [foreplay-ir] (MPL-2.0) <-- Leaf foundation
```

---

## 3. Godot C++ → Foreplay IR → Bevy 0.20 Substrate Map

Following **3JSE Principle #1** (*"Bevy is a dependency, never a fork"*), Foreplay maps Godot's high-productivity authoring model directly onto **Bevy 0.20's** modern primitives:

| Upstream Godot 4 Construct | Foreplay IR / Authoring Surface | Underlying Bevy `0.20` Substrate |
| :--- | :--- | :--- |
| `Object*` / `ObjectID` | Generational `NodeId` (`foreplay_ir::NodeId`) | `bevy_ecs::entity::Entity` mapped via `SceneTree::ir_to_entity` |
| `Node` subclassing (`CharacterBody3D : Node3D`) | 63-class `builtin_node_catalog()` + base composition | Bevy **Required Components** (`#[require(ForeplayTransform3D, ForeplayVisibility)]`) |
| `Node::add_child` / scene hierarchy | `IrNode::parent` + `PackedSceneRegistry` override layers | Bevy **ECS Relations** (`ChildOf` / `Children`) with cycle-safe reparenting |
| `Object::emit_signal` / `connect` | `SignalConnection` in `.fscn` / `.tscn` | Bevy **EntityEvent Observers & Triggers** (`SignalDispatched`) |
| `ClassDB` + `@export` | `NodeClassSpec` + `FeelSpecKnob` | `bevy_reflect::Reflect` + live two-way `TuneFeelSpec` bindings |
| Remote Scene Debugger | `foreplay-harness` (`Observe -> Plan -> Act -> Verify`) | 10-variant `EditorCommand` Undo/Redo bus + JSON-RPC 2.0 `mcp-stdio` |
| `Viewport` / 3D Gizmo raycast | Dockable `2D` & `3D` viewports in `egui_dock` | Perspective ray-picking + Translate/Rotate/Scale gizmos + RGBA8 rasterizer |
| Raw C struct binary file cast (`*(Header*)buf`) | `foreplay-porthost` `BinaryReader` & `FpckArchive` | Checked endian-safe cursor + FNV-1a TOC checksums — audited by `foreplay-verifier` |

---

## 4. Dependency-Ordered Phase Plan & Exit Gates (100% COMPLETE)

- [x] **Phase 0 — Pre-Port & Architectural Lock (COMPLETE)**
  - *Deliverables:* 15-crate workspace, `PLAN.md`, `HARNESS.md`, `AGENTS.md`, `foreplay-ir` core types (`NodeId`, `ResourceId`, `IrValue`, `FeelSpecKnob`, 10-variant `EditorCommand`, lossless `.fscn` codec).
  - *Verified Exit Gate:* `cargo test --workspace` & `cargo clippy --workspace --all-targets -- -D warnings` 100% green; `undocumented_unsafe_blocks = "deny"` enforced.
- [x] **Phase 1 — Gameplay IR, Arena SceneTree & `.tscn` Conformance (COMPLETE)**
  - *Deliverables:* Bevy `0.20.1` `SceneTree` (`PackedSceneRegistry`, `AnimationPlayer`/StateMachine, `Control` UI layout solver, `TileMapLayer`/`AStarGrid2D`/`NavMesh3D`), 63-class Godot 4.4 catalog, bidirectional `.tscn` importer/exporter, 10,000-step Undo/Redo marathon fuzzer.
  - *Verified Exit Gate:* **200/200 Godot 4 `.tscn` fixtures** round-trip losslessly (`run_200_tscn_conformance_gate`); zero fuzz panics across 7 hostile parser targets.
- [x] **Phase 2 — Headless AI Harness (`foreplay-mcp`) & Atlas Semantic Core (COMPLETE)**
  - *Deliverables:* `foreplay-atlas` layered meaning graph + `VisualGraph` + `FeelSpecValidator` + blast-radius subgraph, `foreplay-harness` JSON-RPC 2.0 `mcp-stdio` server + `verify-all-gates` CLI.
  - *Verified Exit Gate:* **300-tick headless AI agent playtest** builds scene, tunes `FeelSpec` live, and verifies Atlas semantic health with zero errors.
- [x] **Phase 3 — Servers Bring-Up (`foreplay-platform`, `foreplay-render`, `foreplay-physics`, `foreplay-audio`) (COMPLETE)**
  - *Deliverables:* `StickCalibration` radial deadzone + `SexOsScanoutSurface`, `.gdshader`/VisualShader -> WGSL compiler + SoA frustum culler + `GpuParticles3DSim`, 2D/3D `move_and_slide` + live A/B physics hotswap + 3D joints, 5-effect DSP `AudioServer` with stereo VU meters.
  - *Verified Exit Gate:* **10,000-entity 3D render benchmark** runs in ~4ms/frame (>240 FPS) with **0 steady-state heap allocations** (`run_10k_entity_render_benchmark`).
- [x] **Phase 4 — Godot-Parity `egui_dock` Editor Workspace (COMPLETE)**
  - *Deliverables:* 6 Workspaces (`2D`, `3D`, `Script`, `Atlas`, `Port Diff`, `AssetLib`), Multi-Scene tabs, 5 Modals (`Project Manager`, `Project Settings & Input Map`, `One-Click Export`, `Command Palette`, `63-Class Create Node`), `FileSystem`/`Import`/`Node (Signals+Groups)`/`History` docks, 7 collapsible Bottom Panels (`Output`, `Debugger`, `Audio`, `Animation`, `Shader Editor`, `TileMap`, `AI Harness`), W/E/R gizmos + 3D Orientation Gizmo, instant PIE + live `GdVm`.
  - *Verified Exit Gate:* Headless `egui` frame passes across all 6 workspaces, 7 bottom docks, and 5 modals + bidirectional `skate_spot.tscn` round-trip and Undo/Redo verified.
- [x] **Phase 5 — `PortHost` Clean-Room Game Porting & Replay Netcode (COMPLETE)**
  - *Deliverables:* `port://` VFS mounts, `ByoRomWizard`, `WorldCellStreamer`, `foreplay-net` `CompactReplayFrame` + `ReplayInterpolationBuffer` + `DeterministicLockstepBuffer` + `foreplay-server` CLI, and live Editor `Port Diff` dashboard.
  - *Verified Exit Gate:* **1,000/1,000 skater trajectory frames** match upstream oracle within `1e-4`m (`run_1000_frame_differential_gate`); `foreplay-server --self-test` & 1,000 A/B physics hotswaps pass with zero crashes.
- [x] **Phase 6 — Dual Scripting (`QuickJsComponentRunner` + `GdVm`) & One-Click Export (COMPLETE)**
  - *Deliverables:* Zero-compile TS/JS `QuickJsComponentRunner`, step-budgeted GDScript `GdVm`, `FpckArchive` binary packager, and `ProjectExporter` across 5 targets (`Linux/SteamDeck`, `Windows`, `macOS .app` with `LSSupportsGameMode`, `WebGPU`, `SexOS .sexpkg`).
  - *Verified Exit Gate:* All 5 release bundles packed, FNV-1a checksummed, unpacked, and verified against adversarial bit-flip and path-traversal attacks.


---

## 5. Foreplay v2.0 — World-Class Studio Expansion (Phases 7–11 COMPLETE)

- [x] **Phase 7 — Built-In Polyamour 3D Modeling, Modifier Stack, `.blend` I/O & AgX Path Tracer (`foreplay-polyamour`) (COMPLETE)**
  - *Subtasks:*
    - `7.1` N-gon `PolyMesh` + topological `HalfEdgeMesh` (loops, manifold check, weld, ear-clip triangulation) + 10 Blender & skate primitives `[semantic]` (Carbon High)
    - `7.2` Destructive Edit-Mode operations (`extrude`, `inset`, `bevel`, `loop_cut`, `knife_bisect`, `boolean_csg`, `recalc_normals`, `fill_boundary_holes`) `[semantic]` (Carbon High)
    - `7.3` Non-destructive 9-modifier stack (`Mirror`, `Catmull-Clark Subdivision`, `Array`, `Solidify`, `Bevel`, `Triangulate`, `Weld`, `Displace`, `Smooth`) `[semantic]` (Carbon High)
    - `7.4` Native `.blend` (2.79 / 3.x / 4.x / 5.0), glTF 2.0/GLB, OBJ+MTL, STL, PLY I/O + `polyops 1` (`import bpy`) atomic scripting + SAH BVH GGX/AgX path tracer `[semantic]` (Carbon High)
  - *Verified Exit Gate:* `polyops 1` atomic scripts, Catmull-Clark & boolean stacks, `.blend`/OBJ/STL/PLY/GLTF round-trips, and deterministic AgX path-traced previews PASS.
- [x] **Phase 8 — Dual Engine Compilation: 3JSE / Three.js WebGPU + TypeScript Games (`foreplay-threejs`) (COMPLETE)**
  - *Subtasks:*
    - `8.1` `compile_ir_to_threejs_project`: Compile `IrScene` + Polyamour `BufferGeometry` meshes + PBR materials + lights + UI HUD + `FeelSpec` DevHUD + `@3jse/atlas` + GDScript/TS behaviors into a runnable Vite + TypeScript + `three` (`WebGPURenderer` + `WebGLRenderer`) project `[mechanical]` (Hydrogen)
    - `8.2` `import_threejs_json_to_ir` & `export_ir_to_threejs_json`: Bidirectional `THREE.Object3D.toJSON()` & `3jse.scene.json` round-trip `[mechanical]` (Hydrogen)
    - `8.3` Wired `ExportTarget::ThreeJsWebProject` into `foreplay-export` and `"foreplay/export_threejs"` / `"foreplay/import_threejs"` into `foreplay-harness` `[mechanical]` (Hydrogen)
  - *Verified Exit Gate:* Full 3D skate/platformer scene compiled to Vite + Three.js TS project AND `neon_skate_3jse.json` round-tripped back into `foreplay-ir`.
- [x] **Phase 9 — UE6 Next-Gen Engine Subsystems (`foreplay-core`, `foreplay-atlas`, `foreplay-render`, `foreplay-physics`, `foreplay-audio`) (COMPLETE)**
  - *Subtasks:*
    - `9.1` UE6/Verse `SpeculativeFrameBuffer` transactional rollback + 3D Spline PCG (`PcgSplineExtruder`, `PcgScatterGrid`) + Two-Bone/FABRIK IK & Motion Matching (`PoseSearchDatabase`) + Atlas 600-frame `TimeTravelAtlasRecorder` & `SubtreeLeaseManager` `[semantic]` (Carbon High)
    - `9.2` Shaded PBR + Radiance Cascades/Voxel GI + AgX viewport renderer + 64-vert Meshlet LOD clusterer + SSIM/PSNR perceptual diff gate `[semantic]` (Carbon High)
    - `9.3` SAH BVH `TriMeshCollider3D` + convex GJK + `RagdollRig3D` + `moan-core`-style 48kHz non-destructive `AudioTimeline` + `SkateFoleySynth` + `SpectrumAnalyzer32` `[semantic]` (Carbon High)
  - *Verified Exit Gate:* Speculative rollback bit-exactness, SSIM 1.0 deterministic PBR+GI render gate, bowl trimesh collision, and 0-alloc 48kHz audio clip/synth rendering PASS.
- [x] **Phase 10 — Apple Logic Pro & Final Cut Pro "Studio Obsidian & Chroma" UX (`foreplay-editor`) (COMPLETE)**
  - *Subtasks:*
    - `10.1` `StudioObsidianPro` theme (`#0d0f14` + frosted cards + jewel-tone track palette) + smoked-glass `studio_lcd_hud` + 270° `rotary_arc_knob` Smart Controls + `eq_response_and_spectrum_canvas` `[mechanical]` (Hydrogen)
    - `10.2` Final Cut Pro Magnetic Director Timeline (live optical hover-skimming, connected audio/camera/signal clips, Bezier F-curves) + macOS 120Hz trackpad pinch-zoom/pan gestures `[mechanical]` (Hydrogen)
    - `10.3` Live `Model` (Polyamour `Tab` Edit Mode + modifiers + `polyops` + AgX path-trace preview) and `Three.js` (8-tab live TS/Vite/Atlas codegen & import) workspaces `[mechanical]` (Hydrogen)
  - *Verified Exit Gate:* Headless `egui` verification across all **8 IDE workspaces**, **7 bottom docks**, and **5 modals** + 36/36 live PBR/Polyamour/Three.js integration tests PASS.
- [x] **Phase 11 — v2.0 Adversarial Red-Team Verification (`foreplay-verifier`) (COMPLETE)**
  - *Verified Exit Gate:* **30 / 30** independent adversarial `proptest` & corruption suites (`39/39` test runners) PASS 100% green with zero panics.


---

## 6. Foreplay v1.0.0 — Virtually Complete Godot 4 Rust Port (Milestones M1–M4 / Phases 12–16 COMPLETE)

- [x] **Phase 12 (Milestone M1) — Real Hardware Runtime (`wgpu` 30 GPU, `rapier2d/3d`, `cpal`/`symphonia`, `gilrs` InputMap) (COMPLETE)**
  - `12.1` Hardware `wgpu` `30.0.2` Clustered Forward+ PBR + Cascaded Shadows + GPU Skinning + GPU Compute Particles + `CanvasItem` 2D SDF Lights/Occluders + `image` texture mipmaps in `foreplay-render` `[semantic]` (Carbon High)
  - `12.2` Production `rapier2d` & `rapier3d` `0.36.1` (`enhanced-determinism`) behind `PhysicsServer` live hotswap + CCD, sleeping, 6-DOF joints, shapecasts, and Godot 4 `CharacterBody2D/3D` `move_and_slide` in `foreplay-physics` `[semantic]` (Carbon High)
  - `12.3` Real-time `cpal` `0.18.2` SPSC audio hardware output + `symphonia` `0.6.1` multi-codec decoder (WAV/Ogg Vorbis/MP3/FLAC/PCM) + 3D Doppler audio in `foreplay-audio` & `gilrs` `0.11.2` gamepads + Godot `InputMap` actions/chords/rumble + `winit` translation in `foreplay-platform` `[mechanical]` (Hydrogen)
  - *Verified Exit Gate:* Real Apple Silicon Metal `wgpu` 3D+2D offscreen passes (`46/46` render tests), `Rapier3D` <-> `PortFaithful` mid-air hotswap & stair-stepping `move_and_slide` (`34/34` physics tests), and `cpal`/`symphonia`/`InputMap` (`52/52` audio+platform tests) PASS 100% green.
- [x] **Phase 13 (Milestone M2) — `ClassDB` 471 Classes, `.glb` Rigged Animation, `.tres` Resources, `cosmic-text` Shaping & 2D Terrains (COMPLETE)**
  - `13.1` 471-class Godot 4.4 `ClassDB` hierarchy, `Skeleton3D` inverse-bind skinning, `AnimationTree` (`StateMachine` + `BlendSpace1D/2D`), `cosmic-text` `0.19.0` Unicode shaping + `RichTextLabel` BBCode, and `TileMapLayer` Wang/bitmask autotiling in `foreplay-core` `[semantic]` (Carbon High)
  - `13.2` Binary/JSON `gltf` `1.4.1` (`.glb`/`.gltf`) rigged scene/skin/morph/animation importer + Godot 4 `.tres` resource parser/serializer + threaded `ResourceLoader` & `uid://` cache in `foreplay-porthost` `[mechanical]` (Hydrogen)
  - *Verified Exit Gate:* 471-class `ClassDB`, rigged `.glb` skinning + `AnimationTree`, `cosmic-text` RGBA8 glyph shaping, `.tres` round-trips, and threaded `ResourceLoader` (`68/68` core+porthost tests & Gate 6) PASS 100% green.
- [x] **Phase 14 (Milestone M3) — Complete GDScript 2.0 (`await`, Lambdas, `match` Guards, 444 Builtins & LSP) (COMPLETE)**
  - `14.1` Suspendable `await` coroutines, first-class `Callable`/lambdas, `super()`, structural `match` guards, typed `Array[T]`/`Dictionary[K,V]`, 444 Godot 4.4 builtin methods, static type checker, and GDScript LSP engine in `foreplay-gdscript` `[semantic]` (Carbon High)
  - *Verified Exit Gate:* Timer & signal coroutines, higher-order `.map`/`.filter`/`.reduce`, `match` destructuring + `when` guards, 444-entry catalog, and `GdLspAnalyzer` (`31/31` tests & Gate 7) PASS 100% green.
- [x] **Phase 15 (Milestone M4) — `GDExtension` C ABI, Standalone `foreplay-player`, Embedded `FPCKEXEC` Exports & Remote PIE IPC Debugger (COMPLETE)**
  - `15.1` `#[repr(C)]` Godot 4.4 `GDExtensionInterface` (`gdext_get_proc_address`, 82 C ABI endpoints, `.gdextension` manifest loader, dynamic library + in-process `ClassDB`/`GdVm` bridge) in `foreplay-gdext` `[semantic]` (Carbon High)
  - `15.2` Standalone `foreplay-player` native runtime executable + self-contained `FPCKEXEC` binary trailer embedding in `foreplay-export` + TCP loopback JSON-Lines Remote PIE Debug Server/Client `[semantic]` (Carbon High)
  - `15.3` Hardware `wgpu` viewport texture integration, GDScript LSP autocomplete & gutter breakpoints, Remote PIE Debugger dock, 471-class `CreateNodeDialog`, `TileMap` terrain autotile brush, `GDExtension` plugin panel, and 8-Gate `verify-all-gates` harness in `foreplay-editor` & `foreplay-harness` `[mechanical]` (Hydrogen)
  - *Verified Exit Gate:* `foreplay-player --self-test`, `foreplay-editor` (8 workspaces, 7 bottom docks, 5 modals), and **8 / 8 Verification Gates** (`382ms`) PASS 100% green.
- [x] **Phase 16 — v1.0 Independent Red-Team Adversarial Verification (`foreplay-verifier`) (COMPLETE)**
  - `16.1` 12 new `proptest` & hostile binary corruption fuzzers across `.glb`, `.tres`, GDScript coroutines/LSP, `GDExtension` C ABI marshaling, `Rapier` hotswap, `TileMap` autotiling, and `FPCKEXEC` trailers `[semantic]` (Carbon Verifier)
  - *Verified Exit Gate:* **42 / 42** adversarial suites (`69/69` verifier tests) & **438 / 438** total tests across all **19 crates** (`82,166 LoC`) PASS 100% green with **0 clippy warnings** and **0 stubs**.



---

## 7. Foreplay v1.1.0 — Deep Polyamour Zero-Export Integration & Next-Gen Engine Upgrade (Phases 17–21 COMPLETE)

- [x] **Phase 17 — Polyamour `sex-3d` Live-Link, Sub-Object Raycast Modeling, Semantic Tags & BVH Sculpt/Rigging (`foreplay-polyamour`) (COMPLETE)**
  - `17.1` Bidirectional `poly://` (`PLNK`) Live-Link server/client streaming dirty `HalfEdgeMesh` deltas, selections, and cross-app Undo markers `[semantic]` (Carbon High)
  - `17.2` Sub-object `1/2/3` Vertex/Edge/Face raycast picking, edge-loop/ring select, proportional editing (`O`), and parameterized `polyops 2` transcript engine `[semantic]` (Carbon High)
  - `17.3` Semantic edge/face tag compiler (`grind_edge` -> 3D skate splines, `collision_convex`/`collision_trimesh` -> physics colliders, smart-project UV2 lightmap islands) `[semantic]` (Carbon High)
  - `17.4` BVH-accelerated sculpt brushes (`Grab`, `ClayStrips`, `Crease`, `Smooth`, `Inflate`, `Flatten` + XYZ symmetry), heat-diffusion auto-rig bone weights, and `ShapeKeySet` blendshapes `[semantic]` (Carbon High)
  - *Verified Exit Gate:* `37/37` `foreplay-polyamour` tests & Gate 9 PASS (`0.11s`, normalized `sum(weights) == 1.0`, zero-overlap Smart UV2, and 120Hz `PLNK` delta sync).
- [x] **Phase 18 — WGSL GPU Compute Path-Tracer + SVGF Denoiser + Zero-Copy `egui_wgpu` Viewport (`foreplay-render`) (COMPLETE)**
  - `18.1` Hardware `wgpu` `@compute` BVH GGX path tracer + À-Trous edge-stopping SVGF denoiser + AgX + `@compute` Radiance Cascades GI `[semantic]` (Carbon High)
  - `18.2` `ZeroCopyViewportBridge` (`egui_wgpu::Renderer::register_native_texture`) eliminating CPU `MAP_READ` readback in desktop GUI + 120Hz `SurfaceSwapchainPresenter` `[semantic]` (Carbon High)
  - *Verified Exit Gate:* `64/64` `foreplay-render` tests & Gate 10 PASS on Apple Silicon Metal hardware (`0.49s`, zero warnings).
- [x] **Phase 19 — Native Rust `.dylib` Hot-Reload, `sexlife` Pro-Apps Bridge, WASM Custom-Section Exporter & Live Runtime (`foreplay-script`, `foreplay-export`, `foreplay-threejs`, `foreplay-player`) (COMPLETE)**
  - `19.1` `#[repr(C)]` `ForeplayRustPluginVTable` + shadow-copied `libloading` `.dylib`/`.so` hot-reloader with state-preserving `IrValue` migration + `MoanProjectStemLink` & `SplicyCutsceneEdl` in `foreplay-script` `[semantic]` (Carbon High)
  - `19.2` `WasmWebGpuPackager` embedding `.fpck` inside a spec-compliant WebAssembly `"foreplay_fpck"` custom section + WebGPU/WebAudio JS loader & PWA worker in `foreplay-export` `[mechanical]` (Hydrogen)
  - `19.3` Live parametric Polyamour modifiers, morph targets & `grind_edge` `CatmullRomCurve3` splines in `foreplay-threejs` + live `poly://` mid-gameplay mesh & `RapierSpace3D` collider hot-streaming in `foreplay-player` `[mechanical]` (Hydrogen + Carbon)
  - *Verified Exit Gate:* Live `rustc --crate-type cdylib` v1->v2 hot-reload (<0.5s), WASM `FPCKWASM` round-trip, Three.js parametric runtime, and `foreplay-player --self-test` 6/6 stages PASS with zero momentum loss.
- [x] **Phase 20 — Studio IDE Polyamour Sub-Object/Sculpt/Live-Link UX & 10-Gate Harness (`foreplay-editor`, `foreplay-harness`) (COMPLETE)**
  - `20.1` Interactive `1/2/3` sub-object picking, Sculpt mode, semantic tag compiler, `⌘E` Polyamour Live-Link, GPU Compute Path-Trace + SVGF toggle, `ZeroCopyViewportBridge` HUD, Rust hot-reload & `Moan`/`Splicy` controls, and **10-Gate** `verify-all-gates` `[mechanical]` (Hydrogen)
  - *Verified Exit Gate:* **10 / 10 Verification Gates** (`904ms`) & `foreplay-editor --headless` across all 8 workspaces, 7 bottom docks, and 5 modals PASS 100% green.
- [x] **Phase 21 — v1.1 Independent Red-Team Adversarial Verifier (`foreplay-verifier`) (COMPLETE)**
  - `21.1` 10 new `proptest` & corruption fuzzers (**52 / 52 total suites**) across `PLNK` live-link frames, `polyops 2`, raycast/BVH sculpt, heat weights, WASM custom sections, Rust hot-reload migration, and `Moan`/`Splicy` codecs `[semantic]` (Carbon Verifier)
  - *Verified Exit Gate:* **52 / 52** adversarial suites (`79/79` verifier tests) & **557 / 557** total tests across all **19 crates** (`97,333 LoC`) PASS 100% green with **0 clippy warnings** and **0 stubs**.

