//! Zero-Copy `egui_wgpu` Native Texture Viewport Bridge & 120Hz `SurfaceSwapchainPresenter`.
//!
//! Provides `ZeroCopyViewportBridge` for binding GPU-resident textures directly into `egui_wgpu::Renderer`
//! without CPU staging readbacks (enabling high-framerate 120Hz GUI rendering), and `SurfaceSwapchainPresenter`
//! for standalone window swapchain lifecycle with low-latency Mailbox/Fifo selection and automatic
//! `SurfaceError::Outdated`/`Lost` recovery.

use crate::RenderHardwareError;

/// Zero-copy GPU viewport bridge between hardware 3D rendering and `egui-wgpu`.
///
/// Keeps the rendered frame completely resident on the GPU (`Rgba8Unorm`), registering and updating
/// native textures inside `egui_wgpu::Renderer` to eliminate expensive staging buffer `MAP_READ` copies.
#[derive(Debug, Default)]
pub struct ZeroCopyViewportBridge {
    texture: Option<wgpu::Texture>,
    view: Option<wgpu::TextureView>,
    width: u32,
    height: u32,
    egui_texture_id: Option<egui::TextureId>,
}

impl ZeroCopyViewportBridge {
    /// Create a new unallocated zero-copy viewport bridge.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new zero-copy viewport bridge with initial dimensions.
    #[must_use]
    pub fn with_dimensions(width: u32, height: u32) -> Self {
        Self {
            texture: None,
            view: None,
            width,
            height,
            egui_texture_id: None,
        }
    }

    /// Current allocated viewport width.
    #[must_use]
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Current allocated viewport height.
    #[must_use]
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Current allocated viewport dimensions (width, height).
    #[must_use]
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Access GPU-resident texture reference if allocated.
    #[must_use]
    pub fn texture(&self) -> Option<&wgpu::Texture> {
        self.texture.as_ref()
    }

    /// Access GPU-resident texture view reference if allocated.
    #[must_use]
    pub fn texture_view(&self) -> Option<&wgpu::TextureView> {
        self.view.as_ref()
    }

    /// Registered `egui::TextureId` if registered with `egui_wgpu::Renderer`.
    #[must_use]
    pub fn egui_texture_id(&self) -> Option<egui::TextureId> {
        self.egui_texture_id
    }

    /// Register or resize native texture in `egui_wgpu::Renderer`.
    ///
    /// Re-allocates the underlying `wgpu::Texture` only when dimensions change, updating
    /// the egui texture binding in-place via `update_egui_texture_from_wgpu_texture`.
    pub fn register_or_update_in_egui_wgpu(
        &mut self,
        wgpu_renderer: &mut egui_wgpu::Renderer,
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) -> egui::TextureId {
        let w = width.max(1);
        let h = height.max(1);

        if self.texture.is_none() || self.width != w || self.height != h {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("zero_copy_viewport_texture"),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });

            let view = texture.create_view(&wgpu::TextureViewDescriptor {
                label: Some("zero_copy_viewport_view"),
                format: Some(wgpu::TextureFormat::Rgba8Unorm),
                dimension: Some(wgpu::TextureViewDimension::D2),
                aspect: wgpu::TextureAspect::All,
                base_mip_level: 0,
                mip_level_count: Some(1),
                base_array_layer: 0,
                array_layer_count: Some(1),
                usage: None,
            });

            if let Some(id) = self.egui_texture_id {
                wgpu_renderer.update_egui_texture_from_wgpu_texture(
                    device,
                    &view,
                    wgpu::FilterMode::Linear,
                    id,
                );
            } else {
                let id = wgpu_renderer.register_native_texture(
                    device,
                    &view,
                    wgpu::FilterMode::Linear,
                );
                self.egui_texture_id = Some(id);
            }

            self.texture = Some(texture);
            self.view = Some(view);
            self.width = w;
            self.height = h;
        }

        self.egui_texture_id.expect("egui texture id must be initialized")
    }

    /// Render directly into the registered GPU-resident native texture view.
    ///
    /// Executes the provided render pass callback directly targeting the zero-copy texture,
    /// with zero CPU memory staging buffer copies.
    pub fn render_3d_into_native_texture<F>(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        clear_color: Option<wgpu::Color>,
        draw_fn: F,
    ) -> Result<(), RenderHardwareError>
    where
        F: FnOnce(&mut wgpu::RenderPass<'_>),
    {
        let view = self.view.as_ref().ok_or(
            RenderHardwareError::InvalidDimensions(self.width, self.height),
        )?;

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("zero_copy_render_encoder"),
        });

        {
            let load_op = match clear_color {
                Some(c) => wgpu::LoadOp::Clear(c),
                None => wgpu::LoadOp::Load,
            };

            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("zero_copy_native_render_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: load_op,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            draw_fn(&mut pass);
        }

        queue.submit(Some(encoder.finish()));
        Ok(())
    }
}

/// Standalone 120Hz swapchain presenter managing `wgpu::Surface` configuration and frame acquisition.
///
/// Handles low-latency Mailbox/Fifo selection, automatic swapchain resizing, and transparent
/// recovery from `SurfaceError::Outdated` and `SurfaceError::Lost`.
#[derive(Debug)]
pub struct SurfaceSwapchainPresenter<'window> {
    surface: wgpu::Surface<'window>,
    config: wgpu::SurfaceConfiguration,
    target_present_mode: wgpu::PresentMode,
    is_low_latency: bool,
}

/// Swapchain presentation and acquisition errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SurfaceError {
    #[error("Surface outdated")]
    Outdated,
    #[error("Surface lost")]
    Lost,
    #[error("Surface timeout")]
    Timeout,
    #[error("Surface occluded")]
    Occluded,
    #[error("Surface validation error")]
    Validation,
}

impl<'window> SurfaceSwapchainPresenter<'window> {
    /// Create and configure a swapchain presenter for a window surface.
    ///
    /// If `low_latency_120hz` is enabled, prioritizes uncapped triple-buffering `PresentMode::Mailbox`
    /// or low-latency `Fifo` with single frame latency (latency = 1).
    #[must_use]
    pub fn new(
        surface: wgpu::Surface<'window>,
        adapter: &wgpu::Adapter,
        device: &wgpu::Device,
        width: u32,
        height: u32,
        low_latency_120hz: bool,
    ) -> Self {
        let caps = surface.get_capabilities(adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .or_else(|| caps.formats.first().copied())
            .unwrap_or(wgpu::TextureFormat::Bgra8UnormSrgb);

        let present_mode = if low_latency_120hz {
            if caps.present_modes.contains(&wgpu::PresentMode::Mailbox) {
                wgpu::PresentMode::Mailbox
            } else if caps.present_modes.contains(&wgpu::PresentMode::AutoVsync) {
                wgpu::PresentMode::AutoVsync
            } else {
                wgpu::PresentMode::Fifo
            }
        } else if caps.present_modes.contains(&wgpu::PresentMode::AutoVsync) {
            wgpu::PresentMode::AutoVsync
        } else {
            wgpu::PresentMode::Fifo
        };

        let alpha_mode = caps
            .alpha_modes
            .first()
            .copied()
            .unwrap_or(wgpu::CompositeAlphaMode::Auto);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: width.max(1),
            height: height.max(1),
            present_mode,
            desired_maximum_frame_latency: if low_latency_120hz { 1 } else { 2 },
            alpha_mode,
            view_formats: vec![],
            color_space: wgpu::SurfaceColorSpace::Auto,
        };

        surface.configure(device, &config);

        Self {
            surface,
            config,
            target_present_mode: present_mode,
            is_low_latency: low_latency_120hz,
        }
    }

    /// Resize swapchain when window dimensions change.
    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        let w = width.max(1);
        let h = height.max(1);
        if self.config.width != w || self.config.height != h {
            self.config.width = w;
            self.config.height = h;
            self.surface.configure(device, &self.config);
        }
    }

    /// Acquire the next surface texture for rendering.
    ///
    /// Automatically detects and recovers from `SurfaceError::Outdated` or `SurfaceError::Lost`
    /// by re-configuring the surface and retrying acquisition once.
    pub fn acquire_frame(&mut self, device: &wgpu::Device) -> Result<wgpu::SurfaceTexture, SurfaceError> {
        match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                Ok(frame)
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface.configure(device, &self.config);
                match self.surface.get_current_texture() {
                    wgpu::CurrentSurfaceTexture::Success(frame) | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                        Ok(frame)
                    }
                    wgpu::CurrentSurfaceTexture::Outdated => Err(SurfaceError::Outdated),
                    wgpu::CurrentSurfaceTexture::Lost => Err(SurfaceError::Lost),
                    wgpu::CurrentSurfaceTexture::Timeout => Err(SurfaceError::Timeout),
                    wgpu::CurrentSurfaceTexture::Occluded => Err(SurfaceError::Occluded),
                    wgpu::CurrentSurfaceTexture::Validation => Err(SurfaceError::Validation),
                }
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(device, &self.config);
                match self.surface.get_current_texture() {
                    wgpu::CurrentSurfaceTexture::Success(frame) | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                        Ok(frame)
                    }
                    wgpu::CurrentSurfaceTexture::Outdated => Err(SurfaceError::Outdated),
                    wgpu::CurrentSurfaceTexture::Lost => Err(SurfaceError::Lost),
                    wgpu::CurrentSurfaceTexture::Timeout => Err(SurfaceError::Timeout),
                    wgpu::CurrentSurfaceTexture::Occluded => Err(SurfaceError::Occluded),
                    wgpu::CurrentSurfaceTexture::Validation => Err(SurfaceError::Validation),
                }
            }
            wgpu::CurrentSurfaceTexture::Timeout => Err(SurfaceError::Timeout),
            wgpu::CurrentSurfaceTexture::Occluded => Err(SurfaceError::Occluded),
            wgpu::CurrentSurfaceTexture::Validation => Err(SurfaceError::Validation),
        }
    }

    /// Present a rendered frame to the physical display via the GPU queue.
    pub fn present(&self, queue: &wgpu::Queue, frame: wgpu::SurfaceTexture) {
        queue.present(frame);
    }

    /// Change presentation mode (e.g. Fifo vs Mailbox vs Immediate).
    pub fn set_present_mode(&mut self, device: &wgpu::Device, mode: wgpu::PresentMode) {
        self.config.present_mode = mode;
        self.target_present_mode = mode;
        self.surface.configure(device, &self.config);
    }

    /// Surface reference.
    #[must_use]
    pub fn surface(&self) -> &wgpu::Surface<'window> {
        &self.surface
    }

    /// SurfaceConfiguration reference.
    #[must_use]
    pub fn config(&self) -> &wgpu::SurfaceConfiguration {
        &self.config
    }

    /// Surface texture format.
    #[must_use]
    pub fn format(&self) -> wgpu::TextureFormat {
        self.config.format
    }

    /// Active presentation mode.
    #[must_use]
    pub fn present_mode(&self) -> wgpu::PresentMode {
        self.config.present_mode
    }

    /// Current width and height.
    #[must_use]
    pub fn dimensions(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    /// Whether low-latency 120Hz mode is enabled.
    #[must_use]
    pub fn is_low_latency(&self) -> bool {
        self.is_low_latency
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WgpuRenderDevice;

    #[test]
    fn test_zero_copy_viewport_bridge_default() {
        let bridge = ZeroCopyViewportBridge::new();
        assert_eq!(bridge.width(), 0);
        assert_eq!(bridge.height(), 0);
        assert_eq!(bridge.dimensions(), (0, 0));
        assert!(bridge.texture().is_none());
        assert!(bridge.texture_view().is_none());
        assert!(bridge.egui_texture_id().is_none());
    }

    #[test]
    fn test_zero_copy_viewport_invalid_dimensions_error() {
        let device_wrapper = WgpuRenderDevice::new_or_fallback();
        if let (Some(device), Some(queue)) = (device_wrapper.device(), device_wrapper.queue()) {
            let bridge = ZeroCopyViewportBridge::new();
            let res = bridge.render_3d_into_native_texture(device, queue, None, |_pass| {});
            assert!(res.is_err(), "Rendering into unallocated bridge must fail with InvalidDimensions");
        }
    }

    #[test]
    fn test_zero_copy_viewport_bridge_hardware_egui_registration() {
        let device_wrapper = WgpuRenderDevice::new_or_fallback();
        if !device_wrapper.is_hardware_accelerated() {
            return;
        }

        let device = device_wrapper.device().expect("device");
        let queue = device_wrapper.queue().expect("queue");

        let mut wgpu_renderer = egui_wgpu::Renderer::new(
            device,
            wgpu::TextureFormat::Rgba8Unorm,
            egui_wgpu::RendererOptions::default(),
        );

        let mut bridge = ZeroCopyViewportBridge::new();

        // 1. First registration
        let id1 = bridge.register_or_update_in_egui_wgpu(&mut wgpu_renderer, device, 320, 240);
        assert_eq!(bridge.dimensions(), (320, 240));
        assert!(bridge.texture().is_some());
        assert!(bridge.texture_view().is_some());
        assert_eq!(bridge.egui_texture_id(), Some(id1));

        // 2. Direct GPU render pass without CPU staging readback
        let render_res = bridge.render_3d_into_native_texture(
            device,
            queue,
            Some(wgpu::Color {
                r: 0.1,
                g: 0.2,
                b: 0.3,
                a: 1.0,
            }),
            |_pass| {},
        );
        assert!(render_res.is_ok(), "Direct 3D render pass must succeed");

        // 3. Resize: TextureId must be preserved in-place
        let id2 = bridge.register_or_update_in_egui_wgpu(&mut wgpu_renderer, device, 640, 480);
        assert_eq!(id1, id2, "egui TextureId must be reused across resizes");
        assert_eq!(bridge.dimensions(), (640, 480));
    }

    #[test]
    fn test_surface_error_variants_and_display() {
        let err_outdated = SurfaceError::Outdated;
        assert_eq!(format!("{err_outdated}"), "Surface outdated");

        let err_lost = SurfaceError::Lost;
        assert_eq!(format!("{err_lost}"), "Surface lost");

        let err_timeout = SurfaceError::Timeout;
        assert_eq!(format!("{err_timeout}"), "Surface timeout");

        let err_occluded = SurfaceError::Occluded;
        assert_eq!(format!("{err_occluded}"), "Surface occluded");

        let err_val = SurfaceError::Validation;
        assert_eq!(format!("{err_val}"), "Surface validation error");
    }
}
