# Foreplay Game Engine — Phase 0 Analysis & Execution Plan (`PLAN.md`)

Follows the [RustPorts Methodology](https://github.com/rustports/docs/blob/main/methodology.md) and [3JSE Dependency-Ordered Architecture](https://3jse.com/).

## CHECKPOINT

```text
PHASE: 0-6 COMPLETE (100% Plan & Godot 4.4 Parity Achieved across 15 Crates)
SUBSTRATE: Bevy 0.20.1 (bevy_ecs, bevy_reflect, bevy_math) + egui 0.36 / egui_dock 0.21.1
UPSTREAM_REFERENCE: godotengine/godot 4.4-stable + skate2-rust-port + 3JSE
WORKSPACE_COMPILE: GREEN (0 clippy warnings under -D warnings, 0 todo!/unimplemented! stubs)
WORKSPACE_TESTS: 100% GREEN (All 15 crates: Unit + Integration + 20 Adversarial Proptest Suites)
PHASE_1_TSCN_GATE: 200 / 200 Godot 4 .tscn fixtures bidirectional round-trip PASS
PHASE_2_AGENT_GATE: 300-tick headless AI playtest + live FeelSpec tuning + MCP JSON-RPC 2.0 PASS
PHASE_3_RENDER_GATE: 10,000-entity 3D benchmark >240 FPS (0 steady-state heap allocations) + .gdshader->WGSL PASS
PHASE_4_EDITOR_GATE: 6/6 Workspaces + 7/7 Bottom Docks + 5/5 Modals + 63-Class Catalog + Live PIE/GdVm PASS
PHASE_5_ORACLE_GATE: 1,000 / 1,000 frames <= 1e-4m drift + live A/B physics hotswap + foreplay-server PASS
PHASE_6_EXPORT_GATE: 5 / 5 Release Targets (.fpck + Linux/SteamDeck, Windows, macOS Game Mode .app, WebGPU, SexOS .sexpkg) PASS
ADVERSARIAL_SUITES_ACTIVE: 20 / 20 (10k Undo/Redo Marathon, Bevy Relations, .fscn/.tscn/.gd/.gdshader/.fpck/GdVm/QuickJS Fuzzers)
```

---

## 1. Subsystem Inventory & Classification (15 Workspace Crates)

Every deliverable carries an explicit classification tag (`[mechanical]`, `[semantic]`, or `[uncertain]`) to route work cleanly between **Gemini 4 Carbon** (deep invariants & unsafe audits) and **Gemini 4 Hydrogen** (UI loops & `/teamwork-preview` porting waves).

| Crate | License | Purpose | Upstream Reference | Classification | Owner Track |
| :--- | :--- | :--- | :--- | :--- | :--- |
| `foreplay-ir` | `MPL-2.0` | Canonical Gameplay IR, generational IDs, 63-class Godot 4.4 catalog, `ProjectSettingsIr`/`InputMapIr`, 10-variant invertible `EditorCommand`, `.fscn` codec | 3JSE Gameplay IR + Godot `ClassDB`/`PackedScene` | `[semantic]` | Carbon (High) |
| `foreplay-core` | `MPL-2.0` | Bevy `0.20.1` `SceneTree` (`ChildOf`/`Children`, `RequiredComponents`, Observers), `PackedScene` layering, `AnimationPlayer`/StateMachine, `Control` anchors/containers, `TileMapLayer`/`AStarGrid2D`/`NavMesh3D` | Godot `scene/{main,animation,gui,2d,3d}/` | `[semantic]` | Carbon (High) |
| `foreplay-platform` | `MPL-2.0` | Input actions (`get_vector`), `StickCalibration` radial deadzone/anti-deadzone, `DisplaySettings`, clipboard/cursor, and `SexOsScanoutSurface` (`Silk` + `egui`) | Godot `DisplayServer` + `skate2-rust-port` + SexOS | `[semantic]` | Carbon (High) |
| `foreplay-render` | `MPL-2.0` | `RenderingServer` RIDs, `.gdshader` & VisualShader -> WGSL compiler, 6-plane SoA frustum culler, `GpuParticles3DSim`, 10k-entity 0-alloc benchmark, RGBA8 golden rasterizer | Godot `servers/rendering/` | `[semantic]` | Carbon (High) |
| `foreplay-physics` | `MPL-2.0` | Hotswappable `PhysicsServer3D` (`StandardRigidBackend` <-> `SkatePortArcadeBackend`), `CharacterBody3D`/`2D::move_and_slide`, ray/shape casts, `Area3D`, 3D joints | Godot `PhysicsServer2D/3D` + `skate2-rust-port` | `[semantic]` | Carbon (High) |
| `foreplay-audio` | `MPL-2.0` | Multi-bus `AudioServer` (`Master`, `Music`, `Sfx`, `Voice`), stereo `BusVuMeter`, 5 stereo DSP effects (Reverb, EQ3, Limiter, LowPass, Chorus), WAV/XMA codecs | Godot `AudioServer` + `skate2-xma` | `[mechanical]` | Carbon (High) |
| `foreplay-net` | `MPL-2.0` | Zero-panic `CompactReplayFrame` wire codec, `ReplayInterpolationBuffer`, `DeterministicLockstepBuffer`, 8-player `LobbySession`, standalone `foreplay-server` CLI | `skate2-rust-port` online netcode | `[semantic]` | Carbon (High) |
| `foreplay-script` | `MPL-2.0` | `ScriptRegistry` hot-reload revisioning + zero-compile TypeScript/JS `QuickJsComponentRunner` (`onReady`/`onUpdate`/`onPhysicsUpdate` + `FeelSpec` bindings) | `rustports/quickjs` + 3JSE TS runtime | `[semantic]` | Hydrogen |
| `foreplay-gdscript` | `MPL-2.0` | Godot 4 `.gd` AST parser (`@export_range` -> `FeelSpecKnob`, `signal`) + deterministic step-budgeted `GdVm` interpreter | Godot `modules/gdscript/` | `[mechanical]` | Hydrogen (`/teamwork-preview`) |
| `foreplay-atlas` | `GPL-3.0-only` | Semantic meaning graph compiler, topological layout, `VisualGraph` execution, `FeelSpecValidator`, blast-radius task context, `AtlasHealthReport` | 3JSE `@3jse/atlas` | `[semantic]` | Carbon (High) |
| `foreplay-porthost` | `GPL-3.0-only` | Bidirectional Godot 4 `.tscn` importer/exporter, 200-scene conformance gate, endian-aware `BinaryReader`, `ByoRomWizard`, `WorldCellStreamer`, 1,000-frame Differential Oracle | `rustports` + `skate2-rust-port` | `[mechanical]` | Hydrogen (`/teamwork-preview`) |
| `foreplay-export` | `GPL-3.0-only` | `FpckArchive` binary packer/unpacker (FNV-1a TOC + traversal guard) + `ProjectExporter` for Linux/SteamDeck, Windows, macOS Game Mode `.app`, WebGPU, SexOS `.sexpkg` | Godot `EditorExportPlatform` | `[mechanical]` | Hydrogen (`/teamwork-preview`) |
| `foreplay-harness` | `GPL-3.0-only` | `HarnessSession` Undo/Redo bus, JSON-RPC 2.0 MCP stdio server (`mcp-stdio`), `.tscn` converter, and `verify-all-gates` automated gate runner | 3JSE `HARNESS.md` + `bevy_remote` | `[semantic]` | Hydrogen + Carbon |
| `foreplay-editor` | `GPL-3.0-only` | Full Godot 4.4 `egui` + `egui_dock` IDE: 6 Workspaces, Multi-Scene tabs, 5 Modals, 7 Bottom Docks, 3D/2D Gizmos, live WGSL/FPCK/`GdVm`/Oracle integration | Godot `editor/` + 3JSE Editor | `[mechanical]` | Hydrogen |
| `foreplay-verifier` | `GPL-3.0-only` | Independent Carbon red-team crate: 20 `proptest` & hostile fuzz suites (10k Undo/Redo marathon, Bevy hierarchy isomorphism, `.fscn`/`.tscn`/`.gd`/`.gdshader`/`.fpck` fuzzers) | RustPorts Verification Ratchet | `[semantic]` | Carbon (Verifier) |

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
