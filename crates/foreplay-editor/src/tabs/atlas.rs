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
    COLOR_ACCENT_BLUE, COLOR_BG_DARK, COLOR_BORDER, COLOR_PANEL_BG, COLOR_SUCCESS_GREEN,
    COLOR_TEXT_MUTED, COLOR_TEXT_NORMAL, COLOR_WARN_YELLOW,
};

pub fn show(app: &mut ForeplayEditorApp, ui: &mut Ui) {
    let atlas_ctx = app.session.observe_atlas();

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Atlas Semantic Meaning Graph").strong());
        ui.colored_label(
            COLOR_TEXT_MUTED,
            format!("(Scene: '{}' | Nodes: {} | Wires: {})", atlas_ctx.scene_name, atlas_ctx.affected_nodes.len(), atlas_ctx.edges.len()),
        );
    });

    let (rect, _response) = ui.allocate_exact_size(ui.available_size(), Sense::click());
    let painter = ui.painter_at(rect);

    // Background
    painter.rect_filled(rect, 0.0, COLOR_BG_DARK);

    // Grid dots/lines
    draw_canvas_grid(&painter, rect);

    let nodes = &app.session.tree.active_scene.nodes;
    let mut card_rects: BTreeMap<NodeId, Rect> = BTreeMap::new();

    // Layout cards in grid columns
    let card_width = 190.0;
    let card_height = 80.0;
    let col_spacing = 260.0;
    let row_spacing = 110.0;

    let start_x = rect.min.x + 40.0;
    let start_y = rect.min.y + 40.0;

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
        let header_rect = Rect::from_min_size(card_rect.min, Vec2::new(card_width, 24.0));
        painter.rect_filled(header_rect, 6.0, Color32::from_rgb(0x18, 0x1b, 0x22));

        if let Some(node) = nodes.get(&node_id) {
            painter.text(
                header_rect.min + Vec2::new(8.0, 4.0),
                Align2::LEFT_TOP,
                &node.name,
                FontId::proportional(12.0),
                if is_selected { COLOR_WARN_YELLOW } else { Color32::WHITE },
            );

            painter.text(
                card_rect.min + Vec2::new(8.0, 32.0),
                Align2::LEFT_TOP,
                format!("Class: {}", node.class_name),
                FontId::proportional(11.0),
                COLOR_SUCCESS_GREEN,
            );

            painter.text(
                card_rect.min + Vec2::new(8.0, 50.0),
                Align2::LEFT_TOP,
                format!("NodeId({}:{})", node_id.index, node_id.generation),
                FontId::proportional(10.0),
                COLOR_TEXT_MUTED,
            );
        }

        // Port circles: input on left, output on right
        let in_port = card_rect.left_center();
        let out_port = card_rect.right_center();
        painter.circle_filled(in_port, 4.0, COLOR_ACCENT_BLUE);
        painter.circle_filled(out_port, 4.0, COLOR_WARN_YELLOW);
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
        let arrow_len = 8.0;
        painter.circle_filled(end, 3.0, COLOR_ACCENT_BLUE);
        painter.line_segment([end - Vec2::new(arrow_len, 4.0), end], Stroke::new(2.0, COLOR_ACCENT_BLUE));
        painter.line_segment([end - Vec2::new(arrow_len, -4.0), end], Stroke::new(2.0, COLOR_ACCENT_BLUE));

        // Relation badge at curve midpoint
        let mid = Pos2::new(
            0.125 * start.x + 0.375 * c1.x + 0.375 * c2.x + 0.125 * end.x,
            0.125 * start.y + 0.375 * c1.y + 0.375 * c2.y + 0.125 * end.y,
        );

        let badge_rect = Rect::from_center_size(mid, Vec2::new(140.0, 20.0));
        painter.rect_filled(badge_rect, 3.0, Color32::from_rgba_unmultiplied(20, 24, 30, 220));
        painter.rect_stroke(badge_rect, 3.0, Stroke::new(1.0, COLOR_ACCENT_BLUE), egui::StrokeKind::Inside);
        painter.text(
            mid,
            Align2::CENTER_CENTER,
            &edge.relation,
            FontId::proportional(10.0),
            COLOR_TEXT_NORMAL,
        );
    }
}

fn draw_canvas_grid(painter: &egui::Painter, rect: Rect) {
    let step = 30.0;
    let stroke = Stroke::new(1.0, Color32::from_rgba_unmultiplied(40, 46, 56, 120));

    let mut x = rect.min.x;
    while x <= rect.max.x {
        painter.line_segment([Pos2::new(x, rect.min.y), Pos2::new(x, rect.max.y)], stroke);
        x += step;
    }

    let mut y = rect.min.y;
    while y <= rect.max.y {
        painter.line_segment([Pos2::new(rect.min.x, y), Pos2::new(rect.max.x, y)], stroke);
        y += step;
    }
}
