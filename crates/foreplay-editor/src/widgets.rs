//! Tactile Apple Logic Pro & Final Cut Pro Custom `egui` Widgets (`foreplay-editor`).
//!
//! Provides:
//! - `studio_lcd_hud`: Smoked-glass rounded center LCD readout in top transport bar (Timecode, BPM, FPS, Nodes, Dual-Target)
//! - `rotary_arc_knob` / `rotary_arc_knob_f64`: 270° swept glowing Logic Pro Smart Control rotary knobs
//! - `eq_response_and_spectrum_canvas`: Logic Pro Channel EQ / ChromaGlow frequency response curve + 32-band spectrum analyzer

use egui::{
    Align2, Color32, FontId, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2,
};
use crate::theme::{
    CHROMA_AMBER_AUDIO, CHROMA_CORAL_3D, CHROMA_CYAN_2D, CHROMA_EMERALD_THREEJS,
    CHROMA_MINT_UI, CHROMA_VIOLET_ANIM, STUDIO_OBSIDIAN_ACCENT, STUDIO_OBSIDIAN_BORDER,
    STUDIO_OBSIDIAN_CANVAS, STUDIO_OBSIDIAN_CARD, STUDIO_OBSIDIAN_TEXT_MUTED,
    STUDIO_OBSIDIAN_TEXT_NORMAL,
};

/// Smoked-glass rounded center LCD readout in the top transport bar (Logic Pro & Final Cut Pro inspired).
///
/// Displays:
/// `Timecode 00:00:00.00 | 120.0 BPM | 60.0 FPS (16.6ms) | 4 Nodes | Dual Target: Rust + Three.js`
/// in crisp monospace amber/cyan LED typography.
pub fn studio_lcd_hud(
    ui: &mut Ui,
    timecode_secs: f64,
    bpm: f32,
    fps: f32,
    active_nodes: usize,
    is_playing: bool,
) -> Response {
    let total_secs = timecode_secs.max(0.0);
    let hours = (total_secs / 3600.0) as u32;
    let mins = ((total_secs % 3600.0) / 60.0) as u32;
    let secs = (total_secs % 60.0) as u32;
    let centis = ((total_secs.fract()) * 100.0) as u32;
    let timecode_str = format!("{hours:02}:{mins:02}:{secs:02}.{centis:02}");
    let frame_ms = if fps > 0.0 { 1000.0 / fps } else { 16.6 };

    let desired_size = Vec2::new(490.0, 26.0);
    let (rect, response) = ui.allocate_exact_size(desired_size, Sense::click());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter_at(rect);

        // Smoked-glass rounded container
        let bg_fill = Color32::from_rgb(0x0a, 0x0c, 0x12);
        let border_stroke = if response.hovered() {
            Stroke::new(1.0, STUDIO_OBSIDIAN_ACCENT)
        } else {
            Stroke::new(1.0, STUDIO_OBSIDIAN_BORDER)
        };
        painter.rect(rect, 6.0, bg_fill, border_stroke, egui::StrokeKind::Inside);

        // Top highlight specular line (1px frosted glass reflection)
        let highlight_line = [
            Pos2::new(rect.min.x + 6.0, rect.min.y + 1.0),
            Pos2::new(rect.max.x - 6.0, rect.min.y + 1.0),
        ];
        painter.line_segment(highlight_line, Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 25)));

        let mut x = rect.min.x + 10.0;
        let y_center = rect.center().y;
        let font = FontId::monospace(11.0);
        let sep_color = Color32::from_rgb(0x3a, 0x45, 0x5a);

        // 1. Timecode (Amber LED)
        let tc_color = if is_playing {
            CHROMA_AMBER_AUDIO
        } else {
            Color32::from_rgb(0xd9, 0x8a, 0x0a)
        };
        let tc_galley = painter.layout_no_wrap(format!("⏱ {timecode_str}"), font.clone(), tc_color);
        painter.galley(Pos2::new(x, y_center - tc_galley.size().y * 0.5), tc_galley.clone(), tc_color);
        x += tc_galley.size().x + 8.0;

        // Separator
        painter.text(Pos2::new(x, y_center), Align2::LEFT_CENTER, "|", font.clone(), sep_color);
        x += 12.0;

        // 2. BPM (Cyan LED)
        let bpm_color = CHROMA_CYAN_2D;
        let bpm_galley = painter.layout_no_wrap(format!("{bpm:.1} BPM"), font.clone(), bpm_color);
        painter.galley(Pos2::new(x, y_center - bpm_galley.size().y * 0.5), bpm_galley.clone(), bpm_color);
        x += bpm_galley.size().x + 8.0;

        // Separator
        painter.text(Pos2::new(x, y_center), Align2::LEFT_CENTER, "|", font.clone(), sep_color);
        x += 12.0;

        // 3. FPS & Frame MS (Mint UI)
        let fps_color = CHROMA_MINT_UI;
        let fps_galley = painter.layout_no_wrap(format!("{fps:.0} FPS ({frame_ms:.1}ms)"), font.clone(), fps_color);
        painter.galley(Pos2::new(x, y_center - fps_galley.size().y * 0.5), fps_galley.clone(), fps_color);
        x += fps_galley.size().x + 8.0;

        // Separator
        painter.text(Pos2::new(x, y_center), Align2::LEFT_CENTER, "|", font.clone(), sep_color);
        x += 12.0;

        // 4. Active Nodes Count
        let node_color = STUDIO_OBSIDIAN_TEXT_NORMAL;
        let node_galley = painter.layout_no_wrap(format!("{active_nodes} Nodes"), font.clone(), node_color);
        painter.galley(Pos2::new(x, y_center - node_galley.size().y * 0.5), node_galley.clone(), node_color);
        x += node_galley.size().x + 8.0;

        // Separator
        painter.text(Pos2::new(x, y_center), Align2::LEFT_CENTER, "|", font.clone(), sep_color);
        x += 12.0;

        // 5. Dual Target Badge
        let target_color = CHROMA_EMERALD_THREEJS;
        painter.text(
            Pos2::new(x, y_center),
            Align2::LEFT_CENTER,
            "Rust + Three.js",
            font,
            target_color,
        );
    }

    response
}

/// Interactive 270° swept glowing Logic Pro Smart Control rotary knob.
///
/// Features:
/// - Swept 270° arc from 135° to 405° (bottom-left around to bottom-right)
/// - Vertical drag tuning with smooth scaling
/// - Double click resets to midpoint (or default)
/// - Draws dark track arc + luminous glowing colored value arc + indicator needle
/// - Formatted value label underneath
pub fn rotary_arc_knob(
    ui: &mut Ui,
    label: &str,
    value: &mut f32,
    min: f32,
    max: f32,
    accent_color: Color32,
) -> bool {
    let desired_size = Vec2::new(54.0, 68.0);
    let (rect, response) = ui.allocate_exact_size(desired_size, Sense::click_and_drag());
    let mut changed = false;

    // Handle mouse drag
    if response.dragged() {
        let delta = -response.drag_delta().y * (max - min) * 0.005;
        let old_val = *value;
        *value = (*value + delta).clamp(min, max);
        if (*value - old_val).abs() > f32::EPSILON {
            changed = true;
        }
    }

    // Handle double-click reset to midpoint
    if response.double_clicked() {
        let default_val = min + (max - min) * 0.5;
        if (*value - default_val).abs() > f32::EPSILON {
            *value = default_val;
            changed = true;
        }
    }

    // Handle mouse wheel when hovered
    if response.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll.abs() > 0.01 {
            let delta = scroll * (max - min) * 0.001;
            let old_val = *value;
            *value = (*value + delta).clamp(min, max);
            if (*value - old_val).abs() > f32::EPSILON {
                changed = true;
            }
        }
    }

    if ui.is_rect_visible(rect) {
        let painter = ui.painter_at(rect);

        // Center of the knob
        let knob_center = Pos2::new(rect.center().x, rect.min.y + 24.0);
        let track_radius = 16.0;
        let cap_radius = 11.5;

        // 270° sweep angles: start at 135° (bottom-left), sweep 270° clockwise to 405° (bottom-right)
        let start_angle = std::f32::consts::PI * 0.75; // 135 deg
        let sweep_range = std::f32::consts::PI * 1.5;  // 270 deg

        let ratio = if (max - min).abs() > f32::EPSILON {
            ((*value - min) / (max - min)).clamp(0.0, 1.0)
        } else {
            0.5
        };
        let current_angle = start_angle + ratio * sweep_range;

        // 1. Draw Background Track Arc
        let track_segments = 32;
        let mut track_points = Vec::with_capacity(track_segments + 1);
        for i in 0..=track_segments {
            let a = start_angle + (i as f32 / track_segments as f32) * sweep_range;
            track_points.push(Pos2::new(
                knob_center.x + a.cos() * track_radius,
                knob_center.y + a.sin() * track_radius,
            ));
        }
        for w in track_points.windows(2) {
            painter.line_segment([w[0], w[1]], Stroke::new(3.5, Color32::from_rgb(0x22, 0x28, 0x36)));
        }

        // 2. Draw Active Swept Value Arc
        let active_segments = (32.0 * ratio).ceil() as usize;
        if active_segments > 0 {
            let mut active_points = Vec::with_capacity(active_segments + 1);
            for i in 0..=active_segments {
                let a = start_angle + (i as f32 / active_segments as f32) * (current_angle - start_angle);
                active_points.push(Pos2::new(
                    knob_center.x + a.cos() * track_radius,
                    knob_center.y + a.sin() * track_radius,
                ));
            }
            for w in active_points.windows(2) {
                painter.line_segment([w[0], w[1]], Stroke::new(3.5, accent_color));
            }

            // Glowing indicator bead at tip of active arc
            let tip = Pos2::new(
                knob_center.x + current_angle.cos() * track_radius,
                knob_center.y + current_angle.sin() * track_radius,
            );
            painter.circle_filled(tip, 2.5, Color32::WHITE);
        }

        // 3. Central Knob Cap
        let cap_fill = if response.hovered() {
            STUDIO_OBSIDIAN_CARD
        } else {
            Color32::from_rgb(0x18, 0x1c, 0x27)
        };
        let cap_border = if response.dragged() {
            accent_color
        } else {
            STUDIO_OBSIDIAN_BORDER
        };
        painter.circle(knob_center, cap_radius, cap_fill, Stroke::new(1.0, cap_border));

        // 4. Indicator Needle Line
        let needle_inner = Pos2::new(
            knob_center.x + current_angle.cos() * 3.0,
            knob_center.y + current_angle.sin() * 3.0,
        );
        let needle_outer = Pos2::new(
            knob_center.x + current_angle.cos() * (cap_radius - 1.5),
            knob_center.y + current_angle.sin() * (cap_radius - 1.5),
        );
        painter.line_segment([needle_inner, needle_outer], Stroke::new(2.0, Color32::WHITE));

        // 5. Value Readout Label Underneath
        let val_text = if (max - min) > 10.0 {
            format!("{value:.1}")
        } else {
            format!("{value:.2}")
        };
        painter.text(
            Pos2::new(rect.center().x, rect.min.y + 44.0),
            Align2::CENTER_CENTER,
            val_text,
            FontId::monospace(10.0),
            STUDIO_OBSIDIAN_TEXT_NORMAL,
        );

        // 6. Label Title Underneath
        painter.text(
            Pos2::new(rect.center().x, rect.min.y + 57.0),
            Align2::CENTER_CENTER,
            label,
            FontId::proportional(10.0),
            STUDIO_OBSIDIAN_TEXT_MUTED,
        );
    }

    changed
}

/// Helper to bind `rotary_arc_knob` directly to an `f64` value (for `FeelSpec` tuning).
pub fn rotary_arc_knob_f64(
    ui: &mut Ui,
    label: &str,
    value: &mut f64,
    min: f64,
    max: f64,
    accent_color: Color32,
) -> bool {
    let mut val_f32 = *value as f32;
    let changed = rotary_arc_knob(ui, label, &mut val_f32, min as f32, max as f32, accent_color);
    if changed {
        *value = val_f32 as f64;
    }
    changed
}

/// Logic Pro Channel EQ / ChromaGlow-style glowing translucent Bezier frequency response curve
/// overlaid on 32 animated gradient spectrum analyzer bars.
pub fn eq_response_and_spectrum_canvas(
    ui: &mut Ui,
    low_db: f32,
    mid_db: f32,
    high_db: f32,
    spectrum_bars: &[f32],
) {
    let desired_size = Vec2::new(ui.available_width().max(280.0), 120.0);
    let (rect, _response) = ui.allocate_exact_size(desired_size, Sense::hover());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter_at(rect);

        // 1. Smoked Glass OLED Background & Subtle Border
        painter.rect(
            rect,
            6.0,
            STUDIO_OBSIDIAN_CANVAS,
            Stroke::new(1.0, STUDIO_OBSIDIAN_BORDER),
            egui::StrokeKind::Inside,
        );

        // 2. dB Grid lines (+12, +6, 0, -6, -12 dB)
        let zero_y = rect.center().y;
        let db_scale = rect.height() * 0.035; // 3.5px per dB

        let grid_lines = [
            (12.0, "+12dB", Color32::from_rgb(0x28, 0x30, 0x42)),
            (6.0, "+6dB", Color32::from_rgb(0x20, 0x26, 0x35)),
            (0.0, "0dB", Color32::from_rgb(0x3a, 0x46, 0x5e)),
            (-6.0, "-6dB", Color32::from_rgb(0x20, 0x26, 0x35)),
            (-12.0, "-12dB", Color32::from_rgb(0x28, 0x30, 0x42)),
        ];

        for (db, label, stroke_color) in grid_lines {
            let y = zero_y - db * db_scale;
            if y >= rect.min.y && y <= rect.max.y {
                let stroke_w = if db == 0.0 { 1.2 } else { 0.8 };
                painter.line_segment(
                    [Pos2::new(rect.min.x + 4.0, y), Pos2::new(rect.max.x - 4.0, y)],
                    Stroke::new(stroke_w, stroke_color),
                );
                painter.text(
                    Pos2::new(rect.min.x + 8.0, y - 2.0),
                    Align2::LEFT_BOTTOM,
                    label,
                    FontId::monospace(8.5),
                    Color32::from_rgb(0x56, 0x64, 0x80),
                );
            }
        }

        // Frequency Markers (100Hz, 1kHz, 10kHz)
        let freq_markers = [
            (0.20, "100Hz"),
            (0.50, "1kHz"),
            (0.80, "10kHz"),
        ];
        for (ratio, freq_label) in freq_markers {
            let fx = rect.min.x + rect.width() * ratio;
            painter.line_segment(
                [Pos2::new(fx, rect.min.y + 4.0), Pos2::new(fx, rect.max.y - 4.0)],
                Stroke::new(0.8, Color32::from_rgb(0x1a, 0x20, 0x2c)),
            );
            painter.text(
                Pos2::new(fx, rect.max.y - 4.0),
                Align2::CENTER_BOTTOM,
                freq_label,
                FontId::monospace(8.5),
                Color32::from_rgb(0x56, 0x64, 0x80),
            );
        }

        // 3. Render 32 Animated Gradient Spectrum Analyzer Bars
        let num_bars = 32;
        let bar_gap = 1.5;
        let total_bar_space = rect.width() - 16.0;
        let bar_w = ((total_bar_space - (num_bars as f32 - 1.0) * bar_gap) / num_bars as f32).max(2.0);

        for i in 0..num_bars {
            let bar_val = if i < spectrum_bars.len() {
                spectrum_bars[i]
            } else {
                let norm = i as f32 / num_bars as f32;
                (norm * 4.0).sin().abs() * 0.45 + 0.1
            };

            let bx = rect.min.x + 8.0 + i as f32 * (bar_w + bar_gap);
            let bar_h = (rect.height() - 20.0) * bar_val.clamp(0.04, 0.95);
            let bar_rect = Rect::from_min_max(
                Pos2::new(bx, rect.max.y - 12.0 - bar_h),
                Pos2::new(bx + bar_w, rect.max.y - 12.0),
            );

            // Chroma gradient coloring across frequency spectrum
            let bar_color = if i < 10 {
                CHROMA_CYAN_2D
            } else if i < 20 {
                CHROMA_AMBER_AUDIO
            } else {
                CHROMA_CORAL_3D
            };

            painter.rect_filled(bar_rect, 1.0, bar_color.linear_multiply(0.40));

            // Peak cap line
            painter.line_segment(
                [Pos2::new(bar_rect.min.x, bar_rect.min.y), Pos2::new(bar_rect.max.x, bar_rect.min.y)],
                Stroke::new(1.5, bar_color),
            );
        }

        // 4. Glowing Translucent Bezier Frequency Response Curve
        // Model 3-band EQ curve using low shelf, mid peak, and high shelf response
        let num_curve_pts = 64;
        let mut curve_points = Vec::with_capacity(num_curve_pts);

        for i in 0..num_curve_pts {
            let t = i as f32 / (num_curve_pts as f32 - 1.0);
            let x = rect.min.x + t * rect.width();

            // Low shelf response (~ t < 0.35)
            let low_weight = (1.0 - t * 2.5).clamp(0.0, 1.0);
            let low_contrib = low_db * low_weight;

            // Mid bell curve centered at t = 0.50 (Gaussian shape)
            let mid_dist = (t - 0.50) / 0.18;
            let mid_weight = (-0.5 * mid_dist * mid_dist).exp();
            let mid_contrib = mid_db * mid_weight;

            // High shelf response (~ t > 0.65)
            let high_weight = ((t - 0.65) * 2.8).clamp(0.0, 1.0);
            let high_contrib = high_db * high_weight;

            let total_db = low_contrib + mid_contrib + high_contrib;
            let y = zero_y - total_db * db_scale;
            curve_points.push(Pos2::new(x, y.clamp(rect.min.y + 4.0, rect.max.y - 4.0)));
        }

        // Area fill under response curve
        let mut fill_polygon = Vec::with_capacity(num_curve_pts + 2);
        fill_polygon.push(Pos2::new(rect.min.x, zero_y));
        fill_polygon.extend_from_slice(&curve_points);
        fill_polygon.push(Pos2::new(rect.max.x, zero_y));
        painter.add(egui::Shape::convex_polygon(
            fill_polygon,
            STUDIO_OBSIDIAN_ACCENT.linear_multiply(0.18),
            Stroke::NONE,
        ));

        // Draw response curve stroke
        for w in curve_points.windows(2) {
            painter.line_segment([w[0], w[1]], Stroke::new(2.5, STUDIO_OBSIDIAN_ACCENT));
        }

        // 5. Band Nodes (Low, Mid, High)
        let band_nodes = [
            (0.16, low_db, "L", CHROMA_CYAN_2D),
            (0.50, mid_db, "M", CHROMA_AMBER_AUDIO),
            (0.84, high_db, "H", CHROMA_VIOLET_ANIM),
        ];

        for (t, db, label, color) in band_nodes {
            let bx = rect.min.x + t * rect.width();
            let by = zero_y - db * db_scale;
            let pt = Pos2::new(bx, by.clamp(rect.min.y + 4.0, rect.max.y - 4.0));

            painter.circle(pt, 5.0, STUDIO_OBSIDIAN_CANVAS, Stroke::new(1.8, color));
            painter.circle_filled(pt, 2.5, color);
            painter.text(
                Pos2::new(pt.x, pt.y - 9.0),
                Align2::CENTER_BOTTOM,
                format!("{label} {db:+.1}dB"),
                FontId::monospace(9.0),
                color,
            );
        }
    }
}
