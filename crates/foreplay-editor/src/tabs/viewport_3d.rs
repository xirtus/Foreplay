//! Interactive 3D Viewport Dock (`foreplay-editor`).
//!
//! Features:
//! - Orbit / Pan / Zoom 3D perspective camera (projected onto `egui::Painter`).
//! - 3D ground plane grid & colored X/Y/Z origin axes.
//! - Wireframe 3D bounding boxes for every node in `active_scene.nodes`.
//! - Selection highlights with node identification labels.
//! - Interactive Translate (`W`) and Scale (`R`) 3D Gizmos dispatching `EditorCommand::SetProperty`.
//! - HUD overlay with live FPS, node count, tick count, workspace, and PIE state.

use bevy_math::Vec3;
use egui::{
    Align2, Color32, FontId, Key, PointerButton, Pos2, Rect, Sense, Stroke, Ui, Vec2,
};
use foreplay_ir::{EditorCommand, IrValue, NodeId};
use crate::ForeplayEditorApp;
use crate::camera::{GizmoAxis, GizmoMode};
use crate::theme::{
    COLOR_ACCENT_BLUE, COLOR_AXIS_X, COLOR_AXIS_Y, COLOR_AXIS_Z, COLOR_BG_DARK, COLOR_BORDER,
    COLOR_SUCCESS_GREEN, COLOR_TEXT_MUTED, COLOR_TEXT_NORMAL, COLOR_WARN_YELLOW,
};

pub fn show(app: &mut ForeplayEditorApp, ui: &mut Ui) {
    // Mode toolbar at the top of the viewport
    ui.horizontal(|ui| {
        ui.label("Gizmo Mode:");
        if ui
            .selectable_label(app.gizmo_mode == GizmoMode::Select, "🔍 Select [Q]")
            .clicked()
        {
            app.gizmo_mode = GizmoMode::Select;
        }
        if ui
            .selectable_label(app.gizmo_mode == GizmoMode::Translate, "✥ Translate [W]")
            .clicked()
        {
            app.gizmo_mode = GizmoMode::Translate;
        }
        if ui
            .selectable_label(app.gizmo_mode == GizmoMode::Scale, "⤢ Scale [R]")
            .clicked()
        {
            app.gizmo_mode = GizmoMode::Scale;
        }

        ui.separator();
        ui.label(RichTextHelper::muted("Right-Drag: Orbit | Shift+Right-Drag: Pan | Scroll: Zoom"));

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("⟲ Reset Camera").clicked() {
                app.camera_3d = Default::default();
            }
        });
    });

    // Check keyboard shortcuts
    if ui.input(|i| i.key_pressed(Key::Q)) {
        app.gizmo_mode = GizmoMode::Select;
    }
    if ui.input(|i| i.key_pressed(Key::W)) {
        app.gizmo_mode = GizmoMode::Translate;
    }
    if ui.input(|i| i.key_pressed(Key::R)) {
        app.gizmo_mode = GizmoMode::Scale;
    }

    // Allocate entire remaining viewport area
    let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
    let painter = ui.painter_at(rect);

    // Draw background
    painter.rect_filled(rect, 0.0, COLOR_BG_DARK);

    // Handle Camera Controls
    let input = ui.input(|i| i.clone());
    if response.dragged_by(PointerButton::Secondary) || response.dragged_by(PointerButton::Middle) {
        if input.modifiers.shift {
            app.camera_3d.pan(response.drag_delta());
        } else {
            app.camera_3d.orbit(response.drag_delta());
        }
    }

    if response.hovered() && input.smooth_scroll_delta.y != 0.0 {
        let factor = 1.0 - input.smooth_scroll_delta.y * 0.002;
        app.camera_3d.zoom(factor);
    }

    // 1. Draw 3D Ground Grid (Y = 0)
    draw_ground_grid(&painter, &app.camera_3d, rect);

    // 2. Draw 3D World Origin Axes (X: red, Y: green, Z: blue)
    draw_origin_axes(&painter, &app.camera_3d, rect);

    // 3. Draw Wireframe 3D Bounding Boxes for every node
    let nodes = app.session.tree.active_scene.nodes.clone();
    for (&node_id, node) in &nodes {
        let pos = get_vec3_prop(node, "position").unwrap_or(Vec3::ZERO);
        let scale = get_vec3_prop(node, "scale").unwrap_or(Vec3::ONE);
        let is_selected = app.selected_node == Some(node_id);

        draw_node_wireframe_box(&painter, &app.camera_3d, rect, pos, scale, &node.name, is_selected);
    }

    // 4. Draw & Interact with Gizmo for selected node
    if let Some(selected_id) = app.selected_node
        && app.gizmo_mode != GizmoMode::Select
        && let Some(node) = nodes.get(&selected_id)
    {
        handle_gizmo_interaction(app, rect, &response, &painter, selected_id, node);
    }

    // 5. Draw HUD stats overlay
    draw_hud_overlay(&painter, rect, app);
}

fn draw_ground_grid(painter: &egui::Painter, camera: &crate::camera::Camera3D, rect: Rect) {
    let grid_size = 14.0;
    let step = 2.0;
    let stroke = Stroke::new(1.0, Color32::from_rgba_unmultiplied(60, 68, 80, 100));

    let mut x = -grid_size;
    while x <= grid_size {
        let p1 = Vec3::new(x, 0.0, -grid_size);
        let p2 = Vec3::new(x, 0.0, grid_size);
        if let (Some(s1), Some(s2)) = (camera.project(p1, rect), camera.project(p2, rect)) {
            painter.line_segment([s1, s2], stroke);
        }
        x += step;
    }

    let mut z = -grid_size;
    while z <= grid_size {
        let p1 = Vec3::new(-grid_size, 0.0, z);
        let p2 = Vec3::new(grid_size, 0.0, z);
        if let (Some(s1), Some(s2)) = (camera.project(p1, rect), camera.project(p2, rect)) {
            painter.line_segment([s1, s2], stroke);
        }
        z += step;
    }
}

fn draw_origin_axes(painter: &egui::Painter, camera: &crate::camera::Camera3D, rect: Rect) {
    let origin = Vec3::ZERO;
    let Some(o_2d) = camera.project(origin, rect) else {
        return;
    };

    let len = 2.0;
    let x_pt = camera.project(Vec3::new(len, 0.0, 0.0), rect);
    let y_pt = camera.project(Vec3::new(0.0, len, 0.0), rect);
    let z_pt = camera.project(Vec3::new(0.0, 0.0, len), rect);

    if let Some(x_2d) = x_pt {
        painter.line_segment([o_2d, x_2d], Stroke::new(2.0, COLOR_AXIS_X));
        painter.text(x_2d, Align2::LEFT_CENTER, "X", FontId::proportional(11.0), COLOR_AXIS_X);
    }
    if let Some(y_2d) = y_pt {
        painter.line_segment([o_2d, y_2d], Stroke::new(2.0, COLOR_AXIS_Y));
        painter.text(y_2d, Align2::CENTER_BOTTOM, "Y", FontId::proportional(11.0), COLOR_AXIS_Y);
    }
    if let Some(z_2d) = z_pt {
        painter.line_segment([o_2d, z_2d], Stroke::new(2.0, COLOR_AXIS_Z));
        painter.text(z_2d, Align2::LEFT_CENTER, "Z", FontId::proportional(11.0), COLOR_AXIS_Z);
    }
}

fn draw_node_wireframe_box(
    painter: &egui::Painter,
    camera: &crate::camera::Camera3D,
    rect: Rect,
    pos: Vec3,
    scale: Vec3,
    name: &str,
    is_selected: bool,
) {
    let hx = (scale.x * 0.5).max(0.1);
    let hy = (scale.y * 0.5).max(0.1);
    let hz = (scale.z * 0.5).max(0.1);

    // 8 box vertices
    let v = [
        pos + Vec3::new(-hx, -hy, -hz), // 0: ---
        pos + Vec3::new(hx, -hy, -hz),  // 1: +--
        pos + Vec3::new(hx, hy, -hz),   // 2: ++-
        pos + Vec3::new(-hx, hy, -hz),  // 3: -+-
        pos + Vec3::new(-hx, -hy, hz),  // 4: --+
        pos + Vec3::new(hx, -hy, hz),   // 5: +-+
        pos + Vec3::new(hx, hy, hz),    // 6: +++
        pos + Vec3::new(-hx, hy, hz),   // 7: -++
    ];

    let mut screen_pts = [None; 8];
    for i in 0..8 {
        screen_pts[i] = camera.project(v[i], rect);
    }

    let stroke = if is_selected {
        Stroke::new(2.2, COLOR_WARN_YELLOW)
    } else {
        Stroke::new(1.0, Color32::from_rgb(100, 150, 190))
    };

    // 12 edges
    let edges = [
        (0, 1), (1, 2), (2, 3), (3, 0), // back face
        (4, 5), (5, 6), (6, 7), (7, 4), // front face
        (0, 4), (1, 5), (2, 6), (3, 7), // connecting
    ];

    for (a, b) in edges {
        if let (Some(pa), Some(pb)) = (screen_pts[a], screen_pts[b]) {
            painter.line_segment([pa, pb], stroke);
        }
    }

    // Draw name tag above the box
    if let Some(top_center_2d) = camera.project(pos + Vec3::new(0.0, hy + 0.3, 0.0), rect) {
        let tag_color = if is_selected {
            COLOR_WARN_YELLOW
        } else {
            COLOR_TEXT_NORMAL
        };
        painter.text(
            top_center_2d,
            Align2::CENTER_BOTTOM,
            name,
            FontId::proportional(12.0),
            tag_color,
        );
    }
}

fn handle_gizmo_interaction(
    app: &mut ForeplayEditorApp,
    rect: Rect,
    response: &egui::Response,
    painter: &egui::Painter,
    node_id: NodeId,
    node: &foreplay_ir::IrNode,
) {
    let pos = get_vec3_prop(node, "position").unwrap_or(Vec3::ZERO);
    let scale = get_vec3_prop(node, "scale").unwrap_or(Vec3::ONE);

    let Some(origin_2d) = app.camera_3d.project(pos, rect) else {
        return;
    };

    let axis_len = 2.4;
    let x_pt = app.camera_3d.project(pos + Vec3::X * axis_len, rect);
    let y_pt = app.camera_3d.project(pos + Vec3::Y * axis_len, rect);
    let z_pt = app.camera_3d.project(pos + Vec3::Z * axis_len, rect);

    let is_translating = app.gizmo_mode == GizmoMode::Translate;
    let is_scaling = app.gizmo_mode == GizmoMode::Scale;

    // Draw gizmo lines and handles
    if let Some(x_2d) = x_pt {
        painter.line_segment([origin_2d, x_2d], Stroke::new(3.0, COLOR_AXIS_X));
        if is_translating {
            painter.circle_filled(x_2d, 6.0, COLOR_AXIS_X);
        } else if is_scaling {
            painter.rect_filled(Rect::from_center_size(x_2d, Vec2::splat(10.0)), 1.0, COLOR_AXIS_X);
        }
    }

    if let Some(y_2d) = y_pt {
        painter.line_segment([origin_2d, y_2d], Stroke::new(3.0, COLOR_AXIS_Y));
        if is_translating {
            painter.circle_filled(y_2d, 6.0, COLOR_AXIS_Y);
        } else if is_scaling {
            painter.rect_filled(Rect::from_center_size(y_2d, Vec2::splat(10.0)), 1.0, COLOR_AXIS_Y);
        }
    }

    if let Some(z_2d) = z_pt {
        painter.line_segment([origin_2d, z_2d], Stroke::new(3.0, COLOR_AXIS_Z));
        if is_translating {
            painter.circle_filled(z_2d, 6.0, COLOR_AXIS_Z);
        } else if is_scaling {
            painter.rect_filled(Rect::from_center_size(z_2d, Vec2::splat(10.0)), 1.0, COLOR_AXIS_Z);
        }
    }

    // Check handle dragging with primary mouse button
    if response.dragged_by(PointerButton::Primary) {
        let delta = response.drag_delta();
        if delta.length_sq() > 0.0 {
            let pointer_pos = response.hover_pos().unwrap_or(origin_2d);

            // Determine which axis is closest to the drag or handle
            let dist_x = x_pt.map_or(f32::MAX, |p| p.distance(pointer_pos));
            let dist_y = y_pt.map_or(f32::MAX, |p| p.distance(pointer_pos));
            let dist_z = z_pt.map_or(f32::MAX, |p| p.distance(pointer_pos));

            let chosen_axis = if dist_x <= dist_y && dist_x <= dist_z {
                GizmoAxis::X
            } else if dist_y <= dist_z {
                GizmoAxis::Y
            } else {
                GizmoAxis::Z
            };

            let step = (delta.x - delta.y) * 0.035;

            if is_translating {
                let mut new_pos = pos;
                match chosen_axis {
                    GizmoAxis::X => new_pos.x += delta.x * 0.04,
                    GizmoAxis::Y => new_pos.y -= delta.y * 0.04,
                    GizmoAxis::Z => new_pos.z += (delta.x + delta.y) * 0.04,
                    GizmoAxis::None => {}
                }

                let cmd = EditorCommand::SetProperty {
                    node_id,
                    key: "position".into(),
                    old_value: Some(IrValue::Vec3([pos.x, pos.y, pos.z])),
                    new_value: Some(IrValue::Vec3([new_pos.x, new_pos.y, new_pos.z])),
                };
                app.execute_command(cmd);
            } else if is_scaling {
                let mut new_scale = scale;
                match chosen_axis {
                    GizmoAxis::X => new_scale.x = (new_scale.x + step).max(0.1),
                    GizmoAxis::Y => new_scale.y = (new_scale.y + step).max(0.1),
                    GizmoAxis::Z => new_scale.z = (new_scale.z + step).max(0.1),
                    GizmoAxis::None => {}
                }

                let cmd = EditorCommand::SetProperty {
                    node_id,
                    key: "scale".into(),
                    old_value: Some(IrValue::Vec3([scale.x, scale.y, scale.z])),
                    new_value: Some(IrValue::Vec3([new_scale.x, new_scale.y, new_scale.z])),
                };
                app.execute_command(cmd);
            }
        }
    }
}

fn draw_hud_overlay(painter: &egui::Painter, rect: Rect, app: &ForeplayEditorApp) {
    let hud_rect = Rect::from_min_size(rect.min + Vec2::new(12.0, 12.0), Vec2::new(260.0, 95.0));
    painter.rect_filled(hud_rect, 4.0, Color32::from_rgba_unmultiplied(26, 29, 36, 220));
    painter.rect_stroke(hud_rect, 4.0, Stroke::new(1.0, COLOR_BORDER), egui::StrokeKind::Inside);

    let entity_count = app.session.tree.ir_to_entity.len();
    let tick_count = app.session.tree.tick_count;
    let pie_status = if app.play_in_editor_running {
        if app.play_in_editor_paused {
            "[PAUSED]"
        } else {
            "[PLAYING]"
        }
    } else {
        "[STOPPED]"
    };

    let text_x = hud_rect.min.x + 8.0;
    let mut text_y = hud_rect.min.y + 8.0;

    painter.text(
        Pos2::new(text_x, text_y),
        Align2::LEFT_TOP,
        format!("Foreplay 3D Viewport | FPS: {:.1}", app.fps),
        FontId::proportional(12.0),
        COLOR_ACCENT_BLUE,
    );
    text_y += 18.0;

    painter.text(
        Pos2::new(text_x, text_y),
        Align2::LEFT_TOP,
        format!("Entities: {entity_count} | World Ticks: {tick_count}"),
        FontId::proportional(11.0),
        COLOR_TEXT_NORMAL,
    );
    text_y += 16.0;

    painter.text(
        Pos2::new(text_x, text_y),
        Align2::LEFT_TOP,
        format!("Workspace: {:?} | PIE: {pie_status}", app.active_workspace),
        FontId::proportional(11.0),
        if app.play_in_editor_running {
            COLOR_SUCCESS_GREEN
        } else {
            COLOR_TEXT_MUTED
        },
    );
    text_y += 16.0;

    painter.text(
        Pos2::new(text_x, text_y),
        Align2::LEFT_TOP,
        format!("Gizmo: {:?} (W: Translate, R: Scale, Q: Select)", app.gizmo_mode),
        FontId::proportional(10.0),
        COLOR_WARN_YELLOW,
    );
}

fn get_vec3_prop(node: &foreplay_ir::IrNode, key: &str) -> Option<Vec3> {
    match node.properties.get(key) {
        Some(IrValue::Vec3(v)) => Some(Vec3::new(v[0], v[1], v[2])),
        _ => None,
    }
}

struct RichTextHelper;
impl RichTextHelper {
    fn muted(text: &str) -> egui::RichText {
        egui::RichText::new(text).color(COLOR_TEXT_MUTED)
    }
}
