//! Interactive 2D Viewport Dock (`foreplay-editor`).
//!
//! Features:
//! - 2D pan and zoom camera canvas.
//! - Toolbar with Grid Snap and Pixel Snap toggles.
//! - Godot Control Anchor Preset buttons (Top Left, Center, Full Rect, Top Wide) when a UI node is selected.
//! - Standard purple 1280×720 reference frame boundary.
//! - 2D coordinate grid & colored origin axes.
//! - Bounding boxes and labels for 2D nodes (`Node2D`, `Sprite2D`, `Control`, etc.).
//! - Selection highlights and interactive 2D translate handles dispatching `EditorCommand::SetProperty`.
//! - HUD overlay with zoom and node statistics.

use egui::{
    Align2, Color32, FontId, PointerButton, Pos2, Rect, Sense, Stroke, Ui, Vec2,
};
use foreplay_ir::{EditorCommand, IrValue};
use crate::ForeplayEditorApp;
use crate::theme::{
    COLOR_ACCENT_BLUE, COLOR_AXIS_X, COLOR_AXIS_Y, COLOR_BG_DARK, COLOR_BORDER,
    COLOR_SUCCESS_GREEN, COLOR_TEXT_MUTED, COLOR_TEXT_NORMAL, COLOR_WARN_YELLOW,
};

pub fn show(app: &mut ForeplayEditorApp, ui: &mut Ui) {
    // Top Toolbar
    ui.horizontal(|ui| {
        // Grid Snap toggle
        let grid_label = if app.snap_2d_grid { "🧲 Grid Snap (16px): ON" } else { "🧲 Grid Snap: OFF" };
        let grid_color = if app.snap_2d_grid { COLOR_SUCCESS_GREEN } else { COLOR_TEXT_MUTED };
        if ui.button(egui::RichText::new(grid_label).color(grid_color)).clicked() {
            app.snap_2d_grid = !app.snap_2d_grid;
        }

        // Pixel Snap toggle
        let pixel_label = if app.snap_2d_pixel { "🔲 Pixel Snap: ON" } else { "🔲 Pixel Snap: OFF" };
        let pixel_color = if app.snap_2d_pixel { COLOR_SUCCESS_GREEN } else { COLOR_TEXT_MUTED };
        if ui.button(egui::RichText::new(pixel_label).color(pixel_color)).clicked() {
            app.snap_2d_pixel = !app.snap_2d_pixel;
        }

        ui.separator();

        // Control UI Anchor Presets if selected node is a Control node
        let selected_control = app.selected_node.and_then(|id| {
            let n = app.session.tree.active_scene.nodes.get(&id)?;
            if n.class_name.contains("Control") || n.class_name.contains("HUD") || n.class_name.contains("Label") || n.class_name.contains("Button") {
                Some((id, n.clone()))
            } else {
                None
            }
        });

        if let Some((selected_id, node)) = selected_control {
            ui.label("Anchors:");
            if ui.button("Top-Left").on_hover_text("Position (0, 0), Size (200, 50)").clicked() {
                apply_anchor_preset(app, selected_id, &node, Vec2::new(0.0, 0.0), Vec2::new(200.0, 50.0));
            }
            if ui.button("Center").on_hover_text("Position (540, 335), Size (200, 50)").clicked() {
                apply_anchor_preset(app, selected_id, &node, Vec2::new(540.0, 335.0), Vec2::new(200.0, 50.0));
            }
            if ui.button("Full Rect").on_hover_text("Position (0, 0), Size (1280, 720)").clicked() {
                apply_anchor_preset(app, selected_id, &node, Vec2::new(0.0, 0.0), Vec2::new(1280.0, 720.0));
            }
            if ui.button("Top Wide").on_hover_text("Position (0, 0), Size (1280, 60)").clicked() {
                apply_anchor_preset(app, selected_id, &node, Vec2::new(0.0, 0.0), Vec2::new(1280.0, 60.0));
            }
            ui.separator();
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("⟲ Reset 2D View").clicked() {
                app.camera_2d = Default::default();
            }
        });
    });

    let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
    let painter = ui.painter_at(rect);

    painter.rect_filled(rect, 0.0, COLOR_BG_DARK);

    // Pan & Zoom controls (macOS 120Hz Trackpad & Mouse Ergonomics)
    if response.dragged_by(PointerButton::Secondary) || response.dragged_by(PointerButton::Middle) {
        let delta = response.drag_delta();
        app.camera_2d.pan += Vec2::new(delta.x / app.camera_2d.zoom, delta.y / app.camera_2d.zoom);
    }

    let input = ui.input(|i| i.clone());
    if response.hovered() {
        // Trackpad pinch-to-zoom gesture
        let pinch = input.zoom_delta();
        if (pinch - 1.0).abs() > 0.001 {
            app.camera_2d.zoom = (app.camera_2d.zoom * pinch).clamp(0.1, 10.0);
        }

        // Smooth two-finger scroll-pan & wheel zoom
        if input.smooth_scroll_delta != Vec2::ZERO && !response.dragged() {
            if input.modifiers.command || input.modifiers.ctrl {
                let zoom_delta = input.smooth_scroll_delta.y * 0.002;
                app.camera_2d.zoom = (app.camera_2d.zoom * (1.0 + zoom_delta)).clamp(0.1, 10.0);
            } else {
                app.camera_2d.pan += Vec2::new(
                    input.smooth_scroll_delta.x / app.camera_2d.zoom,
                    input.smooth_scroll_delta.y / app.camera_2d.zoom,
                );
            }
        }
    }

    // 1. Draw 2D Grid
    draw_2d_grid(&painter, rect, app.camera_2d.pan, app.camera_2d.zoom);

    // 2. Draw 1280×720 Viewport Boundary Frame (purple outline)
    let p_min = app.camera_2d.to_screen(Vec2::ZERO, rect);
    let p_max = app.camera_2d.to_screen(Vec2::new(1280.0, 720.0), rect);
    let frame_rect = Rect::from_two_pos(p_min, p_max);
    let purple_color = Color32::from_rgb(180, 100, 240);
    painter.rect_stroke(frame_rect, 0.0, Stroke::new(1.8, purple_color), egui::StrokeKind::Inside);
    painter.text(
        frame_rect.left_top() + Vec2::new(8.0, 8.0),
        Align2::LEFT_TOP,
        "1280×720 Viewport Reference",
        FontId::proportional(11.0),
        purple_color,
    );

    // 3. Draw 2D Origin Axes
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

    // 4. Draw 2D Nodes
    let nodes = app.session.tree.active_scene.nodes.clone();
    for (&node_id, node) in &nodes {
        let pos = get_vec2_prop(node, "position").unwrap_or(Vec2::ZERO);
        let scale = get_vec2_prop(node, "scale").unwrap_or(Vec2::new(1.0, 1.0));
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
                let mut new_pos = pos + world_delta;

                if app.snap_2d_grid {
                    new_pos.x = (new_pos.x / 16.0).round() * 16.0;
                    new_pos.y = (new_pos.y / 16.0).round() * 16.0;
                } else if app.snap_2d_pixel {
                    new_pos.x = new_pos.x.round();
                    new_pos.y = new_pos.y.round();
                }

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

    // 5. HUD overlay
    let hud_rect = Rect::from_min_size(rect.min + Vec2::new(12.0, 12.0), Vec2::new(220.0, 56.0));
    painter.rect_filled(hud_rect, 4.0, Color32::from_rgba_unmultiplied(26, 29, 36, 220));
    painter.rect_stroke(hud_rect, 4.0, Stroke::new(1.0, COLOR_BORDER), egui::StrokeKind::Inside);
    painter.text(
        hud_rect.min + Vec2::new(8.0, 8.0),
        Align2::LEFT_TOP,
        "2D Canvas Viewport",
        FontId::proportional(12.0),
        COLOR_ACCENT_BLUE,
    );
    let snap_status = if app.snap_2d_grid { "Grid 16px" } else if app.snap_2d_pixel { "Pixel" } else { "Off" };
    painter.text(
        hud_rect.min + Vec2::new(8.0, 28.0),
        Align2::LEFT_TOP,
        format!("Zoom: {:.1}x | Snap: {snap_status} | Nodes: {}", app.camera_2d.zoom, nodes.len()),
        FontId::proportional(11.0),
        COLOR_TEXT_MUTED,
    );
}

fn apply_anchor_preset(
    app: &mut ForeplayEditorApp,
    node_id: foreplay_ir::NodeId,
    node: &foreplay_ir::IrNode,
    new_pos: Vec2,
    new_scale: Vec2,
) {
    let old_pos = get_vec2_prop(node, "position").unwrap_or(Vec2::ZERO);
    let old_scale = get_vec2_prop(node, "scale").unwrap_or(Vec2::ONE);

    app.execute_command(EditorCommand::SetProperty {
        node_id,
        key: "position".into(),
        old_value: Some(IrValue::Vec2([old_pos.x, old_pos.y])),
        new_value: Some(IrValue::Vec2([new_pos.x, new_pos.y])),
    });

    app.execute_command(EditorCommand::SetProperty {
        node_id,
        key: "scale".into(),
        old_value: Some(IrValue::Vec2([old_scale.x, old_scale.y])),
        new_value: Some(IrValue::Vec2([new_scale.x, new_scale.y])),
    });
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
