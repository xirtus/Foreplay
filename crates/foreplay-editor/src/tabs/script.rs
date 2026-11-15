//! Script & FSCN IR Split View Dock (`foreplay-editor`).
//!
//! Provides a side-by-side split pane featuring:
//! - Left: Editable GDScript / TypeScript source buffer.
//! - Right: Live deterministic `.fscn` IR preview (`active_scene.to_fscn_string()`).

use egui::{FontId, RichText, ScrollArea, TextEdit, Ui, Vec2};
use crate::ForeplayEditorApp;
use crate::theme::{COLOR_ACCENT_BLUE, COLOR_SUCCESS_GREEN, COLOR_TEXT_MUTED};

pub fn show(app: &mut ForeplayEditorApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Code & Scene IR Split Pane").strong());
        ui.colored_label(COLOR_TEXT_MUTED, "| Left: GDScript Controller | Right: Deterministic .fscn IR");
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
                        if ui.button("Format").clicked() {
                            // no-op formatter placeholder
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

        // Right Column: Live FSCN IR Preview
        ui.allocate_ui(Vec2::new(half_width, ui.available_height()), |ui| {
            ui.group(|ui| {
                let fscn_text = app.session.tree.active_scene.to_fscn_string();
                ui.horizontal(|ui| {
                    ui.label(RichText::new("📄 scene.fscn (Live Intermediate Representation)").color(COLOR_ACCENT_BLUE).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
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
