//! Godot 4 Dark-Blue/Slate Editor Theme (`foreplay-editor`).
//!
//! Faithful Godot 4 slate/dark-blue styling palette:
//! - Base slate background: `#1a1d24`
//! - Panel slate: `#21252b`
//! - Header / top bar: `#181b20`
//! - Input / widget background: `#2c313c`
//! - Signature Godot blue accent: `#478cbf`
//! - Text normal: `#dfe2e7`, Text muted: `#8a909d`

use egui::{
    Color32, CornerRadius, Margin, Stroke, Visuals,
    style::{WidgetVisuals, Widgets},
};

pub const COLOR_BG_DARK: Color32 = Color32::from_rgb(0x1a, 0x1d, 0x24);
pub const COLOR_PANEL_BG: Color32 = Color32::from_rgb(0x21, 0x25, 0x2b);
pub const COLOR_HEADER_BG: Color32 = Color32::from_rgb(0x18, 0x1b, 0x20);
pub const COLOR_WIDGET_BG: Color32 = Color32::from_rgb(0x2c, 0x31, 0x3c);
pub const COLOR_WIDGET_HOVER: Color32 = Color32::from_rgb(0x37, 0x3e, 0x4c);
pub const COLOR_WIDGET_ACTIVE: Color32 = Color32::from_rgb(0x40, 0x48, 0x58);
pub const COLOR_ACCENT_BLUE: Color32 = Color32::from_rgb(0x47, 0x8c, 0xbf);
pub const COLOR_ACCENT_HOVER: Color32 = Color32::from_rgb(0x5c, 0xa1, 0xd4);
pub const COLOR_TEXT_NORMAL: Color32 = Color32::from_rgb(0xdf, 0xe2, 0xe7);
pub const COLOR_TEXT_MUTED: Color32 = Color32::from_rgb(0x8a, 0x90, 0x9d);
pub const COLOR_BORDER: Color32 = Color32::from_rgb(0x32, 0x38, 0x43);
pub const COLOR_SUCCESS_GREEN: Color32 = Color32::from_rgb(0x4e, 0xc9, 0xb0);
pub const COLOR_WARN_YELLOW: Color32 = Color32::from_rgb(0xdc, 0xca, 0x84);
pub const COLOR_ERROR_RED: Color32 = Color32::from_rgb(0xf4, 0x47, 0x47);

/// X/Y/Z 3D World axis colors
pub const COLOR_AXIS_X: Color32 = Color32::from_rgb(0xeb, 0x4b, 0x4b); // Red
pub const COLOR_AXIS_Y: Color32 = Color32::from_rgb(0x4b, 0xeb, 0x4b); // Green
pub const COLOR_AXIS_Z: Color32 = Color32::from_rgb(0x4b, 0x7b, 0xeb); // Blue

/// Applies the Godot 4 dark-blue / slate editor visual styling to an `egui::Context`.
pub fn apply_godot_theme(ctx: &egui::Context) {
    let mut visuals = Visuals::dark();

    visuals.panel_fill = COLOR_PANEL_BG;
    visuals.window_fill = COLOR_BG_DARK;
    visuals.extreme_bg_color = COLOR_BG_DARK;
    visuals.selection.bg_fill = COLOR_ACCENT_BLUE;
    visuals.selection.stroke = Stroke::new(1.0, Color32::WHITE);

    visuals.window_corner_radius = CornerRadius::same(4);
    visuals.menu_corner_radius = CornerRadius::same(4);
    visuals.window_stroke = Stroke::new(1.0, COLOR_BORDER);

    visuals.widgets = Widgets {
        noninteractive: WidgetVisuals {
            bg_fill: COLOR_PANEL_BG,
            weak_bg_fill: COLOR_PANEL_BG,
            bg_stroke: Stroke::new(1.0, COLOR_BORDER),
            corner_radius: CornerRadius::same(3),
            fg_stroke: Stroke::new(1.0, COLOR_TEXT_MUTED),
            expansion: 0.0,
        },
        inactive: WidgetVisuals {
            bg_fill: COLOR_WIDGET_BG,
            weak_bg_fill: COLOR_WIDGET_BG,
            bg_stroke: Stroke::new(1.0, COLOR_BORDER),
            corner_radius: CornerRadius::same(3),
            fg_stroke: Stroke::new(1.0, COLOR_TEXT_NORMAL),
            expansion: 0.0,
        },
        hovered: WidgetVisuals {
            bg_fill: COLOR_WIDGET_HOVER,
            weak_bg_fill: COLOR_WIDGET_HOVER,
            bg_stroke: Stroke::new(1.0, COLOR_ACCENT_BLUE),
            corner_radius: CornerRadius::same(3),
            fg_stroke: Stroke::new(1.0, Color32::WHITE),
            expansion: 1.0,
        },
        active: WidgetVisuals {
            bg_fill: COLOR_ACCENT_BLUE,
            weak_bg_fill: COLOR_ACCENT_BLUE,
            bg_stroke: Stroke::new(1.0, COLOR_ACCENT_HOVER),
            corner_radius: CornerRadius::same(3),
            fg_stroke: Stroke::new(1.0, Color32::WHITE),
            expansion: 1.0,
        },
        open: WidgetVisuals {
            bg_fill: COLOR_WIDGET_ACTIVE,
            weak_bg_fill: COLOR_WIDGET_ACTIVE,
            bg_stroke: Stroke::new(1.0, COLOR_ACCENT_BLUE),
            corner_radius: CornerRadius::same(3),
            fg_stroke: Stroke::new(1.0, Color32::WHITE),
            expansion: 0.0,
        },
    };

    ctx.set_visuals(visuals);

    let mut style = (*ctx.style_of(egui::Theme::Dark)).clone();
    style.spacing.item_spacing = egui::vec2(6.0, 4.0);
    style.spacing.button_padding = egui::vec2(8.0, 4.0);
    style.spacing.window_margin = Margin::same(6);
    ctx.set_style_of(egui::Theme::Dark, style);
}
