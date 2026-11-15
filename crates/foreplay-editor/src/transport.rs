//! Top Transport Bar (`foreplay-editor`).
//!
//! Provides:
//! - Center workspace tabs: 2D | 3D | Script | Atlas | Port Diff | AssetLib
//! - Left quick actions: + New Demo Scene, Undo, Redo, Physics Backend toggle
//! - Right PIE controls: ▶ Play, ⏸ Pause, ⏭ Step, ⏹ Stop

use egui::{Color32, RichText, Ui};
use crate::ForeplayEditorApp;
use crate::{EditorWorkspace, PhysicsBackendChoice};
use crate::tabs::console::LogLevel;
use crate::theme::{
    COLOR_ACCENT_BLUE, COLOR_ERROR_RED, COLOR_SUCCESS_GREEN, COLOR_TEXT_MUTED,
    COLOR_WARN_YELLOW,
};

pub fn render_top_bar(app: &mut ForeplayEditorApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        // --- Left: Brand + Demo Scene + Undo/Redo + Physics Toggle ---
        ui.label(RichText::new("FOREPLAY").strong().color(COLOR_ACCENT_BLUE));
        ui.separator();

        if ui.button("➕ New Demo Scene").on_hover_text("Populate starter skater/platformer scene").clicked() {
            app.populate_starter_scene();
        }

        if ui.button("↩ Undo").on_hover_text("Undo last command (Ctrl+Z)").clicked() {
            app.undo();
        }

        if ui.button("↪ Redo").on_hover_text("Redo last command (Ctrl+Y)").clicked() {
            app.redo();
        }

        ui.separator();

        // Physics Backend Indicator / Toggle
        let (phys_label, phys_color) = match app.physics_backend {
            PhysicsBackendChoice::RapierStandard => ("Physics: Rapier/Std", COLOR_SUCCESS_GREEN),
            PhysicsBackendChoice::Skate2Port => ("Physics: Skate2 Port", COLOR_WARN_YELLOW),
        };
        if ui
            .button(RichText::new(phys_label).color(phys_color))
            .on_hover_text("Click to toggle live physics backend")
            .clicked()
        {
            app.physics_backend = match app.physics_backend {
                PhysicsBackendChoice::RapierStandard => PhysicsBackendChoice::Skate2Port,
                PhysicsBackendChoice::Skate2Port => PhysicsBackendChoice::RapierStandard,
            };
            app.log(
                LogLevel::Info,
                format!("Physics backend switched to {:?}", app.physics_backend),
            );
        }

        ui.separator();

        // --- Center: Workspace Tabs ---
        // 2D | 3D | Script | Atlas | Port Diff | AssetLib
        workspace_button(app, ui, EditorWorkspace::Workspace2D, "2D");
        workspace_button(app, ui, EditorWorkspace::Workspace3D, "3D");
        workspace_button(app, ui, EditorWorkspace::Script, "Script");
        workspace_button(app, ui, EditorWorkspace::AtlasGraph, "Atlas");
        workspace_button(app, ui, EditorWorkspace::PortDiff, "Port Diff");
        workspace_button(app, ui, EditorWorkspace::AssetLib, "AssetLib");

        // --- Right: Play-In-Editor (PIE) Transport Controls ---
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Stop button
            let stop_btn = egui::Button::new(RichText::new("⏹ Stop").color(if app.play_in_editor_running {
                COLOR_ERROR_RED
            } else {
                COLOR_TEXT_MUTED
            }));
            if ui.add_enabled(app.play_in_editor_running, stop_btn).clicked() {
                app.pie_stop();
            }

            // Step button
            let step_btn = egui::Button::new("⏭ Step");
            if ui.add(step_btn).clicked() {
                app.pie_step();
            }

            // Pause button
            let pause_label = if app.play_in_editor_paused {
                "▶ Resume"
            } else {
                "⏸ Pause"
            };
            let pause_btn = egui::Button::new(RichText::new(pause_label).color(if app.play_in_editor_paused {
                COLOR_WARN_YELLOW
            } else {
                Color32::WHITE
            }));
            if ui.add_enabled(app.play_in_editor_running, pause_btn).clicked() {
                app.pie_pause();
            }

            // Play button
            let play_btn = egui::Button::new(RichText::new("▶ Play").color(if app.play_in_editor_running && !app.play_in_editor_paused {
                COLOR_SUCCESS_GREEN
            } else {
                Color32::WHITE
            }));
            if ui.add(play_btn).clicked() {
                app.pie_play();
            }

            // Tick count readout
            ui.colored_label(
                COLOR_TEXT_MUTED,
                format!("Tick #{}", app.session.tree.tick_count),
            );
        });
    });
}

fn workspace_button(app: &mut ForeplayEditorApp, ui: &mut Ui, ws: EditorWorkspace, label: &str) {
    let is_active = app.active_workspace == ws;
    let text = if is_active {
        RichText::new(label).strong().color(Color32::WHITE)
    } else {
        RichText::new(label).color(Color32::from_rgb(0xc0, 0xc5, 0xd0))
    };

    if ui.selectable_label(is_active, text).clicked() {
        app.set_workspace(ws);
    }
}
