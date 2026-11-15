//! Godot 4 Collapsible Bottom Panel Switcher Bar (`foreplay-editor`).
//!
//! Provides a persistent bottom switcher bar matching Godot 4:
//! `Output` | `Debugger` | `Audio` | `Animation` | `Shader Editor` | `TileMap` | `AI Harness`
//! with expand/collapse toggle.

use egui::{Color32, RichText, Ui};
use crate::ForeplayEditorApp;
use crate::tabs::{ai_harness, animation, audio_mixer, console, debugger, shader_editor, tilemap};
use crate::theme::{COLOR_TEXT_MUTED, COLOR_TEXT_NORMAL};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BottomPanelTab {
    Output,
    Debugger,
    AudioMixer,
    Animation,
    ShaderEditor,
    TileMap,
    AiHarness,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BottomPanelState {
    pub is_expanded: bool,
    pub active_tab: BottomPanelTab,
}

impl Default for BottomPanelState {
    fn default() -> Self {
        Self {
            is_expanded: true,
            active_tab: BottomPanelTab::Output,
        }
    }
}

pub fn render_bottom_panel(app: &mut ForeplayEditorApp, ui: &mut Ui) {
    // 1. Bottom Switcher Bar
    ui.horizontal(|ui| {
        tab_button(app, ui, BottomPanelTab::Output, "📄 Output");
        tab_button(app, ui, BottomPanelTab::Debugger, "🐞 Debugger");
        tab_button(app, ui, BottomPanelTab::AudioMixer, "🔊 Audio");
        tab_button(app, ui, BottomPanelTab::Animation, "🎬 Animation");
        tab_button(app, ui, BottomPanelTab::ShaderEditor, "✨ Shader Editor");
        tab_button(app, ui, BottomPanelTab::TileMap, "🗺 TileMap");
        tab_button(app, ui, BottomPanelTab::AiHarness, "🤖 AI Harness");

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let toggle_label = if app.bottom_panel_state.is_expanded { "▼ Hide" } else { "▲ Show" };
            if ui.button(RichText::new(toggle_label).color(COLOR_TEXT_MUTED)).clicked() {
                app.bottom_panel_state.is_expanded = !app.bottom_panel_state.is_expanded;
            }
        });
    });

    // 2. Panel Content (if expanded)
    if app.bottom_panel_state.is_expanded {
        ui.separator();
        ui.allocate_ui(egui::vec2(ui.available_width(), 200.0), |ui| {
            match app.bottom_panel_state.active_tab {
                BottomPanelTab::Output => console::show(app, ui),
                BottomPanelTab::Debugger => debugger::show(app, ui),
                BottomPanelTab::AudioMixer => audio_mixer::show(app, ui),
                BottomPanelTab::Animation => animation::show(app, ui),
                BottomPanelTab::ShaderEditor => shader_editor::show(app, ui),
                BottomPanelTab::TileMap => tilemap::show(app, ui),
                BottomPanelTab::AiHarness => ai_harness::show(app, ui),
            }
        });
    }
}

fn tab_button(app: &mut ForeplayEditorApp, ui: &mut Ui, tab: BottomPanelTab, label: &str) {
    let is_active = app.bottom_panel_state.active_tab == tab && app.bottom_panel_state.is_expanded;
    let text = if is_active {
        RichText::new(label).strong().color(Color32::WHITE)
    } else {
        RichText::new(label).color(COLOR_TEXT_NORMAL)
    };

    if ui.selectable_label(is_active, text).clicked() {
        if app.bottom_panel_state.active_tab == tab && app.bottom_panel_state.is_expanded {
            app.bottom_panel_state.is_expanded = false;
        } else {
            app.bottom_panel_state.active_tab = tab;
            app.bottom_panel_state.is_expanded = true;
        }
    }
}
