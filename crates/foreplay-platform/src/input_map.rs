//! Complete Godot 4 `InputMap` and frame-stateful `InputServer`.
//!
//! Mirrors Godot 4 `InputMap` action mapping, trigger binding, and `InputServer`
//! frame lifecycle (`begin_frame`, `feed_event`, `is_action_pressed`, `is_action_just_pressed`,
//! `is_action_just_released`, `get_action_strength`, `get_axis`, `get_vector`).

use crate::KeyCode;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Type alias for platform key code identifier.
pub type KeyCodeId = KeyCode;

/// Mouse button identifiers.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub enum MouseButtonId {
    Left,
    Right,
    Middle,
    Other(u16),
}

/// Standard gamepad physical button identifiers.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub enum GamepadButtonId {
    South, // Xbox A / PS Cross / Nintendo B
    East,  // Xbox B / PS Circle / Nintendo A
    North, // Xbox Y / PS Triangle / Nintendo X
    West,  // Xbox X / PS Square / Nintendo Y
    C,
    Z,
    LeftTrigger,
    LeftTrigger2,
    RightTrigger,
    RightTrigger2,
    Select,
    Start,
    Mode,
    LeftThumb,
    RightThumb,
    DPadUp,
    DPadDown,
    DPadLeft,
    DPadRight,
    Unknown(u32),
}

/// Gamepad analog stick and trigger axis identifiers.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub enum GamepadAxisId {
    LeftStickX,
    LeftStickY,
    LeftZ,
    RightStickX,
    RightStickY,
    RightZ,
    DPadX,
    DPadY,
    Unknown(u32),
}

/// Input trigger binding mapping hardware inputs to abstract actions.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub enum InputTrigger {
    /// Keyboard key with optional modifier requirements.
    Key {
        key: KeyCodeId,
        ctrl: bool,
        shift: bool,
        alt: bool,
        meta: bool,
    },
    /// Mouse button click.
    MouseButton(MouseButtonId),
    /// Gamepad digital or analog button.
    GamepadButton {
        pad: Option<u32>,
        button: GamepadButtonId,
    },
    /// Gamepad axis deflection in positive or negative direction.
    GamepadAxis {
        pad: Option<u32>,
        axis: GamepadAxisId,
        direction: f32,
    },
}

impl InputTrigger {
    /// Create a simple key trigger without modifier requirements.
    #[must_use]
    pub const fn key(key: KeyCodeId) -> Self {
        Self::Key {
            key,
            ctrl: false,
            shift: false,
            alt: false,
            meta: false,
        }
    }

    /// Create a key trigger with specific modifier requirements.
    #[must_use]
    pub const fn key_with_modifiers(
        key: KeyCodeId,
        ctrl: bool,
        shift: bool,
        alt: bool,
        meta: bool,
    ) -> Self {
        Self::Key {
            key,
            ctrl,
            shift,
            alt,
            meta,
        }
    }

    /// Create a mouse button trigger.
    #[must_use]
    pub const fn mouse(button: MouseButtonId) -> Self {
        Self::MouseButton(button)
    }

    /// Create a gamepad button trigger for any pad or a specific pad.
    #[must_use]
    pub const fn gamepad_button(pad: Option<u32>, button: GamepadButtonId) -> Self {
        Self::GamepadButton { pad, button }
    }

    /// Create a gamepad axis directional trigger (direction typically +1.0 or -1.0).
    #[must_use]
    pub const fn gamepad_axis(pad: Option<u32>, axis: GamepadAxisId, direction: f32) -> Self {
        Self::GamepadAxis {
            pad,
            axis,
            direction,
        }
    }
}

/// Single input action configuration in `InputMap`.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct ActionConfig {
    /// Action analog deadzone (default 0.5).
    pub deadzone: f32,
    /// List of physical triggers mapped to this action.
    pub triggers: Vec<InputTrigger>,
}

impl Default for ActionConfig {
    fn default() -> Self {
        Self {
            deadzone: 0.5,
            triggers: Vec::new(),
        }
    }
}

/// Central registry mapping abstract action names to physical input triggers (Godot 4 `InputMap`).
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct InputMap {
    actions: BTreeMap<String, ActionConfig>,
}

impl InputMap {
    /// Create an empty input map.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct `InputMap` populated with Godot 4 standard UI actions (`ui_*`).
    #[must_use]
    pub fn with_godot_defaults() -> Self {
        let mut map = Self::new();

        // ui_accept
        map.add_action("ui_accept", 0.5);
        map.action_add_trigger("ui_accept", InputTrigger::key(KeyCode::Space));
        map.action_add_trigger("ui_accept", InputTrigger::key(KeyCode::Enter));
        map.action_add_trigger(
            "ui_accept",
            InputTrigger::gamepad_button(None, GamepadButtonId::South),
        );

        // ui_cancel
        map.add_action("ui_cancel", 0.5);
        map.action_add_trigger("ui_cancel", InputTrigger::key(KeyCode::Escape));
        map.action_add_trigger(
            "ui_cancel",
            InputTrigger::gamepad_button(None, GamepadButtonId::East),
        );

        // ui_left
        map.add_action("ui_left", 0.5);
        map.action_add_trigger("ui_left", InputTrigger::key(KeyCode::ArrowLeft));
        map.action_add_trigger("ui_left", InputTrigger::key(KeyCode::KeyA));
        map.action_add_trigger(
            "ui_left",
            InputTrigger::gamepad_button(None, GamepadButtonId::DPadLeft),
        );
        map.action_add_trigger(
            "ui_left",
            InputTrigger::gamepad_axis(None, GamepadAxisId::LeftStickX, -1.0),
        );

        // ui_right
        map.add_action("ui_right", 0.5);
        map.action_add_trigger("ui_right", InputTrigger::key(KeyCode::ArrowRight));
        map.action_add_trigger("ui_right", InputTrigger::key(KeyCode::KeyD));
        map.action_add_trigger(
            "ui_right",
            InputTrigger::gamepad_button(None, GamepadButtonId::DPadRight),
        );
        map.action_add_trigger(
            "ui_right",
            InputTrigger::gamepad_axis(None, GamepadAxisId::LeftStickX, 1.0),
        );

        // ui_up
        map.add_action("ui_up", 0.5);
        map.action_add_trigger("ui_up", InputTrigger::key(KeyCode::ArrowUp));
        map.action_add_trigger("ui_up", InputTrigger::key(KeyCode::KeyW));
        map.action_add_trigger(
            "ui_up",
            InputTrigger::gamepad_button(None, GamepadButtonId::DPadUp),
        );
        map.action_add_trigger(
            "ui_up",
            InputTrigger::gamepad_axis(None, GamepadAxisId::LeftStickY, -1.0),
        );

        // ui_down
        map.add_action("ui_down", 0.5);
        map.action_add_trigger("ui_down", InputTrigger::key(KeyCode::ArrowDown));
        map.action_add_trigger("ui_down", InputTrigger::key(KeyCode::KeyS));
        map.action_add_trigger(
            "ui_down",
            InputTrigger::gamepad_button(None, GamepadButtonId::DPadDown),
        );
        map.action_add_trigger(
            "ui_down",
            InputTrigger::gamepad_axis(None, GamepadAxisId::LeftStickY, 1.0),
        );

        // ui_focus_next
        map.add_action("ui_focus_next", 0.5);
        map.action_add_trigger("ui_focus_next", InputTrigger::key(KeyCode::Tab));

        map
    }

    /// Add an action if it does not already exist.
    pub fn add_action(&mut self, action: impl Into<String>, deadzone: f32) {
        self.actions
            .entry(action.into())
            .or_insert_with(|| ActionConfig {
                deadzone: deadzone.clamp(0.0, 1.0),
                triggers: Vec::new(),
            });
    }

    /// Check whether an action exists in the map.
    #[must_use]
    pub fn has_action(&self, action: &str) -> bool {
        self.actions.contains_key(action)
    }

    /// Erase an action from the map.
    pub fn erase_action(&mut self, action: &str) -> bool {
        self.actions.remove(action).is_some()
    }

    /// Add an input trigger to an action.
    pub fn action_add_trigger(&mut self, action: &str, trigger: InputTrigger) {
        if let Some(config) = self.actions.get_mut(action) {
            config.triggers.push(trigger);
        }
    }

    /// Get list of triggers registered for an action.
    #[must_use]
    pub fn triggers_for(&self, action: &str) -> &[InputTrigger] {
        self.actions
            .get(action)
            .map_or(&[], |cfg| &cfg.triggers)
    }

    /// Get action deadzone threshold.
    #[must_use]
    pub fn action_get_deadzone(&self, action: &str) -> f32 {
        self.actions.get(action).map_or(0.5, |cfg| cfg.deadzone)
    }

    /// Set action deadzone threshold.
    pub fn action_set_deadzone(&mut self, action: &str, deadzone: f32) {
        if let Some(config) = self.actions.get_mut(action) {
            config.deadzone = deadzone.clamp(0.0, 1.0);
        }
    }

    /// Load bindings from Foreplay IR `InputMapIr`.
    pub fn load_from_ir(&mut self, ir: &foreplay_ir::InputMapIr) {
        for (action, bindings) in &ir.actions {
            self.add_action(action, 0.5);
            for b in bindings {
                match b {
                    foreplay_ir::InputBinding::Key { keycode } => {
                        if let Some(key) = parse_key_str(keycode) {
                            self.action_add_trigger(action, InputTrigger::key(key));
                        }
                    }
                    foreplay_ir::InputBinding::MouseButton { button } => {
                        let btn = match *button {
                            0 => MouseButtonId::Left,
                            1 => MouseButtonId::Right,
                            2 => MouseButtonId::Middle,
                            other => MouseButtonId::Other(other as u16),
                        };
                        self.action_add_trigger(action, InputTrigger::mouse(btn));
                    }
                    foreplay_ir::InputBinding::JoyButton { button } => {
                        let btn = match *button {
                            0 => GamepadButtonId::South,
                            1 => GamepadButtonId::East,
                            2 => GamepadButtonId::West,
                            3 => GamepadButtonId::North,
                            other => GamepadButtonId::Unknown(other),
                        };
                        self.action_add_trigger(action, InputTrigger::gamepad_button(None, btn));
                    }
                    foreplay_ir::InputBinding::JoyAxis {
                        axis,
                        direction,
                        deadzone,
                    } => {
                        self.action_set_deadzone(action, *deadzone);
                        let axis_id = match *axis {
                            0 => GamepadAxisId::LeftStickX,
                            1 => GamepadAxisId::LeftStickY,
                            2 => GamepadAxisId::RightStickX,
                            3 => GamepadAxisId::RightStickY,
                            other => GamepadAxisId::Unknown(other),
                        };
                        self.action_add_trigger(
                            action,
                            InputTrigger::gamepad_axis(None, axis_id, *direction),
                        );
                    }
                }
            }
        }
    }
}

fn parse_key_str(s: &str) -> Option<KeyCodeId> {
    match s {
        "Space" => Some(KeyCode::Space),
        "Enter" => Some(KeyCode::Enter),
        "Escape" => Some(KeyCode::Escape),
        "Tab" => Some(KeyCode::Tab),
        "Backspace" => Some(KeyCode::Backspace),
        "Left" => Some(KeyCode::ArrowLeft),
        "Right" => Some(KeyCode::ArrowRight),
        "Up" => Some(KeyCode::ArrowUp),
        "Down" => Some(KeyCode::ArrowDown),
        "A" => Some(KeyCode::KeyA),
        "B" => Some(KeyCode::KeyB),
        "C" => Some(KeyCode::KeyC),
        "D" => Some(KeyCode::KeyD),
        "E" => Some(KeyCode::KeyE),
        "F" => Some(KeyCode::KeyF),
        "G" => Some(KeyCode::KeyG),
        "H" => Some(KeyCode::KeyH),
        "I" => Some(KeyCode::KeyI),
        "J" => Some(KeyCode::KeyJ),
        "K" => Some(KeyCode::KeyK),
        "L" => Some(KeyCode::KeyL),
        "M" => Some(KeyCode::KeyM),
        "N" => Some(KeyCode::KeyN),
        "O" => Some(KeyCode::KeyO),
        "P" => Some(KeyCode::KeyP),
        "Q" => Some(KeyCode::KeyQ),
        "R" => Some(KeyCode::KeyR),
        "S" => Some(KeyCode::KeyS),
        "T" => Some(KeyCode::KeyT),
        "U" => Some(KeyCode::KeyU),
        "V" => Some(KeyCode::KeyV),
        "W" => Some(KeyCode::KeyW),
        "X" => Some(KeyCode::KeyX),
        "Y" => Some(KeyCode::KeyY),
        "Z" => Some(KeyCode::KeyZ),
        "0" => Some(KeyCode::Num0),
        "1" => Some(KeyCode::Num1),
        "2" => Some(KeyCode::Num2),
        "3" => Some(KeyCode::Num3),
        "4" => Some(KeyCode::Num4),
        "5" => Some(KeyCode::Num5),
        "6" => Some(KeyCode::Num6),
        "7" => Some(KeyCode::Num7),
        "8" => Some(KeyCode::Num8),
        "9" => Some(KeyCode::Num9),
        _ => None,
    }
}

/// Normalized raw hardware input event fed into `InputServer`.
#[derive(Clone, Debug, PartialEq)]
pub enum InputEvent {
    /// Keyboard key event.
    Key {
        key: KeyCodeId,
        pressed: bool,
        ctrl: bool,
        shift: bool,
        alt: bool,
        meta: bool,
    },
    /// Mouse button event.
    MouseButton {
        button: MouseButtonId,
        pressed: bool,
    },
    /// Mouse motion relative and absolute coordinates.
    MouseMotion {
        delta: [f32; 2],
        position: [f32; 2],
    },
    /// Gamepad button event.
    GamepadButton {
        pad: u32,
        button: GamepadButtonId,
        pressed: bool,
        strength: f32,
    },
    /// Gamepad analog axis event.
    GamepadAxis {
        pad: u32,
        axis: GamepadAxisId,
        value: f32, // -1.0 ..= 1.0
    },
}

#[derive(Clone, Copy, PartialEq, Debug, Default)]
struct ActionRuntimeState {
    strength: f32,
    is_pressed: bool,
    just_pressed: bool,
    just_released: bool,
}

/// Central frame-stateful input server managing action transitions and vector computation (Godot 4 `Input`).
#[derive(Clone, Debug)]
pub struct InputServer {
    map: InputMap,
    action_states: BTreeMap<String, ActionRuntimeState>,
    // Raw state caches
    pressed_keys: BTreeSet<KeyCodeId>,
    pressed_mouse_buttons: BTreeSet<MouseButtonId>,
    gamepad_buttons: BTreeMap<(u32, GamepadButtonId), f32>,
    gamepad_axes: BTreeMap<(u32, GamepadAxisId), f32>,
    ctrl_down: bool,
    shift_down: bool,
    alt_down: bool,
    meta_down: bool,
}

impl Default for InputServer {
    fn default() -> Self {
        Self::new(InputMap::with_godot_defaults())
    }
}

impl InputServer {
    /// Initialize input server with the given input map.
    #[must_use]
    pub fn new(map: InputMap) -> Self {
        let mut server = Self {
            map,
            action_states: BTreeMap::new(),
            pressed_keys: BTreeSet::new(),
            pressed_mouse_buttons: BTreeSet::new(),
            gamepad_buttons: BTreeMap::new(),
            gamepad_axes: BTreeMap::new(),
            ctrl_down: false,
            shift_down: false,
            alt_down: false,
            meta_down: false,
        };
        server.recalculate_all_actions();
        server
    }

    /// Access active input map.
    #[must_use]
    pub const fn map(&self) -> &InputMap {
        &self.map
    }

    /// Mutable access to input map.
    pub fn map_mut(&mut self) -> &mut InputMap {
        &mut self.map
    }

    /// Begin a new engine frame.
    ///
    /// Clears single-frame transition flags (`just_pressed` and `just_released`).
    pub fn begin_frame(&mut self) {
        for state in self.action_states.values_mut() {
            state.just_pressed = false;
            state.just_released = false;
        }
    }

    /// Feed a raw hardware input event into the server and update affected actions.
    pub fn feed_event(&mut self, event: InputEvent) {
        match event {
            InputEvent::Key {
                key,
                pressed,
                ctrl,
                shift,
                alt,
                meta,
            } => {
                self.ctrl_down = ctrl;
                self.shift_down = shift;
                self.alt_down = alt;
                self.meta_down = meta;

                if pressed {
                    self.pressed_keys.insert(key);
                } else {
                    self.pressed_keys.remove(&key);
                }
            }
            InputEvent::MouseButton { button, pressed } => {
                if pressed {
                    self.pressed_mouse_buttons.insert(button);
                } else {
                    self.pressed_mouse_buttons.remove(&button);
                }
            }
            InputEvent::MouseMotion { .. } => {}
            InputEvent::GamepadButton {
                pad,
                button,
                pressed,
                strength,
            } => {
                let st = if pressed {
                    if strength > 0.0 {
                        strength.clamp(0.0, 1.0)
                    } else {
                        1.0
                    }
                } else {
                    0.0
                };
                self.gamepad_buttons.insert((pad, button), st);
            }
            InputEvent::GamepadAxis { pad, axis, value } => {
                self.gamepad_axes
                    .insert((pad, axis), value.clamp(-1.0, 1.0));
            }
        }

        self.recalculate_all_actions();
    }

    fn recalculate_all_actions(&mut self) {
        for (action_name, config) in &self.map.actions {
            let mut max_strength = 0.0_f32;

            for trigger in &config.triggers {
                let s = self.evaluate_trigger_strength(trigger);
                if s > max_strength {
                    max_strength = s;
                }
            }

            let effective_strength = if max_strength >= config.deadzone {
                max_strength
            } else {
                0.0
            };

            let is_now_pressed = effective_strength > 0.0;
            let current_entry = self.action_states.entry(action_name.clone()).or_default();

            if is_now_pressed && !current_entry.is_pressed {
                current_entry.just_pressed = true;
            } else if !is_now_pressed && current_entry.is_pressed {
                current_entry.just_released = true;
            }

            current_entry.is_pressed = is_now_pressed;
            current_entry.strength = effective_strength;
        }
    }

    fn evaluate_trigger_strength(&self, trigger: &InputTrigger) -> f32 {
        match trigger {
            InputTrigger::Key {
                key,
                ctrl,
                shift,
                alt,
                meta,
            } => {
                if !self.pressed_keys.contains(key) {
                    return 0.0;
                }
                if *ctrl && !self.ctrl_down {
                    return 0.0;
                }
                if *shift && !self.shift_down {
                    return 0.0;
                }
                if *alt && !self.alt_down {
                    return 0.0;
                }
                if *meta && !self.meta_down {
                    return 0.0;
                }
                1.0
            }
            InputTrigger::MouseButton(button) => {
                if self.pressed_mouse_buttons.contains(button) {
                    1.0
                } else {
                    0.0
                }
            }
            InputTrigger::GamepadButton { pad, button } => {
                if let Some(p) = pad {
                    self.gamepad_buttons.get(&(*p, *button)).copied().unwrap_or(0.0)
                } else {
                    let mut max_s = 0.0_f32;
                    for ((_, b), &s) in &self.gamepad_buttons {
                        if b == button && s > max_s {
                            max_s = s;
                        }
                    }
                    max_s
                }
            }
            InputTrigger::GamepadAxis {
                pad,
                axis,
                direction,
            } => {
                let dir_sign = if *direction >= 0.0 { 1.0 } else { -1.0 };
                let val = if let Some(p) = pad {
                    self.gamepad_axes.get(&(*p, *axis)).copied().unwrap_or(0.0)
                } else {
                    let mut max_val = 0.0_f32;
                    for ((_, a), &v) in &self.gamepad_axes {
                        if a == axis {
                            let projected = v * dir_sign;
                            if projected > max_val {
                                max_val = projected;
                            }
                        }
                    }
                    max_val * dir_sign
                };

                let deflection = val * dir_sign;
                if deflection > 0.0 {
                    deflection.clamp(0.0, 1.0)
                } else {
                    0.0
                }
            }
        }
    }

    /// Returns `true` if the action is currently pressed.
    #[must_use]
    pub fn is_action_pressed(&self, action: &str) -> bool {
        self.action_states
            .get(action)
            .is_some_and(|s| s.is_pressed)
    }

    /// Returns `true` only on the frame step the action was initially pressed.
    #[must_use]
    pub fn is_action_just_pressed(&self, action: &str) -> bool {
        self.action_states
            .get(action)
            .is_some_and(|s| s.just_pressed)
    }

    /// Returns `true` only on the frame step the action was released.
    #[must_use]
    pub fn is_action_just_released(&self, action: &str) -> bool {
        self.action_states
            .get(action)
            .is_some_and(|s| s.just_released)
    }

    /// Returns action strength in range `0.0..=1.0`.
    #[must_use]
    pub fn get_action_strength(&self, action: &str) -> f32 {
        self.action_states.get(action).map_or(0.0, |s| s.strength)
    }

    /// Computes 1D axis deflection (`positive_action_strength - negative_action_strength`).
    #[must_use]
    pub fn get_axis(&self, negative_action: &str, positive_action: &str) -> f32 {
        let neg = self.get_action_strength(negative_action);
        let pos = self.get_action_strength(positive_action);
        pos - neg
    }

    /// Computes a 2D directional vector from 4 actions with radial deadzone clamping (Godot 4 `Input.get_vector`).
    #[must_use]
    pub fn get_vector(
        &self,
        neg_x: &str,
        pos_x: &str,
        neg_y: &str,
        pos_y: &str,
        deadzone: f32,
    ) -> [f32; 2] {
        let x = self.get_axis(neg_x, pos_x);
        let y = self.get_axis(neg_y, pos_y);

        let len = (x * x + y * y).sqrt();
        let dz = deadzone.clamp(0.0, 0.999);

        if len <= dz || len <= 1e-7 {
            return [0.0, 0.0];
        }

        // Remap length past deadzone to [0.0, 1.0] and clamp to unit circle
        let remapped = ((len - dz) / (1.0 - dz)).clamp(0.0, 1.0);
        let inv_len = 1.0 / len;

        [x * inv_len * remapped, y * inv_len * remapped]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_map_godot_defaults() {
        let map = InputMap::with_godot_defaults();
        assert!(map.has_action("ui_accept"));
        assert!(map.has_action("ui_cancel"));
        assert!(map.has_action("ui_left"));
        assert!(map.has_action("ui_right"));
        assert!(map.has_action("ui_up"));
        assert!(map.has_action("ui_down"));
        assert!(map.has_action("ui_focus_next"));

        let accept_triggers = map.triggers_for("ui_accept");
        assert!(accept_triggers.len() >= 3);
        assert_eq!(map.action_get_deadzone("ui_accept"), 0.5);
    }

    #[test]
    fn test_input_map_add_erase_trigger() {
        let mut map = InputMap::new();
        assert!(!map.has_action("custom_jump"));

        map.add_action("custom_jump", 0.3);
        assert!(map.has_action("custom_jump"));
        assert_eq!(map.action_get_deadzone("custom_jump"), 0.3);

        map.action_add_trigger("custom_jump", InputTrigger::key(KeyCode::Space));
        map.action_add_trigger("custom_jump", InputTrigger::gamepad_button(None, GamepadButtonId::South));
        assert_eq!(map.triggers_for("custom_jump").len(), 2);

        map.action_set_deadzone("custom_jump", 0.4);
        assert_eq!(map.action_get_deadzone("custom_jump"), 0.4);

        assert!(map.erase_action("custom_jump"));
        assert!(!map.has_action("custom_jump"));
        assert!(map.triggers_for("custom_jump").is_empty());
    }

    #[test]
    fn test_input_map_load_from_ir() {
        let mut ir = foreplay_ir::InputMapIr::new();
        ir.bind("skate_ollie", foreplay_ir::InputBinding::key("Space"));
        ir.bind("skate_ollie", foreplay_ir::InputBinding::joy_button(0));
        ir.bind("steer", foreplay_ir::InputBinding::joy_axis(0, 1.0, 0.25));

        let mut map = InputMap::new();
        map.load_from_ir(&ir);

        assert!(map.has_action("skate_ollie"));
        assert_eq!(map.triggers_for("skate_ollie").len(), 2);
        assert!(map.has_action("steer"));
        assert_eq!(map.action_get_deadzone("steer"), 0.25);
    }

    #[test]
    fn test_input_server_key_press_and_just_pressed_lifecycle() {
        let map = InputMap::with_godot_defaults();
        let mut server = InputServer::new(map);

        assert!(!server.is_action_pressed("ui_accept"));
        assert!(!server.is_action_just_pressed("ui_accept"));

        // Frame 1: Key down
        server.feed_event(InputEvent::Key {
            key: KeyCode::Space,
            pressed: true,
            ctrl: false,
            shift: false,
            alt: false,
            meta: false,
        });

        assert!(server.is_action_pressed("ui_accept"));
        assert!(server.is_action_just_pressed("ui_accept"));
        assert!(!server.is_action_just_released("ui_accept"));
        assert_eq!(server.get_action_strength("ui_accept"), 1.0);

        // Frame 2: Begin new frame
        server.begin_frame();
        assert!(server.is_action_pressed("ui_accept"));
        assert!(!server.is_action_just_pressed("ui_accept"), "just_pressed must clear on new frame");
        assert!(!server.is_action_just_released("ui_accept"));

        // Frame 3: Key up
        server.feed_event(InputEvent::Key {
            key: KeyCode::Space,
            pressed: false,
            ctrl: false,
            shift: false,
            alt: false,
            meta: false,
        });

        assert!(!server.is_action_pressed("ui_accept"));
        assert!(!server.is_action_just_pressed("ui_accept"));
        assert!(server.is_action_just_released("ui_accept"));

        // Frame 4: Next frame
        server.begin_frame();
        assert!(!server.is_action_pressed("ui_accept"));
        assert!(!server.is_action_just_released("ui_accept"), "just_released must clear on new frame");
    }

    #[test]
    fn test_input_server_analog_axis_deadzone() {
        let map = InputMap::with_godot_defaults(); // ui_right deadzone is 0.5
        let mut server = InputServer::new(map);

        // Deflect stick X to 0.3 (below 0.5 deadzone)
        server.feed_event(InputEvent::GamepadAxis {
            pad: 0,
            axis: GamepadAxisId::LeftStickX,
            value: 0.3,
        });
        assert!(!server.is_action_pressed("ui_right"));
        assert_eq!(server.get_action_strength("ui_right"), 0.0);

        // Deflect stick X to 0.8 (above deadzone)
        server.feed_event(InputEvent::GamepadAxis {
            pad: 0,
            axis: GamepadAxisId::LeftStickX,
            value: 0.8,
        });
        assert!(server.is_action_pressed("ui_right"));
        assert!((server.get_action_strength("ui_right") - 0.8).abs() < 1e-4);
    }

    #[test]
    fn test_input_server_get_axis_and_vector() {
        let map = InputMap::with_godot_defaults();
        let mut server = InputServer::new(map);

        assert_eq!(server.get_axis("ui_left", "ui_right"), 0.0);

        // Press right
        server.feed_event(InputEvent::Key {
            key: KeyCode::KeyD,
            pressed: true,
            ctrl: false,
            shift: false,
            alt: false,
            meta: false,
        });
        assert_eq!(server.get_axis("ui_left", "ui_right"), 1.0);

        // Press left as well -> balance to 0.0
        server.feed_event(InputEvent::Key {
            key: KeyCode::KeyA,
            pressed: true,
            ctrl: false,
            shift: false,
            alt: false,
            meta: false,
        });
        assert_eq!(server.get_axis("ui_left", "ui_right"), 0.0);

        // Release left
        server.feed_event(InputEvent::Key {
            key: KeyCode::KeyA,
            pressed: false,
            ctrl: false,
            shift: false,
            alt: false,
            meta: false,
        });

        // Also press up
        server.feed_event(InputEvent::Key {
            key: KeyCode::KeyW,
            pressed: true,
            ctrl: false,
            shift: false,
            alt: false,
            meta: false,
        });

        // 2D diagonal vector: x=1.0, y=-1.0 (ui_up is neg_y, ui_down is pos_y)
        let vec = server.get_vector("ui_left", "ui_right", "ui_down", "ui_up", 0.1);
        let len = (vec[0] * vec[0] + vec[1] * vec[1]).sqrt();
        assert!((len - 1.0).abs() < 1e-4, "Diagonal vector must be normalized to unit circle length");
        assert!((vec[0] - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-4);
        assert!((vec[1] - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-4);
    }

    #[test]
    fn test_input_server_modifier_keys() {
        let mut map = InputMap::new();
        map.add_action("save", 0.5);
        map.action_add_trigger("save", InputTrigger::key_with_modifiers(KeyCode::KeyS, true, false, false, false));

        let mut server = InputServer::new(map);

        // Press S without Ctrl -> does NOT trigger
        server.feed_event(InputEvent::Key {
            key: KeyCode::KeyS,
            pressed: true,
            ctrl: false,
            shift: false,
            alt: false,
            meta: false,
        });
        assert!(!server.is_action_pressed("save"));

        // Press S with Ctrl -> triggers!
        server.feed_event(InputEvent::Key {
            key: KeyCode::KeyS,
            pressed: true,
            ctrl: true,
            shift: false,
            alt: false,
            meta: false,
        });
        assert!(server.is_action_pressed("save"));
    }

    #[test]
    fn test_input_server_mouse_and_gamepad_buttons() {
        let mut map = InputMap::new();
        map.add_action("fire", 0.5);
        map.action_add_trigger("fire", InputTrigger::mouse(MouseButtonId::Left));
        map.action_add_trigger("fire", InputTrigger::gamepad_button(Some(0), GamepadButtonId::RightTrigger2));

        let mut server = InputServer::new(map);

        // Mouse click
        server.feed_event(InputEvent::MouseButton {
            button: MouseButtonId::Left,
            pressed: true,
        });
        assert!(server.is_action_pressed("fire"));

        server.feed_event(InputEvent::MouseButton {
            button: MouseButtonId::Left,
            pressed: false,
        });
        assert!(!server.is_action_pressed("fire"));

        // Gamepad trigger on pad 0
        server.feed_event(InputEvent::GamepadButton {
            pad: 0,
            button: GamepadButtonId::RightTrigger2,
            pressed: true,
            strength: 0.9,
        });
        assert!(server.is_action_pressed("fire"));
        assert!((server.get_action_strength("fire") - 0.9).abs() < 1e-4);
    }
}
