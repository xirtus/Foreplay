//! Script & FSCN IR Split View Dock (`foreplay-editor`).
//!
//! Provides a side-by-side split pane featuring:
//! - Left: Editable GDScript 2.0 source buffer with Gutter Breakpoints, live `GdVm` execution,
//!   and static `GdLspAnalyzer` diagnostics, symbols, completions, and hover inspections.
//! - Right: Live deterministic `.fscn` IR preview & Godot 4 `.tscn` export.

use egui::{Color32, FontId, RichText, ScrollArea, TextEdit, Ui, Vec2};
use foreplay_gdscript::{GdCompletionItem, GdLspAnalyzer, GdSymbolKind};
use crate::ForeplayEditorApp;
use crate::tabs::console::LogLevel;
use crate::theme::{
    COLOR_ACCENT_BLUE, COLOR_SUCCESS_GREEN,
    COLOR_TEXT_MUTED, COLOR_TEXT_NORMAL, COLOR_WARN_YELLOW,
};

pub fn show(app: &mut ForeplayEditorApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Code & Scene IR Split Pane").strong());
        ui.colored_label(
            COLOR_TEXT_MUTED,
            "| Left: GDScript 2.0 LSP & Gutter Breakpoints | Right: Deterministic .fscn IR & Godot .tscn",
        );
    });

    let total_width = ui.available_width();
    let half_width = (total_width * 0.5 - 8.0).max(100.0);

    ui.horizontal(|ui| {
        // Left Column: GDScript Buffer with Breakpoint Gutter & LSP Panel
        ui.allocate_ui(Vec2::new(half_width, ui.available_height()), |ui| {
            ui.group(|ui| {
                let bp_count = app.debugger_state.breakpoints.len();
                ui.horizontal(|ui| {
                    ui.label(RichText::new("📜 skater_controller.gd").color(COLOR_SUCCESS_GREEN).strong());
                    if bp_count > 0 {
                        ui.colored_label(Color32::from_rgb(230, 60, 60), format!("🔴 {bp_count} Breakpoints"));
                    }

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
                                    app.log(
                                        LogLevel::Info,
                                        format!(
                                            "GDScript validated OK: extends {ext}, {} exports, {} signals",
                                            m.exports.len(),
                                            m.signals.len()
                                        ),
                                    );
                                }
                                Err(e) => {
                                    app.log(LogLevel::Warn, format!("GDScript syntax warning: {e}"));
                                }
                            }
                        }
                    });
                });

                ui.separator();

                // Gutter & Code Editor Side-by-Side
                let line_count = app.script_buffer.lines().count().max(1);
                ui.horizontal(|ui| {
                    // Breakpoint Gutter Column
                    ui.allocate_ui(Vec2::new(42.0, 240.0), |ui| {
                        ScrollArea::vertical()
                            .id_salt("script_gutter_scroll")
                            .show(ui, |ui| {
                                for line_num in 1..=line_count {
                                    let has_bp = app
                                        .debugger_state
                                        .breakpoints
                                        .iter()
                                        .any(|(_, l)| *l == line_num);

                                    ui.horizontal(|ui| {
                                        if has_bp {
                                            if ui
                                                .button(RichText::new("🔴").size(9.0))
                                                .on_hover_text(format!("Line {line_num}: Remove breakpoint"))
                                                .clicked()
                                            {
                                                app.debugger_state.breakpoints.retain(|(_, l)| *l != line_num);
                                                app.log(LogLevel::Info, format!("Removed breakpoint at line {line_num}"));
                                            }
                                        } else if ui
                                            .button(
                                                RichText::new(format!("{line_num:>2}"))
                                                    .color(COLOR_TEXT_MUTED)
                                                    .size(10.0)
                                                    .monospace(),
                                            )
                                            .on_hover_text(format!("Line {line_num}: Click to toggle breakpoint"))
                                            .clicked()
                                        {
                                            app.debugger_state.breakpoints.push((
                                                "res://scripts/skater_controller.gd".to_string(),
                                                line_num,
                                            ));
                                            app.log(LogLevel::Info, format!("Set breakpoint at line {line_num}"));
                                        }
                                    });
                                }
                            });
                    });

                    ui.separator();

                    // Multi-line Source Buffer
                    ScrollArea::both()
                        .id_salt("script_editor_scroll")
                        .max_height(240.0)
                        .show(ui, |ui| {
                            ui.add(
                                TextEdit::multiline(&mut app.script_buffer)
                                    .font(FontId::monospace(12.0))
                                    .desired_width(f32::INFINITY)
                                    .desired_rows(18)
                                    .code_editor(),
                            );
                        });
                });

                ui.separator();

                // GDScript 2.0 LSP Analyzer Panel
                let lsp = GdLspAnalyzer::new();
                let analysis = lsp.analyze_document("res://scripts/skater_controller.gd", &app.script_buffer);

                ui.horizontal(|ui| {
                    ui.label(RichText::new("🔍 GDScript 2.0 LSP").strong().color(COLOR_ACCENT_BLUE));
                    if analysis.diagnostics.is_empty() {
                        ui.colored_label(COLOR_SUCCESS_GREEN, "✔ 0 Diagnostics (Clean)");
                    } else {
                        ui.colored_label(COLOR_WARN_YELLOW, format!("⚠ {} Diagnostics", analysis.diagnostics.len()));
                    }
                    ui.colored_label(COLOR_TEXT_MUTED, "|");
                    ui.colored_label(COLOR_ACCENT_BLUE, format!("📌 {} Symbols", analysis.symbols.len()));
                    ui.colored_label(COLOR_TEXT_MUTED, "|");
                    ui.colored_label(COLOR_TEXT_MUTED, format!("⚙ {} Executable", analysis.executable_lines.len()));
                });

                // Display active diagnostics if any exist
                if !analysis.diagnostics.is_empty() {
                    ScrollArea::vertical()
                        .id_salt("script_diagnostics_scroll")
                        .max_height(60.0)
                        .show(ui, |ui| {
                            for diag in &analysis.diagnostics {
                                ui.horizontal(|ui| {
                                    ui.colored_label(COLOR_WARN_YELLOW, "⚠");
                                    ui.label(
                                        RichText::new(format!(
                                            "Line {}:{}: {}",
                                            diag.range.start.line, diag.range.start.col, diag.message
                                        ))
                                        .monospace()
                                        .color(COLOR_TEXT_NORMAL),
                                    );
                                });
                            }
                        });
                }

                // Symbols Preview Tray
                ScrollArea::horizontal()
                    .id_salt("script_symbols_scroll")
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            for sym in &analysis.symbols {
                                let (kind_str, color) = match sym.kind {
                                    GdSymbolKind::Class => ("[Class]", COLOR_ACCENT_BLUE),
                                    GdSymbolKind::Function => ("[Func]", COLOR_SUCCESS_GREEN),
                                    GdSymbolKind::Signal => ("[Signal]", Color32::from_rgb(220, 160, 60)),
                                    GdSymbolKind::Property => ("[Prop]", COLOR_TEXT_NORMAL),
                                    GdSymbolKind::Constant => ("[Const]", COLOR_WARN_YELLOW),
                                    GdSymbolKind::Enum => ("[Enum]", COLOR_ACCENT_BLUE),
                                    _ => ("[Symbol]", COLOR_TEXT_MUTED),
                                };
                                ui.colored_label(color, format!("{kind_str} {}", sym.name));
                            }
                        });
                    });

                // Context Hover & Completions Tray
                let completions: Vec<GdCompletionItem> = lsp.complete_at(&app.script_buffer, 1, 1);
                if !completions.is_empty() {
                    ui.horizontal(|ui| {
                        ui.colored_label(COLOR_TEXT_MUTED, "LSP Completions:");
                        for comp in completions.iter().take(4) {
                            ui.label(RichText::new(&comp.label).monospace().color(COLOR_ACCENT_BLUE));
                        }
                    });
                }
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
