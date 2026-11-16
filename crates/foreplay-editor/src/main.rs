//! Foreplay Game Engine Editor Executable (`foreplay-editor` / `Foreplay`).
//!
//! Smart dual-mode entrypoint:
//! - Desktop App Launch (when invoked as `Foreplay` inside macOS `Foreplay.app` or Linux AppDir,
//!   or when `--gui` / `FOREPLAY_GUI=1` is passed): Launches the native desktop `eframe` IDE window
//!   with embedded Studio Obsidian & Chroma app icon, Wayland/X11 `app_id`, and optional `.tscn`/`.fscn` file open.
//! - CLI / CI Smoke Mode (default when invoked as `foreplay-editor` without `--gui`, or whenever
//!   `--headless` / `--smoke-test` is passed): Executes full headless verification across all 8 IDE workspaces,
//!   7 bottom docks, and 5 modal dialogs, and exits 0 cleanly.

use std::env;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use foreplay_editor::bottom_panel::BottomPanelTab;
use foreplay_editor::modals::ActiveModal;
use foreplay_editor::{EditorWorkspace, ForeplayEditorApp};

/// Synthesizes the 64x64 RGBA8 "Studio Obsidian & Chroma" window/dock icon.
fn build_studio_icon_rgba64() -> egui::IconData {
    let size: usize = 64;
    let mut rgba = vec![0u8; size * size * 4];
    let center = (size as f32 - 1.0) * 0.5;

    for y in 0..size {
        for x in 0..size {
            let fx = x as f32;
            let fy = y as f32;
            let dx = (fx - center).abs();
            let dy = (fy - center).abs();

            // Squircle superellipse mask (macOS / modern Linux icon silhouette)
            let nx = dx / 29.5;
            let ny = dy / 29.5;
            let r4 = nx.powi(4) + ny.powi(4);
            if r4 > 1.0 {
                continue;
            }
            let edge_aa = ((1.0 - r4) * 14.0).clamp(0.0, 1.0);

            // Deep Studio Obsidian base (#0D0F14 -> #171B26 radial glow)
            let radial = ((fx - center).hypot(fy - center) / 32.0).clamp(0.0, 1.0);
            let mut r = 13.0 + (1.0 - radial) * 16.0;
            let mut g = 15.0 + (1.0 - radial) * 20.0;
            let mut b = 20.0 + (1.0 - radial) * 32.0;

            // Subtle jewel-chroma border rim
            if r4 > 0.82 {
                let rim = ((r4 - 0.82) / 0.18).clamp(0.0, 1.0);
                let angle = (fy - center).atan2(fx - center);
                let cyan_mix = (angle.cos() * 0.5 + 0.5).clamp(0.0, 1.0);
                r += rim * (56.0 * cyan_mix + 236.0 * (1.0 - cyan_mix));
                g += rim * (189.0 * cyan_mix + 72.0 * (1.0 - cyan_mix));
                b += rim * (248.0 * cyan_mix + 153.0 * (1.0 - cyan_mix));
            }

            // Stylized Chroma Play-Prism triangle in the center
            let px = (fx - 22.0) / 23.0;
            let py = (fy - center) / 18.0;
            if (0.0..=1.0).contains(&px) && py.abs() <= (1.0 - px * 0.92) {
                let t = px.clamp(0.0, 1.0);
                let v = (py * 0.5 + 0.5).clamp(0.0, 1.0);
                // Gradient: Electric Cyan (#38BDF8) -> Vivid Violet (#A855F7) -> Warm Amber (#F59E0B)
                r = 56.0 * (1.0 - t) + 168.0 * t + 70.0 * v * t;
                g = 189.0 * (1.0 - t) + 85.0 * t + 65.0 * (1.0 - v);
                b = 248.0 * (1.0 - t * 0.65) + 40.0 * (1.0 - v);
            }

            let idx = (y * size + x) * 4;
            rgba[idx] = r.clamp(0.0, 255.0) as u8;
            rgba[idx + 1] = g.clamp(0.0, 255.0) as u8;
            rgba[idx + 2] = b.clamp(0.0, 255.0) as u8;
            rgba[idx + 3] = (edge_aa * 255.0) as u8;
        }
    }

    egui::IconData {
        rgba,
        width: size as u32,
        height: size as u32,
    }
}

fn should_launch_desktop_gui(args: &[String]) -> bool {
    if args.iter().any(|a| a == "--headless" || a == "--smoke-test") {
        return false;
    }
    if args.iter().any(|a| a == "--gui") {
        return true;
    }
    if env::var("FOREPLAY_GUI")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
    {
        return true;
    }
    // When packaged as macOS `Foreplay.app/Contents/MacOS/Foreplay` or Linux desktop binary `Foreplay`,
    // double-clicking launches the binary without CLI flags — open the Studio GUI automatically!
    if let Ok(exe_path) = env::current_exe() {
        let path_str = exe_path.to_string_lossy();
        if path_str.contains(".app/Contents/MacOS/") {
            return true;
        }
        if let Some(stem) = exe_path.file_stem().and_then(|s| s.to_str())
            && stem.eq_ignore_ascii_case("foreplay")
        {
            return true;
        }
    }
    false
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!(
            "Foreplay Game Engine v{} ({}/{})",
            env!("CARGO_PKG_VERSION"),
            env::consts::OS,
            env::consts::ARCH
        );
        return Ok(());
    }

    let launch_gui = should_launch_desktop_gui(&args);

    let mut app = ForeplayEditorApp::new("ForeplayParkDemo");
    app.populate_starter_scene();

    // Optional scene file open from Finder / XDG desktop file association or CLI path
    for arg in args.iter().skip(1) {
        if arg.starts_with('-') {
            continue;
        }
        let candidate = Path::new(arg);
        if candidate.is_file()
            && let Ok(contents) = fs::read_to_string(candidate)
        {
            if arg.ends_with(".tscn")
                && let Ok(scene) = foreplay_porthost::parse_tscn_to_ir(&contents)
            {
                app.session.tree.active_scene = scene;
            } else if arg.ends_with(".fscn")
                && let Ok(scene) = foreplay_ir::IrScene::from_fscn_str(&contents)
            {
                app.session.tree.active_scene = scene;
            }
        }
    }

    if launch_gui {
        println!(
            "Launching Foreplay Game Engine v1.1.0 Desktop Studio IDE ({}/{})...",
            env::consts::OS,
            env::consts::ARCH
        );
        let icon = Arc::new(build_studio_icon_rgba64());
        let native_options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_app_id("dev.sexos.foreplay")
                .with_inner_size([1440.0, 900.0])
                .with_min_inner_size([960.0, 600.0])
                .with_icon(icon)
                .with_title("Foreplay Game Engine — v1.1.0 (Studio Obsidian & Chroma Pro IDE)"),
            ..Default::default()
        };

        eframe::run_native(
            "Foreplay Game Engine — v1.1.0",
            native_options,
            Box::new(|_cc| Ok(Box::new(app))),
        )?;
    } else {
        println!("Foreplay Game Engine Editor v1.1.0 (Headless Smoke Verification)");
        println!(
            "Project: '{}' | Nodes: {}",
            app.session.tree.active_scene.name,
            app.session.tree.active_scene.nodes.len()
        );
        println!("Active Theme: {:?}", app.theme_preset);

        // 1. Verify all 8 workspaces
        println!(
            "\n[1/3] Verifying 8 IDE Workspaces (3D, 2D, Model, Script, Three.js, Atlas, Port Diff, AssetLib):"
        );
        let workspaces = [
            EditorWorkspace::Workspace3D,
            EditorWorkspace::Workspace2D,
            EditorWorkspace::ModelPolyamour,
            EditorWorkspace::Script,
            EditorWorkspace::ThreeJsWeb,
            EditorWorkspace::AtlasGraph,
            EditorWorkspace::PortDiff,
            EditorWorkspace::AssetLib,
        ];

        for ws in workspaces {
            app.set_workspace(ws);
            let mut output = app.run_headless_frame([1280.0, 720.0]);
            let shapes_count = output.shapes.len();
            assert!(
                shapes_count > 0,
                "Workspace {ws:?} produced 0 shapes in headless frame"
            );
            println!("  ✓ Workspace {ws:?}: {shapes_count} clipped paint shapes rendered cleanly");
            output.textures_delta.clear();
        }

        // 2. Verify all 7 bottom panel tabs
        println!("\n[2/3] Verifying Bottom Panel Docks:");
        let bottom_tabs = [
            BottomPanelTab::Output,
            BottomPanelTab::Debugger,
            BottomPanelTab::AudioMixer,
            BottomPanelTab::Animation,
            BottomPanelTab::ShaderEditor,
            BottomPanelTab::TileMap,
            BottomPanelTab::AiHarness,
        ];

        for tab in bottom_tabs {
            app.bottom_panel_state.is_expanded = true;
            app.bottom_panel_state.active_tab = tab;
            let mut output = app.run_headless_frame([1280.0, 720.0]);
            let shapes_count = output.shapes.len();
            assert!(
                shapes_count > 0,
                "Bottom tab {tab:?} produced 0 shapes in headless frame"
            );
            println!(
                "  ✓ Bottom Dock {tab:?}: {shapes_count} clipped paint shapes rendered cleanly"
            );
            output.textures_delta.clear();
        }

        // 3. Verify all 5 active modals
        println!("\n[3/3] Verifying Modal Dialogs:");
        let modals = [
            ActiveModal::ProjectManager,
            ActiveModal::ProjectSettings,
            ActiveModal::ExportProject,
            ActiveModal::CommandPalette,
            ActiveModal::CreateNodeDialog,
        ];

        for modal in modals {
            app.active_modal = Some(modal);
            let mut output = app.run_headless_frame([1280.0, 720.0]);
            let shapes_count = output.shapes.len();
            assert!(
                shapes_count > 0,
                "Modal {modal:?} produced 0 shapes in headless frame"
            );
            println!("  ✓ Modal {modal:?}: {shapes_count} clipped paint shapes rendered cleanly");
            output.textures_delta.clear();
        }

        app.active_modal = None;
        println!(
            "\nHeadless smoke verification: 100% PASS across all 8 IDE workspaces, 7 bottom docks, and 5 modals."
        );
    }

    Ok(())
}
