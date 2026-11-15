//! Animation Timeline & StateMachine Dock (`foreplay-editor`).
//!
//! Mirrors Godot 4's AnimationPlayer bottom panel:
//! - Timeline scrubber (0.0s .. 2.0s) with play/pause, loop, and FPS controls.
//! - Property keyframe diamond tracks (`position`, `rotation`, `scale`).
//! - `+ Insert Key` button committing keyframe state through `EditorCommand`.
//! - `AnimationTree` StateMachine graph sub-tab (`Idle -> Run -> Ollie -> Grind -> Land`).

use egui::{
    epaint::CubicBezierShape, Align2, Color32, FontId, Pos2, Rect, RichText, ScrollArea, Sense,
    Slider, Stroke, Ui, Vec2,
};
use foreplay_ir::{EditorCommand, IrValue};
use crate::ForeplayEditorApp;
use crate::theme::{
    COLOR_ACCENT_BLUE, COLOR_BG_DARK, COLOR_BORDER, COLOR_PANEL_BG, COLOR_SUCCESS_GREEN,
    COLOR_TEXT_MUTED, COLOR_WARN_YELLOW,
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AnimationViewMode {
    Timeline,
    StateMachine,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AnimationDockState {
    pub view_mode: AnimationViewMode,
    pub playhead_secs: f32,
    pub duration_secs: f32,
    pub is_playing: bool,
    pub is_looping: bool,
    pub fps: u32,
    pub active_animation: String,
}

impl Default for AnimationDockState {
    fn default() -> Self {
        Self {
            view_mode: AnimationViewMode::Timeline,
            playhead_secs: 0.4,
            duration_secs: 2.0,
            is_playing: false,
            is_looping: true,
            fps: 60,
            active_animation: "skate_ollie_kickflip".into(),
        }
    }
}

pub fn show(app: &mut ForeplayEditorApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Animation Player").strong().color(COLOR_ACCENT_BLUE));
        ui.selectable_value(&mut app.animation_state.view_mode, AnimationViewMode::Timeline, "⏱ Timeline");
        ui.selectable_value(&mut app.animation_state.view_mode, AnimationViewMode::StateMachine, "🕸 StateMachine Graph");

        ui.separator();
        ui.label(format!("Animation: '{}'", app.animation_state.active_animation));
    });

    ui.separator();

    match app.animation_state.view_mode {
        AnimationViewMode::Timeline => show_timeline(app, ui),
        AnimationViewMode::StateMachine => show_state_machine(app, ui),
    }
}

fn show_timeline(app: &mut ForeplayEditorApp, ui: &mut Ui) {
    // Transport controls
    ui.horizontal(|ui| {
        let play_text = if app.animation_state.is_playing { "⏸ Pause" } else { "▶ Play" };
        if ui.button(play_text).clicked() {
            app.animation_state.is_playing = !app.animation_state.is_playing;
        }

        if ui.button("⏹ Stop").clicked() {
            app.animation_state.is_playing = false;
            app.animation_state.playhead_secs = 0.0;
        }

        ui.checkbox(&mut app.animation_state.is_looping, "🔁 Loop");

        ui.label("Time:");
        ui.add(Slider::new(&mut app.animation_state.playhead_secs, 0.0..=app.animation_state.duration_secs).step_by(0.01).suffix("s"));

        if ui.button("➕ Insert Keyframe").on_hover_text("Insert property keyframe at playhead").clicked()
            && let Some(selected_id) = app.selected_node
        {
            let cmd = EditorCommand::SetProperty {
                node_id: selected_id,
                key: "anim_key_time".into(),
                old_value: None,
                new_value: Some(IrValue::F64(app.animation_state.playhead_secs as f64)),
            };
            app.execute_command(cmd);
        }
    });

    ui.separator();

    // Timeline tracks
    ScrollArea::vertical().show(ui, |ui| {
        let tracks = [
            ("SkaterRoot:position", vec![0.0, 0.4, 0.8, 1.2, 1.6, 2.0]),
            ("SkaterRoot:rotation", vec![0.2, 0.5, 0.9, 1.4]),
            ("SkaterMesh:scale", vec![0.0, 0.8, 1.6]),
        ];

        for (track_name, keyframes) in tracks {
            ui.horizontal(|ui| {
                ui.allocate_ui(Vec2::new(140.0, 24.0), |ui| {
                    ui.label(RichText::new(track_name).strong());
                });

                // Track bar
                let (track_rect, _resp) = ui.allocate_exact_size(Vec2::new(ui.available_width() - 20.0, 24.0), Sense::click());
                let painter = ui.painter_at(track_rect);
                painter.rect_filled(track_rect, 2.0, COLOR_PANEL_BG);
                painter.rect_stroke(track_rect, 2.0, Stroke::new(1.0, COLOR_BORDER), egui::StrokeKind::Inside);

                // Draw diamond keyframes
                for &k in &keyframes {
                    let ratio = k / app.animation_state.duration_secs;
                    let kx = track_rect.min.x + track_rect.width() * ratio;
                    let ky = track_rect.center().y;
                    draw_keyframe_diamond(&painter, Pos2::new(kx, ky), COLOR_WARN_YELLOW);
                }

                // Draw playhead vertical line
                let playhead_ratio = app.animation_state.playhead_secs / app.animation_state.duration_secs;
                let px = track_rect.min.x + track_rect.width() * playhead_ratio;
                painter.line_segment([Pos2::new(px, track_rect.min.y), Pos2::new(px, track_rect.max.y)], Stroke::new(2.0, Color32::RED));
            });
            ui.add_space(2.0);
        }
    });
}

fn draw_keyframe_diamond(painter: &egui::Painter, center: Pos2, color: Color32) {
    let r = 5.0;
    let points = vec![
        Pos2::new(center.x, center.y - r),
        Pos2::new(center.x + r, center.y),
        Pos2::new(center.x, center.y + r),
        Pos2::new(center.x - r, center.y),
    ];
    painter.add(egui::Shape::convex_polygon(points, color, Stroke::NONE));
}

fn show_state_machine(_app: &mut ForeplayEditorApp, ui: &mut Ui) {
    ui.label(RichText::new("AnimationTree StateMachine Graph").strong());
    ui.colored_label(COLOR_TEXT_MUTED, "Character transitions driven by Bevy ECS physics queries");

    let (rect, _response) = ui.allocate_exact_size(ui.available_size(), Sense::click());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, COLOR_BG_DARK);

    let states = [
        ("Idle", Pos2::new(rect.min.x + 60.0, rect.center().y)),
        ("Run", Pos2::new(rect.min.x + 200.0, rect.center().y - 50.0)),
        ("Ollie Pop", Pos2::new(rect.min.x + 360.0, rect.center().y - 80.0)),
        ("Grind Slide", Pos2::new(rect.min.x + 520.0, rect.center().y)),
        ("Landing", Pos2::new(rect.min.x + 360.0, rect.center().y + 60.0)),
    ];

    let mut state_rects = Vec::new();
    for (name, pos) in states {
        let card_rect = Rect::from_center_size(pos, Vec2::new(110.0, 44.0));
        state_rects.push((name, card_rect));
        painter.rect_filled(card_rect, 6.0, COLOR_PANEL_BG);
        painter.rect_stroke(card_rect, 6.0, Stroke::new(1.5, COLOR_ACCENT_BLUE), egui::StrokeKind::Inside);
        painter.text(pos, Align2::CENTER_CENTER, name, FontId::proportional(12.0), Color32::WHITE);
    }

    // Connect transitions: Idle -> Run -> Ollie -> Grind -> Landing -> Idle
    let transitions = [
        (0, 1, "speed > 0.1"),
        (1, 2, "jump_pressed"),
        (2, 3, "rail_contact"),
        (3, 4, "ground_contact"),
        (4, 0, "velocity ≈ 0"),
    ];

    for (from_idx, to_idx, label) in transitions {
        let p1 = state_rects[from_idx].1.center();
        let p2 = state_rects[to_idx].1.center();

        let dx = (p2.x - p1.x).abs() * 0.4 + 20.0;
        let c1 = p1 + Vec2::new(dx, 0.0);
        let c2 = p2 - Vec2::new(dx, 0.0);

        let shape = CubicBezierShape::from_points_stroke(
            [p1, c1, c2, p2],
            false,
            Color32::TRANSPARENT,
            Stroke::new(2.0, COLOR_SUCCESS_GREEN),
        );
        painter.add(shape);

        let mid = Pos2::new((p1.x + p2.x) * 0.5, (p1.y + p2.y) * 0.5);
        painter.text(mid, Align2::CENTER_CENTER, label, FontId::proportional(10.0), COLOR_WARN_YELLOW);
    }
}
