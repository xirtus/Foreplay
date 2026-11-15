# Foreplay Game Engine

**The Godot-parity, AI-native Rust game engine & editor for desktop, WebGPU, clean-room ports, and the SexOS `sexlife` suite.**

Foreplay (`foreplay`) combines:
1. **Built *Around* Modular Bevy 0.20 (`Bevy is a dependency, never a fork`)** — Just as [3JSE](https://3jse.com/) wraps Three.js without forking it, Foreplay builds around upgradable `bevy_*` `0.20.1` subcrates (`bevy_ecs` **Required Components**, **ECS Relations** `ChildOf`/`Children`, **Observers & Bubbling Triggers**, `bevy_reflect`, `bevy_remote` BRP, `bevy_render` `wgpu`, `bevy_picking`)—inheriting 3,900+ ecosystem crates (`avian3d`, `bevy_rapier`, `bevy_kira_audio`) and saving 18+ months of low-level plumbing while letting users drop down to raw Bevy ECS at any layer.
2. **Godot 4.x Workflow Parity** — Dockable `2D`, `3D`, `Script`, `Atlas`, `Port Diff`, and `AssetLib` workspaces built on `egui` + `egui_dock` (targeting Linux/Wayland/Steam Deck, macOS, Windows, WebGPU, and the **SexOS `egui`/Silk** userspace backend), with `SceneTree` composition, `PackedScene` inheritance, `.fscn`/`.tscn` text scenes, and low-level `RID` Server ergonomics.
3. **3JSE Unified Architecture (`One Game, Four Frontends`)** — Visual Editor, Native Rust / `rustports-quickjs` TypeScript / GDScript-to-IR scripts, Visual Gameplay Graphs, and AI Agents all read and mutate a single canonical, deterministic, git-diffable **Gameplay IR** (`foreplay-ir`), with **Foreplay Atlas** (`FeelSpec` live tuning + semantic graphs) and the **AI Harness** (`Observe → Plan → Act → Verify` over `EditorCommand` + `bevy_remote` JSON-RPC / MCP).
4. **`rustports.org` & Clean-Room Port Hosting (`skate2-rust-port` Model)** — Zero-FFI integration with native `rustports` crates, `port://` read-only BYO-ROM retail asset mounting (`foreplay-porthost`), live physics/subsystem A/B hotswapping, frame-by-frame differential oracle inspection, and replay-frame multiplayer (`--server`).

---

## Copyleft Licensing Model (No MIT Giveaway, Commercial Game Friendly)

Foreplay rejects permissive MIT giveaways while allowing studios and indie developers to ship commercial closed-source games:

- **Editor, AI Harness, Atlas & Porting Suite (`GPL-3.0-only`):** `foreplay-editor`, `foreplay-harness`, `foreplay-atlas`, and `foreplay-porthost` are licensed under **GNU GPL v3.0 only** (matching SexOS, 3JSE, and `skate2-rust-port`). Any fork of the editor or AI harness must remain free software.
- **Engine Runtime Crates (`MPL-2.0` / `LGPL-3.0-or-later WITH Classpath-exception-2.0` compatible):** `foreplay-ir`, `foreplay-core`, `foreplay-platform`, `foreplay-render`, `foreplay-physics`, `foreplay-audio`, `foreplay-net`, `foreplay-script`, and `foreplay-gdscript` use file-level strong copyleft (**MPL-2.0**, secondary-compatible with **GPL-3.0**).
  - **Why not plain GPL-2.0?** Rust statically links `.rlib` crates by default; plain GPL-2.0 would infect a developer's proprietary game code on static link and is incompatible with GPL-3.0 editor crates.
  - **What MPL-2.0 guarantees:** Any modification to Foreplay's runtime crates *must* be released as open source (zero proprietary engine forks), while game developers can statically link unmodified runtime crates into closed-source commercial games on Steam, consoles, Linux, and SexOS.

---

## Quick Start

```bash
cargo check --workspace
cargo test --workspace
cargo run -p foreplay-editor
cargo run -p foreplay-harness -- status
```

## Key Documents

- [`PLAN.md`](PLAN.md) — RustPorts Phase 0 inventory, dependency DAG, `[mechanical]`/`[semantic]` wave tags, C++-to-Rust idiom map, and checkpoint ledger.
- [`HARNESS.md`](HARNESS.md) — 3JSE-style Agent API surface, `FeelSpec` specification, Reuse Ladder, and Carbon/Argon verification gates.
- [`AGENTS.md`](AGENTS.md) — Auto-loaded multi-agent operating rules (Carbon vs. Argon track boundaries).
