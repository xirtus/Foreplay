# Foreplay Game Engine — Phase 0 Analysis & Execution Plan (`PLAN.md`)

Follows the [RustPorts Methodology](https://github.com/rustports/docs/blob/main/methodology.md) and [3JSE Dependency-Ordered Architecture](https://3jse.com/).

## CHECKPOINT

```text
PHASE: 0 (Bootstrap & Trait Lock)
UPSTREAM_REFERENCE: godotengine/godot (target pin: 4.4-stable in upstream/godot)
WORKSPACE_COMPILE: GREEN
WORKSPACE_UNIT_TESTS: PASS
TSCN_CORPUS_PASS: 0 / 200 (Phase 1 target)
PORT_ORACLE_FRAMES: 0 / 1000 (Phase 5 target)
FUZZ_TARGETS_ACTIVE: 0 / 6 (Phase 1+ target)
```

---

## 1. Subsystem Inventory & Classification

Every deliverable carries an explicit classification tag (`[mechanical]`, `[semantic]`, or `[uncertain]`) to route work cleanly between **Gemini 4 Carbon** (deep invariants & unsafe audits) and **Gemini 4 Argon** (UI loops & `/teamwork` porting waves).

| Crate | Purpose | Upstream Reference | Classification | Owner Track |
| :--- | :--- | :--- | :--- | :--- |
| `foreplay-ir` | Canonical Gameplay IR, generational IDs (`NodeId`, `ResourceId`, `Rid`), `Reflect`, invertible `EditorCommand`, `.fscn` text codec | 3JSE Gameplay IR + Godot `Variant`/`PackedScene` | `[semantic]` | Carbon (High) |
| `foreplay-core` | Generational `SlotMap` SceneTree, `PackedScene` override layering, typed `Signal` bus, VFS (`res://`, `user://`, `port://`) | Godot `scene/main/{node,scene_tree}.cpp` | `[semantic]` | Carbon (High) |
| `foreplay-platform` | Windowing, gamepad (`SDL3` deadzones), FS & surface bridge across Linux/Wayland/SteamDeck, macOS, Windows, Web, and SexOS (`Silk` + `egui` backend) | Godot `DisplayServer` + SexOS `sex-app` | `[semantic]` | Carbon (High) + Argon |
| `foreplay-render` | `RenderingServer` RIDs over `wgpu` (2D batcher, 3D clustered forward PBR, skinning SIMD, offscreen editor viewports) + SexOS scanout fallback | Godot `servers/rendering/` | `[semantic]` | Carbon (High) |
| `foreplay-physics` | `PhysicsServer2D/3D` over Rapier + hotswappable custom `PhysicsBackend` trait for faithful game ports | Godot `PhysicsServer3D` + `skate2-rust-port` | `[semantic]` | Carbon + Argon |
| `foreplay-audio` | `AudioServer` (`cpal` mixer, audio buses, pluggable codecs including XMA/Vorbis/Opus) | Godot `AudioServer` + `skate2-xma` | `[mechanical]` | Carbon + Argon (`/teamwork`) |
| `foreplay-net` | Deterministic input lockstep + jitter-proof compact replay-frame streaming + `--server` headless runner | `skate2-rust-port` online netcode | `[semantic]` | Carbon (High) |
| `foreplay-script` | Native Rust hot-reload host + `rustports-quickjs` TS/JS zero-compile scripting bridge | `rustports/quickjs` + 3JSE TS runtime | `[semantic]` | Argon + Carbon |
| `foreplay-gdscript` | Godot `.gd` lexer/parser/typechecker compiling directly into `foreplay-ir` / QuickJS bytecode | Godot `modules/gdscript/` | `[mechanical]` | Argon (`/teamwork`) |
| `foreplay-atlas` | Semantic meaning graph compiler (`Player -> Input -> Move -> Camera`), `FeelSpec` live knobs, blast-radius context slices | 3JSE `@3jse/atlas` | `[semantic]` | Carbon (High) |
| `foreplay-harness` | `foreplay-mcp` + CLI Agent API (`Observe -> Plan -> Act -> Verify`), headless runner, visual/perf quality gates | 3JSE `HARNESS.md` | `[semantic]` | Carbon + Argon |
| `foreplay-porthost` | BYO-ROM `port://` archive mount wizard, safe binary cursor combinators, frame-by-frame Differential Oracle | `rustports` + `skate2-rust-port` | `[mechanical]` | Argon (`/teamwork`) + Carbon Verifier |
| `foreplay-editor` | `egui` + `egui_dock` IDE (`2D`, `3D`, `Script`, `Atlas`, `Port Diff`, `AssetLib`), Gizmos, Inspector drawers, Play-In-Editor | Godot `editor/` + 3JSE Editor | `[mechanical]` | Argon (Med/High) |

---

## 2. Dependency DAG (Leaves to Top)

```text
                    [foreplay-editor] (GPL-3.0)
                   /        |         \
     [foreplay-harness] [foreplay-atlas] [foreplay-porthost] (GPL-3.0)
              \             |              /
    [foreplay-script] [foreplay-gdscript] [foreplay-net]
               \            |            /
  [foreplay-render] [foreplay-physics] [foreplay-audio]
                   \        |        /
                  [foreplay-platform]
                            |
                     [foreplay-core]
                            |
                      [foreplay-ir]   <-- Leaf foundation (zero heavy deps)
```

---

## 3. Godot C++ → Foreplay IR → Bevy 0.20 Substrate Map

Following **3JSE Principle #1** (*"Bevy is a dependency, never a fork"*), Foreplay maps Godot's high-productivity authoring model directly onto **Bevy 0.20's** modern primitives:

| Upstream Godot 4 Construct | Foreplay IR / Authoring Surface | Underlying Bevy `0.20` Substrate |
| :--- | :--- | :--- |
| `Object*` / `ObjectID` | Generational `NodeId` (`foreplay_ir::NodeId`) | `bevy_ecs::entity::Entity` mapped via `SceneTree::ir_to_entity` |
| `Node` subclassing (`CharacterBody3D : Node3D`) | `class_name` + automatic base property composition | Bevy **Required Components** (`#[require(Transform, Visibility)]`) |
| `Node::add_child` / scene hierarchy | `IrNode::parent` + `PackedScene` override layers | Bevy **ECS Relations** (`ChildOf` / `Children`) with automatic cleanup |
| `Object::emit_signal` / `connect` | `SignalConnection` in `.fscn` / `.tscn` | Bevy **Observers & Bubbling Triggers** (`world.trigger_targets(...)`) |
| `ClassDB` + `@export` | `ReflectNode` + `FeelSpecKnob` | `bevy_reflect::Reflect` + custom type attributes (`#[reflect(@FeelSpec)]`) |
| Remote Scene Debugger | `foreplay-harness` (`Observe -> Plan -> Act -> Verify`) | `EditorCommand` Undo/Redo bus + `bevy_remote` (BRP JSON-RPC 2.0) + MCP |
| `Viewport` / 3D Gizmo raycast | Dockable `2D` & `3D` viewports in `egui_dock` | `bevy_render` offscreen `RenderTarget::Image` + `bevy_picking` |
| Raw C struct binary file cast (`*(Header*)buf`) | `foreplay-porthost` `port://` archive reader | Checked reader cursor / `bytemuck` — audited by `cargo-fuzz` |

---

## 4. Dependency-Ordered Phase Plan & Exit Gates

- **Phase 0 — Pre-Port & Architectural Lock**
  - *Deliverables:* Workspace skeleton, `PLAN.md`, `HARNESS.md`, `AGENTS.md`, `foreplay-ir` core types (`NodeId`, `Rid`, `IrValue`, `FeelSpec`, `EditorCommand`, deterministic `.fscn` smoke round-trip).
  - *Exit Gate:* `cargo test --workspace` 100% green; clippy `undocumented_unsafe_blocks = "deny"` active.
- **Phase 1 — Gameplay IR, Arena SceneTree & `.tscn` Conformance**
  - *Subtasks:*
    - `1.1` Generational `SceneTree` + `PackedScene` override resolution `[semantic]` (Carbon High)
    - `1.2` Deterministic `.fscn` emitter & parser `[semantic]` (Carbon High)
    - `1.3` Godot `.tscn` / `.tres` parser & importer `[mechanical]` (Argon `/teamwork` wave)
    - `1.4` `proptest` 10k-step Undo/Redo + `cargo-fuzz` scene parser targets `[semantic]` (Carbon Verifier)
  - *Exit Gate:* 200/200 Godot `.tscn` fixtures round-trip losslessly; zero Miri/fuzz panics.
- **Phase 2 — Headless AI Harness (`foreplay-mcp`) & Atlas Semantic Core**
  - *Subtasks:*
    - `2.1` `foreplay-atlas` graph compiler + `FeelSpec` live binding + blast-radius context export `[semantic]` (Carbon High)
    - `2.2` `foreplay-harness` JSON-RPC / MCP command server + headless tick runner `[semantic]` (Argon Med)
  - *Exit Gate:* Headless agent builds a playable test scene, ticks 300 frames, tunes `FeelSpec`, and passes assertions via CLI/MCP.
- **Phase 3 — Servers Bring-Up (`foreplay-platform`, `foreplay-render`, `foreplay-physics`, `foreplay-audio`)**
  - *Subtasks:*
    - `3.1` `foreplay-platform` Linux (`winit`+`SDL3`), macOS, Windows & SexOS (`Silk`/`egui`) adapters `[semantic]`
    - `3.2` `foreplay-render` `wgpu` 2D batcher + 3D PBR + offscreen editor `TextureView` + gizmo pass `[semantic]` (Carbon High)
    - `3.3` `foreplay-physics` Rapier backend + hotswappable `PhysicsBackend` trait `[semantic]`
    - `3.4` `foreplay-audio` `cpal` bus mixer + codec trait `[mechanical]`
  - *Exit Gate:* 10,000-entity 3D scene @ 120 FPS, zero `wgpu` validation warnings, 0 steady-state frame allocations.
- **Phase 4 — Godot-Parity `egui_dock` Editor Workspace**
  - *Subtasks:*
    - `4.1` Dockable shell (`2D`, `3D`, `Script`, `Atlas`, `Port Diff`, `AssetLib`) `[mechanical]` (Argon)
    - `4.2` Hierarchy tree, schema-driven Inspector drawers, Content Browser `[mechanical]` (Argon)
    - `4.3` 2D/3D viewport gizmos + instant in-process Play-In-Editor (Pause/Inspect/Tweak/Resume) `[mechanical]` (Argon)
  - *Exit Gate:* Import a Godot 4 project, edit via gizmos, Undo/Redo cleanly, and Play-In-Editor in <1s on Linux & macOS (plus SexOS `egui` target check).
- **Phase 5 — `PortHost` Clean-Room Game Porting & Replay Netcode**
  - *Subtasks:*
    - `5.1` `port://` read-only BYO-ROM VFS mount + binary archive combinators `[mechanical]` (Argon `/teamwork`)
    - `5.2` Differential Frame Oracle + Editor **Port Diff** heatmap inspector `[mechanical]` (Argon)
    - `5.3` `foreplay-net` replay-frame streaming + `--server` dedicated binary `[semantic]` (Carbon High)
    - `5.4` Malformed archive & packet fuzzing `[semantic]` (Carbon Verifier)
  - *Exit Gate:* Stream an external port world, hotswap two physics backends live, verify 1,000 frame traces against upstream oracle, zero fuzz crashes.
- **Phase 6 — Dual Scripting (`rustports-quickjs` + `foreplay-gdscript`) & One-Click Export**
  - *Subtasks:*
    - `6.1` `rustports-quickjs` TS/JS component binding `[semantic]`
    - `6.2` `foreplay-gdscript` `.gd` → Gameplay IR compiler `[mechanical]` (Argon `/teamwork`)
    - `6.3` One-click export templates (Linux/SteamDeck, Windows, macOS `.app`, WebGPU URL, SexOS package) `[mechanical]`
