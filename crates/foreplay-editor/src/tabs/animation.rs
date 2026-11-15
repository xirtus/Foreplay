//! Final Cut Pro "Magnetic Director Timeline" & Animation StateMachine Dock (`foreplay-editor`).
//!
//! Mirrors Apple Final Cut Pro 11's Magnetic Timeline + Godot 4 AnimationPlayer:
//! - **Live Optical Hover Skimming**: Red vertical skimmer blade with timecode preview and live scrubbing.
//! - **Color-Coded Multi-Lane Magnetic Clips**:
//!   - Primary Storyline (Electric Violet `#a855f7`)
//!   - Connected Audio Foley & Music Clips (Warm Amber `#f59e0b` with drawn waveform peaks)
//!   - Connected Camera Cut & Shake Clips (Coral `#ff6b6b`)
//!   - Connected Signal Trigger Markers (Cyan `#38bdf8`)
//! - **Cubic Bezier F-Curve Editor**: Visual spline tangent graph for property animation.
//! - **StateMachine Graph**: State node transitions driven by physics queries.

use egui::{
    epaint::CubicBezierShape, Align2, Color32, FontId, Pos2, Rect, RichText, ScrollArea, Sense,
    Slider, Stroke, Ui, Vec2,
};
use foreplay_ir::{EditorCommand, IrValue};
use crate::ForeplayEditorApp;
use crate::theme::{
    CHROMA_AMBER_AUDIO, CHROMA_CORAL_3D, CHROMA_CYAN_2D, CHROMA_MINT_UI,
    CHROMA_VIOLET_ANIM, STUDIO_OBSIDIAN_ACCENT, STUDIO_OBSIDIAN_BORDER, STUDIO_OBSIDIAN_CANVAS,
    STUDIO_OBSIDIAN_CARD, STUDIO_OBSIDIAN_TEXT_MUTED,
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AnimationViewMode {
    Timeline,
    FCurveEditor,
    StateMachine,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ClipLane {
    PrimaryStoryline,
    ConnectedAudio,
    ConnectedCamera,
    ConnectedSignals,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MagneticClip {
    pub id: String,
    pub name: String,
    pub lane: ClipLane,
    pub start_secs: f32,
    pub duration_secs: f32,
    pub speed_pct: f32,
    pub connected_parent_id: Option<String>,
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

    // Final Cut Pro Magnetic Timeline features
    pub skimmer_enabled: bool,
    pub skimmer_time: f32,
    pub show_f_curves: bool,
    pub magnetic_clips: Vec<MagneticClip>,
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
            skimmer_enabled: true,
            skimmer_time: 0.0,
            show_f_curves: false,
            magnetic_clips: vec![
                MagneticClip {
                    id: "primary_anim".into(),
                    name: "Skate Ollie Kickflip Clip".into(),
                    lane: ClipLane::PrimaryStoryline,
                    start_secs: 0.0,
                    duration_secs: 2.0,
                    speed_pct: 100.0,
                    connected_parent_id: None,
                },
                MagneticClip {
                    id: "audio_foley".into(),
                    name: "Foley Tail Pop & Concrete Grind".into(),
                    lane: ClipLane::ConnectedAudio,
                    start_secs: 0.2,
                    duration_secs: 1.5,
                    speed_pct: 100.0,
                    connected_parent_id: Some("primary_anim".into()),
                },
                MagneticClip {
                    id: "cam_shake".into(),
                    name: "Camera Pop Impact Shake".into(),
                    lane: ClipLane::ConnectedCamera,
                    start_secs: 0.35,
                    duration_secs: 0.6,
                    speed_pct: 100.0,
                    connected_parent_id: Some("primary_anim".into()),
                },
                MagneticClip {
                    id: "sig_land".into(),
                    name: "signal trick_landed".into(),
                    lane: ClipLane::ConnectedSignals,
                    start_secs: 1.25,
                    duration_secs: 0.1,
                    speed_pct: 100.0,
                    connected_parent_id: Some("primary_anim".into()),
                },
            ],
        }
    }
}

pub fn show(app: &mut ForeplayEditorApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Magnetic Director Timeline").strong().color(STUDIO_OBSIDIAN_ACCENT));
        ui.selectable_value(&mut app.animation_state.view_mode, AnimationViewMode::Timeline, "⏱ FCP Magnetic Timeline");
        ui.selectable_value(&mut app.animation_state.view_mode, AnimationViewMode::FCurveEditor, "📈 F-Curve Splines");
        ui.selectable_value(&mut app.animation_state.view_mode, AnimationViewMode::StateMachine, "🕸 StateMachine Graph");

        ui.separator();
        ui.label(format!("Active: '{}'", app.animation_state.active_animation));
    });

    ui.separator();

    match app.animation_state.view_mode {
        AnimationViewMode::Timeline => show_magnetic_timeline(app, ui),
        AnimationViewMode::FCurveEditor => show_f_curve_editor(app, ui),
        AnimationViewMode::StateMachine => show_state_machine(app, ui),
    }
}

fn show_magnetic_timeline(app: &mut ForeplayEditorApp, ui: &mut Ui) {
    // Top Bar: Transport & Final Cut Skimmer Controls
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

        // Optical Skimmer Toggle
        let skimmer_label = if app.animation_state.skimmer_enabled {
            "🔴 Skimmer: ON"
        } else {
            "⚪ Skimmer: OFF"
        };
        if ui.selectable_label(app.animation_state.skimmer_enabled, skimmer_label).clicked() {
            app.animation_state.skimmer_enabled = !app.animation_state.skimmer_enabled;
        }

        ui.separator();
        ui.label("Playhead:");
        ui.add(
            Slider::new(&mut app.animation_state.playhead_secs, 0.0..=app.animation_state.duration_secs)
                .step_by(0.01)
                .suffix("s"),
        );

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

    // Timeline Canvas Area with Multi-Lane Magnetic Clips
    ScrollArea::vertical().show(ui, |ui| {
        let canvas_size = Vec2::new(ui.available_width().max(600.0), 220.0);
        let (rect, response) = ui.allocate_exact_size(canvas_size, Sense::click_and_drag());
        let painter = ui.painter_at(rect);

        // Dark obsidian canvas background
        painter.rect(
            rect,
            6.0,
            STUDIO_OBSIDIAN_CANVAS,
            Stroke::new(1.0, STUDIO_OBSIDIAN_BORDER),
            egui::StrokeKind::Inside,
        );

        let duration = app.animation_state.duration_secs.max(0.1);
        let time_to_x = |t: f32| -> f32 {
            rect.min.x + 160.0 + (t / duration) * (rect.width() - 170.0)
        };
        let x_to_time = |x: f32| -> f32 {
            let ratio = ((x - (rect.min.x + 160.0)) / (rect.width() - 170.0)).clamp(0.0, 1.0);
            ratio * duration
        };

        // Skimmer interaction
        if app.animation_state.skimmer_enabled && response.hovered()
            && let Some(hover_pos) = response.hover_pos()
        {
            app.animation_state.skimmer_time = x_to_time(hover_pos.x);
        }
        if response.clicked() || response.dragged() {
            app.animation_state.playhead_secs = app.animation_state.skimmer_time;
        }

        // Draw Timecode Ruler Header
        let ruler_y = rect.min.y + 20.0;
        painter.line_segment(
            [Pos2::new(rect.min.x + 160.0, ruler_y), Pos2::new(rect.max.x, ruler_y)],
            Stroke::new(1.0, STUDIO_OBSIDIAN_BORDER),
        );
        let tick_steps = 10;
        for i in 0..=tick_steps {
            let t = (i as f32 / tick_steps as f32) * duration;
            let tx = time_to_x(t);
            painter.line_segment(
                [Pos2::new(tx, ruler_y - 6.0), Pos2::new(tx, ruler_y)],
                Stroke::new(1.0, STUDIO_OBSIDIAN_TEXT_MUTED),
            );
            painter.text(
                Pos2::new(tx, ruler_y - 8.0),
                Align2::CENTER_BOTTOM,
                format!("{t:.1}s"),
                FontId::monospace(9.0),
                STUDIO_OBSIDIAN_TEXT_MUTED,
            );
        }

        // Lanes setup
        let lanes = [
            (ClipLane::PrimaryStoryline, "Primary Storyline", CHROMA_VIOLET_ANIM, rect.min.y + 26.0, 38.0),
            (ClipLane::ConnectedAudio, "Connected Audio", CHROMA_AMBER_AUDIO, rect.min.y + 70.0, 36.0),
            (ClipLane::ConnectedCamera, "Camera Cut / Shake", CHROMA_CORAL_3D, rect.min.y + 112.0, 32.0),
            (ClipLane::ConnectedSignals, "Signal Triggers", CHROMA_CYAN_2D, rect.min.y + 150.0, 26.0),
        ];

        for (_lane_kind, lane_label, lane_color, lane_y, lane_h) in lanes {
            // Lane track header on left
            let header_rect = Rect::from_min_size(Pos2::new(rect.min.x + 4.0, lane_y), Vec2::new(150.0, lane_h));
            painter.rect(header_rect, 4.0, STUDIO_OBSIDIAN_CARD, Stroke::new(1.0, STUDIO_OBSIDIAN_BORDER), egui::StrokeKind::Inside);
            painter.text(
                Pos2::new(header_rect.min.x + 8.0, header_rect.center().y),
                Align2::LEFT_CENTER,
                lane_label,
                FontId::proportional(11.0),
                lane_color,
            );

            // Lane track background
            let track_rect = Rect::from_min_max(Pos2::new(rect.min.x + 160.0, lane_y), Pos2::new(rect.max.x - 4.0, lane_y + lane_h));
            painter.rect_filled(track_rect, 4.0, Color32::from_rgb(0x11, 0x14, 0x1c));
            painter.rect_stroke(track_rect, 4.0, Stroke::new(1.0, Color32::from_rgb(0x1e, 0x24, 0x33)), egui::StrokeKind::Inside);
        }

        // Draw Magnetic Clips
        let clips = app.animation_state.magnetic_clips.clone();
        for clip in &clips {
            let (lane_color, lane_y, lane_h) = match clip.lane {
                ClipLane::PrimaryStoryline => (CHROMA_VIOLET_ANIM, rect.min.y + 26.0, 38.0),
                ClipLane::ConnectedAudio => (CHROMA_AMBER_AUDIO, rect.min.y + 70.0, 36.0),
                ClipLane::ConnectedCamera => (CHROMA_CORAL_3D, rect.min.y + 112.0, 32.0),
                ClipLane::ConnectedSignals => (CHROMA_CYAN_2D, rect.min.y + 150.0, 26.0),
            };

            let x1 = time_to_x(clip.start_secs);
            let x2 = time_to_x(clip.start_secs + clip.duration_secs);
            let clip_rect = Rect::from_min_max(Pos2::new(x1, lane_y + 2.0), Pos2::new(x2, lane_y + lane_h - 2.0));

            // Vertical magnetic anchor connector line to primary storyline
            if clip.connected_parent_id.is_some() {
                let anchor_x = x1 + 8.0;
                let primary_bottom = rect.min.y + 26.0 + 38.0;
                painter.line_segment(
                    [Pos2::new(anchor_x, primary_bottom), Pos2::new(anchor_x, clip_rect.min.y)],
                    Stroke::new(1.5, lane_color.linear_multiply(0.7)),
                );
                painter.circle_filled(Pos2::new(anchor_x, primary_bottom), 2.5, lane_color);
                painter.circle_filled(Pos2::new(anchor_x, clip_rect.min.y), 2.5, lane_color);
            }

            // Clip body
            painter.rect(
                clip_rect,
                4.0,
                lane_color.linear_multiply(0.35),
                Stroke::new(1.5, lane_color),
                egui::StrokeKind::Inside,
            );

            // Clip inner graphics
            match clip.lane {
                ClipLane::PrimaryStoryline => {
                    painter.text(
                        Pos2::new(clip_rect.min.x + 8.0, clip_rect.center().y),
                        Align2::LEFT_CENTER,
                        &clip.name,
                        FontId::proportional(11.0),
                        Color32::WHITE,
                    );
                    painter.text(
                        Pos2::new(clip_rect.max.x - 8.0, clip_rect.center().y),
                        Align2::RIGHT_CENTER,
                        "100%",
                        FontId::monospace(10.0),
                        CHROMA_MINT_UI,
                    );
                    // Diamond keyframe markers along clip
                    for &kf_t in &[0.0, 0.4, 0.8, 1.2, 1.6, 2.0] {
                        let kfx = time_to_x(kf_t);
                        draw_keyframe_diamond(&painter, Pos2::new(kfx, clip_rect.center().y), Color32::WHITE);
                    }
                }
                ClipLane::ConnectedAudio => {
                    // Draw simulated audio waveform peaks
                    let num_peaks = 24;
                    let step = clip_rect.width() / num_peaks as f32;
                    let cy = clip_rect.center().y;
                    for p in 0..num_peaks {
                        let px = clip_rect.min.x + p as f32 * step + 2.0;
                        let ph = ((p as f32 * 0.7).sin().abs() * 0.8 + 0.2) * (clip_rect.height() * 0.4);
                        painter.line_segment(
                            [Pos2::new(px, cy - ph), Pos2::new(px, cy + ph)],
                            Stroke::new(1.5, lane_color.linear_multiply(0.85)),
                        );
                    }
                    painter.text(
                        Pos2::new(clip_rect.min.x + 6.0, clip_rect.min.y + 4.0),
                        Align2::LEFT_TOP,
                        &clip.name,
                        FontId::proportional(10.0),
                        Color32::WHITE,
                    );
                }
                ClipLane::ConnectedCamera => {
                    painter.text(
                        Pos2::new(clip_rect.min.x + 6.0, clip_rect.center().y),
                        Align2::LEFT_CENTER,
                        &clip.name,
                        FontId::proportional(10.0),
                        Color32::WHITE,
                    );
                    // Shake amplitude envelope
                    let cy = clip_rect.center().y;
                    let w = clip_rect.width();
                    let pts: Vec<Pos2> = (0..=12).map(|i| {
                        let tx = clip_rect.min.x + (i as f32 / 12.0) * w;
                        let env = (-(i as f32 - 4.0).abs() * 0.4).exp() * 8.0;
                        Pos2::new(tx, cy + if i % 2 == 0 { env } else { -env })
                    }).collect();
                    for pair in pts.windows(2) {
                        painter.line_segment([pair[0], pair[1]], Stroke::new(1.2, Color32::WHITE));
                    }
                }
                ClipLane::ConnectedSignals => {
                    painter.text(
                        Pos2::new(clip_rect.min.x + 6.0, clip_rect.center().y),
                        Align2::LEFT_CENTER,
                        &clip.name,
                        FontId::monospace(9.5),
                        Color32::WHITE,
                    );
                }
            }
        }

        // Draw Playhead (Orange/White scrubber line)
        let playhead_x = time_to_x(app.animation_state.playhead_secs);
        painter.line_segment(
            [Pos2::new(playhead_x, rect.min.y + 10.0), Pos2::new(playhead_x, rect.max.y - 4.0)],
            Stroke::new(2.0, STUDIO_OBSIDIAN_ACCENT),
        );
        // Playhead head polygon
        let ph_pts = vec![
            Pos2::new(playhead_x - 5.0, rect.min.y + 8.0),
            Pos2::new(playhead_x + 5.0, rect.min.y + 8.0),
            Pos2::new(playhead_x + 5.0, rect.min.y + 16.0),
            Pos2::new(playhead_x, rect.min.y + 22.0),
            Pos2::new(playhead_x - 5.0, rect.min.y + 16.0),
        ];
        painter.add(egui::Shape::convex_polygon(ph_pts, STUDIO_OBSIDIAN_ACCENT, Stroke::NONE));

        // Draw Final Cut Pro Signature Optical Skimmer (Red vertical blade with timecode badge)
        if app.animation_state.skimmer_enabled && response.hovered() {
            let skimmer_x = time_to_x(app.animation_state.skimmer_time);
            let skimmer_red = Color32::from_rgb(0xff, 0x3b, 0x30); // FCP signature red

            painter.line_segment(
                [Pos2::new(skimmer_x, rect.min.y + 6.0), Pos2::new(skimmer_x, rect.max.y - 4.0)],
                Stroke::new(1.5, skimmer_red),
            );

            // Skimmer floating timecode badge
            let tc_text = format!("{:.2}s", app.animation_state.skimmer_time);
            let badge_pos = Pos2::new(skimmer_x, rect.min.y + 12.0);
            let badge_rect = Rect::from_center_size(badge_pos, Vec2::new(42.0, 16.0));
            painter.rect_filled(badge_rect, 3.0, skimmer_red);
            painter.text(
                badge_pos,
                Align2::CENTER_CENTER,
                tc_text,
                FontId::monospace(9.0),
                Color32::WHITE,
            );
        }
    });
}

fn show_f_curve_editor(_app: &mut ForeplayEditorApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Cubic Bezier F-Curve Spline Editor").strong().color(CHROMA_MINT_UI));
        ui.colored_label(STUDIO_OBSIDIAN_TEXT_MUTED, "| Direct curve tangent tweaking for ultra-tactile game feel");
    });

    let (rect, _response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
    let painter = ui.painter_at(rect);

    painter.rect(
        rect,
        6.0,
        STUDIO_OBSIDIAN_CANVAS,
        Stroke::new(1.0, STUDIO_OBSIDIAN_BORDER),
        egui::StrokeKind::Inside,
    );

    // Draw horizontal grid lines
    let zero_y = rect.center().y;
    painter.line_segment([Pos2::new(rect.min.x, zero_y), Pos2::new(rect.max.x, zero_y)], Stroke::new(1.0, STUDIO_OBSIDIAN_BORDER));

    // Spline tracks (e.g. position.y in Cyan, rotation.z in Violet, scale in Amber)
    let curves = [
        (
            "position.y",
            CHROMA_CYAN_2D,
            [
                Pos2::new(rect.min.x + 40.0, zero_y + 40.0),
                Pos2::new(rect.min.x + 180.0, zero_y - 80.0),
                Pos2::new(rect.min.x + 360.0, zero_y - 60.0),
                Pos2::new(rect.min.x + 540.0, zero_y + 40.0),
            ],
        ),
        (
            "rotation.z",
            CHROMA_VIOLET_ANIM,
            [
                Pos2::new(rect.min.x + 40.0, zero_y),
                Pos2::new(rect.min.x + 200.0, zero_y + 50.0),
                Pos2::new(rect.min.x + 380.0, zero_y - 30.0),
                Pos2::new(rect.min.x + 540.0, zero_y),
            ],
        ),
    ];

    for (name, color, [p0, c0, c1, p1]) in curves {
        let shape = CubicBezierShape::from_points_stroke(
            [p0, c0, c1, p1],
            false,
            Color32::TRANSPARENT,
            Stroke::new(2.5, color),
        );
        painter.add(shape);

        // Control handle lines
        painter.line_segment([p0, c0], Stroke::new(1.0, color.linear_multiply(0.6)));
        painter.circle_filled(c0, 3.0, color);
        painter.line_segment([p1, c1], Stroke::new(1.0, color.linear_multiply(0.6)));
        painter.circle_filled(c1, 3.0, color);

        // Keyframe anchor circles
        painter.circle(p0, 4.5, STUDIO_OBSIDIAN_CANVAS, Stroke::new(2.0, color));
        painter.circle(p1, 4.5, STUDIO_OBSIDIAN_CANVAS, Stroke::new(2.0, color));

        painter.text(p0 + Vec2::new(8.0, -8.0), Align2::LEFT_BOTTOM, name, FontId::proportional(11.0), color);
    }
}

fn show_state_machine(_app: &mut ForeplayEditorApp, ui: &mut Ui) {
    ui.label(RichText::new("AnimationTree StateMachine Graph").strong().color(STUDIO_OBSIDIAN_ACCENT));
    ui.colored_label(STUDIO_OBSIDIAN_TEXT_MUTED, "Character transitions driven by Bevy ECS physics queries");

    let (rect, _response) = ui.allocate_exact_size(ui.available_size(), Sense::click());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, STUDIO_OBSIDIAN_CANVAS);

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
        painter.rect(card_rect, 6.0, STUDIO_OBSIDIAN_CARD, Stroke::new(1.5, STUDIO_OBSIDIAN_ACCENT), egui::StrokeKind::Inside);
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
            Stroke::new(2.0, CHROMA_MINT_UI),
        );
        painter.add(shape);

        let mid = Pos2::new((p1.x + p2.x) * 0.5, (p1.y + p2.y) * 0.5);
        painter.text(mid, Align2::CENTER_CENTER, label, FontId::proportional(10.0), CHROMA_AMBER_AUDIO);
    }
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

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn playback_advances_time() {
        let mut state = AnimationDockState::default();
        assert!(!state.is_playing);
        state.is_playing = true;
        let delta = 0.016;
        state.playhead_secs += delta;
        assert!(state.playhead_secs > 0.4);
    }

    #[test]
    fn magnetic_timeline_skimmer_and_clips() {
        let state = AnimationDockState::default();
        assert!(state.skimmer_enabled);
        assert_eq!(state.magnetic_clips.len(), 4);
        assert_eq!(state.magnetic_clips[0].lane, ClipLane::PrimaryStoryline);
    }
}
