//! Shader Editor Dock (`foreplay-editor`).
//!
//! Mirrors Godot 4's Shader Editor bottom dock:
//! - Visual Shader node graph (`TIME -> Sin -> UV -> ALBEDO`).
//! - Live `.gdshader` code editor with instant re-compilation into WebGPU WGSL.
//! - Live WGSL compiler status label and syntax diagnostic preview powered by
//!   `foreplay_render::shader::compile_gdshader_to_wgsl`.

use egui::{
    epaint::CubicBezierShape, Align2, Color32, FontId, Pos2, Rect, RichText, ScrollArea, Sense,
    Stroke, TextEdit, Ui, Vec2,
};
use foreplay_render::shader::compile_gdshader_to_wgsl;
use crate::ForeplayEditorApp;
use crate::theme::{
    COLOR_ACCENT_BLUE, COLOR_BG_DARK, COLOR_ERROR_RED, COLOR_PANEL_BG, COLOR_SUCCESS_GREEN,
    COLOR_TEXT_MUTED, COLOR_WARN_YELLOW,
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ShaderViewMode {
    VisualShader,
    GdShaderCode,
    WgslCompiled,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ShaderEditorState {
    pub view_mode: ShaderViewMode,
    pub shader_code: String,
}

impl Default for ShaderEditorState {
    fn default() -> Self {
        let code = r#"shader_type spatial;
render_mode blend_mix, depth_draw_opaque, cull_back;

uniform sampler2D albedo_texture : source_color;
uniform float rough_mult : hint_range(0.0, 1.0) = 0.65;
uniform vec4 skate_glow : source_color = vec4(0.2, 0.7, 1.0, 1.0);

void fragment() {
    vec2 uv_tiled = UV * 2.0;
    float pulse = (sin(TIME * 2.5) + 1.0) * 0.5;
    ALBEDO = texture(albedo_texture, uv_tiled).rgb;
    EMISSION = skate_glow.rgb * pulse * 0.4;
    ROUGHNESS = rough_mult;
}
"#
        .to_string();

        Self {
            view_mode: ShaderViewMode::VisualShader,
            shader_code: code,
        }
    }
}

pub fn show(app: &mut ForeplayEditorApp, ui: &mut Ui) {
    // Dynamic compilation pass through foreplay-render compiler
    let compile_result = compile_gdshader_to_wgsl(&app.shader_editor_state.shader_code);

    ui.horizontal(|ui| {
        ui.label(RichText::new("Shader Editor").strong().color(COLOR_ACCENT_BLUE));
        ui.selectable_value(&mut app.shader_editor_state.view_mode, ShaderViewMode::VisualShader, "🕸 Visual Shader");
        ui.selectable_value(&mut app.shader_editor_state.view_mode, ShaderViewMode::GdShaderCode, "📜 .gdshader");
        ui.selectable_value(&mut app.shader_editor_state.view_mode, ShaderViewMode::WgslCompiled, "⚡ Compiled WGSL");

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            match &compile_result {
                Ok(_) => {
                    ui.colored_label(COLOR_SUCCESS_GREEN, "⚡ WGSL Compiler: Verified OK");
                }
                Err(err) => {
                    ui.colored_label(COLOR_ERROR_RED, format!("❌ WGSL Compiler: {err}"));
                }
            }
        });
    });

    ui.separator();

    match app.shader_editor_state.view_mode {
        ShaderViewMode::VisualShader => show_visual_shader(ui),
        ShaderViewMode::GdShaderCode => show_gdshader(app, ui),
        ShaderViewMode::WgslCompiled => show_wgsl(ui, &compile_result),
    }
}

fn show_visual_shader(ui: &mut Ui) {
    let (rect, _resp) = ui.allocate_exact_size(ui.available_size(), Sense::click());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, COLOR_BG_DARK);

    let nodes = [
        ("Input: TIME", Pos2::new(rect.min.x + 80.0, rect.center().y - 60.0), COLOR_WARN_YELLOW),
        ("Math: Sin", Pos2::new(rect.min.x + 240.0, rect.center().y - 60.0), COLOR_ACCENT_BLUE),
        ("Vector: UV * 2", Pos2::new(rect.min.x + 160.0, rect.center().y + 50.0), COLOR_SUCCESS_GREEN),
        ("Output: Fragment", Pos2::new(rect.min.x + 440.0, rect.center().y), Color32::from_rgb(0xfc, 0x7f, 0x7f)),
    ];

    let mut rects = Vec::new();
    for (name, pos, color) in nodes {
        let card = Rect::from_center_size(pos, Vec2::new(130.0, 50.0));
        rects.push(card);
        painter.rect_filled(card, 4.0, COLOR_PANEL_BG);
        painter.rect_stroke(card, 4.0, Stroke::new(1.5, color), egui::StrokeKind::Inside);
        painter.text(pos, Align2::CENTER_CENTER, name, FontId::proportional(11.0), Color32::WHITE);

        // Port circles
        painter.circle_filled(card.right_center(), 4.0, color);
        painter.circle_filled(card.left_center(), 4.0, COLOR_TEXT_MUTED);
    }

    // Bezier wires: TIME -> Sin -> Fragment, UV -> Fragment
    let wires = [(0, 1), (1, 3), (2, 3)];
    for (from_idx, to_idx) in wires {
        let p1 = rects[from_idx].right_center();
        let p2 = rects[to_idx].left_center();
        let dx = (p2.x - p1.x) * 0.5;
        let shape = CubicBezierShape::from_points_stroke(
            [p1, p1 + Vec2::new(dx, 0.0), p2 - Vec2::new(dx, 0.0), p2],
            false,
            Color32::TRANSPARENT,
            Stroke::new(2.5, COLOR_ACCENT_BLUE),
        );
        painter.add(shape);
    }
}

fn show_gdshader(app: &mut ForeplayEditorApp, ui: &mut Ui) {
    ScrollArea::both().show(ui, |ui| {
        ui.add(
            TextEdit::multiline(&mut app.shader_editor_state.shader_code)
                .font(FontId::monospace(12.0))
                .desired_width(f32::INFINITY)
                .code_editor(),
        );
    });
}

fn show_wgsl(
    ui: &mut Ui,
    compile_result: &Result<String, foreplay_render::shader::ShaderCompileError>,
) {
    ScrollArea::both().show(ui, |ui| {
        match compile_result {
            Ok(wgsl) => {
                let mut display_code = wgsl.clone();
                ui.add(
                    TextEdit::multiline(&mut display_code)
                        .font(FontId::monospace(11.5))
                        .desired_width(f32::INFINITY)
                        .interactive(false),
                );
            }
            Err(err) => {
                ui.vertical(|ui| {
                    ui.add_space(10.0);
                    ui.group(|ui| {
                        ui.colored_label(COLOR_ERROR_RED, format!("❌ Shader Compilation Error:\n{err}"));
                        ui.add_space(6.0);
                        ui.colored_label(
                            COLOR_TEXT_MUTED,
                            "Fix errors in the '.gdshader' tab. The WGSL compiler dynamically verifies syntax on every keystroke.",
                        );
                    });
                });
            }
        }
    });
}
