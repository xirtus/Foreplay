//! Interactive 2D Viewport Dock (`foreplay-editor`).
//!
//! Features:
//! - 2D pan and zoom camera canvas.
//! - 2D coordinate grid & colored origin axes.
//! - Bounding boxes and labels for 2D nodes (`Node2D`, `Sprite2D`, etc.).
//! - Selection highlights and interactive 2D translate handles dispatching `EditorCommand::SetProperty`.
//! - HUD overlay with active workspace info.

use egui::{
    Align2, Color32, FontId, PointerButton, Pos2, Rect, Sense, Stroke, Ui, Vec2,
};
use foreplay_ir::{EditorCommand, IrValue};
use crate::ForeplayEditorApp;
use crate::theme::{
    COLOR_ACCENT_BLUE, COLOR_AXIS_X, COLOR_AXIS_Y, COLOR_BG_DARK, COLOR_BORDER,
    COLOR_TEXT_MUTED, COLOR_TEXT_NORMAL, COLOR_WARN_YELLOW,
};

pub fn show(app: &mut ForeplayEditorApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.label("2D Viewport | Pan: Right-Drag | Zoom: Scroll");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("⟲ Reset 2D View").clicked() {
                app.camera_2d = Default::default();
            }
        });
    });

    let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
    let painter = ui.painter_at(rect);

    painter.rect_filled(rect, 0.0, COLOR_BG_DARK);

    // Pan & Zoom controls
    if response.dragged_by(PointerButton::Secondary) || response.dragged_by(PointerButton::Middle) {
        let delta = response.drag_delta();
        app.camera_2d.pan += Vec2::new(delta.x / app.camera_2d.zoom, delta.y / app.camera_2d.zoom);
    }

    let input = ui.input(|i| i.clone());
    if response.hovered() && input.smooth_scroll_delta.y != 0.0 {
        let zoom_delta = input.smooth_scroll_delta.y * 0.002;
        app.camera_2d.zoom = (app.camera_2d.zoom * (1.0 + zoom_delta)).clamp(0.1, 10.0);
    }

    // 1. Draw 2D Grid
    draw_2d_grid(&painter, rect, app.camera_2d.pan, app.camera_2d.zoom);

    // 2. Draw 2D Origin Axes
    let origin_screen = app.camera_2d.to_screen(Vec2::ZERO, rect);
    painter.line_segment(
        [origin_screen, origin_screen + Vec2::new(60.0, 0.0)],
        Stroke::new(2.0, COLOR_AXIS_X),
    );
    painter.text(
        origin_screen + Vec2::new(65.0, 0.0),
        Align2::LEFT_CENTER,
        "X",
        FontId::proportional(11.0),
        COLOR_AXIS_X,
    );

    painter.line_segment(
        [origin_screen, origin_screen + Vec2::new(0.0, 60.0)],
        Stroke::new(2.0, COLOR_AXIS_Y),
    );
    painter.text(
        origin_screen + Vec2::new(0.0, 65.0),
        Align2::CENTER_TOP,
        "Y",
        FontId::proportional(11.0),
        COLOR_AXIS_Y,
    );

    // 3. Draw 2D Nodes
    let nodes = app.session.tree.active_scene.nodes.clone();
    for (&node_id, node) in &nodes {
        let pos = get_vec2_prop(node, "position").unwrap_or(Vec2::ZERO);
        let scale = get_vec2_prop(node, "scale").unwrap_or(Vec2::ONE);
        let is_selected = app.selected_node == Some(node_id);

        let screen_center = app.camera_2d.to_screen(pos, rect);
        let size = Vec2::new(scale.x * 48.0 * app.camera_2d.zoom, scale.y * 48.0 * app.camera_2d.zoom);
        let node_rect = Rect::from_center_size(screen_center, size);

        let fill = if is_selected {
            Color32::from_rgba_unmultiplied(71, 140, 191, 80)
        } else {
            Color32::from_rgba_unmultiplied(44, 49, 60, 160)
        };
        let stroke = if is_selected {
            Stroke::new(2.0, COLOR_WARN_YELLOW)
        } else {
            Stroke::new(1.0, COLOR_BORDER)
        };

        painter.rect(node_rect, 3.0, fill, stroke, egui::StrokeKind::Inside);
        painter.text(
            node_rect.center_bottom() + Vec2::new(0.0, 4.0),
            Align2::CENTER_TOP,
            &node.name,
            FontId::proportional(11.0),
            if is_selected { COLOR_WARN_YELLOW } else { COLOR_TEXT_NORMAL },
        );

        // Interactive translate dragging on selected 2D node
        if is_selected && response.dragged_by(PointerButton::Primary) {
            let delta = response.drag_delta();
            if delta.length_sq() > 0.0 {
                let world_delta = Vec2::new(delta.x / app.camera_2d.zoom, delta.y / app.camera_2d.zoom);
                let new_pos = pos + world_delta;
                let cmd = EditorCommand::SetProperty {
                    node_id,
                    key: "position".into(),
                    old_value: Some(IrValue::Vec2([pos.x, pos.y])),
                    new_value: Some(IrValue::Vec2([new_pos.x, new_pos.y])),
                };
                app.execute_command(cmd);
            }
        }
    }

    // 4. HUD overlay
    let hud_rect = Rect::from_min_size(rect.min + Vec2::new(12.0, 12.0), Vec2::new(210.0, 50.0));
    painter.rect_filled(hud_rect, 4.0, Color32::from_rgba_unmultiplied(26, 29, 36, 220));
    painter.rect_stroke(hud_rect, 4.0, Stroke::new(1.0, COLOR_BORDER), egui::StrokeKind::Inside);
    painter.text(
        hud_rect.min + Vec2::new(8.0, 8.0),
        Align2::LEFT_TOP,
        "2D Canvas Viewport",
        FontId::proportional(12.0),
        COLOR_ACCENT_BLUE,
    );
    painter.text(
        hud_rect.min + Vec2::new(8.0, 26.0),
        Align2::LEFT_TOP,
        format!("Zoom: {:.1}x | Nodes: {}", app.camera_2d.zoom, nodes.len()),
        FontId::proportional(11.0),
        COLOR_TEXT_MUTED,
    );
}

fn draw_2d_grid(painter: &egui::Painter, rect: Rect, pan: Vec2, zoom: f32) {
    let step = 50.0 * zoom;
    if step < 10.0 {
        return;
    }

    let offset_x = (rect.center().x + pan.x * zoom) % step;
    let offset_y = (rect.center().y + pan.y * zoom) % step;
    let stroke = Stroke::new(1.0, Color32::from_rgba_unmultiplied(50, 56, 68, 80));

    let mut x = rect.min.x + offset_x;
    while x <= rect.max.x {
        painter.line_segment([Pos2::new(x, rect.min.y), Pos2::new(x, rect.max.y)], stroke);
        x += step;
    }

    let mut y = rect.min.y + offset_y;
    while y <= rect.max.y {
        painter.line_segment([Pos2::new(rect.min.x, y), Pos2::new(rect.max.x, y)], stroke);
        y += step;
    }
}

fn get_vec2_prop(node: &foreplay_ir::IrNode, key: &str) -> Option<Vec2> {
    match node.properties.get(key) {
        Some(IrValue::Vec2(v)) => Some(Vec2::new(v[0], v[1])),
        _ => None,
    }
}
