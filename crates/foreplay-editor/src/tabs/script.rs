//! Script & FSCN IR Split View Dock (`foreplay-editor`).
//!
//! Provides a side-by-side split pane featuring:
//! - Left: Editable GDScript / TypeScript source buffer with live `GdVm` execution.
//! - Right: Live deterministic `.fscn` IR preview & Godot 4 `.tscn` export.

use egui::{FontId, RichText, ScrollArea, TextEdit, Ui, Vec2};
use crate::ForeplayEditorApp;
use crate::tabs::console::LogLevel;
use crate::theme::{COLOR_ACCENT_BLUE, COLOR_SUCCESS_GREEN, COLOR_TEXT_MUTED};

pub fn show(app: &mut ForeplayEditorApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Code & Scene IR Split Pane").strong());
        ui.colored_label(COLOR_TEXT_MUTED, "| Left: GDScript Controller (GdVm) | Right: Deterministic .fscn IR & Godot .tscn");
    });

    let total_width = ui.available_width();
    let half_width = (total_width * 0.5 - 8.0).max(100.0);

    ui.horizontal(|ui| {
        // Left Column: GDScript Buffer
        ui.allocate_ui(Vec2::new(half_width, ui.available_height()), |ui| {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("📜 skater_controller.gd").color(COLOR_SUCCESS_GREEN).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("▶ Run Script in GdVm").on_hover_text("Execute GDScript via deterministic GdVm VM").clicked() {
                            match app.run_gdscript_vm() {
                                Ok(steps) => {
                                    app.log(LogLevel::Info, format!("GdVm executed {steps} steps successfully"));
                                }
                                Err(err) => {
                                    app.log(LogLevel::Warn, format!("GdVm execution error: {err}"));
                                }
                            }
                        }
                        if ui.button("Format / Validate").clicked() {
                            match foreplay_gdscript::parse_gdscript(&app.script_buffer) {
                                Ok(m) => {
                                    let ext = &m.extends;
                                    app.log(LogLevel::Info, format!("GDScript validated OK: extends {ext}, {} exports, {} signals", m.exports.len(), m.signals.len()));
                                }
                                Err(e) => {
                                    app.log(LogLevel::Warn, format!("GDScript syntax warning: {e}"));
                                }
                            }
                        }
                    });
                });
                ui.separator();

                ScrollArea::both().id_salt("script_editor_scroll").show(ui, |ui| {
                    ui.add(
                        TextEdit::multiline(&mut app.script_buffer)
                            .font(FontId::monospace(12.0))
                            .desired_width(f32::INFINITY)
                            .desired_rows(28)
                            .code_editor(),
                    );
                });
            });
        });

        ui.separator();

        // Right Column: Live FSCN IR Preview & TSCN Export
        ui.allocate_ui(Vec2::new(half_width, ui.available_height()), |ui| {
            ui.group(|ui| {
                let fscn_text = app.session.tree.active_scene.to_fscn_string();
                ui.horizontal(|ui| {
                    ui.label(RichText::new("📄 scene.fscn (Live Intermediate Representation)").color(COLOR_ACCENT_BLUE).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("📤 Copy/Log .tscn").on_hover_text("Export active scene to Godot 4 .tscn text format").clicked() {
                            let tscn = app.export_active_scene_tscn();
                            ui.ctx().copy_text(tscn.clone());
                            app.log(LogLevel::Info, format!("Copied Godot 4 .tscn ({} bytes) to clipboard:\n{}", tscn.len(), tscn));
                        }
                        if ui.button("📋 Copy FSCN").clicked() {
                            ui.ctx().copy_text(fscn_text.clone());
                        }
                    });
                });
                ui.separator();

                ScrollArea::both().id_salt("fscn_preview_scroll").show(ui, |ui| {
                    ui.add(
                        TextEdit::multiline(&mut fscn_text.as_str())
                            .font(FontId::monospace(11.5))
                            .desired_width(f32::INFINITY)
                            .desired_rows(28)
                            .interactive(false),
                    );
                });
            });
        });
    });
}
