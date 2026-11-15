//! 3D & 2D Viewport Camera and Gizmo Projections (`foreplay-editor`).
//!
//! Provides orbit/pan/zoom perspective camera math and viewport projection
//! onto `egui::Painter` coordinates, along with transform gizmo state.

use bevy_math::Vec3;
use egui::{Pos2, Rect, Vec2};
use foreplay_ir::{IrValue, NodeId};

/// 3D perspective orbit camera for the interactive viewport.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera3D {
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub target: Vec3,
    pub fov_y: f32,
}

impl Default for Camera3D {
    fn default() -> Self {
        Self {
            yaw: -0.75,
            pitch: 0.45,
            distance: 18.0,
            target: Vec3::new(0.0, 1.0, 0.0),
            fov_y: 60.0_f32.to_radians(),
        }
    }
}

impl Camera3D {
    /// Computes the camera eye position in world space.
    pub fn eye(&self) -> Vec3 {
        let x = self.target.x + self.distance * self.yaw.cos() * self.pitch.cos();
        let y = self.target.y + self.distance * self.pitch.sin();
        let z = self.target.z + self.distance * self.yaw.sin() * self.pitch.cos();
        Vec3::new(x, y, z)
    }

    /// Unit vector pointing in the camera's viewing direction.
    pub fn forward(&self) -> Vec3 {
        (self.target - self.eye()).normalize_or_zero()
    }

    /// Unit vector pointing to the camera's right.
    pub fn right(&self) -> Vec3 {
        let fwd = self.forward();
        let r = fwd.cross(Vec3::Y);
        if r.length_squared() < 1e-4 {
            Vec3::X
        } else {
            r.normalize()
        }
    }

    /// Unit vector pointing to the camera's up direction.
    pub fn up(&self) -> Vec3 {
        self.right().cross(self.forward()).normalize_or_zero()
    }

    /// Projects a 3D world position into 2D screen coordinates within `rect`.
    /// Returns `None` if the point is behind the camera near-plane.
    pub fn project(&self, point: Vec3, rect: Rect) -> Option<Pos2> {
        let eye = self.eye();
        let rel = point - eye;
        let fwd = self.forward();
        let r = self.right();
        let u = self.up();

        let vz = rel.dot(fwd);
        if vz <= 0.15 {
            return None;
        }

        let vx = rel.dot(r);
        let vy = rel.dot(u);

        let aspect = (rect.width() / rect.height().max(1.0)).max(0.1);
        let tan_half_fov = (self.fov_y * 0.5).tan();

        let norm_x = vx / (vz * tan_half_fov * aspect);
        let norm_y = vy / (vz * tan_half_fov);

        let px = rect.center().x + norm_x * rect.width() * 0.5;
        let py = rect.center().y - norm_y * rect.height() * 0.5;

        Some(Pos2::new(px, py))
    }

    /// Orbit the camera around the target point.
    pub fn orbit(&mut self, delta: Vec2) {
        self.yaw += delta.x * 0.006;
        self.pitch = (self.pitch - delta.y * 0.006).clamp(-1.45, 1.45);
    }

    /// Pan the camera target in the camera plane.
    pub fn pan(&mut self, delta: Vec2) {
        let pan_speed = self.distance * 0.0015;
        let offset = (-self.right() * delta.x + self.up() * delta.y) * pan_speed;
        self.target += offset;
    }

    /// Zoom in or out by adjusting distance to target.
    pub fn zoom(&mut self, factor: f32) {
        self.distance = (self.distance * factor).clamp(1.5, 300.0);
    }

    /// Aligns camera view angles to a specific cardinal orientation.
    pub fn align_cardinal(&mut self, yaw: f32, pitch: f32) {
        self.yaw = yaw;
        self.pitch = pitch.clamp(-1.45, 1.45);
    }
}

/// 2D pan/zoom viewport camera.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera2D {
    pub pan: Vec2,
    pub zoom: f32,
}

impl Default for Camera2D {
    fn default() -> Self {
        Self {
            pan: Vec2::ZERO,
            zoom: 1.0,
        }
    }
}

impl Camera2D {
    /// Transforms 2D world coords to screen pixels.
    pub fn to_screen(&self, world_pos: Vec2, rect: Rect) -> Pos2 {
        let center = rect.center();
        Pos2::new(
            center.x + (world_pos.x + self.pan.x) * self.zoom,
            center.y + (world_pos.y + self.pan.y) * self.zoom,
        )
    }

    /// Transforms screen pixels to 2D world coords.
    pub fn to_world(&self, screen_pos: Pos2, rect: Rect) -> Vec2 {
        let center = rect.center();
        Vec2::new(
            (screen_pos.x - center.x) / self.zoom - self.pan.x,
            (screen_pos.y - center.y) / self.zoom - self.pan.y,
        )
    }
}

/// Interactive Transform Gizmo operation modes.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum GizmoMode {
    Select,
    Translate,
    Rotate,
    Scale,
}

/// Transform Gizmo axis selection.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum GizmoAxis {
    None,
    X,
    Y,
    Z,
}

/// Active drag state when manipulating an axis handle.
#[derive(Clone, Debug, PartialEq)]
pub struct GizmoDragState {
    pub node_id: NodeId,
    pub axis: GizmoAxis,
    pub mode: GizmoMode,
    pub start_mouse_pos: Pos2,
    pub initial_value: IrValue,
}
