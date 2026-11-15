# Multi-Agent Operating Instructions (`AGENTS.md`)

Auto-loaded context for Gemini (Carbon / Hydrogen), Claude, and Codex threads working in `/Users/xmacbookm5/Projects/sexlife/Foreplay`.

## 1. Ground Rules
- **Standalone `sexlife` Repo:** Foreplay is part of the SexOS Apps Suite (`sexlife`), running natively on **Linux (Wayland/X11/Steam Deck), macOS, Windows, WebGPU**, AND **SexOS userspace** (`Silk` shell + SexOS `egui` backend) via `foreplay-platform`. Never put raw POSIX/Linux syscalls outside `foreplay-platform`.
- **Single Source of Truth:** All four frontends (Editor `egui_dock` UI, Rust/TS/GDScript code, Visual Graph, MCP Agent API) read and write `foreplay-ir`. Never bypass `EditorCommand` when mutating scenes from `foreplay-editor`.
- **RustPorts Discipline:** When porting Godot C++ subsystems or clean-room game modules, cite upstream sources (`Mirrors <symbol> (<file>)`), tag waves in `PLAN.md` (`[mechanical]` / `[semantic]`), and never leave silent stubs—use explicit `todo!("wave Na: ...")` only during Step A skeletons and clear 100% of them before committing Step C.
- **Licensing Boundary:** Keep runtime crates (`foreplay-ir`, `foreplay-core`, `foreplay-platform`, `foreplay-render`, `foreplay-physics`, `foreplay-audio`, `foreplay-net`, `foreplay-script`, `foreplay-gdscript`) under **`MPL-2.0`** (so game devs can ship closed-source commercial games while engine changes stay copyleft), and keep `foreplay-editor`, `foreplay-harness`, `foreplay-atlas`, `foreplay-porthost`, `foreplay-export`, and `foreplay-verifier` under **`GPL-3.0-only`**. Never add `GPL-3.0-only` dependencies into runtime crates!

## 2. Carbon vs. Hydrogen Ownership Split
- **Gemini 4 Carbon (High) — Track A & B:** Owns `foreplay-ir`, `foreplay-core`, `foreplay-atlas`, `foreplay-platform`, `foreplay-render`, `foreplay-physics`, `foreplay-audio`, `foreplay-net`. Responsible for borrow-checker lifetimes, generational arenas, Bevy `0.20` ECS relations, `unsafe` `wgpu`/SIMD invariants (`// SAFETY:`), and locking traits before parallel waves run.
- **Gemini 4 Hydrogen — Track C:** Owns `foreplay-editor` (`egui` + `egui_dock`). Builds workspaces, gizmos, inspectors, modals, bottom docks, and `Port Diff` visualizers strictly atop `EditorCommand`.
- **Gemini 4 Hydrogen (`/teamwork-preview`) — Track D:** Owns `[mechanical]` RustPorts waves (`foreplay-gdscript`, `.tscn`/`.tres` codecs, `foreplay-script`, `foreplay-porthost` archive readers, `foreplay-export` `.fpck` & 5-platform release bundler, `foreplay-harness` JSON-RPC/MCP CLI, and `templates/`).
- **Gemini 4 Carbon (Verifier) — Track E:** Read-only on feature crates; owns `foreplay-verifier` adversarial `proptest` state machines and parser/fuzzing suites.
