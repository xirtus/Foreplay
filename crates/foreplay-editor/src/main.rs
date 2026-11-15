//! Foreplay Game Engine Editor Executable (`foreplay-editor`).
//!
//! Dual-mode entrypoint:
//! - Default / `--headless`: Executes full headless smoke verification across all 6 workspaces
//!   (`Workspace2D`, `Workspace3D`, `Script`, `AtlasGraph`, `PortDiff`, `AssetLib`),
//!   verifies non-empty paint shapes, prints status report, and exits 0 cleanly.
//! - `--gui`: Launches the native desktop `eframe` window.

use std::env;
use foreplay_editor::{EditorWorkspace, ForeplayEditorApp};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    let launch_gui = args.iter().any(|arg| arg == "--gui");

    let mut app = ForeplayEditorApp::new("ForeplayParkDemo");
    app.populate_starter_scene();

    if launch_gui {
        println!("Launching Foreplay Game Engine Editor Desktop GUI...");
        let native_options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1440.0, 900.0])
                .with_min_inner_size([960.0, 600.0])
                .with_title("Foreplay Game Engine — v0.1.0 (Godot 4 Dockable IDE)"),
            ..Default::default()
        };

        eframe::run_native(
            "Foreplay Game Engine — v0.1.0",
            native_options,
            Box::new(|_cc| Ok(Box::new(app))),
        )?;
    } else {
        println!("Foreplay Game Engine Editor v0.1.0 (Headless Smoke Verification)");
        println!("Project: '{}' | Nodes: {}", app.session.tree.active_scene.name, app.session.tree.active_scene.nodes.len());

        let workspaces = [
            EditorWorkspace::Workspace3D,
            EditorWorkspace::Workspace2D,
            EditorWorkspace::Script,
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

        println!("Headless smoke verification: 100% PASS across all 6 IDE workspaces.");
    }

    Ok(())
}
