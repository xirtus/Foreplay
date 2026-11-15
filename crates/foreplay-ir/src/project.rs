//! Foreplay Project Configuration & Input Map IR (`project.godot` / `project.foreplay` parity).

use crate::IrValue;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

/// Physical or virtual input device binding mapped to an abstract action name.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub enum InputBinding {
    Key {
        keycode: String,
    },
    MouseButton {
        button: u32,
    },
    JoyButton {
        button: u32,
    },
    JoyAxis {
        axis: u32,
        direction: f32,
        deadzone: f32,
    },
}

impl InputBinding {
    pub fn key(keycode: impl Into<String>) -> Self {
        Self::Key {
            keycode: keycode.into(),
        }
    }

    pub fn mouse_button(button: u32) -> Self {
        Self::MouseButton { button }
    }

    pub fn joy_button(button: u32) -> Self {
        Self::JoyButton { button }
    }

    pub fn joy_axis(axis: u32, direction: f32, deadzone: f32) -> Self {
        Self::JoyAxis {
            axis,
            direction,
            deadzone,
        }
    }
}

/// Abstract input action map binding named actions (`"ui_accept"`, `"jump"`, `"ollie"`) to physical inputs.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct InputMapIr {
    pub actions: BTreeMap<String, Vec<InputBinding>>,
}

impl InputMapIr {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns standard Godot 4.4 default input actions including UI navigation and skate gameplay actions.
    pub fn default_godot_actions() -> Self {
        let mut map = Self::new();
        map.bind("ui_accept", InputBinding::key("Space"));
        map.bind("ui_accept", InputBinding::key("Enter"));
        map.bind("ui_accept", InputBinding::joy_button(0)); // A button

        map.bind("ui_cancel", InputBinding::key("Escape"));
        map.bind("ui_cancel", InputBinding::joy_button(1)); // B button

        map.bind("ui_left", InputBinding::key("Left"));
        map.bind("ui_left", InputBinding::key("A"));
        map.bind("ui_left", InputBinding::joy_axis(0, -1.0, 0.5));

        map.bind("ui_right", InputBinding::key("Right"));
        map.bind("ui_right", InputBinding::key("D"));
        map.bind("ui_right", InputBinding::joy_axis(0, 1.0, 0.5));

        map.bind("ui_up", InputBinding::key("Up"));
        map.bind("ui_up", InputBinding::key("W"));
        map.bind("ui_up", InputBinding::joy_axis(1, -1.0, 0.5));

        map.bind("ui_down", InputBinding::key("Down"));
        map.bind("ui_down", InputBinding::key("S"));
        map.bind("ui_down", InputBinding::joy_axis(1, 1.0, 0.5));

        // Core Skate / Action Gameplay
        map.bind("jump", InputBinding::key("Space"));
        map.bind("ollie", InputBinding::joy_axis(3, 1.0, 0.3)); // Right stick flick down->up
        map.bind("move_left", InputBinding::key("A"));
        map.bind("move_right", InputBinding::key("D"));

        map
    }

    pub fn bind(&mut self, action: impl Into<String>, binding: InputBinding) {
        self.actions.entry(action.into()).or_default().push(binding);
    }

    pub fn bindings_for(&self, action: &str) -> &[InputBinding] {
        self.actions.get(action).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn remove_action(&mut self, action: &str) -> Option<Vec<InputBinding>> {
        self.actions.remove(action)
    }

    /// Deterministic TOML-formatted serialization for git-diffable project files.
    pub fn to_toml_string(&self) -> String {
        let mut out = String::from("[input]\n");
        for (action, bindings) in &self.actions {
            out.push_str(&format!("{action} = [\n"));
            for b in bindings {
                match b {
                    InputBinding::Key { keycode } => {
                        out.push_str(&format!("  {{ type = \"Key\", keycode = \"{keycode}\" }},\n"));
                    }
                    InputBinding::MouseButton { button } => {
                        out.push_str(&format!("  {{ type = \"MouseButton\", button = {button} }},\n"));
                    }
                    InputBinding::JoyButton { button } => {
                        out.push_str(&format!("  {{ type = \"JoyButton\", button = {button} }},\n"));
                    }
                    InputBinding::JoyAxis {
                        axis,
                        direction,
                        deadzone,
                    } => {
                        out.push_str(&format!(
                            "  {{ type = \"JoyAxis\", axis = {axis}, direction = {direction:.2}, deadzone = {deadzone:.2} }},\n"
                        ));
                    }
                }
            }
            out.push_str("]\n");
        }
        out
    }

    /// Zero-panic parser converting TOML-formatted input configuration back to `InputMapIr`.
    pub fn from_toml_str(input: &str) -> Result<Self, String> {
        let mut map = Self::new();
        let mut current_action: Option<String> = None;
        let mut in_input = !input.contains("[input]");

        for line in input.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            if trimmed.starts_with('[') && trimmed.ends_with(']') && !trimmed.contains('=') {
                in_input = trimmed == "[input]";
                current_action = None;
                continue;
            }

            if !in_input {
                continue;
            }

            if trimmed.starts_with('{') {
                if let Some(ref act) = current_action
                    && let Some(binding) = parse_binding_line(trimmed)
                {
                    map.actions.get_mut(act).unwrap().push(binding);
                }
            } else if let Some((action, rest)) = trimmed.split_once('=') {
                let act_name = action.trim().to_string();
                current_action = Some(act_name.clone());
                map.actions.entry(act_name).or_default();
                if rest.trim() == "[]" {
                    current_action = None;
                }
            } else if trimmed == "]" {
                current_action = None;
            }
        }

        Ok(map)
    }
}

fn parse_binding_line(line: &str) -> Option<InputBinding> {
    let trimmed = line.trim().trim_end_matches(',').trim();
    let clean = trimmed.trim_start_matches('{').trim_end_matches('}').trim();
    let mut type_name = "";
    let mut keycode = "";
    let mut button: u32 = 0;
    let mut axis: u32 = 0;
    let mut direction: f32 = 1.0;
    let mut deadzone: f32 = 0.5;

    for part in clean.split(',') {
        if let Some((k, v)) = part.split_once('=') {
            let k = k.trim();
            let v = v.trim().trim_matches('"');
            match k {
                "type" => type_name = v,
                "keycode" => keycode = v,
                "button" => button = v.parse().unwrap_or(0),
                "axis" => axis = v.parse().unwrap_or(0),
                "direction" => direction = v.parse().unwrap_or(1.0),
                "deadzone" => deadzone = v.parse().unwrap_or(0.5),
                _ => {}
            }
        }
    }

    match type_name {
        "Key" => Some(InputBinding::Key {
            keycode: keycode.to_string(),
        }),
        "MouseButton" => Some(InputBinding::MouseButton { button }),
        "JoyButton" => Some(InputBinding::JoyButton { button }),
        "JoyAxis" => Some(InputBinding::JoyAxis {
            axis,
            direction,
            deadzone,
        }),
        _ => None,
    }
}

/// Root Project Configuration (`project.foreplay` / `project.godot`).
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct ProjectSettingsIr {
    pub name: String,
    pub main_scene: String,
    pub physics_ticks_per_second: u32,
    pub renderer: String,
    pub window_width: u32,
    pub window_height: u32,
    pub input_map: InputMapIr,
    pub custom_settings: BTreeMap<String, IrValue>,
}

impl ProjectSettingsIr {
    pub fn default_project(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            main_scene: "res://main.fscn".into(),
            physics_ticks_per_second: 60,
            renderer: "Forward+".into(),
            window_width: 1920,
            window_height: 1080,
            input_map: InputMapIr::default_godot_actions(),
            custom_settings: BTreeMap::new(),
        }
    }

    /// Serializes project settings to a deterministic TOML document.
    pub fn to_toml_string(&self) -> String {
        let mut out = format!(
            "[application]\nname = \"{}\"\nmain_scene = \"{}\"\n\n",
            self.name, self.main_scene
        );
        out.push_str(&format!(
            "[physics]\nphysics_ticks_per_second = {}\n\n",
            self.physics_ticks_per_second
        ));
        out.push_str(&format!(
            "[display]\nwindow_width = {}\nwindow_height = {}\nrenderer = \"{}\"\n\n",
            self.window_width, self.window_height, self.renderer
        ));
        out.push_str(&self.input_map.to_toml_string());
        out
    }

    /// Parses project settings from a TOML document.
    pub fn from_toml_str(input: &str) -> Result<Self, String> {
        let mut name = "ForeplayProject".to_string();
        let mut main_scene = "res://main.fscn".to_string();
        let mut physics_ticks_per_second = 60;
        let mut renderer = "Forward+".to_string();
        let mut window_width = 1920;
        let mut window_height = 1080;

        for line in input.lines() {
            let trimmed = line.trim();
            if let Some((k, v)) = trimmed.split_once('=') {
                let k = k.trim();
                let v = v.trim().trim_matches('"');
                match k {
                    "name" => name = v.to_string(),
                    "main_scene" => main_scene = v.to_string(),
                    "physics_ticks_per_second" => {
                        physics_ticks_per_second = v.parse().unwrap_or(60);
                    }
                    "renderer" => renderer = v.to_string(),
                    "window_width" => window_width = v.parse().unwrap_or(1920),
                    "window_height" => window_height = v.parse().unwrap_or(1080),
                    _ => {}
                }
            }
        }

        let input_map = InputMapIr::from_toml_str(input)?;

        Ok(Self {
            name,
            main_scene,
            physics_ticks_per_second,
            renderer,
            window_width,
            window_height,
            input_map,
            custom_settings: BTreeMap::new(),
        })
    }
}

impl fmt::Display for ProjectSettingsIr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ProjectSettings({})", self.name)
    }
}
