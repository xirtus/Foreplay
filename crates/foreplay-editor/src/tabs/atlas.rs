//! Atlas Semantic Meaning Graph Dock (`foreplay-editor`).
//!
//! Visualizes the game semantic graph exported by `session.observe_atlas()`,
//! rendering nodes as cards and SignalConnection semantic edges as Bezier curves.

use std::collections::BTreeMap;
use egui::{
    epaint::CubicBezierShape, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Ui, Vec2,
};
use foreplay_ir::NodeId;
use crate::ForeplayEditorApp;
use crate::theme::{
    COLOR_ACCENT_BLUE, COLOR_BORDER, COLOR_PANEL_BG, COLOR_SUCCESS_GREEN,
    COLOR_TEXT_MUTED, COLOR_TEXT_NORMAL, COLOR_WARN_YELLOW, STUDIO_OBSIDIAN_CANVAS,
};

pub fn show(app: &mut ForeplayEditorApp, ui: &mut Ui) {
    let atlas_ctx = app.session.observe_atlas();

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Atlas Semantic Meaning Graph").strong());
        ui.colored_label(
            COLOR_TEXT_MUTED,
            format!("(Scene: '{}' | Nodes: {} | Wires: {})", atlas_ctx.scene_name, atlas_ctx.affected_nodes.len(), atlas_ctx.edges.len()),
        );

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("⟲ Reset View").clicked() {
                app.atlas_pan = Vec2::ZERO;
                app.atlas_zoom = 1.0;
            }
        });
    });

    ui.separator();

    // Time-Travel Pulse Scrubber Bar
    ui.horizontal(|ui| {
        let frame_count = app.atlas_recorder.len();
        let earliest = app.atlas_recorder.earliest_tick().unwrap_or(0);
        let latest = app.atlas_recorder.latest_tick().unwrap_or(0);

        ui.label(egui::RichText::new("⏱ Time-Travel Scrubber:").strong().color(crate::theme::CHROMA_GOLD_POLYAMOUR));
        ui.colored_label(COLOR_TEXT_MUTED, format!("{frame_count} frames recorded"));

        if earliest < latest {
            let mut current = app.atlas_scrub_tick.unwrap_or(latest);
            let slider = egui::Slider::new(&mut current, earliest..=latest)
                .text("Tick")
                .show_value(true);
            if ui.add(slider).changed() {
                app.atlas_scrub_tick = Some(current);
            }

            if ui.button("⏺ Live").clicked() {
                app.atlas_scrub_tick = None;
            }
        }

        if let Some(scrub_tick) = app.atlas_scrub_tick {
            if let Some(view) = app.atlas_recorder.scrub_to_tick(scrub_tick) {
                ui.separator();
                ui.colored_label(
                    crate::theme::CHROMA_MINT_UI,
                    format!("Tick #{}: {} pulses, {} feel changes (hash: {:08x})",
                        view.tick, view.active_pulses.len(), view.active_feel_changes.len(), view.state_hash),
                );
            }
        } else {
            ui.separator();
            ui.colored_label(COLOR_SUCCESS_GREEN, "● Live Tracking");
        }
    });

    ui.separator();

    let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
    let painter = ui.painter_at(rect);

    // macOS 120Hz Trackpad & Mouse Pan & Zoom Ergonomics
    let input = ui.input(|i| i.clone());
    if response.dragged_by(egui::PointerButton::Secondary) || response.dragged_by(egui::PointerButton::Middle) {
        app.atlas_pan += response.drag_delta();
    }
    if response.hovered() {
        let pinch = input.zoom_delta();
        if (pinch - 1.0).abs() > 0.001 {
            app.atlas_zoom = (app.atlas_zoom * pinch).clamp(0.2, 5.0);
        }
        if input.smooth_scroll_delta != Vec2::ZERO && !response.dragged() {
            if input.modifiers.command || input.modifiers.ctrl {
                let zoom_delta = input.smooth_scroll_delta.y * 0.002;
                app.atlas_zoom = (app.atlas_zoom * (1.0 + zoom_delta)).clamp(0.2, 5.0);
            } else {
                app.atlas_pan += input.smooth_scroll_delta;
            }
        }
    }

    // Background
    painter.rect_filled(rect, 0.0, STUDIO_OBSIDIAN_CANVAS);

    // Grid dots/lines
    draw_canvas_grid(&painter, rect, app.atlas_pan);

    let nodes = &app.session.tree.active_scene.nodes;
    let mut card_rects: BTreeMap<NodeId, Rect> = BTreeMap::new();

    // Layout cards in grid columns, translated by atlas_pan
    let card_width = 190.0 * app.atlas_zoom;
    let card_height = 80.0 * app.atlas_zoom;
    let col_spacing = 260.0 * app.atlas_zoom;
    let row_spacing = 110.0 * app.atlas_zoom;

    let start_x = rect.min.x + 40.0 + app.atlas_pan.x;
    let start_y = rect.min.y + 40.0 + app.atlas_pan.y;

    for (i, &node_id) in atlas_ctx.affected_nodes.iter().enumerate() {
        let col = i % 3;
        let row = i / 3;

        let card_pos = Pos2::new(start_x + col as f32 * col_spacing, start_y + row as f32 * row_spacing);
        let card_rect = Rect::from_min_size(card_pos, Vec2::new(card_width, card_height));
        card_rects.insert(node_id, card_rect);

        let is_selected = app.selected_node == Some(node_id);

        // Draw card background
        let bg_fill = if is_selected {
            Color32::from_rgb(0x2d, 0x37, 0x48)
        } else {
            COLOR_PANEL_BG
        };
        let stroke = if is_selected {
            Stroke::new(2.0, COLOR_WARN_YELLOW)
        } else {
            Stroke::new(1.0, COLOR_BORDER)
        };

        painter.rect(card_rect, 6.0, bg_fill, stroke, egui::StrokeKind::Inside);

        // Card header
        let header_rect = Rect::from_min_size(card_rect.min, Vec2::new(card_width, 24.0 * app.atlas_zoom));
        painter.rect_filled(header_rect, 6.0, Color32::from_rgb(0x18, 0x1b, 0x22));

        if let Some(node) = nodes.get(&node_id) {
            painter.text(
                header_rect.min + Vec2::new(8.0, 4.0),
                Align2::LEFT_TOP,
                &node.name,
                FontId::proportional(12.0 * app.atlas_zoom),
                if is_selected { COLOR_WARN_YELLOW } else { Color32::WHITE },
            );

            painter.text(
                card_rect.min + Vec2::new(8.0, 32.0 * app.atlas_zoom),
                Align2::LEFT_TOP,
                format!("Class: {}", node.class_name),
                FontId::proportional(11.0 * app.atlas_zoom),
                COLOR_SUCCESS_GREEN,
            );

            painter.text(
                card_rect.min + Vec2::new(8.0, 50.0 * app.atlas_zoom),
                Align2::LEFT_TOP,
                format!("NodeId({}:{})", node_id.index, node_id.generation),
                FontId::proportional(10.0 * app.atlas_zoom),
                COLOR_TEXT_MUTED,
            );
        }

        // Port circles: input on left, output on right
        let in_port = card_rect.left_center();
        let out_port = card_rect.right_center();
        painter.circle_filled(in_port, 4.0 * app.atlas_zoom, COLOR_ACCENT_BLUE);
        painter.circle_filled(out_port, 4.0 * app.atlas_zoom, COLOR_WARN_YELLOW);
    }

    // Draw Bezier curves for Semantic Edges
    for edge in &atlas_ctx.edges {
        let Some(&from_rect) = card_rects.get(&edge.from) else {
            continue;
        };
        let Some(&to_rect) = card_rects.get(&edge.to) else {
            continue;
        };

        let start = from_rect.right_center();
        let end = to_rect.left_center();

        let dx = (end.x - start.x).abs() * 0.5 + 40.0;
        let c1 = start + Vec2::new(dx, 0.0);
        let c2 = end - Vec2::new(dx, 0.0);

        let shape = CubicBezierShape::from_points_stroke(
            [start, c1, c2, end],
            false,
            Color32::TRANSPARENT,
            Stroke::new(2.5, COLOR_ACCENT_BLUE),
        );
        painter.add(shape);

        // Arrow head at destination
        let arrow_len = 8.0 * app.atlas_zoom;
        painter.circle_filled(end, 3.0 * app.atlas_zoom, COLOR_ACCENT_BLUE);
        painter.line_segment([end - Vec2::new(arrow_len, 4.0), end], Stroke::new(2.0, COLOR_ACCENT_BLUE));
        painter.line_segment([end - Vec2::new(arrow_len, -4.0), end], Stroke::new(2.0, COLOR_ACCENT_BLUE));

        // Relation badge at curve midpoint
        let mid = Pos2::new((start.x + end.x) * 0.5, (start.y + end.y) * 0.5);
        painter.rect_filled(
            Rect::from_center_size(mid, Vec2::new(70.0 * app.atlas_zoom, 18.0 * app.atlas_zoom)),
            3.0,
            Color32::from_rgb(0x18, 0x1b, 0x22),
        );
        painter.text(
            mid,
            Align2::CENTER_CENTER,
            format!("{:?}", edge.relation),
            FontId::proportional(10.0 * app.atlas_zoom),
            COLOR_TEXT_NORMAL,
        );
    }
}

fn draw_canvas_grid(painter: &egui::Painter, rect: Rect, pan: Vec2) {
    let step = 32.0;
    let grid_stroke = Stroke::new(1.0, Color32::from_rgb(0x18, 0x1c, 0x26));

    let offset_x = pan.x % step;
    let mut x = rect.min.x + offset_x;
    while x <= rect.max.x {
        painter.line_segment([Pos2::new(x, rect.min.y), Pos2::new(x, rect.max.y)], grid_stroke);
        x += step;
    }

    let offset_y = pan.y % step;
    let mut y = rect.min.y + offset_y;
    while y <= rect.max.y {
        painter.line_segment([Pos2::new(rect.min.x, y), Pos2::new(rect.max.x, y)], grid_stroke);
        y += step;
    }
}
