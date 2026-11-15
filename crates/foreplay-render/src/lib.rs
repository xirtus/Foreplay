pub mod benchmark;
pub mod canvas2d;
pub mod meshlet;
pub mod pbr_gi;
pub mod perceptual;
pub mod pipeline;
pub mod shader;
pub mod textures;
pub mod wgpu_backend;

pub use benchmark::{run_10k_entity_render_benchmark, DrawCommand, RenderBenchmarkReceipt};
pub use canvas2d::{
    CanvasBatch2D, CanvasBoneWeight2D, CanvasItem, CanvasItemBatcher2D, CanvasItemKind,
    CanvasModulate, CanvasPolygon2D, CanvasVertex2D, DirectionalLight2D, LightOccluder2D,
    OccluderCullMode, PointLight2D, Rect2D, SdfLightField2D,
};
pub use meshlet::{build_meshlets, cull_meshlets, Meshlet, MAX_MESHLET_TRIANGLES, MAX_MESHLET_VERTICES};
pub use pbr_gi::{
    agx_tonemap_srgb, render_shaded_pbr_viewport_rgba8, PbrMaterialDesc, PbrViewportParams,
    RadianceCascade2D, RenderTriangleMesh, VoxelGiGrid,
};
pub use perceptual::{compute_psnr_rgba8, compute_ssim_rgba8};
pub use pipeline::{cull_aabbs_frustum, EmissionShape, Frustum, GpuParticles3DSim, Particle3D, Plane, ViewportDebugDraw};
pub use shader::{compile_gdshader_to_wgsl, ShaderCompileError, ShaderType, ShaderUniform, UniformHint, UniformType, VisualShaderGraph, VisualShaderNode};
pub use textures::{DecodedTexture, GpuTexture, MipFilter, TextureAssetLoader, TextureError};
pub use wgpu_backend::{
    ComputeParticle3D, HardwareBonePalette, HardwareDrawItem3D, HardwareFrameOutput,
    HardwareLight3D, HardwareLightType, HardwareVertex3D, RenderHardwareError, WgpuFrameTelemetry,
    WgpuRenderDevice,
};

use std::collections::BTreeMap;
use bevy_math::{Mat4, Quat, Vec2, Vec3, Vec4};
use foreplay_ir::Rid;

#[derive(Debug)]
pub enum MeshTag {}
#[derive(Debug)]
pub enum MaterialTag {}
#[derive(Debug)]
pub enum ViewportTag {}
#[derive(Debug)]
pub enum Camera3DTag {}
#[derive(Debug)]
pub enum DirectionalLightTag {}

pub type MeshRid = Rid<MeshTag>;
pub type MaterialRid = Rid<MaterialTag>;
pub type ViewportRid = Rid<ViewportTag>;
pub type Camera3DRid = Rid<Camera3DTag>;
pub type LightRid = Rid<DirectionalLightTag>;

/// 3D ray for screen-space picking and intersection tests.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Ray3D {
    pub origin: Vec3,
    pub direction: Vec3,
}

/// Perspective 3D camera with view-projection transforms, world-to-screen projection,
/// and screen-to-ray mouse picking.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct PerspectiveCamera3D {
    pub position: Vec3,
    pub rotation: Quat,
    pub fov_y_radians: f32,
    pub aspect_ratio: f32,
    pub near: f32,
    pub far: f32,
}

impl Default for PerspectiveCamera3D {
    fn default() -> Self {
        Self {
            position: Vec3::new(0.0, 2.0, 5.0),
            rotation: Quat::IDENTITY,
            fov_y_radians: 60.0_f32.to_radians(),
            aspect_ratio: 16.0 / 9.0,
            near: 0.1,
            far: 1000.0,
        }
    }
}

impl PerspectiveCamera3D {
    /// Create a perspective camera looking towards -Z.
    #[must_use]
    pub fn new(position: Vec3, fov_y_degrees: f32, aspect_ratio: f32) -> Self {
        Self {
            position,
            rotation: Quat::IDENTITY,
            fov_y_radians: fov_y_degrees.to_radians(),
            aspect_ratio,
            near: 0.1,
            far: 1000.0,
        }
    }

    /// Orient camera to look at a target point.
    pub fn look_at(&mut self, target: Vec3, up: Vec3) {
        let forward = (target - self.position).normalize_or_zero();
        if forward.length_squared() > 1e-6 {
            let right = forward.cross(up).normalize_or_zero();
            let true_up = right.cross(forward).normalize_or_zero();
            if right.length_squared() > 1e-6 && true_up.length_squared() > 1e-6 {
                let rot_mat = bevy_math::Mat3::from_cols(right, true_up, -forward);
                self.rotation = Quat::from_mat3(&rot_mat);
            }
        }
    }

    /// World-to-view matrix.
    #[must_use]
    pub fn view_matrix(&self) -> Mat4 {
        Mat4::from_rotation_translation(self.rotation, self.position).inverse()
    }

    /// View-to-clip projection matrix (Right-Handed Vulkan/wgpu [0, 1] depth range).
    #[must_use]
    pub fn projection_matrix(&self) -> Mat4 {
        let inv_length = 1.0 / (self.near - self.far);
        let f = 1.0 / (self.fov_y_radians * 0.5).tan();
        let a = f / self.aspect_ratio;
        let b = f;
        let c = self.far * inv_length;
        let d = (self.near * self.far) * inv_length;

        Mat4::from_cols(
            Vec4::new(a, 0.0, 0.0, 0.0),
            Vec4::new(0.0, b, 0.0, 0.0),
            Vec4::new(0.0, 0.0, c, -1.0),
            Vec4::new(0.0, 0.0, d, 0.0),
        )
    }

    /// Combined view-projection matrix.
    #[must_use]
    pub fn view_projection_matrix(&self) -> Mat4 {
        self.projection_matrix() * self.view_matrix()
    }

    /// Project a 3D world position into 2D viewport coordinates `(x, y)` in `[0, width)` x `[0, height)`.
    /// Returns `None` if the point is behind the camera or clipped outside the near/far planes.
    #[must_use]
    pub fn project_point(&self, world_pos: Vec3, viewport_width: f32, viewport_height: f32) -> Option<Vec2> {
        let vp = self.view_projection_matrix();
        let clip = vp * Vec4::new(world_pos.x, world_pos.y, world_pos.z, 1.0);

        // Point is behind camera or too close to near plane
        if clip.w <= 1e-6 {
            return None;
        }

        let ndc = clip.truncate() / clip.w;
        // Check depth within [0.0, 1.0]
        if ndc.z < 0.0 || ndc.z > 1.0 {
            return None;
        }

        let screen_x = (ndc.x * 0.5 + 0.5) * viewport_width;
        let screen_y = (1.0 - (ndc.y * 0.5 + 0.5)) * viewport_height;

        Some(Vec2::new(screen_x, screen_y))
    }

    /// Construct a 3D ray passing from camera through screen coordinate `(x, y)` for picking.
    #[must_use]
    pub fn viewport_to_ray(&self, screen_pos: Vec2, viewport_width: f32, viewport_height: f32) -> Ray3D {
        let ndc_x = (screen_pos.x / viewport_width.max(1.0)) * 2.0 - 1.0;
        let ndc_y = 1.0 - (screen_pos.y / viewport_height.max(1.0)) * 2.0;

        let inv_vp = self.view_projection_matrix().inverse();
        let p_near_clip = Vec4::new(ndc_x, ndc_y, 0.0, 1.0);
        let p_far_clip = Vec4::new(ndc_x, ndc_y, 1.0, 1.0);

        let p_near = inv_vp * p_near_clip;
        let p_far = inv_vp * p_far_clip;

        let p_near_w = if p_near.w.abs() > 1e-7 { p_near.truncate() / p_near.w } else { self.position };
        let p_far_w = if p_far.w.abs() > 1e-7 { p_far.truncate() / p_far.w } else { self.position + Vec3::NEG_Z * 1000.0 };

        let direction = (p_far_w - p_near_w).normalize_or_zero();
        Ray3D {
            origin: p_near_w,
            direction,
        }
    }
}

/// A line segment in 3D space with RGBA color.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct GizmoLine {
    pub start: Vec3,
    pub end: Vec3,
    pub color: [f32; 4],
}

/// Immediate-mode debug gizmo draw list for grid, axis indicators, and collider bounds.
#[derive(Clone, Debug, Default)]
pub struct GizmoDrawList {
    lines: Vec<GizmoLine>,
}

impl GizmoDrawList {
    #[must_use]
    pub fn new() -> Self {
        Self { lines: Vec::new() }
    }

    /// Add a colored 3D line segment.
    pub fn add_line(&mut self, start: Vec3, end: Vec3, color: [f32; 4]) {
        self.lines.push(GizmoLine { start, end, color });
    }

    /// Add an axis-aligned bounding box (12 line segments).
    pub fn add_aabb(&mut self, min: Vec3, max: Vec3, color: [f32; 4]) {
        let c000 = Vec3::new(min.x, min.y, min.z);
        let c001 = Vec3::new(min.x, min.y, max.z);
        let c010 = Vec3::new(min.x, max.y, min.z);
        let c011 = Vec3::new(min.x, max.y, max.z);
        let c100 = Vec3::new(max.x, min.y, min.z);
        let c101 = Vec3::new(max.x, min.y, max.z);
        let c110 = Vec3::new(max.x, max.y, min.z);
        let c111 = Vec3::new(max.x, max.y, max.z);

        // Bottom quad
        self.add_line(c000, c100, color);
        self.add_line(c100, c101, color);
        self.add_line(c101, c001, color);
        self.add_line(c001, c000, color);

        // Top quad
        self.add_line(c010, c110, color);
        self.add_line(c110, c111, color);
        self.add_line(c111, c011, color);
        self.add_line(c011, c010, color);

        // Vertical pillars
        self.add_line(c000, c010, color);
        self.add_line(c100, c110, color);
        self.add_line(c101, c111, color);
        self.add_line(c001, c011, color);
    }

    /// Add a ground plane grid centered at the origin on the XZ plane.
    pub fn add_grid(&mut self, size: f32, subdivisions: u32, color: [f32; 4]) {
        let half = size * 0.5;
        let steps = subdivisions.max(1);
        let step = size / (steps as f32);

        for i in 0..=steps {
            let offset = -half + (i as f32) * step;
            // Lines parallel to Z
            self.add_line(Vec3::new(offset, 0.0, -half), Vec3::new(offset, 0.0, half), color);
            // Lines parallel to X
            self.add_line(Vec3::new(-half, 0.0, offset), Vec3::new(half, 0.0, offset), color);
        }
    }

    /// Add 3D coordinate axis indicators (X: Red, Y: Green, Z: Blue).
    pub fn add_axis_gizmo(&mut self, origin: Vec3, length: f32) {
        self.add_line(origin, origin + Vec3::X * length, [1.0, 0.0, 0.0, 1.0]);
        self.add_line(origin, origin + Vec3::Y * length, [0.0, 1.0, 0.0, 1.0]);
        self.add_line(origin, origin + Vec3::Z * length, [0.0, 0.0, 1.0, 1.0]);
    }

    /// Clear all recorded gizmo lines.
    pub fn clear(&mut self) {
        self.lines.clear();
    }

    /// Slice of active gizmo lines.
    #[must_use]
    pub fn lines(&self) -> &[GizmoLine] {
        &self.lines
    }
}

/// Standard 3D vertex layout.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Vertex3D {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

/// Mesh geometry descriptor containing vertex and index buffers.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct MeshDescriptor {
    pub vertices: Vec<Vertex3D>,
    pub indices: Vec<u32>,
}

impl MeshDescriptor {
    /// Generate a 3D unit cube centered at origin with edge length `size`.
    #[must_use]
    pub fn cube(size: f32) -> Self {
        let h = size * 0.5;
        let corners = [
            [-h, -h, -h], [h, -h, -h], [h, h, -h], [-h, h, -h],
            [-h, -h,  h], [h, -h,  h], [h, h,  h], [-h, h,  h],
        ];

        let mut vertices = Vec::with_capacity(8);
        for c in corners {
            vertices.push(Vertex3D {
                position: c,
                normal: [0.0, 1.0, 0.0],
                uv: [0.0, 0.0],
                color: [1.0, 1.0, 1.0, 1.0],
            });
        }

        let indices = vec![
            // Front (-Z)
            0, 2, 1, 0, 3, 2,
            // Back (+Z)
            4, 5, 6, 4, 6, 7,
            // Left (-X)
            0, 7, 3, 0, 4, 7,
            // Right (+X)
            1, 2, 6, 1, 6, 5,
            // Top (+Y)
            3, 7, 6, 3, 6, 2,
            // Bottom (-Y)
            0, 1, 5, 0, 5, 4,
        ];

        Self { vertices, indices }
    }

    /// Generate a planar grid on the XZ plane.
    #[must_use]
    pub fn grid(size: f32, subdivisions: u32) -> Self {
        let half = size * 0.5;
        let divs = subdivisions.max(1);
        let step = size / (divs as f32);

        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        for z_i in 0..=divs {
            let z = -half + (z_i as f32) * step;
            for x_i in 0..=divs {
                let x = -half + (x_i as f32) * step;
                let u = (x_i as f32) / (divs as f32);
                let v = (z_i as f32) / (divs as f32);
                vertices.push(Vertex3D {
                    position: [x, 0.0, z],
                    normal: [0.0, 1.0, 0.0],
                    uv: [u, v],
                    color: [0.8, 0.8, 0.8, 1.0],
                });
            }
        }

        let stride = divs + 1;
        for z_i in 0..divs {
            for x_i in 0..divs {
                let top_left = z_i * stride + x_i;
                let top_right = top_left + 1;
                let bottom_left = (z_i + 1) * stride + x_i;
                let bottom_right = bottom_left + 1;

                indices.push(top_left);
                indices.push(bottom_left);
                indices.push(top_right);

                indices.push(top_right);
                indices.push(bottom_left);
                indices.push(bottom_right);
            }
        }

        Self { vertices, indices }
    }

    /// Generate line arrows representing X, Y, and Z axes.
    #[must_use]
    pub fn gizmo_arrows(length: f32) -> Self {
        let vertices = vec![
            Vertex3D { position: [0.0, 0.0, 0.0], normal: [0.0, 0.0, 0.0], uv: [0.0, 0.0], color: [1.0, 0.0, 0.0, 1.0] },
            Vertex3D { position: [length, 0.0, 0.0], normal: [0.0, 0.0, 0.0], uv: [0.0, 0.0], color: [1.0, 0.0, 0.0, 1.0] },
            Vertex3D { position: [0.0, 0.0, 0.0], normal: [0.0, 0.0, 0.0], uv: [0.0, 0.0], color: [0.0, 1.0, 0.0, 1.0] },
            Vertex3D { position: [0.0, length, 0.0], normal: [0.0, 0.0, 0.0], uv: [0.0, 0.0], color: [0.0, 1.0, 0.0, 1.0] },
            Vertex3D { position: [0.0, 0.0, 0.0], normal: [0.0, 0.0, 0.0], uv: [0.0, 0.0], color: [0.0, 0.0, 1.0, 1.0] },
            Vertex3D { position: [0.0, 0.0, length], normal: [0.0, 0.0, 0.0], uv: [0.0, 0.0], color: [0.0, 0.0, 1.0, 1.0] },
        ];
        let indices = vec![0, 1, 2, 3, 4, 5];
        Self { vertices, indices }
    }
}

/// Material properties descriptor.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct MaterialDescriptor {
    pub albedo_color: [f32; 4],
    pub roughness: f32,
    pub metallic: f32,
    pub wireframe: bool,
}

impl Default for MaterialDescriptor {
    fn default() -> Self {
        Self {
            albedo_color: [1.0, 1.0, 1.0, 1.0],
            roughness: 0.5,
            metallic: 0.0,
            wireframe: false,
        }
    }
}

/// Directional light descriptor.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct DirectionalLightDescriptor {
    pub direction: [f32; 3],
    pub color: [f32; 3],
    pub energy: f32,
}

impl Default for DirectionalLightDescriptor {
    fn default() -> Self {
        Self {
            direction: [-0.5, -1.0, -0.5],
            color: [1.0, 1.0, 1.0],
            energy: 1.0,
        }
    }
}

/// Viewport configuration and state.
#[derive(Clone, Debug)]
struct ViewportData {
    width: u32,
    height: u32,
    clear_color: [u8; 4],
    camera: Option<Camera3DRid>,
    debug_draw: ViewportDebugDraw,
}

/// FNV-1a 64-bit deterministic golden-frame hash helper.
#[must_use]
pub fn compute_golden_frame_hash(framebuffer: &[u8]) -> u64 {
    const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

    let mut hash = FNV_OFFSET_BASIS;
    for &byte in framebuffer {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// High-performance `RenderingServer` with typed RID arenas, gizmo lists,
/// and headless deterministic RGBA8 software rasterization.
#[derive(Debug)]
pub struct RenderingServer {
    next_raw_rid: u64,
    meshes: BTreeMap<u64, MeshDescriptor>,
    materials: BTreeMap<u64, MaterialDescriptor>,
    cameras: BTreeMap<u64, PerspectiveCamera3D>,
    lights: BTreeMap<u64, DirectionalLightDescriptor>,
    viewports: BTreeMap<u64, ViewportData>,
    pub gizmos: GizmoDrawList,
}

impl Default for RenderingServer {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderingServer {
    /// Create an initialized `RenderingServer`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            next_raw_rid: 1,
            meshes: BTreeMap::new(),
            materials: BTreeMap::new(),
            cameras: BTreeMap::new(),
            lights: BTreeMap::new(),
            viewports: BTreeMap::new(),
            gizmos: GizmoDrawList::new(),
        }
    }

    fn alloc_rid<Tag>(&mut self) -> Rid<Tag> {
        let id = self.next_raw_rid;
        self.next_raw_rid += 1;
        Rid::from_raw(id)
    }

    /// Create a new viewport handle with default dimensions (1280x720) and dark clear color.
    pub fn create_viewport(&mut self) -> ViewportRid {
        let rid = self.alloc_rid::<ViewportTag>();
        self.viewports.insert(
            rid.raw,
            ViewportData {
                width: 1280,
                height: 720,
                clear_color: [20, 24, 32, 255],
                camera: None,
                debug_draw: ViewportDebugDraw::LitPbr,
            },
        );
        rid
    }

    /// Set viewport debug draw visualization mode.
    pub fn set_viewport_debug_draw(&mut self, viewport: ViewportRid, mode: ViewportDebugDraw) {
        if let Some(vp) = self.viewports.get_mut(&viewport.raw) {
            vp.debug_draw = mode;
        }
    }

    /// Get viewport debug draw mode.
    #[must_use]
    pub fn get_viewport_debug_draw(&self, viewport: ViewportRid) -> ViewportDebugDraw {
        self.viewports.get(&viewport.raw).map(|v| v.debug_draw).unwrap_or_default()
    }

    /// Update viewport render resolution.
    pub fn set_viewport_size(&mut self, viewport: ViewportRid, width: u32, height: u32) {
        if let Some(vp) = self.viewports.get_mut(&viewport.raw) {
            vp.width = width.max(1);
            vp.height = height.max(1);
        }
    }

    /// Set background clear color for a viewport.
    pub fn set_viewport_clear_color(&mut self, viewport: ViewportRid, color: [u8; 4]) {
        if let Some(vp) = self.viewports.get_mut(&viewport.raw) {
            vp.clear_color = color;
        }
    }

    /// Assign active 3D camera to a viewport.
    pub fn set_viewport_camera(&mut self, viewport: ViewportRid, camera: Camera3DRid) {
        if let Some(vp) = self.viewports.get_mut(&viewport.raw) {
            vp.camera = Some(camera);
        }
    }

    /// Register a 3D mesh.
    pub fn create_mesh(&mut self, descriptor: MeshDescriptor) -> MeshRid {
        let rid = self.alloc_rid::<MeshTag>();
        self.meshes.insert(rid.raw, descriptor);
        rid
    }

    /// Register a material.
    pub fn create_material(&mut self, descriptor: MaterialDescriptor) -> MaterialRid {
        let rid = self.alloc_rid::<MaterialTag>();
        self.materials.insert(rid.raw, descriptor);
        rid
    }

    /// Register a perspective 3D camera.
    pub fn create_camera_3d(&mut self, camera: PerspectiveCamera3D) -> Camera3DRid {
        let rid = self.alloc_rid::<Camera3DTag>();
        self.cameras.insert(rid.raw, camera);
        rid
    }

    /// Register a directional light.
    pub fn create_directional_light(&mut self, light: DirectionalLightDescriptor) -> LightRid {
        let rid = self.alloc_rid::<DirectionalLightTag>();
        self.lights.insert(rid.raw, light);
        rid
    }

    /// Access camera reference.
    #[must_use]
    pub fn get_camera_3d(&self, camera: Camera3DRid) -> Option<&PerspectiveCamera3D> {
        self.cameras.get(&camera.raw)
    }

    /// Mutable access to camera.
    pub fn get_camera_3d_mut(&mut self, camera: Camera3DRid) -> Option<&mut PerspectiveCamera3D> {
        self.cameras.get_mut(&camera.raw)
    }

    /// Free a mesh resource handle.
    pub fn free_mesh(&mut self, rid: MeshRid) -> bool {
        self.meshes.remove(&rid.raw).is_some()
    }

    /// Free a material resource handle.
    pub fn free_material(&mut self, rid: MaterialRid) -> bool {
        self.materials.remove(&rid.raw).is_some()
    }

    /// Free a camera resource handle.
    pub fn free_camera_3d(&mut self, rid: Camera3DRid) -> bool {
        self.cameras.remove(&rid.raw).is_some()
    }

    /// Free a directional light resource handle.
    pub fn free_directional_light(&mut self, rid: LightRid) -> bool {
        self.lights.remove(&rid.raw).is_some()
    }

    /// Free a viewport resource handle.
    pub fn free_viewport(&mut self, rid: ViewportRid) -> bool {
        self.viewports.remove(&rid.raw).is_some()
    }

    /// Deterministic headless software RGBA8 wireframe & clear rasterizer.
    ///
    /// Ideal for SexOS userspace fallback and bit-for-bit reproducible golden CI tests.
    #[must_use]
    pub fn render_viewport_rgba8(&self, viewport: ViewportRid, width: u32, height: u32) -> Vec<u8> {
        let w = width.max(1);
        let h = height.max(1);
        let total_pixels = (w as usize) * (h as usize);
        let mut framebuffer = vec![0u8; total_pixels * 4];

        let (clear_color, camera_opt) = match self.viewports.get(&viewport.raw) {
            Some(vp) => (vp.clear_color, vp.camera),
            None => ([20, 24, 32, 255], None),
        };

        // Solid clear
        for chunk in framebuffer.chunks_exact_mut(4) {
            chunk.copy_from_slice(&clear_color);
        }

        // If a camera is assigned, render projected gizmo lines
        if let Some(camera) = camera_opt.and_then(|rid| self.cameras.get(&rid.raw)) {
            // Update aspect ratio for projection
            let mut active_cam = *camera;
            active_cam.aspect_ratio = (w as f32) / (h as f32);

            for line in self.gizmos.lines() {
                let p0_opt = active_cam.project_point(line.start, w as f32, h as f32);
                let p1_opt = active_cam.project_point(line.end, w as f32, h as f32);

                if let (Some(p0), Some(p1)) = (p0_opt, p1_opt) {
                    let color_u8 = [
                        (line.color[0].clamp(0.0, 1.0) * 255.0).round() as u8,
                        (line.color[1].clamp(0.0, 1.0) * 255.0).round() as u8,
                        (line.color[2].clamp(0.0, 1.0) * 255.0).round() as u8,
                        (line.color[3].clamp(0.0, 1.0) * 255.0).round() as u8,
                    ];

                    draw_line_bresenham(
                        &mut framebuffer,
                        (w, h),
                        [p0.x.round() as i32, p0.y.round() as i32],
                        [p1.x.round() as i32, p1.y.round() as i32],
                        color_u8,
                    );
                }
            }
        }

        framebuffer
    }
}

/// Integer Bresenham line rasterization clipped to viewport bounds.
fn draw_line_bresenham(
    fb: &mut [u8],
    (width, height): (u32, u32),
    [x0, y0]: [i32; 2],
    [x1, y1]: [i32; 2],
    color: [u8; 4],
) {
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    let mut x = x0;
    let mut y = y0;

    let w_i32 = width as i32;
    let h_i32 = height as i32;

    loop {
        if x >= 0 && x < w_i32 && y >= 0 && y < h_i32 {
            let offset = ((y as usize) * (width as usize) + (x as usize)) * 4;
            fb[offset..offset + 4].copy_from_slice(&color);
        }

        if x == x1 && y == y1 {
            break;
        }

        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_projection_and_ray_math() {
        let mut cam = PerspectiveCamera3D::new(Vec3::new(0.0, 0.0, 5.0), 60.0, 16.0 / 9.0);
        cam.look_at(Vec3::ZERO, Vec3::Y);

        // Center origin should project to exact viewport center
        let screen = cam.project_point(Vec3::ZERO, 1920.0, 1080.0).expect("visible point");
        assert!((screen.x - 960.0).abs() < 1.0);
        assert!((screen.y - 540.0).abs() < 1.0);

        // Point behind camera should return None
        let behind = cam.project_point(Vec3::new(0.0, 0.0, 10.0), 1920.0, 1080.0);
        assert!(behind.is_none());

        // Viewport center ray should point straight down -Z towards (0, 0, 0)
        let ray = cam.viewport_to_ray(Vec2::new(960.0, 540.0), 1920.0, 1080.0);
        assert!((ray.direction.z - (-1.0)).abs() < 1e-4);
        assert!(ray.direction.x.abs() < 1e-4);
        assert!(ray.direction.y.abs() < 1e-4);
    }

    #[test]
    fn server_rid_lifecycle() {
        let mut server = RenderingServer::new();
        let vp = server.create_viewport();
        let mesh = server.create_mesh(MeshDescriptor::cube(1.0));
        let mat = server.create_material(MaterialDescriptor::default());
        let cam = server.create_camera_3d(PerspectiveCamera3D::default());
        let light = server.create_directional_light(DirectionalLightDescriptor::default());

        assert!(server.free_mesh(mesh));
        assert!(!server.free_mesh(mesh));
        assert!(server.free_material(mat));
        assert!(server.free_camera_3d(cam));
        assert!(server.free_directional_light(light));
        assert!(server.free_viewport(vp));
    }

    #[test]
    fn software_rasterizer_deterministic_hash() {
        let mut server = RenderingServer::new();
        let vp = server.create_viewport();
        let mut cam = PerspectiveCamera3D::new(Vec3::new(0.0, 3.0, 6.0), 60.0, 1.0);
        cam.look_at(Vec3::ZERO, Vec3::Y);
        let cam_rid = server.create_camera_3d(cam);
        server.set_viewport_camera(vp, cam_rid);

        // Add 3D gizmo lines
        server.gizmos.add_grid(4.0, 4, [0.5, 0.5, 0.5, 1.0]);
        server.gizmos.add_axis_gizmo(Vec3::ZERO, 1.0);

        let fb1 = server.render_viewport_rgba8(vp, 128, 128);
        let fb2 = server.render_viewport_rgba8(vp, 128, 128);

        let hash1 = compute_golden_frame_hash(&fb1);
        let hash2 = compute_golden_frame_hash(&fb2);

        assert_eq!(fb1.len(), 128 * 128 * 4);
        assert_eq!(hash1, hash2);
        assert_ne!(hash1, 0xcbf2_9ce4_8422_2325); // Hash must not equal empty basis
    }

    #[test]
    fn gizmo_aabb_and_grid_counts() {
        let mut gizmos = GizmoDrawList::new();
        assert_eq!(gizmos.lines().len(), 0);

        gizmos.add_aabb(Vec3::new(-1.0, -1.0, -1.0), Vec3::new(1.0, 1.0, 1.0), [1.0; 4]);
        assert_eq!(gizmos.lines().len(), 12);

        gizmos.clear();
        assert_eq!(gizmos.lines().len(), 0);

        // 4 subdivisions -> 5 lines along X, 5 along Z = 10 lines
        gizmos.add_grid(10.0, 4, [0.5; 4]);
        assert_eq!(gizmos.lines().len(), 10);
    }

    #[test]
    fn picking_ray_ground_plane_intersection() {
        let mut cam = PerspectiveCamera3D::new(Vec3::new(0.0, 10.0, 10.0), 60.0, 1.0);
        cam.look_at(Vec3::ZERO, Vec3::Y);

        let ray = cam.viewport_to_ray(Vec2::new(500.0, 500.0), 1000.0, 1000.0);
        // Intersect ray with ground plane Y = 0: origin.y + t * dir.y = 0 -> t = -origin.y / dir.y
        assert!(ray.direction.y < -1e-4);
        let t = -ray.origin.y / ray.direction.y;
        assert!(t > 0.0);
        let hit = ray.origin + ray.direction * t;
        assert!(hit.x.abs() < 1e-3);
        assert!(hit.y.abs() < 1e-4);
        assert!(hit.z.abs() < 1e-3);
    }

    #[test]
    fn gdshader_to_wgsl_compilation() {
        let shader_src = r#"
shader_type spatial;
render_mode cull_back, unshaded;

uniform vec4 albedo_color : source_color = vec4(1.0, 0.5, 0.2, 1.0);
uniform float roughness : hint_range(0.0, 1.0) = 0.5;
uniform sampler2D main_tex : hint_default_white;

void fragment() {
    vec4 tex = texture(main_tex, UV);
    ALBEDO = albedo_color.rgb * tex.rgb;
    ROUGHNESS = roughness;
}
"#;
        let wgsl = compile_gdshader_to_wgsl(shader_src).expect("must compile valid wgsl");
        assert!(wgsl.contains("struct MaterialUniforms"));
        assert!(wgsl.contains("albedo_color: vec4<f32>"));
        assert!(wgsl.contains("roughness: f32"));
        assert!(wgsl.contains("var main_tex_texture: texture_2d<f32>"));
        assert!(wgsl.contains("@fragment"));
        assert!(wgsl.contains("textureSample(main_tex_texture, main_tex_sampler, UV)"));
    }

    #[test]
    fn visual_shader_graph_wgsl_compilation() {
        let mut graph = VisualShaderGraph::new(ShaderType::Spatial);
        let col_node = graph.add_node(VisualShaderNode::ColorConstant([1.0, 0.0, 0.0, 1.0]));
        // Connect color node to OutputSpatial albedo port (0)
        graph.connect_ports((col_node, 0), (0, 0));

        let wgsl = graph.compile_wgsl().expect("visual shader compiles to wgsl");
        assert!(wgsl.contains("ALBEDO"));
        assert!(wgsl.contains("@fragment"));
    }

    #[test]
    fn frustum_extraction_and_aabb_culling() {
        let cam_pos = Vec3::new(0.0, 0.0, 10.0);
        let mut cam = PerspectiveCamera3D::new(cam_pos, 60.0, 1.0);
        cam.look_at(Vec3::ZERO, Vec3::Y);
        let frustum = Frustum::from_view_projection(&cam.view_projection_matrix());

        // Inside frustum: box centered at origin
        assert!(frustum.intersects_aabb(Vec3::splat(-1.0), Vec3::splat(1.0)));

        // Outside frustum: box behind camera
        assert!(!frustum.intersects_aabb(Vec3::new(-1.0, -1.0, 15.0), Vec3::new(1.0, 1.0, 20.0)));

        // Batch culling
        let aabbs_min = vec![Vec3::splat(-1.0), Vec3::new(-1.0, -1.0, 15.0)];
        let aabbs_max = vec![Vec3::splat(1.0), Vec3::new(1.0, 1.0, 20.0)];
        let mut visible = Vec::new();
        cull_aabbs_frustum(&frustum, &aabbs_min, &aabbs_max, &mut visible);
        assert_eq!(visible, vec![0]);
    }

    #[test]
    fn gpu_particles_3d_simulation() {
        let mut emitter = GpuParticles3DSim::new(100);
        emitter.emission_rate = 50.0;
        emitter.lifetime = 1.0;
        assert_eq!(emitter.active_count(), 0);

        emitter.step(0.1);
        assert!(emitter.active_count() > 0);

        // Advance past lifetime
        emitter.emission_rate = 0.0;
        emitter.step(1.5);
        assert_eq!(emitter.active_count(), 0);
    }

    #[test]
    fn phase_3_10k_entity_render_benchmark_exit_gate() {
        let receipt = run_10k_entity_render_benchmark(10_000);
        assert_eq!(receipt.total_entities, 10_000);
        assert!(receipt.visible_entities > 0);
        assert!(receipt.zero_alloc_steady_state, "must have zero heap allocations in steady-state loop");
        assert!(receipt.meets_120_fps_gate, "frame time {}ms must be <8.33ms (>120 FPS)", receipt.frame_time_ms);
    }
}
