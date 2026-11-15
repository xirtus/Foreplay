//! Foreplay Game Engine Editor Executable (`foreplay-editor`).
//!
//! Dual-mode entrypoint:
//! - Default / `--headless`: Executes full headless smoke verification across:
//!   - All 8 top workspaces (`Workspace3D`, `Workspace2D`, `ModelPolyamour`, `Script`, `ThreeJsWeb`, `AtlasGraph`, `PortDiff`, `AssetLib`)
//!   - All 7 bottom panel tabs (`Output`, `Debugger`, `AudioMixer`, `Animation`, `ShaderEditor`, `TileMap`, `AiHarness`)
//!   - All 5 active modals (`ProjectManager`, `ProjectSettings`, `ExportProject`, `CommandPalette`, `CreateNodeDialog`)
//!     verifies non-empty paint shapes, prints status report, and exits 0 cleanly.
//! - `--gui`: Launches the native desktop `eframe` window.

use std::env;
use foreplay_editor::bottom_panel::BottomPanelTab;
use foreplay_editor::modals::ActiveModal;
use foreplay_editor::{EditorWorkspace, ForeplayEditorApp};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    let launch_gui = args.iter().any(|arg| arg == "--gui");

    let mut app = ForeplayEditorApp::new("ForeplayParkDemo");
    app.populate_starter_scene();

    if launch_gui {
        println!("Launching Foreplay Game Engine Editor Desktop GUI (Studio Obsidian Pro)...");
        let native_options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1440.0, 900.0])
                .with_min_inner_size([960.0, 600.0])
                .with_title("Foreplay Game Engine — v2.0 (Studio Obsidian & Chroma Pro IDE)"),
            ..Default::default()
        };

        eframe::run_native(
            "Foreplay Game Engine — v2.0",
            native_options,
            Box::new(|_cc| Ok(Box::new(app))),
        )?;
    } else {
        println!("Foreplay Game Engine Editor v2.0 (Headless Smoke Verification)");
        println!("Project: '{}' | Nodes: {}", app.session.tree.active_scene.name, app.session.tree.active_scene.nodes.len());
        println!("Active Theme: {:?}", app.theme_preset);

        // 1. Verify all 8 workspaces
        println!("\n[1/3] Verifying 8 IDE Workspaces (3D, 2D, Model, Script, Three.js, Atlas, Port Diff, AssetLib):");
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
            println!("  ✓ Bottom Dock {tab:?}: {shapes_count} clipped paint shapes rendered cleanly");
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
        println!("\nHeadless smoke verification: 100% PASS across all 8 IDE workspaces, 7 bottom docks, and 5 modals.");
    }

    Ok(())
}
