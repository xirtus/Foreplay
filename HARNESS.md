# Foreplay AI Harness & Atlas Specification (`HARNESS.md`)

Adapted from **[3JSE (`xirtus/3JSE`)](https://3jse.com/)** for native Rust, Godot-parity scene workflows, and clean-room `rustports` engineering.

> **Foundational Rule:** *Everything the visual editor can do, `EditorCommand` and the Agent API can do.* The visual editor is a human GUI over the exact same `GameplayIr` command bus that AI agents operate via CLI and MCP.

---

## 1. The Agent Loop: `Observe → Plan → Act → Verify`

AI agents never synthesize fake mouse clicks or regex-hack serialized scene files blindly. Every agent interaction executes four deterministic phases through `foreplay-harness`:

1. **Observe:**
   - Query `foreplay-atlas` for the **Semantic Graph** (`Player → Input → TrickStateMachine → Physics → Camera`) and a **Blast-Radius Task Context** containing only affected `NodeId`s, schemas, `FeelSpec` blocks, and upstream port references.
   - Inspect live frame telemetry, collision contacts, console diagnostics, or Differential Port Oracle deltas.
2. **Plan:**
   - Walk the **Reuse Ladder** before writing new Rust code:
     1. Existing `foreplay-*` engine server / built-in node (`foreplay-core`, `foreplay-physics`, `foreplay-render`)
     2. Verified [`rustports.org`](https://rustports.org/) native crate (`rustports-quickjs`, ported codecs/math)
     3. Curated workspace plugin / template (`templates/cleanroom-port`, `templates/godot-migration`)
     4. Custom Rust / TypeScript / GDScript code *(last resort)*
3. **Act:**
   - Dispatch batch atomic `EditorCommand` transactions (`CreateNode`, `ReparentNode`, `SetProperty`, `TuneFeelSpec`, `ConnectSignal`, `SwapPhysicsBackend`) or compile typed Rust/script systems.
4. **Verify (Evidence-First Completion):**
   - Run headless deterministic playtests (`foreplay-harness verify --ticks 300`), perceptual viewport golden-image diffs, performance frame-time budgets, and differential upstream oracle checks. No task is complete without green gate receipts.

---

## 2. Foreplay Atlas & `FeelSpec`

Code is for execution; **Atlas is for human and agent understanding.**

- **Semantic Contracts:** Systems and nodes declare inputs, outputs, signals, and invariants. `foreplay-atlas` compiles them into a deterministic layered graph showing *meaning* rather than low-level branch noise.
- **`FeelSpec` Tuning Blocks:** Gameplay intent parameters (e.g., `ollie_pop_impulse`, `grind_magnet_radius`, `camera_spring_damping`) declare:
  - Physical units, min/max safe bounds, default value, and human intent description.
  - Direct two-way binding to live component fields—modifiable in real time from the Inspector slider, `FeelSpec` text file, or MCP agent call with zero recompile.

---

## 3. Hard Quality Gates

Every PR and porting wave must clear five automated ratchets:

1. **Borrow & Memory Gate:** Zero `undocumented_unsafe_blocks`; `cargo +nightly miri test` clean on `foreplay-ir` and `foreplay-core`.
2. **Round-Trip Determinism Gate:** `emit_fscn(parse_fscn(doc)) == doc` byte-for-byte; git diffs stay minimal and human-legible.
3. **Undo/Redo Invertibility Gate:** Every `EditorCommand` satisfies `apply(cmd) -> apply(cmd.inverse()) == initial_state`.
4. **Render & Perf Gate:** Zero `wgpu` validation errors; zero steady-state heap allocations inside per-frame server hot loops.
5. **Port Differential Gate (`foreplay-porthost`):** Ported `[mechanical]` and `[semantic]` waves cannot regress the `CHECKPOINT` pass count in `PLAN.md`.
