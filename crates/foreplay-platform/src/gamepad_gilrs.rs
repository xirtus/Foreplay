//! Hardware gamepad polling hub wrapping `gilrs 0.11.2`, `winit 0.30.13` event translators, and dual-motor rumble queue.
//!
//! Provides `GilrsGamepadBridge::try_new_or_virtual` which automatically connects to host
//! gamepad APIs or transparently enters virtual fallback mode for headless CI/tests.

use crate::input_map::{GamepadAxisId, GamepadButtonId, InputEvent, InputServer, MouseButtonId};
use crate::KeyCode;
use gilrs::{Axis, Button, EventType, Gilrs};

/// Dual-motor force-feedback vibration request.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct RumbleEffect {
    /// Zero-based target gamepad identifier.
    pub pad: u32,
    /// High-frequency weak motor intensity (`0.0..=1.0`).
    pub weak_magnitude: f32,
    /// Low-frequency strong motor intensity (`0.0..=1.0`).
    pub strong_magnitude: f32,
    /// Vibration duration in milliseconds.
    pub duration_ms: u32,
}

impl RumbleEffect {
    /// Create a new rumble effect.
    #[must_use]
    pub fn new(pad: u32, weak_magnitude: f32, strong_magnitude: f32, duration_ms: u32) -> Self {
        Self {
            pad,
            weak_magnitude: weak_magnitude.clamp(0.0, 1.0),
            strong_magnitude: strong_magnitude.clamp(0.0, 1.0),
            duration_ms,
        }
    }
}

/// Hardware gamepad polling hub with live `gilrs` support and headless virtual simulation.
pub struct GilrsGamepadBridge {
    gilrs: Option<Gilrs>,
    virtual_events: Vec<InputEvent>,
    rumble_queue: Vec<RumbleEffect>,
}

impl std::fmt::Debug for GilrsGamepadBridge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GilrsGamepadBridge")
            .field("is_hardware", &self.gilrs.is_some())
            .field("virtual_events_queued", &self.virtual_events.len())
            .field("pending_rumbles", &self.rumble_queue.len())
            .finish()
    }
}

impl GilrsGamepadBridge {
    /// Initialize gamepad bridge with hardware `gilrs` or virtual fallback.
    ///
    /// Never panics or fails if OS gamepad subsystem is absent or unpermitted.
    #[must_use]
    pub fn try_new_or_virtual() -> Self {
        let gilrs = Gilrs::new().ok();
        Self {
            gilrs,
            virtual_events: Vec::new(),
            rumble_queue: Vec::new(),
        }
    }

    /// Explicitly create a virtual gamepad bridge for headless unit testing.
    #[must_use]
    pub fn new_virtual() -> Self {
        Self {
            gilrs: None,
            virtual_events: Vec::new(),
            rumble_queue: Vec::new(),
        }
    }

    /// Whether connected to live hardware gamepad subsystem.
    #[must_use]
    pub fn is_hardware(&self) -> bool {
        self.gilrs.is_some()
    }

    /// Whether running in headless virtual mode.
    #[must_use]
    pub fn is_virtual(&self) -> bool {
        self.gilrs.is_none()
    }

    /// Queue a dual-motor vibration rumble command.
    pub fn queue_rumble(&mut self, effect: RumbleEffect) {
        self.rumble_queue.push(effect);
    }

    /// Pop next pending rumble effect from queue.
    pub fn pop_rumble(&mut self) -> Option<RumbleEffect> {
        if self.rumble_queue.is_empty() {
            None
        } else {
            Some(self.rumble_queue.remove(0))
        }
    }

    /// Count of pending rumble effects in queue.
    #[must_use]
    pub fn pending_rumble_count(&self) -> usize {
        self.rumble_queue.len()
    }

    /// Clear all pending rumble commands.
    pub fn clear_rumble(&mut self) {
        self.rumble_queue.clear();
    }

    /// Inject a virtual gamepad event (used in unit tests and automated playback).
    pub fn inject_virtual_event(&mut self, event: InputEvent) {
        self.virtual_events.push(event);
    }

    /// Poll all pending gamepad events from hardware or virtual queue and feed into `InputServer`.
    pub fn poll_events(&mut self, server: &mut InputServer) {
        // 1. Drain hardware gilrs events if available
        if let Some(gilrs) = &mut self.gilrs {
            while let Some(event) = gilrs.next_event() {
                let pad_id = usize::from(event.id) as u32;

                match event.event {
                    EventType::ButtonPressed(btn, _) => {
                        let button_id = map_gilrs_button(btn);
                        server.feed_event(InputEvent::GamepadButton {
                            pad: pad_id,
                            button: button_id,
                            pressed: true,
                            strength: 1.0,
                        });
                    }
                    EventType::ButtonReleased(btn, _) => {
                        let button_id = map_gilrs_button(btn);
                        server.feed_event(InputEvent::GamepadButton {
                            pad: pad_id,
                            button: button_id,
                            pressed: false,
                            strength: 0.0,
                        });
                    }
                    EventType::ButtonChanged(btn, val, _) => {
                        let button_id = map_gilrs_button(btn);
                        let pressed = val > 0.05;
                        server.feed_event(InputEvent::GamepadButton {
                            pad: pad_id,
                            button: button_id,
                            pressed,
                            strength: val,
                        });
                    }
                    EventType::AxisChanged(axis, val, _) => {
                        let axis_id = map_gilrs_axis(axis);
                        server.feed_event(InputEvent::GamepadAxis {
                            pad: pad_id,
                            axis: axis_id,
                            value: val,
                        });
                    }
                    _ => {}
                }
            }
        }

        // 2. Drain virtual events
        let queued = std::mem::take(&mut self.virtual_events);
        for ev in queued {
            server.feed_event(ev);
        }
    }

    /// Advance server frame and poll gamepad events in a single step.
    pub fn begin_frame_and_poll(&mut self, server: &mut InputServer) {
        server.begin_frame();
        self.poll_events(server);
    }
}

/// Map `gilrs::Button` to engine `GamepadButtonId`.
#[must_use]
pub fn map_gilrs_button(button: Button) -> GamepadButtonId {
    match button {
        Button::South => GamepadButtonId::South,
        Button::East => GamepadButtonId::East,
        Button::North => GamepadButtonId::North,
        Button::West => GamepadButtonId::West,
        Button::C => GamepadButtonId::C,
        Button::Z => GamepadButtonId::Z,
        Button::LeftTrigger => GamepadButtonId::LeftTrigger,
        Button::LeftTrigger2 => GamepadButtonId::LeftTrigger2,
        Button::RightTrigger => GamepadButtonId::RightTrigger,
        Button::RightTrigger2 => GamepadButtonId::RightTrigger2,
        Button::Select => GamepadButtonId::Select,
        Button::Start => GamepadButtonId::Start,
        Button::Mode => GamepadButtonId::Mode,
        Button::LeftThumb => GamepadButtonId::LeftThumb,
        Button::RightThumb => GamepadButtonId::RightThumb,
        Button::DPadUp => GamepadButtonId::DPadUp,
        Button::DPadDown => GamepadButtonId::DPadDown,
        Button::DPadLeft => GamepadButtonId::DPadLeft,
        Button::DPadRight => GamepadButtonId::DPadRight,
        Button::Unknown => GamepadButtonId::Unknown(0),
    }
}

/// Map `gilrs::Axis` to engine `GamepadAxisId`.
#[must_use]
pub fn map_gilrs_axis(axis: Axis) -> GamepadAxisId {
    match axis {
        Axis::LeftStickX => GamepadAxisId::LeftStickX,
        Axis::LeftStickY => GamepadAxisId::LeftStickY,
        Axis::LeftZ => GamepadAxisId::LeftZ,
        Axis::RightStickX => GamepadAxisId::RightStickX,
        Axis::RightStickY => GamepadAxisId::RightStickY,
        Axis::RightZ => GamepadAxisId::RightZ,
        Axis::DPadX => GamepadAxisId::DPadX,
        Axis::DPadY => GamepadAxisId::DPadY,
        Axis::Unknown => GamepadAxisId::Unknown(0),
    }
}

/// Convert a `winit 0.30` keyboard `KeyCode` to engine `KeyCode`.
#[must_use]
pub fn from_winit_keycode(key: winit::keyboard::KeyCode) -> Option<KeyCode> {
    use winit::keyboard::KeyCode as W;
    match key {
        W::KeyA => Some(KeyCode::KeyA),
        W::KeyB => Some(KeyCode::KeyB),
        W::KeyC => Some(KeyCode::KeyC),
        W::KeyD => Some(KeyCode::KeyD),
        W::KeyE => Some(KeyCode::KeyE),
        W::KeyF => Some(KeyCode::KeyF),
        W::KeyG => Some(KeyCode::KeyG),
        W::KeyH => Some(KeyCode::KeyH),
        W::KeyI => Some(KeyCode::KeyI),
        W::KeyJ => Some(KeyCode::KeyJ),
        W::KeyK => Some(KeyCode::KeyK),
        W::KeyL => Some(KeyCode::KeyL),
        W::KeyM => Some(KeyCode::KeyM),
        W::KeyN => Some(KeyCode::KeyN),
        W::KeyO => Some(KeyCode::KeyO),
        W::KeyP => Some(KeyCode::KeyP),
        W::KeyQ => Some(KeyCode::KeyQ),
        W::KeyR => Some(KeyCode::KeyR),
        W::KeyS => Some(KeyCode::KeyS),
        W::KeyT => Some(KeyCode::KeyT),
        W::KeyU => Some(KeyCode::KeyU),
        W::KeyV => Some(KeyCode::KeyV),
        W::KeyW => Some(KeyCode::KeyW),
        W::KeyX => Some(KeyCode::KeyX),
        W::KeyY => Some(KeyCode::KeyY),
        W::KeyZ => Some(KeyCode::KeyZ),
        W::Digit0 => Some(KeyCode::Num0),
        W::Digit1 => Some(KeyCode::Num1),
        W::Digit2 => Some(KeyCode::Num2),
        W::Digit3 => Some(KeyCode::Num3),
        W::Digit4 => Some(KeyCode::Num4),
        W::Digit5 => Some(KeyCode::Num5),
        W::Digit6 => Some(KeyCode::Num6),
        W::Digit7 => Some(KeyCode::Num7),
        W::Digit8 => Some(KeyCode::Num8),
        W::Digit9 => Some(KeyCode::Num9),
        W::Space => Some(KeyCode::Space),
        W::Enter => Some(KeyCode::Enter),
        W::Escape => Some(KeyCode::Escape),
        W::Tab => Some(KeyCode::Tab),
        W::Backspace => Some(KeyCode::Backspace),
        W::ShiftLeft => Some(KeyCode::ShiftLeft),
        W::ShiftRight => Some(KeyCode::ShiftRight),
        W::ControlLeft => Some(KeyCode::ControlLeft),
        W::ControlRight => Some(KeyCode::ControlRight),
        W::AltLeft => Some(KeyCode::AltLeft),
        W::AltRight => Some(KeyCode::AltRight),
        W::ArrowUp => Some(KeyCode::ArrowUp),
        W::ArrowDown => Some(KeyCode::ArrowDown),
        W::ArrowLeft => Some(KeyCode::ArrowLeft),
        W::ArrowRight => Some(KeyCode::ArrowRight),
        _ => None,
    }
}

/// Convert a `winit 0.30` `MouseButton` to engine `MouseButtonId`.
#[must_use]
pub fn from_winit_mouse_button(button: winit::event::MouseButton) -> MouseButtonId {
    match button {
        winit::event::MouseButton::Left => MouseButtonId::Left,
        winit::event::MouseButton::Right => MouseButtonId::Right,
        winit::event::MouseButton::Middle => MouseButtonId::Middle,
        winit::event::MouseButton::Back => MouseButtonId::Other(3),
        winit::event::MouseButton::Forward => MouseButtonId::Other(4),
        winit::event::MouseButton::Other(code) => MouseButtonId::Other(code),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gilrs_bridge_virtual_mode() {
        let mut bridge = GilrsGamepadBridge::new_virtual();
        assert!(bridge.is_virtual());
        assert!(!bridge.is_hardware());

        let mut server = InputServer::default();
        bridge.poll_events(&mut server);
    }

    #[test]
    fn test_gilrs_bridge_try_new_or_virtual() {
        let bridge = GilrsGamepadBridge::try_new_or_virtual();
        // Regardless of OS hardware availability, must not panic and must be a valid bridge
        assert!(bridge.is_virtual() || bridge.is_hardware());
    }

    #[test]
    fn test_gilrs_bridge_inject_virtual_button_and_axis() {
        let mut bridge = GilrsGamepadBridge::new_virtual();
        let mut server = InputServer::default(); // has ui_accept mapped to GamepadButton South

        bridge.inject_virtual_event(InputEvent::GamepadButton {
            pad: 0,
            button: GamepadButtonId::South,
            pressed: true,
            strength: 1.0,
        });

        bridge.poll_events(&mut server);
        assert!(server.is_action_pressed("ui_accept"));
        assert!(server.is_action_just_pressed("ui_accept"));

        // Release
        bridge.inject_virtual_event(InputEvent::GamepadButton {
            pad: 0,
            button: GamepadButtonId::South,
            pressed: false,
            strength: 0.0,
        });
        bridge.begin_frame_and_poll(&mut server);
        assert!(!server.is_action_pressed("ui_accept"));
        assert!(server.is_action_just_released("ui_accept"));
    }

    #[test]
    fn test_gilrs_bridge_rumble_queue() {
        let mut bridge = GilrsGamepadBridge::new_virtual();
        assert_eq!(bridge.pending_rumble_count(), 0);

        let effect = RumbleEffect::new(0, 0.75, 0.5, 300);
        bridge.queue_rumble(effect);
        assert_eq!(bridge.pending_rumble_count(), 1);

        let popped = bridge.pop_rumble().expect("rumble effect popped");
        assert_eq!(popped.pad, 0);
        assert_eq!(popped.weak_magnitude, 0.75);
        assert_eq!(popped.strong_magnitude, 0.5);
        assert_eq!(popped.duration_ms, 300);
        assert_eq!(bridge.pending_rumble_count(), 0);

        bridge.queue_rumble(effect);
        bridge.clear_rumble();
        assert_eq!(bridge.pending_rumble_count(), 0);
    }

    #[test]
    fn test_from_winit_keycode_conversion() {
        assert_eq!(from_winit_keycode(winit::keyboard::KeyCode::KeyW), Some(KeyCode::KeyW));
        assert_eq!(from_winit_keycode(winit::keyboard::KeyCode::KeyA), Some(KeyCode::KeyA));
        assert_eq!(from_winit_keycode(winit::keyboard::KeyCode::KeyS), Some(KeyCode::KeyS));
        assert_eq!(from_winit_keycode(winit::keyboard::KeyCode::KeyD), Some(KeyCode::KeyD));
        assert_eq!(from_winit_keycode(winit::keyboard::KeyCode::Space), Some(KeyCode::Space));
        assert_eq!(from_winit_keycode(winit::keyboard::KeyCode::Enter), Some(KeyCode::Enter));
        assert_eq!(from_winit_keycode(winit::keyboard::KeyCode::Escape), Some(KeyCode::Escape));
        assert_eq!(from_winit_keycode(winit::keyboard::KeyCode::Tab), Some(KeyCode::Tab));
        assert_eq!(from_winit_keycode(winit::keyboard::KeyCode::Digit5), Some(KeyCode::Num5));
        assert_eq!(from_winit_keycode(winit::keyboard::KeyCode::ArrowUp), Some(KeyCode::ArrowUp));
    }

    #[test]
    fn test_from_winit_mouse_button_conversion() {
        assert_eq!(from_winit_mouse_button(winit::event::MouseButton::Left), MouseButtonId::Left);
        assert_eq!(from_winit_mouse_button(winit::event::MouseButton::Right), MouseButtonId::Right);
        assert_eq!(from_winit_mouse_button(winit::event::MouseButton::Middle), MouseButtonId::Middle);
        assert_eq!(from_winit_mouse_button(winit::event::MouseButton::Other(99)), MouseButtonId::Other(99));
    }

    #[test]
    fn test_map_gilrs_button_and_axis() {
        assert_eq!(map_gilrs_button(Button::South), GamepadButtonId::South);
        assert_eq!(map_gilrs_button(Button::East), GamepadButtonId::East);
        assert_eq!(map_gilrs_button(Button::North), GamepadButtonId::North);
        assert_eq!(map_gilrs_button(Button::West), GamepadButtonId::West);
        assert_eq!(map_gilrs_button(Button::DPadUp), GamepadButtonId::DPadUp);
        assert_eq!(map_gilrs_button(Button::LeftTrigger2), GamepadButtonId::LeftTrigger2);

        assert_eq!(map_gilrs_axis(Axis::LeftStickX), GamepadAxisId::LeftStickX);
        assert_eq!(map_gilrs_axis(Axis::LeftStickY), GamepadAxisId::LeftStickY);
        assert_eq!(map_gilrs_axis(Axis::RightStickX), GamepadAxisId::RightStickX);
    }
}
