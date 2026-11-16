//! Apple Logic Pro & Godot 4 Audio Bus Mixer Dock (`foreplay-editor`).
//!
//! Features:
//! - Tactile Channel EQ & ChromaGlow frequency response curve overlaid on 32-band spectrum analyzer
//! - 270° swept rotary knobs for parametric EQ (Low, Mid, High) and Reverb wet mix
//! - Vertical channel strips (`Master`, `Music`, `SFX`, `Voice`) with peak VU meters, volume faders, Mute/Solo.

use egui::{Color32, RichText, ScrollArea, Slider, Stroke, Ui, Vec2};
use crate::ForeplayEditorApp;
use crate::theme::{
    CHROMA_AMBER_AUDIO, CHROMA_CYAN_2D, CHROMA_MINT_UI, CHROMA_VIOLET_ANIM,
    COLOR_BORDER, COLOR_ERROR_RED, COLOR_SUCCESS_GREEN, COLOR_TEXT_MUTED,
    COLOR_TEXT_NORMAL, COLOR_WARN_YELLOW, STUDIO_OBSIDIAN_ACCENT,
};
use crate::widgets::{eq_response_and_spectrum_canvas, rotary_arc_knob};

#[derive(Clone, Debug, PartialEq)]
pub struct AudioBusStrip {
    pub name: &'static str,
    pub volume_db: f32,
    pub is_muted: bool,
    pub is_solo: bool,
    pub send_target: &'static str,
    pub reverb_enabled: bool,
    pub eq_enabled: bool,
    pub eq_low_db: f32,
    pub eq_mid_db: f32,
    pub eq_high_db: f32,
    pub reverb_wet: f32,
    pub pan: f32,
}

#[derive(Clone, Debug)]
pub struct AudioMixerState {
    pub strips: Vec<AudioBusStrip>,
    pub spectrum_bars: [f32; 32],
    pub synth: foreplay_audio::procedural_synth::SkateFoleySynth,
    pub analyzer: foreplay_audio::procedural_synth::SpectrumAnalyzer32,
}

impl Default for AudioMixerState {
    fn default() -> Self {
        let mut synth = foreplay_audio::procedural_synth::SkateFoleySynth::new();
        synth.set_telemetry(12.0, 0.6, true, 8.0);
        let mut analyzer = foreplay_audio::procedural_synth::SpectrumAnalyzer32::new();
        let mut buf = vec![[0.0_f32; 2]; 512];
        synth.generate_block(&mut buf);
        let dbs = analyzer.process_block(&buf);
        let mut bars = [0.0f32; 32];
        for (i, bar) in bars.iter_mut().enumerate() {
            *bar = ((dbs[i] + 70.0) / 70.0).clamp(0.05, 0.95);
        }

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
                    eq_low_db: 0.0,
                    eq_mid_db: 0.5,
                    eq_high_db: -0.5,
                    reverb_wet: 0.15,
                    pan: 0.0,
                },
                AudioBusStrip {
                    name: "Music",
                    volume_db: -3.5,
                    is_muted: false,
                    is_solo: false,
                    send_target: "Master",
                    reverb_enabled: false,
                    eq_enabled: true,
                    eq_low_db: -1.0,
                    eq_mid_db: 1.0,
                    eq_high_db: 2.0,
                    reverb_wet: 0.10,
                    pan: 0.0,
                },
                AudioBusStrip {
                    name: "SFX",
                    volume_db: -1.0,
                    is_muted: false,
                    is_solo: false,
                    send_target: "Master",
                    reverb_enabled: true,
                    eq_enabled: true,
                    eq_low_db: 2.5,
                    eq_mid_db: -1.5,
                    eq_high_db: 1.0,
                    reverb_wet: 0.40,
                    pan: -0.1,
                },
                AudioBusStrip {
                    name: "Voice",
                    volume_db: 0.0,
                    is_muted: false,
                    is_solo: false,
                    send_target: "Master",
                    reverb_enabled: false,
                    eq_enabled: true,
                    eq_low_db: -2.0,
                    eq_mid_db: 3.0,
                    eq_high_db: 1.5,
                    reverb_wet: 0.20,
                    pan: 0.0,
                },
            ],
            spectrum_bars: bars,
            synth,
            analyzer,
        }
    }
}

pub fn show(app: &mut ForeplayEditorApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Logic Pro Studio Audio Bus Mixer").strong().color(STUDIO_OBSIDIAN_ACCENT));
        ui.colored_label(COLOR_TEXT_MUTED, "| ChromaGlow Channel EQ & 32-Band Spectrum Analyzer");
    });

    ui.separator();

    // 1. Logic Pro Channel EQ & 32-band Spectrum Canvas Header
    let speed = if let Some(master) = app.audio_mixer_state.strips.first() {
        ((master.volume_db + 30.0) / 36.0 * 20.0).max(0.0)
    } else {
        10.0
    };
    app.audio_mixer_state.synth.set_telemetry(speed, 0.65, true, speed * 0.8);
    let mut buf = vec![[0.0_f32; 2]; 512];
    app.audio_mixer_state.synth.generate_block(&mut buf);
    let dbs = app.audio_mixer_state.analyzer.process_block(&buf);
    for (i, bar) in app.audio_mixer_state.spectrum_bars.iter_mut().enumerate() {
        *bar = ((dbs[i] + 70.0) / 70.0).clamp(0.05, 0.95);
    }

    let (master_low, master_mid, master_high) = if let Some(master) = app.audio_mixer_state.strips.first() {
        (master.eq_low_db, master.eq_mid_db, master.eq_high_db)
    } else {
        (0.0, 0.0, 0.0)
    };

    ui.group(|ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Master Channel EQ Response").strong().color(CHROMA_AMBER_AUDIO));
            ui.colored_label(COLOR_TEXT_MUTED, "(Live 20Hz - 20kHz Transfer Function)");
        });
        eq_response_and_spectrum_canvas(
            ui,
            master_low,
            master_mid,
            master_high,
            &app.audio_mixer_state.spectrum_bars,
        );
    });

    ui.separator();

    // 2. Channel Strips
    ScrollArea::horizontal().show(ui, |ui| {
        ui.horizontal(|ui| {
            for strip in &mut app.audio_mixer_state.strips {
                render_channel_strip(ui, strip);
                ui.separator();
            }
        });
    });

    ui.separator();

    // 3. SexLife Moan Adaptive Music DAW & Stem Manifest Tray
    ui.collapsing("🎵 SexLife Moan DAW — Adaptive Skate Soundtrack Stems", |ui| {
        ui.horizontal_wrapped(|ui| {
            let moan_link = foreplay_script::MoanProjectStemLink::default_skate_soundtrack();
            let feel_intensity = ((speed - 5.0) / 15.0).clamp(0.0, 1.0);
            let snap = moan_link.evaluate_adaptive_mix(feel_intensity, true, 16.0);

            ui.group(|ui| {
                ui.label(RichText::new("Track: 'SkateOrCry_Mix' (120.0 BPM)").strong().color(CHROMA_AMBER_AUDIO));
                ui.label(format!("Bar: {} | Beat: {:.1} | Intensity: {:.2}", snap.current_bar, snap.beat_in_bar, snap.intensity));
                let trans_str = if snap.is_quantized_transition_point { "● QUANTIZED TRANSITION" } else { "○ IN-BAR" };
                ui.colored_label(CHROMA_MINT_UI, trans_str);
            });

            for (stem_name, gain) in &snap.stem_gains {
                let cutoff = snap.stem_cutoffs_hz.get(stem_name).copied().unwrap_or(20000.0);
                ui.group(|ui| {
                    ui.label(RichText::new(stem_name).strong().color(CHROMA_CYAN_2D));
                    ui.label(format!("Gain: {:.2}", gain));
                    ui.label(format!("Cutoff: {:.0} Hz", cutoff));
                    let audible = *gain > 0.05;
                    ui.colored_label(if audible { COLOR_SUCCESS_GREEN } else { COLOR_TEXT_MUTED }, if audible { "● AUDIBLE" } else { "○ MUTED" });
                });
            }
        });
    });
}

fn render_channel_strip(ui: &mut Ui, strip: &mut AudioBusStrip) {
    ui.allocate_ui(Vec2::new(170.0, 310.0), |ui| {
        ui.group(|ui| {
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(strip.name).strong().color(STUDIO_OBSIDIAN_ACCENT));
            });
            ui.separator();

            // VU Meter & Fader
            ui.horizontal(|ui| {
                // VU Meter bar
                let (meter_rect, _resp) = ui.allocate_exact_size(Vec2::new(12.0, 110.0), egui::Sense::hover());
                let painter = ui.painter_at(meter_rect);
                painter.rect_filled(meter_rect, 2.0, Color32::from_rgb(0x11, 0x14, 0x1c));
                painter.rect_stroke(meter_rect, 2.0, Stroke::new(1.0, COLOR_BORDER), egui::StrokeKind::Inside);

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
                ui.add_sized(Vec2::new(26.0, 110.0), slider);

                // Readout & Pan
                ui.vertical(|ui| {
                    ui.colored_label(COLOR_TEXT_NORMAL, format!("{:.1}\ndB", strip.volume_db));
                    ui.add_space(8.0);
                    rotary_arc_knob(ui, "Pan", &mut strip.pan, -1.0, 1.0, CHROMA_CYAN_2D);
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

            // Tactile FX Rack with Rotary Arc Knobs
            ui.collapsing("🎛 Smart FX Rack", |ui| {
                ui.checkbox(&mut strip.eq_enabled, "3-Band Parametric EQ");
                if strip.eq_enabled {
                    ui.horizontal_wrapped(|ui| {
                        rotary_arc_knob(ui, "Low", &mut strip.eq_low_db, -12.0, 12.0, CHROMA_CYAN_2D);
                        rotary_arc_knob(ui, "Mid", &mut strip.eq_mid_db, -12.0, 12.0, CHROMA_AMBER_AUDIO);
                        rotary_arc_knob(ui, "High", &mut strip.eq_high_db, -12.0, 12.0, CHROMA_VIOLET_ANIM);
                    });
                }

                ui.separator();
                ui.checkbox(&mut strip.reverb_enabled, "Studio Reverb");
                if strip.reverb_enabled {
                    ui.horizontal(|ui| {
                        rotary_arc_knob(ui, "Wet", &mut strip.reverb_wet, 0.0, 1.0, CHROMA_MINT_UI);
                    });
                }
            });
        });
    });
}
