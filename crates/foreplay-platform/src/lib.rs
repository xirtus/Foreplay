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

impl HostTarget {
    /// Detect the target host environment based on compile-time target attributes.
    #[must_use]
    pub const fn detect_current() -> Self {
        #[cfg(target_os = "macos")]
        {
            Self::MacOsAppleSilicon
        }
        #[cfg(target_os = "windows")]
        {
            Self::WindowsDesktop
        }
        #[cfg(target_arch = "wasm32")]
        {
            Self::WebGpuBrowser
        }
        #[cfg(all(target_os = "linux", not(feature = "sexos-userspace")))]
        {
            Self::LinuxDesktop
        }
        #[cfg(feature = "sexos-userspace")]
        {
            Self::SexOsSilkUserspace
        }
        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_arch = "wasm32",
            target_os = "linux",
            feature = "sexos-userspace"
        )))]
        {
            Self::LinuxDesktop
        }
    }

    /// Whether this target runs natively inside SexOS userspace.
    #[must_use]
    pub const fn is_sexos(&self) -> bool {
        matches!(self, Self::SexOsSilkUserspace)
    }

    /// Whether this host is a handheld console or embedded device (e.g. Steam Deck).
    #[must_use]
    pub const fn is_handheld(&self) -> bool {
        matches!(self, Self::SteamDeck)
    }
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

impl StickCalibration {
    /// Apply radial deadzone, anti-deadzone remapping, sensitivity multiplier scaling,
    /// and unit-circle clamping.
    ///
    /// Matches the RPCS3 / `skate2-rust-port` stick deadzone transfer function.
    /// Safely handles zero vectors, subnormal values, and NaN/infinity floats without panicking.
    #[must_use]
    pub fn apply_radial(&self, raw_xy: [f32; 2]) -> [f32; 2] {
        let x = raw_xy[0];
        let y = raw_xy[1];

        // Reject non-finite values safely
        if !x.is_finite() || !y.is_finite() {
            return [0.0, 0.0];
        }

        let radius = (x * x + y * y).sqrt();
        if radius <= 1e-7 {
            return [0.0, 0.0];
        }

        let deadzone = self.deadzone.clamp(0.0, 0.9999);
        if radius <= deadzone {
            return [0.0, 0.0];
        }

        // Remap magnitude past deadzone into normalized [0.0, 1.0] range
        let normalized = ((radius - deadzone) / (1.0 - deadzone)).clamp(0.0, 1.0);

        // Anti-deadzone offset jumps from 0 to anti_deadzone immediately past the deadzone threshold
        let anti_deadzone = self.anti_deadzone.clamp(0.0, 0.9999);
        let remapped_radius = anti_deadzone + (1.0 - anti_deadzone) * normalized;

        // Apply multiplier and clamp to unit circle
        let scaled_radius = (remapped_radius * self.multiplier.max(0.0)).clamp(0.0, 1.0);

        let inv_r = 1.0 / radius;
        let unit_x = x * inv_r;
        let unit_y = y * inv_r;

        [unit_x * scaled_radius, unit_y * scaled_radius]
    }
}

/// Target frame-rate limiter behavior.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FrameRateCap {
    /// Uncapped render rate.
    Uncapped,
    /// Fixed target frames per second (e.g. 30, 60, 120, 144).
    Fixed(u32),
    /// Lock to monitor refresh rate (typically via presentation timing / VSync).
    MonitorRefresh,
}

/// High-level display and presentation settings.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct DisplaySettings {
    /// Target frame-rate cap mode.
    pub frame_rate_cap: FrameRateCap,
    /// Whether vertical synchronization is enabled.
    pub vsync: bool,
    /// Resolution scale factor in the range `0.5..=2.0`.
    /// 3D gameplay viewports render scaled, while UI / HUD remains at native screen resolution.
    resolution_scale: f32,
    /// Borderless fullscreen presentation mode.
    pub borderless_fullscreen: bool,
}

impl Default for DisplaySettings {
    fn default() -> Self {
        Self {
            frame_rate_cap: FrameRateCap::MonitorRefresh,
            vsync: true,
            resolution_scale: 1.0,
            borderless_fullscreen: false,
        }
    }
}

impl DisplaySettings {
    /// Create new display settings with default parameters.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Current resolution scale factor (guaranteed in `0.5..=2.0`).
    #[must_use]
    pub const fn resolution_scale(&self) -> f32 {
        self.resolution_scale
    }

    /// Set resolution scale clamped to `0.5..=2.0`.
    pub fn set_resolution_scale(&mut self, scale: f32) {
        if scale.is_finite() {
            self.resolution_scale = scale.clamp(0.5, 2.0);
        }
    }

    /// Calculate scaled 3D viewport dimensions based on native display dimensions.
    #[must_use]
    pub fn scaled_viewport_size(&self, native_width: u32, native_height: u32) -> (u32, u32) {
        let scaled_w = ((native_width as f32) * self.resolution_scale)
            .round()
            .max(1.0) as u32;
        let scaled_h = ((native_height as f32) * self.resolution_scale)
            .round()
            .max(1.0) as u32;
        (scaled_w, scaled_h)
    }
}

/// Bounding dirty rectangle for partial scanout damage tracking.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct DirtyRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl DirtyRect {
    /// Create a new dirty rectangle.
    #[must_use]
    pub const fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Merge two rectangles into their minimal bounding union.
    #[must_use]
    pub fn union(&self, other: &Self) -> Self {
        let x1 = self.x.min(other.x);
        let y1 = self.y.min(other.y);
        let x2 = (self.x + self.width).max(other.x + other.width);
        let y2 = (self.y + self.height).max(other.y + other.height);
        Self {
            x: x1,
            y: y1,
            width: x2.saturating_sub(x1),
            height: y2.saturating_sub(y1),
        }
    }
}

/// Silk desktop window resize handshake tracking.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct SilkResizeHandshake {
    pub pending_width: u32,
    pub pending_height: u32,
    pub request_generation: u64,
    pub acked_generation: u64,
}

impl SilkResizeHandshake {
    /// Check whether a window resize request from Silk is pending scanout surface reallocation.
    #[must_use]
    pub const fn is_pending(&self) -> bool {
        self.request_generation > self.acked_generation
    }
}

/// RGBA8 software scanout framebuffer with pitch, dirty-rect tracking,
/// and Silk window resize handshake metadata.
///
/// Serves as the zero-dependency display target for SexOS userspace (`Silk` desktop compositor)
/// and headless CI test rasterization.
#[derive(Clone, Debug)]
pub struct SexOsScanoutSurface {
    width: u32,
    height: u32,
    pitch: usize,
    pixels: Vec<u8>,
    dirty_rect: Option<DirtyRect>,
    handshake: SilkResizeHandshake,
}

impl SexOsScanoutSurface {
    /// Create a new scanout surface of given width and height (RGBA8: 4 bytes per pixel).
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        let w = width.max(1);
        let h = height.max(1);
        let pitch = (w as usize) * 4;
        let total_bytes = pitch * (h as usize);
        Self {
            width: w,
            height: h,
            pitch,
            pixels: vec![0; total_bytes],
            dirty_rect: Some(DirtyRect::new(0, 0, w, h)),
            handshake: SilkResizeHandshake::default(),
        }
    }

    /// Surface width in pixels.
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Surface height in pixels.
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Row pitch in bytes (typically `width * 4`).
    #[must_use]
    pub const fn pitch(&self) -> usize {
        self.pitch
    }

    /// Read-only access to raw RGBA8 pixel bytes.
    #[must_use]
    pub fn buffer(&self) -> &[u8] {
        &self.pixels
    }

    /// Mutable access to raw RGBA8 pixel bytes.
    pub fn buffer_mut(&mut self) -> &mut [u8] {
        &mut self.pixels
    }

    /// Current dirty bounds if any region has been modified.
    #[must_use]
    pub const fn dirty_rect(&self) -> Option<DirtyRect> {
        self.dirty_rect
    }

    /// True if the surface has damage pending scanout composition.
    #[must_use]
    pub const fn is_dirty(&self) -> bool {
        self.dirty_rect.is_some()
    }

    /// Clear dirty tracking after successful scanout presentation.
    pub fn clear_dirty(&mut self) {
        self.dirty_rect = None;
    }

    /// Mark a damaged rectangular region as dirty.
    pub fn mark_dirty(&mut self, rect: DirtyRect) {
        let clamped = DirtyRect {
            x: rect.x.min(self.width),
            y: rect.y.min(self.height),
            width: rect.width.min(self.width.saturating_sub(rect.x)),
            height: rect.height.min(self.height.saturating_sub(rect.y)),
        };

        if clamped.width == 0 || clamped.height == 0 {
            return;
        }

        self.dirty_rect = Some(match self.dirty_rect {
            Some(existing) => existing.union(&clamped),
            None => clamped,
        });
    }

    /// Mark the entire surface as dirty.
    pub fn mark_all_dirty(&mut self) {
        self.dirty_rect = Some(DirtyRect::new(0, 0, self.width, self.height));
    }

    /// Silk resize handshake state.
    #[must_use]
    pub const fn resize_handshake(&self) -> &SilkResizeHandshake {
        &self.handshake
    }

    /// Request a window resize from the Silk compositor.
    pub fn request_silk_resize(&mut self, new_width: u32, new_height: u32) {
        self.handshake.pending_width = new_width.max(1);
        self.handshake.pending_height = new_height.max(1);
        self.handshake.request_generation += 1;
    }

    /// Acknowledge the pending resize request, reallocating the framebuffer.
    pub fn ack_silk_resize(&mut self) {
        if !self.handshake.is_pending() {
            return;
        }

        self.width = self.handshake.pending_width;
        self.height = self.handshake.pending_height;
        self.pitch = (self.width as usize) * 4;
        let total_bytes = self.pitch * (self.height as usize);
        self.pixels.resize(total_bytes, 0);
        self.pixels.fill(0);

        self.handshake.acked_generation = self.handshake.request_generation;
        self.mark_all_dirty();
    }

    /// Write an RGBA8 pixel at `(x, y)` and expand the dirty rect.
    pub fn put_pixel(&mut self, x: u32, y: u32, rgba: [u8; 4]) {
        if x >= self.width || y >= self.height {
            return;
        }

        let offset = (y as usize) * self.pitch + (x as usize) * 4;
        self.pixels[offset..offset + 4].copy_from_slice(&rgba);
        self.mark_dirty(DirtyRect::new(x, y, 1, 1));
    }

    /// Read an RGBA8 pixel at `(x, y)`.
    #[must_use]
    pub fn get_pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }

        let offset = (y as usize) * self.pitch + (x as usize) * 4;
        let mut rgba = [0u8; 4];
        rgba.copy_from_slice(&self.pixels[offset..offset + 4]);
        Some(rgba)
    }

    /// Clear the entire surface with a solid color.
    pub fn clear(&mut self, rgba: [u8; 4]) {
        for chunk in self.pixels.chunks_exact_mut(4) {
            chunk.copy_from_slice(&rgba);
        }
        self.mark_all_dirty();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn radial_deadzone_zero_and_nan_safe() {
        let calib = StickCalibration::default();
        assert_eq!(calib.apply_radial([0.0, 0.0]), [0.0, 0.0]);
        assert_eq!(calib.apply_radial([f32::NAN, 0.5]), [0.0, 0.0]);
        assert_eq!(calib.apply_radial([0.5, f32::INFINITY]), [0.0, 0.0]);
    }

    #[test]
    fn radial_deadzone_filters_inner_range() {
        let calib = StickCalibration {
            multiplier: 1.0,
            deadzone: 0.15,
            anti_deadzone: 0.0,
        };

        // Within deadzone (r = 0.1) -> 0.0
        assert_eq!(calib.apply_radial([0.1, 0.0]), [0.0, 0.0]);
        assert_eq!(calib.apply_radial([0.0, 0.1]), [0.0, 0.0]);
        assert_eq!(calib.apply_radial([0.1, 0.1]), [0.0, 0.0]); // sqrt(0.02) ~= 0.1414 <= 0.15
    }

    #[test]
    fn radial_deadzone_anti_deadzone_and_unit_clamp() {
        let calib = StickCalibration {
            multiplier: 1.5,
            deadzone: 0.10,
            anti_deadzone: 0.20,
        };

        // Exactly at unit radius
        let res = calib.apply_radial([1.0, 0.0]);
        assert!((res[0] - 1.0).abs() < 1e-4);
        assert!((res[1] - 0.0).abs() < 1e-4);

        // Outside deadzone but scaled by 1.5 -> clamped to 1.0
        let res2 = calib.apply_radial([0.9, 0.0]);
        assert!((res2[0] - 1.0).abs() < 1e-4);

        // Just past deadzone (r = 0.1001), output magnitude should start at anti_deadzone * multiplier
        let res_edge = calib.apply_radial([0.1001, 0.0]);
        assert!(res_edge[0] >= 0.20 * 1.5 - 0.01);
    }

    #[test]
    fn display_settings_resolution_scale_clamped() {
        let mut settings = DisplaySettings::default();
        assert_eq!(settings.resolution_scale(), 1.0);

        settings.set_resolution_scale(0.2);
        assert_eq!(settings.resolution_scale(), 0.5);

        settings.set_resolution_scale(3.0);
        assert_eq!(settings.resolution_scale(), 2.0);

        settings.set_resolution_scale(1.5);
        let (scaled_w, scaled_h) = settings.scaled_viewport_size(1920, 1080);
        assert_eq!(scaled_w, 2880);
        assert_eq!(scaled_h, 1620);
    }

    #[test]
    fn scanout_surface_pixel_and_dirty_tracking() {
        let mut surface = SexOsScanoutSurface::new(64, 64);
        assert!(surface.is_dirty());
        surface.clear_dirty();
        assert!(!surface.is_dirty());

        let color = [255, 128, 64, 255];
        surface.put_pixel(10, 20, color);
        assert_eq!(surface.get_pixel(10, 20), Some(color));
        assert!(surface.is_dirty());

        let dirty = surface.dirty_rect().unwrap();
        assert_eq!(dirty.x, 10);
        assert_eq!(dirty.y, 20);
        assert_eq!(dirty.width, 1);
        assert_eq!(dirty.height, 1);
    }

    #[test]
    fn scanout_surface_silk_resize_handshake() {
        let mut surface = SexOsScanoutSurface::new(100, 100);
        assert!(!surface.resize_handshake().is_pending());

        surface.request_silk_resize(200, 150);
        assert!(surface.resize_handshake().is_pending());
        assert_eq!(surface.width(), 100);

        surface.ack_silk_resize();
        assert!(!surface.resize_handshake().is_pending());
        assert_eq!(surface.width(), 200);
        assert_eq!(surface.height(), 150);
        assert_eq!(surface.pitch(), 200 * 4);
    }
}
