//! History Dock (`foreplay-editor`).
//!
//! Mirrors Godot 4's History dock beside the Inspector:
//! Displays an interactive timeline of all executed commands on the Undo/Redo stack.

use egui::{RichText, ScrollArea, Ui};
use crate::ForeplayEditorApp;
use crate::theme::{COLOR_ACCENT_BLUE, COLOR_SUCCESS_GREEN, COLOR_TEXT_MUTED, COLOR_TEXT_NORMAL, COLOR_WARN_YELLOW};

pub fn show(app: &mut ForeplayEditorApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Action History (Undo/Redo Stack)").strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("↪ Redo").clicked() {
                app.redo();
            }
            if ui.button("↩ Undo").clicked() {
                app.undo();
            }
        });
    });

    ui.separator();

    ScrollArea::vertical().show(ui, |ui| {
        let commands = app
            .console_logs
            .iter()
            .filter(|e| e.level == crate::tabs::console::LogLevel::Command)
            .collect::<Vec<_>>();

        if commands.is_empty() {
            ui.colored_label(COLOR_TEXT_MUTED, "No commands recorded yet in history.");
            return;
        }

        for (idx, entry) in commands.iter().enumerate().rev() {
            ui.horizontal(|ui| {
                ui.colored_label(COLOR_TEXT_MUTED, format!("#{idx:03}"));
                let secs = entry.timestamp_ms / 1000;
                let ms = entry.timestamp_ms % 1000;
                ui.colored_label(COLOR_TEXT_MUTED, format!("[{secs:02}.{ms:03}]"));

                if entry.message.contains("Undo") {
                    ui.colored_label(COLOR_WARN_YELLOW, "↩ Undo");
                } else if entry.message.contains("Redo") {
                    ui.colored_label(COLOR_SUCCESS_GREEN, "↪ Redo");
                } else {
                    ui.colored_label(COLOR_ACCENT_BLUE, "⚡ Act");
                }

                ui.label(RichText::new(&entry.message).color(COLOR_TEXT_NORMAL));
            });
        }
    });
}
