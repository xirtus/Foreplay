//! Project Settings & Godot Input Map Modal (`foreplay-editor`).
//!
//! Mirrors Godot 4's Project Settings dialog:
//! - General settings (Application name, main scene, physics ticks, gravity, window dimensions)
//! - Godot `Input Map` tab: named actions, key/joypad bindings, deadzone sliders.

use std::collections::BTreeMap;
use egui::{DragValue, RichText, ScrollArea, Slider, Ui};
use crate::ForeplayEditorApp;
use crate::theme::{COLOR_ACCENT_BLUE, COLOR_SUCCESS_GREEN};

#[derive(Clone, Debug, PartialEq)]
pub struct InputActionBinding {
    pub name: String,
    pub deadzone: f32,
    pub events: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProjectSettingsState {
    pub active_tab: ProjectSettingsTab,
    pub app_name: String,
    pub main_scene_path: String,
    pub physics_ticks_per_sec: u32,
    pub default_gravity: f32,
    pub window_width: u32,
    pub window_height: u32,
    pub vsync_enabled: bool,
    pub new_action_name: String,
    pub input_actions: BTreeMap<String, InputActionBinding>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ProjectSettingsTab {
    General,
    InputMap,
}

impl Default for ProjectSettingsState {
    fn default() -> Self {
        let mut input_actions = BTreeMap::new();
        input_actions.insert(
            "ollie".into(),
            InputActionBinding {
                name: "ollie".into(),
                deadzone: 0.5,
                events: vec!["Key: Space".into(), "Joypad: Button A / Cross".into()],
            },
        );
        input_actions.insert(
            "grind".into(),
            InputActionBinding {
                name: "grind".into(),
                deadzone: 0.5,
                events: vec!["Key: Shift".into(), "Joypad: Button X / Square".into()],
            },
        );
        input_actions.insert(
            "move_forward".into(),
            InputActionBinding {
                name: "move_forward".into(),
                deadzone: 0.2,
                events: vec!["Key: W".into(), "Key: Up".into(), "Joypad: Left Stick Up".into()],
            },
        );
        input_actions.insert(
            "move_backward".into(),
            InputActionBinding {
                name: "move_backward".into(),
                deadzone: 0.2,
                events: vec!["Key: S".into(), "Key: Down".into(), "Joypad: Left Stick Down".into()],
            },
        );
        input_actions.insert(
            "move_left".into(),
            InputActionBinding {
                name: "move_left".into(),
                deadzone: 0.2,
                events: vec!["Key: A".into(), "Key: Left".into(), "Joypad: Left Stick Left".into()],
            },
        );
        input_actions.insert(
            "move_right".into(),
            InputActionBinding {
                name: "move_right".into(),
                deadzone: 0.2,
                events: vec!["Key: D".into(), "Key: Right".into(), "Joypad: Left Stick Right".into()],
            },
        );

        Self {
            active_tab: ProjectSettingsTab::InputMap,
            app_name: "Foreplay Game Engine".into(),
            main_scene_path: "res://scenes/park_demo.fscn".into(),
            physics_ticks_per_sec: 60,
            default_gravity: 9.8,
            window_width: 1280,
            window_height: 720,
            vsync_enabled: true,
            new_action_name: String::new(),
            input_actions,
        }
    }
}

pub fn show(app: &mut ForeplayEditorApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Project Settings").strong().size(16.0).color(COLOR_ACCENT_BLUE));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("✖ Close").clicked() {
                app.active_modal = None;
            }
        });
    });

    ui.separator();

    ui.horizontal(|ui| {
        ui.selectable_value(&mut app.project_settings_state.active_tab, ProjectSettingsTab::General, "⚙ General");
        ui.selectable_value(&mut app.project_settings_state.active_tab, ProjectSettingsTab::InputMap, "🎮 Input Map");
    });

    ui.separator();

    match app.project_settings_state.active_tab {
        ProjectSettingsTab::General => show_general_tab(app, ui),
        ProjectSettingsTab::InputMap => show_input_map_tab(app, ui),
    }
}

fn show_general_tab(app: &mut ForeplayEditorApp, ui: &mut Ui) {
    ScrollArea::vertical().show(ui, |ui| {
        ui.label(RichText::new("Application").strong());
        ui.horizontal(|ui| {
            ui.label("Config / Name:");
            ui.text_edit_singleline(&mut app.project_settings_state.app_name);
        });
        ui.horizontal(|ui| {
            ui.label("Run / Main Scene:");
            ui.text_edit_singleline(&mut app.project_settings_state.main_scene_path);
        });

        ui.separator();
        ui.label(RichText::new("Physics").strong());
        ui.horizontal(|ui| {
            ui.label("Physics Process Ticks/Sec:");
            ui.add(DragValue::new(&mut app.project_settings_state.physics_ticks_per_sec).range(10..=240));
        });
        ui.horizontal(|ui| {
            ui.label("3D Default Gravity (m/s²):");
            ui.add(DragValue::new(&mut app.project_settings_state.default_gravity).speed(0.1));
        });

        ui.separator();
        ui.label(RichText::new("Display / Window").strong());
        ui.horizontal(|ui| {
            ui.label("Size / Viewport Width:");
            ui.add(DragValue::new(&mut app.project_settings_state.window_width));
            ui.label("Height:");
            ui.add(DragValue::new(&mut app.project_settings_state.window_height));
        });
        ui.checkbox(&mut app.project_settings_state.vsync_enabled, "VSync Mode Enabled");
    });
}

fn show_input_map_tab(app: &mut ForeplayEditorApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.label("Action Name:");
        ui.text_edit_singleline(&mut app.project_settings_state.new_action_name);
        if ui.button("➕ Add Action").clicked() && !app.project_settings_state.new_action_name.is_empty() {
            let action_key = app.project_settings_state.new_action_name.clone();
            app.project_settings_state.input_actions.insert(
                action_key.clone(),
                InputActionBinding {
                    name: action_key,
                    deadzone: 0.5,
                    events: vec!["Key: Unassigned".into()],
                },
            );
            app.project_settings_state.new_action_name.clear();
        }
    });

    ui.separator();

    ScrollArea::vertical().show(ui, |ui| {
        let mut to_remove_action = None;

        for (action_key, binding) in &mut app.project_settings_state.input_actions {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&binding.name).strong().color(COLOR_ACCENT_BLUE));
                    ui.label("Deadzone:");
                    ui.add(Slider::new(&mut binding.deadzone, 0.0..=1.0).step_by(0.05));

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("🗑 Delete").clicked() {
                            to_remove_action = Some(action_key.clone());
                        }
                        if ui.button("➕ Add Event").clicked() {
                            binding.events.push("Key: Space / Button".into());
                        }
                    });
                });

                ui.indent("binding_events", |ui| {
                    let mut remove_event_idx = None;
                    for (idx, event) in binding.events.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            ui.colored_label(COLOR_SUCCESS_GREEN, "🎮");
                            ui.text_edit_singleline(event);
                            if ui.small_button("✖").clicked() {
                                remove_event_idx = Some(idx);
                            }
                        });
                    }
                    if let Some(idx) = remove_event_idx {
                        binding.events.remove(idx);
                    }
                });
            });
            ui.add_space(2.0);
        }

        if let Some(action) = to_remove_action {
            app.project_settings_state.input_actions.remove(&action);
        }
    });
}
