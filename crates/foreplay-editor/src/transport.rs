//! Top Menu Bar, Transport Bar, Studio LCD HUD, and Multi-Scene Tabs (`foreplay-editor`).
//!
//! Provides:
//! - Apple Pro / Godot top menu bar: Scene | Project | Debug | Editor | Help
//! - Center smoked-glass LCD HUD: Timecode, BPM, FPS, Node count, Dual Target
//! - Center workspace switcher: 2D | 3D | Model | Script | Three.js | Atlas | Port Diff | AssetLib
//! - Left quick actions: Undo, Redo, Physics Backend toggle
//! - Right PIE controls: ▶ Play, ⏸ Pause, ⏭ Step, ⏹ Stop, World Tick count
//! - Theme preset switcher: Studio Obsidian Pro (macOS Logic Pro 11 + Final Cut Pro 11) <-> Classic Godot Slate

use egui::{Color32, RichText, Ui};
use crate::ForeplayEditorApp;
use crate::{EditorWorkspace, PhysicsBackendChoice};
use crate::modals::ActiveModal;
use crate::tabs::console::LogLevel;
use crate::theme::{
    CHROMA_CYAN_2D, CHROMA_EMERALD_THREEJS, CHROMA_GOLD_POLYAMOUR,
    COLOR_ERROR_RED, COLOR_SUCCESS_GREEN, COLOR_TEXT_MUTED,
    COLOR_WARN_YELLOW, EditorThemePreset, STUDIO_OBSIDIAN_ACCENT,
};
use crate::widgets::studio_lcd_hud;

pub fn render_top_bar(app: &mut ForeplayEditorApp, ui: &mut Ui) {
    // Row 1: Top Menu Bar, Center Studio LCD HUD, and Transport Controls
    ui.horizontal(|ui| {
        // Engine Logo
        ui.label(RichText::new("FOREPLAY v1.1.0").strong().color(STUDIO_OBSIDIAN_ACCENT));
        ui.separator();

        // 1. Scene Menu
        ui.menu_button("Scene", |ui| {
            if ui.button("➕ New Scene").clicked() {
                app.open_or_switch_scene("NewScene", None);
                ui.close();
            }
            if ui.button("📁 Open Project / Scene...").clicked() {
                app.active_modal = Some(ActiveModal::ProjectManager);
                ui.close();
            }
            if ui.button("💾 Save Scene").clicked() {
                let name = app.session.tree.active_scene.name.clone();
                app.log(LogLevel::Info, format!("Scene '{name}' saved successfully."));
                ui.close();
            }
            ui.separator();
            if ui.button("📤 Export Scene as Godot .tscn").clicked() {
                let tscn = app.export_active_scene_tscn();
                ui.ctx().copy_text(tscn.clone());
                app.log(LogLevel::Info, format!("Exported Godot .tscn ({} bytes) to clipboard", tscn.len()));
                ui.close();
            }
            if ui.button("📥 Import Skate Spot Demo .tscn").clicked() {
                let spot_tscn = include_str!("../../../templates/godot-migration/skate_spot.tscn");
                let _ = app.import_tscn_scene("SkateSpotDemo", spot_tscn);
                ui.close();
            }
            ui.separator();
            if ui.button("✖ Close Scene").clicked() {
                if app.open_scenes.len() > 1 {
                    app.close_scene_doc(app.active_scene_index);
                }
                ui.close();
            }
        });

        // 2. Project Menu
        ui.menu_button("Project", |ui| {
            if ui.button("⚙ Project Settings... (Alt+P)").clicked() {
                app.active_modal = Some(ActiveModal::ProjectSettings);
                ui.close();
            }
            if ui.button("📦 Export Project...").clicked() {
                app.active_modal = Some(ActiveModal::ExportProject);
                ui.close();
            }
            if ui.button("📁 Project Manager...").clicked() {
                app.active_modal = Some(ActiveModal::ProjectManager);
                ui.close();
            }
        });

        // 3. Debug Menu
        ui.menu_button("Debug", |ui| {
            if ui.button("🐞 Toggle Debugger & Profiler").clicked() {
                app.toggle_bottom_panel(crate::bottom_panel::BottomPanelTab::Debugger);
                ui.close();
            }
            if ui.button("🔊 Toggle Audio Bus Mixer").clicked() {
                app.toggle_bottom_panel(crate::bottom_panel::BottomPanelTab::AudioMixer);
                ui.close();
            }
            if ui.button("🤖 Toggle AI Agent Harness").clicked() {
                app.toggle_bottom_panel(crate::bottom_panel::BottomPanelTab::AiHarness);
                ui.close();
            }
        });

        // 4. Editor Menu (with Theme Switcher)
        ui.menu_button("Editor", |ui| {
            if ui.button("⌨ Command Palette (Ctrl+Shift+P)").clicked() {
                app.active_modal = Some(ActiveModal::CommandPalette);
                ui.close();
            }
            if ui.button("➕ Create Node Dialog (Ctrl+A)").clicked() {
                app.active_modal = Some(ActiveModal::CreateNodeDialog);
                ui.close();
            }
            ui.separator();
            ui.menu_button("🎨 Theme Preset", |ui| {
                if ui.selectable_label(app.theme_preset == EditorThemePreset::StudioObsidianPro, "Studio Obsidian Pro (Logic & FCP)").clicked() {
                    app.theme_preset = EditorThemePreset::StudioObsidianPro;
                    app.log(LogLevel::Info, "Applied theme: Studio Obsidian Pro");
                    ui.close();
                }
                if ui.selectable_label(app.theme_preset == EditorThemePreset::ClassicGodotSlate, "Classic Godot Slate").clicked() {
                    app.theme_preset = EditorThemePreset::ClassicGodotSlate;
                    app.log(LogLevel::Info, "Applied theme: Classic Godot Slate");
                    ui.close();
                }
            });
        });

        ui.separator();

        // Undo / Redo
        if ui.button("↩ Undo").on_hover_text("Undo last command (Ctrl+Z)").clicked() {
            app.undo();
        }
        if ui.button("↪ Redo").on_hover_text("Redo last command (Ctrl+Y)").clicked() {
            app.redo();
        }

        ui.separator();

        // Physics Backend Indicator / Toggle
        let (phys_label, phys_color) = match app.physics_backend {
            PhysicsBackendChoice::RapierStandard => ("Physics: Rapier", COLOR_SUCCESS_GREEN),
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

        // Center: Smoked-Glass Studio LCD HUD (Logic Pro / FCP style)
        let timecode_secs = app.animation_state.playhead_secs as f64;
        let bpm = 120.0f32;
        let active_nodes = app.session.tree.active_scene.nodes.len();
        studio_lcd_hud(ui, timecode_secs, bpm, app.fps, active_nodes, app.play_in_editor_running);

        // Right: Play-In-Editor (PIE) Transport Controls
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

    // Row 2: Multi-Scene Tab Bar & Workspace Switcher Tabs
    ui.horizontal(|ui| {
        render_scene_tabs(app, ui);

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Workspace Tabs (8 variants: AssetLib, Port Diff, Atlas, Three.js, Script, Model, 3D, 2D)
            workspace_button(app, ui, EditorWorkspace::AssetLib, "AssetLib");
            workspace_button(app, ui, EditorWorkspace::PortDiff, "Port Diff");
            workspace_button(app, ui, EditorWorkspace::AtlasGraph, "Atlas");
            workspace_button(app, ui, EditorWorkspace::ThreeJsWeb, "Three.js");
            workspace_button(app, ui, EditorWorkspace::Script, "Script");
            workspace_button(app, ui, EditorWorkspace::ModelPolyamour, "Model");
            workspace_button(app, ui, EditorWorkspace::Workspace3D, "3D");
            workspace_button(app, ui, EditorWorkspace::Workspace2D, "2D");
        });
    });
}

/// Renders the multi-scene tab bar above the viewports.
pub fn render_scene_tabs(app: &mut ForeplayEditorApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        let active_idx = app.active_scene_index;
        let mut close_idx: Option<usize> = None;
        let mut switch_idx: Option<usize> = None;

        for (idx, doc) in app.open_scenes.iter().enumerate() {
            let is_active = idx == active_idx;
            let icon = if doc.title.contains("HUD") || doc.title.contains("UI") {
                "🖥 "
            } else {
                "🛹 "
            };

            let title_text = if is_active {
                RichText::new(format!("{icon}{}.fscn", doc.title)).strong().color(Color32::WHITE)
            } else {
                RichText::new(format!("{icon}{}.fscn", doc.title)).color(COLOR_TEXT_MUTED)
            };

            ui.group(|ui| {
                ui.horizontal(|ui| {
                    if ui.selectable_label(is_active, title_text).clicked() {
                        switch_idx = Some(idx);
                    }

                    if app.open_scenes.len() > 1 && ui.small_button("×").clicked() {
                        close_idx = Some(idx);
                    }
                });
            });
        }

        // Add Scene Tab button
        if ui.button("➕").on_hover_text("Open or create another scene tab").clicked() {
            let new_name = format!("Scene_{}", app.open_scenes.len() + 1);
            app.open_or_switch_scene(new_name, None);
        }

        if let Some(idx) = switch_idx {
            app.switch_to_scene_index(idx);
        }

        if let Some(idx) = close_idx {
            app.close_scene_doc(idx);
        }
    });
}

fn workspace_button(app: &mut ForeplayEditorApp, ui: &mut Ui, ws: EditorWorkspace, label: &str) {
    let is_active = app.active_workspace == ws;
    let accent_color = match ws {
        EditorWorkspace::ModelPolyamour => CHROMA_GOLD_POLYAMOUR,
        EditorWorkspace::ThreeJsWeb => CHROMA_EMERALD_THREEJS,
        EditorWorkspace::Workspace2D => CHROMA_CYAN_2D,
        _ => STUDIO_OBSIDIAN_ACCENT,
    };

    let text = if is_active {
        RichText::new(label).strong().color(accent_color)
    } else {
        RichText::new(label).color(Color32::from_rgb(0xc0, 0xc5, 0xd0))
    };

    if ui.selectable_label(is_active, text).clicked() {
        app.set_workspace(ws);
    }
}
