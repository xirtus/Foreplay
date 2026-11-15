//! Apple Logic Pro & Final Cut Pro "Studio Obsidian & Chroma" & Godot Slate Editor Themes (`foreplay-editor`).
//!
//! Provides:
//! - `EditorThemePreset::StudioObsidianPro`: Apple Logic Pro 11 + Final Cut Pro 11 luxury aesthetic
//!   - Deep OLED obsidian canvas (`#0d0f14`), frosted charcoal panels (`#151922`), elevated cards (`#1d2230`)
//!   - Smooth 8px panel/window radii, 6px pill widget radii, crisp 1px `#2b3347` borders
//!   - Luminous electric cobalt/indigo accent (`#5e81ff`)
//!   - Jewel-Toned Semantic Track Chroma palette (Coral 3D, Cyan 2D, Mint UI, Violet Anim, Amber Audio, Magenta AI, Gold Polyamour, Emerald Three.js)
//! - `EditorThemePreset::ClassicGodotSlate`: Faithful Godot 4 dark-blue / slate palette (`#1a1d24`, `#478cbf`)

use egui::{
    Color32, CornerRadius, Margin, Stroke, Visuals,
    style::{WidgetVisuals, Widgets},
};

/// Active editor theme preset.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize, Hash)]
pub enum EditorThemePreset {
    #[default]
    StudioObsidianPro,
    ClassicGodotSlate,
}

impl EditorThemePreset {
    pub const ALL: [Self; 2] = [Self::StudioObsidianPro, Self::ClassicGodotSlate];

    pub fn display_name(self) -> &'static str {
        match self {
            Self::StudioObsidianPro => "Studio Obsidian Pro (Logic & FCP)",
            Self::ClassicGodotSlate => "Classic Godot Slate",
        }
    }
}

// =========================================================================
// Apple Pro "Studio Obsidian & Chroma" Palette
// =========================================================================

pub const STUDIO_OBSIDIAN_CANVAS: Color32 = Color32::from_rgb(0x0d, 0x0f, 0x14);
pub const STUDIO_OBSIDIAN_PANEL: Color32 = Color32::from_rgb(0x15, 0x19, 0x22);
pub const STUDIO_OBSIDIAN_CARD: Color32 = Color32::from_rgb(0x1d, 0x22, 0x30);
pub const STUDIO_OBSIDIAN_CARD_HOVER: Color32 = Color32::from_rgb(0x25, 0x2c, 0x3e);
pub const STUDIO_OBSIDIAN_CARD_ACTIVE: Color32 = Color32::from_rgb(0x2e, 0x36, 0x4c);
pub const STUDIO_OBSIDIAN_BORDER: Color32 = Color32::from_rgb(0x2b, 0x33, 0x47);
pub const STUDIO_OBSIDIAN_ACCENT: Color32 = Color32::from_rgb(0x5e, 0x81, 0xff);
pub const STUDIO_OBSIDIAN_ACCENT_HOVER: Color32 = Color32::from_rgb(0x7a, 0x97, 0xff);
pub const STUDIO_OBSIDIAN_ACCENT_ACTIVE: Color32 = Color32::from_rgb(0x4b, 0x6f, 0xeb);
pub const STUDIO_OBSIDIAN_TEXT_NORMAL: Color32 = Color32::from_rgb(0xf0, 0xf3, 0xf8);
pub const STUDIO_OBSIDIAN_TEXT_MUTED: Color32 = Color32::from_rgb(0x8e, 0x9b, 0xb0);

// =========================================================================
// Jewel-Toned Semantic Track Chroma Palette
// =========================================================================

pub const CHROMA_CORAL_3D: Color32 = Color32::from_rgb(0xff, 0x6b, 0x6b);
pub const CHROMA_CYAN_2D: Color32 = Color32::from_rgb(0x38, 0xbd, 0xf8);
pub const CHROMA_MINT_UI: Color32 = Color32::from_rgb(0x34, 0xd3, 0x99);
pub const CHROMA_VIOLET_ANIM: Color32 = Color32::from_rgb(0xa8, 0x55, 0xf7);
pub const CHROMA_AMBER_AUDIO: Color32 = Color32::from_rgb(0xf5, 0x9e, 0x0b);
pub const CHROMA_MAGENTA_AI: Color32 = Color32::from_rgb(0xec, 0x48, 0x99);
pub const CHROMA_GOLD_POLYAMOUR: Color32 = Color32::from_rgb(0xfb, 0xbf, 0x24);
pub const CHROMA_EMERALD_THREEJS: Color32 = Color32::from_rgb(0x10, 0xb9, 0x81);

// =========================================================================
// Classic Godot Slate Palette (Preserved for backwards compatibility)
// =========================================================================

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

/// Applies the active editor theme preset to an `egui::Context`.
pub fn apply_theme(ctx: &egui::Context, preset: EditorThemePreset) {
    match preset {
        EditorThemePreset::StudioObsidianPro => apply_studio_obsidian_theme(ctx),
        EditorThemePreset::ClassicGodotSlate => apply_godot_theme(ctx),
    }
}

/// Applies the macOS Logic Pro 11 + Final Cut Pro 11 "Studio Obsidian & Chroma" luxury styling.
pub fn apply_studio_obsidian_theme(ctx: &egui::Context) {
    let mut visuals = Visuals::dark();

    visuals.panel_fill = STUDIO_OBSIDIAN_PANEL;
    visuals.window_fill = STUDIO_OBSIDIAN_CANVAS;
    visuals.extreme_bg_color = STUDIO_OBSIDIAN_CANVAS;
    visuals.selection.bg_fill = STUDIO_OBSIDIAN_ACCENT;
    visuals.selection.stroke = Stroke::new(1.0, Color32::WHITE);

    visuals.window_corner_radius = CornerRadius::same(8);
    visuals.menu_corner_radius = CornerRadius::same(6);
    visuals.window_stroke = Stroke::new(1.0, STUDIO_OBSIDIAN_BORDER);

    visuals.widgets = Widgets {
        noninteractive: WidgetVisuals {
            bg_fill: STUDIO_OBSIDIAN_PANEL,
            weak_bg_fill: STUDIO_OBSIDIAN_PANEL,
            bg_stroke: Stroke::new(1.0, STUDIO_OBSIDIAN_BORDER),
            corner_radius: CornerRadius::same(6),
            fg_stroke: Stroke::new(1.0, STUDIO_OBSIDIAN_TEXT_MUTED),
            expansion: 0.0,
        },
        inactive: WidgetVisuals {
            bg_fill: STUDIO_OBSIDIAN_CARD,
            weak_bg_fill: STUDIO_OBSIDIAN_CARD,
            bg_stroke: Stroke::new(1.0, STUDIO_OBSIDIAN_BORDER),
            corner_radius: CornerRadius::same(6),
            fg_stroke: Stroke::new(1.0, STUDIO_OBSIDIAN_TEXT_NORMAL),
            expansion: 0.0,
        },
        hovered: WidgetVisuals {
            bg_fill: STUDIO_OBSIDIAN_CARD_HOVER,
            weak_bg_fill: STUDIO_OBSIDIAN_CARD_HOVER,
            bg_stroke: Stroke::new(1.0, STUDIO_OBSIDIAN_ACCENT),
            corner_radius: CornerRadius::same(6),
            fg_stroke: Stroke::new(1.0, Color32::WHITE),
            expansion: 1.0,
        },
        active: WidgetVisuals {
            bg_fill: STUDIO_OBSIDIAN_ACCENT,
            weak_bg_fill: STUDIO_OBSIDIAN_ACCENT,
            bg_stroke: Stroke::new(1.0, STUDIO_OBSIDIAN_ACCENT_HOVER),
            corner_radius: CornerRadius::same(6),
            fg_stroke: Stroke::new(1.0, Color32::WHITE),
            expansion: 1.0,
        },
        open: WidgetVisuals {
            bg_fill: STUDIO_OBSIDIAN_CARD_ACTIVE,
            weak_bg_fill: STUDIO_OBSIDIAN_CARD_ACTIVE,
            bg_stroke: Stroke::new(1.0, STUDIO_OBSIDIAN_ACCENT),
            corner_radius: CornerRadius::same(6),
            fg_stroke: Stroke::new(1.0, Color32::WHITE),
            expansion: 0.0,
        },
    };

    ctx.set_visuals(visuals);

    let mut style = (*ctx.style_of(egui::Theme::Dark)).clone();
    style.spacing.item_spacing = egui::vec2(6.0, 4.0);
    style.spacing.button_padding = egui::vec2(8.0, 5.0);
    style.spacing.window_margin = Margin::same(8);
    ctx.set_style_of(egui::Theme::Dark, style);
}

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
