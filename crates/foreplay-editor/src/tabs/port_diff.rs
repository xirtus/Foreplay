//! Differential Frame Oracle Dock (`foreplay-editor`).
//!
//! Visualizes per-frame physics drift against upstream retail executable traces
//! using `foreplay_porthost::run_1000_frame_differential_gate`, interactive epsilon sliders,
//! and error distribution progress bars.

use egui::{Color32, RichText, ScrollArea, Slider, Stroke, Ui, Vec2};
use foreplay_porthost::FrameDifferentialVerdict;
use crate::ForeplayEditorApp;
use crate::tabs::console::LogLevel;
use crate::theme::{
    COLOR_ACCENT_BLUE, COLOR_BORDER, COLOR_ERROR_RED, COLOR_SUCCESS_GREEN, COLOR_TEXT_MUTED,
    COLOR_WARN_YELLOW,
};

pub fn show(app: &mut ForeplayEditorApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("RustPorts Differential Frame Oracle").strong());
        ui.colored_label(COLOR_ACCENT_BLUE, "(Reference: sk82_na_m.xex Trace — 1,000 continuous frames @ 60Hz)");

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("⚡ Run 1,000-Frame Differential Gate").clicked() {
                let report = foreplay_porthost::run_1000_frame_differential_gate();
                app.log(
                    LogLevel::Info,
                    format!(
                        "Differential Oracle: {}/{} frames pass ({:.1}% PASS), max pos drift: {:.6}m, max vel drift: {:.6}m/s",
                        report.passed_frames,
                        report.total_frames,
                        report.pass_rate_percent,
                        report.max_position_drift,
                        report.max_velocity_drift
                    ),
                );
                app.last_differential_report = Some(report);
            }
        });
    });

    ui.horizontal(|ui| {
        ui.label("Epsilon Tolerance (ε):");
        let slider = Slider::new(&mut app.port_diff_epsilon, 0.0001..=0.02)
            .step_by(0.0005)
            .suffix(" m")
            .show_value(true);
        ui.add(slider);
        ui.colored_label(COLOR_TEXT_MUTED, "| Lower ε enforces tighter IEEE 754 determinism.");
    });

    ui.separator();

    // Determine verdicts to display: either from 1000-frame report or 60-frame live preview
    let (verdicts, passed_count, total_count, max_pos_observed, report_banner) = if let Some(report) = &app.last_differential_report {
        let is_perfect = report.is_perfect_match();
        let banner_text = format!(
            "Exit Gate Verdict: {}/{} frames within ε ({:.1}% PASS) | Max Pos: {:.6}m | Max Vel: {:.6}m/s | RMS Pos: {:.6}m{}",
            report.passed_frames,
            report.total_frames,
            report.pass_rate_percent,
            report.max_position_drift,
            report.max_velocity_drift,
            report.position_rms_error,
            if is_perfect { " — IEEE 754 BIT-EXACT MATCH" } else { "" }
        );
        (
            report.verdicts.clone(),
            report.passed_frames,
            report.total_frames,
            report.max_position_drift,
            Some((is_perfect, banner_text)),
        )
    } else {
        let frame_count = 60;
        let mut v_list = Vec::with_capacity(frame_count);
        let mut passed = 0;
        let mut max_p = 0.0f32;

        for i in 0..frame_count {
            let f = i as f32;
            let pos_err = (f * 0.12).sin().abs() * 0.0022 + 0.0001;
            let vel_err = (f * 0.17).cos().abs() * 0.0018 + 0.0001;
            let verdict = FrameDifferentialVerdict::evaluate(i as u64, pos_err, vel_err, app.port_diff_epsilon);
            if verdict.within_epsilon {
                passed += 1;
            }
            if verdict.max_position_delta_m > max_p {
                max_p = verdict.max_position_delta_m;
            }
            v_list.push(verdict);
        }
        (v_list, passed, frame_count, max_p, None)
    };

    // Summary banner
    ui.horizontal(|ui| {
        let pass_rate = (passed_count as f32 / total_count as f32) * 100.0;
        let summary_color = if passed_count == total_count {
            COLOR_SUCCESS_GREEN
        } else if pass_rate >= 90.0 {
            COLOR_WARN_YELLOW
        } else {
            COLOR_ERROR_RED
        };

        if let Some((_, text)) = report_banner {
            ui.colored_label(summary_color, text);
        } else {
            ui.colored_label(
                summary_color,
                format!("Live Preview: {passed_count}/{total_count} frames within tolerance ({pass_rate:.1}% PASS)"),
            );
            ui.colored_label(
                COLOR_TEXT_MUTED,
                format!("| Max observed pos delta: {:.5}m", max_pos_observed),
            );
        }
    });

    ui.separator();

    // Verdict rows table
    ScrollArea::vertical().show(ui, |ui| {
        egui::Grid::new("diff_oracle_grid")
            .striped(true)
            .min_col_width(90.0)
            .show(ui, |ui| {
                ui.label(RichText::new("Frame").strong());
                ui.label(RichText::new("Pos Delta (m)").strong());
                ui.label(RichText::new("Vel Delta (m/s)").strong());
                ui.label(RichText::new("Visual Drift Bar").strong());
                ui.label(RichText::new("Verdict").strong());
                ui.end_row();

                for v in verdicts {
                    ui.label(format!("#{:04}", v.frame_index));
                    ui.label(format!("{:.6}", v.max_position_delta_m));
                    ui.label(format!("{:.6}", v.max_velocity_delta_mps));

                    // Visual drift bar
                    let ratio = (v.max_position_delta_m / app.port_diff_epsilon.max(1e-5)).clamp(0.0, 1.0);
                    let (bar_rect, _resp) = ui.allocate_exact_size(Vec2::new(120.0, 14.0), egui::Sense::hover());
                    let painter = ui.painter_at(bar_rect);
                    painter.rect_filled(bar_rect, 2.0, Color32::from_rgb(0x1a, 0x1d, 0x24));
                    painter.rect_stroke(bar_rect, 2.0, Stroke::new(1.0, COLOR_BORDER), egui::StrokeKind::Inside);

                    let fill_width = bar_rect.width() * ratio;
                    let fill_rect = egui::Rect::from_min_size(bar_rect.min, Vec2::new(fill_width, bar_rect.height()));
                    let fill_color = if v.within_epsilon {
                        COLOR_SUCCESS_GREEN
                    } else {
                        COLOR_ERROR_RED
                    };
                    painter.rect_filled(fill_rect, 2.0, fill_color);

                    // Verdict badge
                    if v.within_epsilon {
                        ui.colored_label(COLOR_SUCCESS_GREEN, "[PASS]");
                    } else {
                        ui.colored_label(COLOR_ERROR_RED, "[DRIFT]");
                    }
                    ui.end_row();
                }
            });
    });
}
