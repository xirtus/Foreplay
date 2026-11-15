//! Console & Undo/Redo History Dock (`foreplay-editor`).
//!
//! Displays a clickable log stream with timestamped command executions,
//! Undo/Redo stack depth readouts, and quick action buttons.

use egui::{RichText, ScrollArea, Ui};
use crate::ForeplayEditorApp;
use crate::theme::{
    COLOR_ACCENT_BLUE, COLOR_SUCCESS_GREEN, COLOR_TEXT_MUTED, COLOR_TEXT_NORMAL, COLOR_WARN_YELLOW,
};

#[derive(Clone, Debug, PartialEq)]
pub struct ConsoleLogEntry {
    pub timestamp_ms: u64,
    pub level: LogLevel,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogLevel {
    Info,
    Warn,
    Command,
    Pie,
}

pub fn show(app: &mut ForeplayEditorApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Editor Console & Undo/Redo Command Bus").strong());
        ui.colored_label(
            COLOR_TEXT_MUTED,
            format!("(Executed Commands: {})", app.command_history_count),
        );

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Clear Log").clicked() {
                app.console_logs.clear();
            }
            if ui.button("Redo (Ctrl+Y)").clicked() {
                app.redo();
            }
            if ui.button("Undo (Ctrl+Z)").clicked() {
                app.undo();
            }
        });
    });

    ui.separator();

    ScrollArea::vertical()
        .auto_shrink([false, false])
        .stick_to_bottom(true)
        .show(ui, |ui| {
            if app.console_logs.is_empty() {
                ui.colored_label(COLOR_TEXT_MUTED, "Console is idle. Actions will be logged here.");
                return;
            }

            for entry in &app.console_logs {
                ui.horizontal(|ui| {
                    let secs = entry.timestamp_ms / 1000;
                    let ms = entry.timestamp_ms % 1000;
                    ui.colored_label(COLOR_TEXT_MUTED, format!("[{secs:04}.{ms:03}]"));

                    let (tag, color) = match entry.level {
                        LogLevel::Info => ("[INFO]", COLOR_TEXT_NORMAL),
                        LogLevel::Warn => ("[WARN]", COLOR_WARN_YELLOW),
                        LogLevel::Command => ("[CMD]", COLOR_ACCENT_BLUE),
                        LogLevel::Pie => ("[PIE]", COLOR_SUCCESS_GREEN),
                    };
                    ui.colored_label(color, tag);
                    ui.label(&entry.message);
                });
            }
        });
}
