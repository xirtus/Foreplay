//! Godot 4 Audio Bus Mixer Dock (`foreplay-editor`).
//!
//! Mirrors Godot 4's Audio Bus Mixer bottom dock:
//! - Vertical channel strips (`Master`, `Music`, `SFX`, `Voice`)
//! - VU meter peak bars, volume faders, Mute/Solo buttons, bus sends, and FX racks.

use egui::{Color32, RichText, ScrollArea, Slider, Stroke, Ui, Vec2};
use crate::ForeplayEditorApp;
use crate::theme::{COLOR_ACCENT_BLUE, COLOR_BORDER, COLOR_ERROR_RED, COLOR_SUCCESS_GREEN, COLOR_TEXT_MUTED, COLOR_TEXT_NORMAL, COLOR_WARN_YELLOW};

#[derive(Clone, Debug, PartialEq)]
pub struct AudioBusStrip {
    pub name: &'static str,
    pub volume_db: f32,
    pub is_muted: bool,
    pub is_solo: bool,
    pub send_target: &'static str,
    pub reverb_enabled: bool,
    pub eq_enabled: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AudioMixerState {
    pub strips: Vec<AudioBusStrip>,
}

impl Default for AudioMixerState {
    fn default() -> Self {
        Self {
            strips: vec![
                AudioBusStrip {
                    name: "Master",
                    volume_db: 0.0,
                    is_muted: false,
                    is_solo: false,
                    send_target: "Output",
                    reverb_enabled: false,
                    eq_enabled: true,
                },
                AudioBusStrip {
                    name: "Music",
                    volume_db: -3.5,
                    is_muted: false,
                    is_solo: false,
                    send_target: "Master",
                    reverb_enabled: false,
                    eq_enabled: false,
                },
                AudioBusStrip {
                    name: "SFX",
                    volume_db: -1.0,
                    is_muted: false,
                    is_solo: false,
                    send_target: "Master",
                    reverb_enabled: true,
                    eq_enabled: true,
                },
                AudioBusStrip {
                    name: "Voice",
                    volume_db: 0.0,
                    is_muted: false,
                    is_solo: false,
                    send_target: "Master",
                    reverb_enabled: false,
                    eq_enabled: false,
                },
            ],
        }
    }
}

pub fn show(app: &mut ForeplayEditorApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Audio Bus Mixer").strong().color(COLOR_ACCENT_BLUE));
        ui.colored_label(COLOR_TEXT_MUTED, "| Multi-track real-time mixing with Bevy AudioServer backend");
    });

    ui.separator();

    ScrollArea::horizontal().show(ui, |ui| {
        ui.horizontal(|ui| {
            for strip in &mut app.audio_mixer_state.strips {
                render_channel_strip(ui, strip);
                ui.separator();
            }
        });
    });
}

fn render_channel_strip(ui: &mut Ui, strip: &mut AudioBusStrip) {
    ui.allocate_ui(Vec2::new(140.0, 220.0), |ui| {
        ui.group(|ui| {
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(strip.name).strong().color(COLOR_ACCENT_BLUE));
            });
            ui.separator();

            ui.horizontal(|ui| {
                // VU Meter bar
                let (meter_rect, _resp) = ui.allocate_exact_size(Vec2::new(14.0, 110.0), egui::Sense::hover());
                let painter = ui.painter_at(meter_rect);
                painter.rect_filled(meter_rect, 2.0, Color32::from_rgb(0x18, 0x1b, 0x22));
                painter.rect_stroke(meter_rect, 2.0, Stroke::new(1.0, COLOR_BORDER), egui::StrokeKind::Inside);

                // Simulated active signal level
                let level_ratio = if strip.is_muted {
                    0.0
                } else {
                    ((strip.volume_db + 60.0) / 66.0).clamp(0.05, 0.88)
                };
                let fill_h = meter_rect.height() * level_ratio;
                let fill_rect = egui::Rect::from_min_max(
                    egui::pos2(meter_rect.min.x, meter_rect.max.y - fill_h),
                    meter_rect.max,
                );

                let meter_color = if level_ratio > 0.85 {
                    COLOR_ERROR_RED
                } else if level_ratio > 0.70 {
                    COLOR_WARN_YELLOW
                } else {
                    COLOR_SUCCESS_GREEN
                };
                painter.rect_filled(fill_rect, 2.0, meter_color);

                // Vertical Volume Slider
                let slider = Slider::new(&mut strip.volume_db, -60.0..=6.0)
                    .vertical()
                    .show_value(false);
                ui.add_sized(Vec2::new(28.0, 110.0), slider);

                // Readout
                ui.vertical(|ui| {
                    ui.colored_label(COLOR_TEXT_NORMAL, format!("{:.1}\ndB", strip.volume_db));
                });
            });

            ui.separator();

            // Mute / Solo buttons
            ui.horizontal(|ui| {
                let mute_btn = ui.selectable_label(strip.is_muted, "M");
                if mute_btn.clicked() {
                    strip.is_muted = !strip.is_muted;
                }

                let solo_btn = ui.selectable_label(strip.is_solo, "S");
                if solo_btn.clicked() {
                    strip.is_solo = !strip.is_solo;
                }

                ui.colored_label(COLOR_TEXT_MUTED, format!("-> {}", strip.send_target));
            });

            ui.separator();

            // FX Rack
            ui.collapsing("FX Rack", |ui| {
                ui.checkbox(&mut strip.reverb_enabled, "Reverb");
                ui.checkbox(&mut strip.eq_enabled, "3-Band EQ");
            });
        });
    });
}
