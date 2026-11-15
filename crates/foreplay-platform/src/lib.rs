//! Host abstraction isolating OS/windowing specifics so the exact same `foreplay-editor`
//! and runtime crates ship on Linux, Steam Deck, macOS, Windows, Web, and SexOS (`sexlife` suite).

/// Target execution host detected or configured at compile/boot time.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HostTarget {
    LinuxDesktop,
    SteamDeck,
    MacOsAppleSilicon,
    WindowsDesktop,
    WebGpuBrowser,
    /// Native SexOS single-address-space userspace (`Silk` desktop shell + SexOS `egui` backend).
    SexOsSilkUserspace,
}

/// Controller stick calibration (`skate2-rust-port` RPCS3-grade parity).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct StickCalibration {
    pub multiplier: f32,
    pub deadzone: f32,
    pub anti_deadzone: f32,
}

impl Default for StickCalibration {
    fn default() -> Self {
        Self {
            multiplier: 1.0,
            deadzone: 0.12,
            anti_deadzone: 0.0,
        }
    }
}
