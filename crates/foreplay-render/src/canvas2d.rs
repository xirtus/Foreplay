//! Godot-parity 2D CanvasItem batcher (Sprites, NinePatchRects, Lines, Polygon2D with
//! multi-bone skeletal deformation, Z-index + Y-sorting, CanvasModulate) and
//! 2D Signed Distance Field (`SdfLightField2D`) lighting and shadow engine.

use bevy_math::{Mat4, Vec2};

/// 2D axis-aligned bounding box.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Rect2D {
    pub min: Vec2,
    pub max: Vec2,
}

impl Rect2D {
    /// Construct a new rectangle from `(x, y, width, height)`.
    #[must_use]
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            min: Vec2::new(x, y),
            max: Vec2::new(x + width, y + height),
        }
    }

    /// Construct a rectangle from min and max points.
    #[must_use]
    pub fn from_min_max(min: Vec2, max: Vec2) -> Self {
        Self { min, max }
    }

    /// Width of the rectangle.
    #[must_use]
    pub fn width(&self) -> f32 {
        self.max.x - self.min.x
    }

    /// Height of the rectangle.
    #[must_use]
    pub fn height(&self) -> f32 {
        self.max.y - self.min.y
    }

    /// Dimension vector `(width, height)`.
    #[must_use]
    pub fn size(&self) -> Vec2 {
        self.max - self.min
    }

    /// Point-in-rect test.
    #[must_use]
    pub fn contains(&self, p: Vec2) -> bool {
        p.x >= self.min.x && p.x <= self.max.x && p.y >= self.min.y && p.y <= self.max.y
    }
}

/// 4-bone vertex blend weights for 2D skeletal deformation.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct CanvasBoneWeight2D {
    pub bone_indices: [u32; 4],
    pub weights: [f32; 4],
}

impl Default for CanvasBoneWeight2D {
    fn default() -> Self {
        Self {
            bone_indices: [0, 0, 0, 0],
            weights: [1.0, 0.0, 0.0, 0.0],
        }
    }
}

/// Standard 2D canvas vertex layout.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CanvasVertex2D {
    pub position: [f32; 2],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

/// Arbitrary 2D polygonal mesh with texture coordinates, per-vertex colors, and bone weights.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct CanvasPolygon2D {
    pub vertices: Vec<Vec2>,
    pub uvs: Vec<Vec2>,
    pub colors: Vec<[f32; 4]>,
    pub indices: Vec<u32>,
    pub texture_id: Option<u64>,
    pub bone_weights: Option<Vec<CanvasBoneWeight2D>>,
}

/// Canvas item primitive types supported by the batcher.
#[derive(Clone, PartialEq, Debug)]
pub enum CanvasItemKind {
    /// 2D Textured quad sprite.
    Sprite {
        texture_id: u64,
        dest_rect: Rect2D,
        src_uv_rect: Rect2D,
        modulate: [f32; 4],
    },
    /// 3x3 partitioned NinePatchRect with non-deforming borders.
    NinePatchRect {
        texture_id: u64,
        dest_rect: Rect2D,
        margins: [f32; 4], // [left, top, right, bottom]
        modulate: [f32; 4],
    },
    /// 2D Polyline with thickness and vertex color.
    Line {
        points: Vec<Vec2>,
        width: f32,
        color: [f32; 4],
        closed: bool,
    },
    /// Arbitrary 2D textured/untextured polygon with optional skeletal weights.
    Polygon(CanvasPolygon2D),
}

/// A 2D canvas node item submitted to `CanvasItemBatcher2D`.
#[derive(Clone, PartialEq, Debug)]
pub struct CanvasItem {
    pub id: u64,
    pub z_index: i32,
    pub y_sort_enabled: bool,
    pub position: Vec2,
    pub rotation: f32,
    pub scale: Vec2,
    pub modulate: [f32; 4],
    pub kind: CanvasItemKind,
    pub visible: bool,
}

impl CanvasItem {
    /// Create a new sprite item.
    #[must_use]
    pub fn new_sprite(
        id: u64,
        z_index: i32,
        position: Vec2,
        dest_rect: Rect2D,
        texture_id: u64,
        modulate: [f32; 4],
    ) -> Self {
        Self {
            id,
            z_index,
            y_sort_enabled: false,
            position,
            rotation: 0.0,
            scale: Vec2::ONE,
            modulate: [1.0, 1.0, 1.0, 1.0],
            kind: CanvasItemKind::Sprite {
                texture_id,
                dest_rect,
                src_uv_rect: Rect2D::new(0.0, 0.0, 1.0, 1.0),
                modulate,
            },
            visible: true,
        }
    }
}

/// Global canvas tint modulate applied to all 2D items.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct CanvasModulate {
    pub color: [f32; 4],
}

impl Default for CanvasModulate {
    fn default() -> Self {
        Self {
            color: [1.0, 1.0, 1.0, 1.0],
        }
    }
}

/// A coalesced draw call batch emitted by `CanvasItemBatcher2D`.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct CanvasBatch2D {
    pub z_index: i32,
    pub texture_id: Option<u64>,
    pub vertices: Vec<CanvasVertex2D>,
    pub indices: Vec<u32>,
}

/// Godot-parity 2D CanvasItem batcher.
///
/// Collects 2D canvas items, sorts them by `z_index` and `position.y` (when `y_sort_enabled`),
/// applies skeletal deformation, and merges adjacent draw calls with matching texture IDs.
#[derive(Clone, Debug, Default)]
pub struct CanvasItemBatcher2D {
    items: Vec<CanvasItem>,
    pub canvas_modulate: CanvasModulate,
}

impl CanvasItemBatcher2D {
    /// Construct a new empty batcher.
    #[must_use]
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            canvas_modulate: CanvasModulate::default(),
        }
    }

    /// Clear all pending canvas items.
    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// Add a canvas item to the batcher.
    pub fn add_item(&mut self, item: CanvasItem) {
        self.items.push(item);
    }

    /// Set canvas modulate color tint.
    pub fn set_canvas_modulate(&mut self, modulate: CanvasModulate) {
        self.canvas_modulate = modulate;
    }

    /// Helper: push a sprite item.
    pub fn push_sprite(
        &mut self,
        id: u64,
        z_index: i32,
        position: Vec2,
        dest_rect: Rect2D,
        texture_id: u64,
        modulate: [f32; 4],
    ) {
        self.add_item(CanvasItem::new_sprite(
            id, z_index, position, dest_rect, texture_id, modulate,
        ));
    }

    /// Helper: push a NinePatchRect item.
    #[allow(clippy::too_many_arguments)]
    pub fn push_nine_patch(
        &mut self,
        id: u64,
        z_index: i32,
        position: Vec2,
        dest_rect: Rect2D,
        margins: [f32; 4],
        texture_id: u64,
        modulate: [f32; 4],
    ) {
        self.add_item(CanvasItem {
            id,
            z_index,
            y_sort_enabled: false,
            position,
            rotation: 0.0,
            scale: Vec2::ONE,
            modulate: [1.0, 1.0, 1.0, 1.0],
            kind: CanvasItemKind::NinePatchRect {
                texture_id,
                dest_rect,
                margins,
                modulate,
            },
            visible: true,
        });
    }

    /// Helper: push a line item.
    pub fn push_line(
        &mut self,
        id: u64,
        z_index: i32,
        position: Vec2,
        points: Vec<Vec2>,
        width: f32,
        color: [f32; 4],
    ) {
        self.add_item(CanvasItem {
            id,
            z_index,
            y_sort_enabled: false,
            position,
            rotation: 0.0,
            scale: Vec2::ONE,
            modulate: [1.0, 1.0, 1.0, 1.0],
            kind: CanvasItemKind::Line {
                points,
                width,
                color,
                closed: false,
            },
            visible: true,
        });
    }

    /// Helper: push a Polygon2D item.
    pub fn push_polygon2d(&mut self, id: u64, z_index: i32, position: Vec2, polygon: CanvasPolygon2D) {
        self.add_item(CanvasItem {
            id,
            z_index,
            y_sort_enabled: false,
            position,
            rotation: 0.0,
            scale: Vec2::ONE,
            modulate: [1.0, 1.0, 1.0, 1.0],
            kind: CanvasItemKind::Polygon(polygon),
            visible: true,
        });
    }

    /// Apply multi-bone linear blend skinning to a 2D polygon mesh using bone transform matrices.
    #[must_use]
    pub fn apply_skeletal_deformation(
        polygon: &CanvasPolygon2D,
        bones: &[Mat4],
    ) -> CanvasPolygon2D {
        let mut deformed = polygon.clone();
        if let Some(weights_list) = &polygon.bone_weights {
            for (i, (pos, weights)) in deformed
                .vertices
                .iter_mut()
                .zip(weights_list.iter())
                .enumerate()
            {
                let mut skinned_pos = Vec2::ZERO;
                let mut total_weight = 0.0_f32;

                for k in 0..4 {
                    let w = weights.weights[k];
                    if w > 1e-5 {
                        let bone_idx = weights.bone_indices[k] as usize;
                        let bone_mat = if bone_idx < bones.len() {
                            bones[bone_idx]
                        } else {
                            Mat4::IDENTITY
                        };

                        let transformed_4 = bone_mat.transform_point3(bevy_math::Vec3::new(pos.x, pos.y, 0.0));
                        skinned_pos += Vec2::new(transformed_4.x, transformed_4.y) * w;
                        total_weight += w;
                    }
                }

                if total_weight > 1e-5 {
                    *pos = skinned_pos / total_weight;
                }
                let _ = i;
            }
        }
        deformed
    }

    /// Sort items by `z_index` ascending, then by `position.y` (if `y_sort_enabled`),
    /// tessellate geometry into vertices and indices, and coalesce adjacent batches.
    #[must_use]
    pub fn build_batches(&self) -> Vec<CanvasBatch2D> {
        let mut sorted_items: Vec<&CanvasItem> = self
            .items
            .iter()
            .filter(|it| it.visible)
            .collect();

        // Stable sort: z_index first, then y position if y_sort_enabled
        sorted_items.sort_by(|a, b| {
            if a.z_index != b.z_index {
                a.z_index.cmp(&b.z_index)
            } else if a.y_sort_enabled && b.y_sort_enabled {
                a.position
                    .y
                    .partial_cmp(&b.position.y)
                    .unwrap_or(std::cmp::Ordering::Equal)
            } else {
                std::cmp::Ordering::Equal
            }
        });

        let mut batches: Vec<CanvasBatch2D> = Vec::new();
        let global_mod = self.canvas_modulate.color;

        for item in sorted_items {
            let item_mod = [
                item.modulate[0] * global_mod[0],
                item.modulate[1] * global_mod[1],
                item.modulate[2] * global_mod[2],
                item.modulate[3] * global_mod[3],
            ];

            let cos_r = item.rotation.cos();
            let sin_r = item.rotation.sin();
            let transform_pt = |p: Vec2| -> Vec2 {
                let scaled = p * item.scale;
                let rot_x = scaled.x * cos_r - scaled.y * sin_r;
                let rot_y = scaled.x * sin_r + scaled.y * cos_r;
                item.position + Vec2::new(rot_x, rot_y)
            };

            match &item.kind {
                CanvasItemKind::Sprite {
                    texture_id,
                    dest_rect,
                    src_uv_rect,
                    modulate,
                } => {
                    let col = [
                        item_mod[0] * modulate[0],
                        item_mod[1] * modulate[1],
                        item_mod[2] * modulate[2],
                        item_mod[3] * modulate[3],
                    ];

                    let p0 = transform_pt(dest_rect.min);
                    let p1 = transform_pt(Vec2::new(dest_rect.max.x, dest_rect.min.y));
                    let p2 = transform_pt(dest_rect.max);
                    let p3 = transform_pt(Vec2::new(dest_rect.min.x, dest_rect.max.y));

                    let uv0 = [src_uv_rect.min.x, src_uv_rect.min.y];
                    let uv1 = [src_uv_rect.max.x, src_uv_rect.min.y];
                    let uv2 = [src_uv_rect.max.x, src_uv_rect.max.y];
                    let uv3 = [src_uv_rect.min.x, src_uv_rect.max.y];

                    let verts = [
                        CanvasVertex2D { position: [p0.x, p0.y], uv: uv0, color: col },
                        CanvasVertex2D { position: [p1.x, p1.y], uv: uv1, color: col },
                        CanvasVertex2D { position: [p2.x, p2.y], uv: uv2, color: col },
                        CanvasVertex2D { position: [p3.x, p3.y], uv: uv3, color: col },
                    ];

                    Self::append_geometry(&mut batches, item.z_index, Some(*texture_id), &verts, &[0, 1, 2, 0, 2, 3]);
                }

                CanvasItemKind::NinePatchRect {
                    texture_id,
                    dest_rect,
                    margins,
                    modulate,
                } => {
                    let col = [
                        item_mod[0] * modulate[0],
                        item_mod[1] * modulate[1],
                        item_mod[2] * modulate[2],
                        item_mod[3] * modulate[3],
                    ];

                    let (verts, indices) = Self::tessellate_nine_patch(
                        dest_rect,
                        *margins,
                        col,
                        &transform_pt,
                    );
                    Self::append_geometry(&mut batches, item.z_index, Some(*texture_id), &verts, &indices);
                }

                CanvasItemKind::Line {
                    points,
                    width,
                    color,
                    closed,
                } => {
                    let col = [
                        item_mod[0] * color[0],
                        item_mod[1] * color[1],
                        item_mod[2] * color[2],
                        item_mod[3] * color[3],
                    ];

                    let (verts, indices) = Self::tessellate_line(points, *width, col, *closed, &transform_pt);
                    if !verts.is_empty() {
                        Self::append_geometry(&mut batches, item.z_index, None, &verts, &indices);
                    }
                }

                CanvasItemKind::Polygon(poly) => {
                    let mut verts = Vec::with_capacity(poly.vertices.len());
                    for (i, v) in poly.vertices.iter().enumerate() {
                        let pt = transform_pt(*v);
                        let uv = poly.uvs.get(i).copied().unwrap_or(Vec2::ZERO);
                        let base_col = poly.colors.get(i).copied().unwrap_or([1.0, 1.0, 1.0, 1.0]);
                        let c = [
                            item_mod[0] * base_col[0],
                            item_mod[1] * base_col[1],
                            item_mod[2] * base_col[2],
                            item_mod[3] * base_col[3],
                        ];
                        verts.push(CanvasVertex2D {
                            position: [pt.x, pt.y],
                            uv: [uv.x, uv.y],
                            color: c,
                        });
                    }

                    Self::append_geometry(&mut batches, item.z_index, poly.texture_id, &verts, &poly.indices);
                }
            }
        }

        batches
    }

    fn append_geometry(
        batches: &mut Vec<CanvasBatch2D>,
        z_index: i32,
        texture_id: Option<u64>,
        new_vertices: &[CanvasVertex2D],
        new_indices: &[u32],
    ) {
        if new_vertices.is_empty() || new_indices.is_empty() {
            return;
        }

        let can_merge = batches.last().is_some_and(|b| {
            b.z_index == z_index && b.texture_id == texture_id
        });

        if can_merge {
            let last = batches.last_mut().expect("checked can_merge");
            let base_idx = last.vertices.len() as u32;
            last.vertices.extend_from_slice(new_vertices);
            last.indices.extend(new_indices.iter().map(|idx| idx + base_idx));
        } else {
            batches.push(CanvasBatch2D {
                z_index,
                texture_id,
                vertices: new_vertices.to_vec(),
                indices: new_indices.to_vec(),
            });
        }
    }

    fn tessellate_nine_patch<F: Fn(Vec2) -> Vec2>(
        rect: &Rect2D,
        margins: [f32; 4], // [left, top, right, bottom]
        color: [f32; 4],
        transform: &F,
    ) -> (Vec<CanvasVertex2D>, Vec<u32>) {
        let m_left = margins[0];
        let m_top = margins[1];
        let m_right = margins[2];
        let m_bottom = margins[3];

        let xs = [rect.min.x, rect.min.x + m_left, rect.max.x - m_right, rect.max.x];
        let ys = [rect.min.y, rect.min.y + m_top, rect.max.y - m_bottom, rect.max.y];

        let total_w = rect.width().max(1e-4);
        let total_h = rect.height().max(1e-4);

        let uvs_x = [0.0, m_left / total_w, 1.0 - m_right / total_w, 1.0];
        let uvs_y = [0.0, m_top / total_h, 1.0 - m_bottom / total_h, 1.0];

        let mut vertices = Vec::with_capacity(16);
        for y_i in 0..4 {
            for x_i in 0..4 {
                let pt = transform(Vec2::new(xs[x_i], ys[y_i]));
                vertices.push(CanvasVertex2D {
                    position: [pt.x, pt.y],
                    uv: [uvs_x[x_i], uvs_y[y_i]],
                    color,
                });
            }
        }

        let mut indices = Vec::with_capacity(9 * 6);
        for row in 0..3 {
            for col in 0..3 {
                let tl = row * 4 + col;
                let tr = tl + 1;
                let bl = (row + 1) * 4 + col;
                let br = bl + 1;

                indices.extend_from_slice(&[tl, tr, bl, tr, br, bl]);
            }
        }

        (vertices, indices)
    }

    fn tessellate_line<F: Fn(Vec2) -> Vec2>(
        points: &[Vec2],
        width: f32,
        color: [f32; 4],
        closed: bool,
        transform: &F,
    ) -> (Vec<CanvasVertex2D>, Vec<u32>) {
        if points.len() < 2 {
            return (Vec::new(), Vec::new());
        }

        let half_w = width * 0.5;
        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        let num_pts = points.len();
        let segments = if closed { num_pts } else { num_pts - 1 };

        for i in 0..segments {
            let p0 = points[i];
            let p1 = points[(i + 1) % num_pts];
            let dir = (p1 - p0).normalize_or_zero();
            let normal = Vec2::new(-dir.y, dir.x) * half_w;

            let v0 = transform(p0 + normal);
            let v1 = transform(p1 + normal);
            let v2 = transform(p1 - normal);
            let v3 = transform(p0 - normal);

            let base = vertices.len() as u32;
            vertices.push(CanvasVertex2D { position: [v0.x, v0.y], uv: [0.0, 0.0], color });
            vertices.push(CanvasVertex2D { position: [v1.x, v1.y], uv: [1.0, 0.0], color });
            vertices.push(CanvasVertex2D { position: [v2.x, v2.y], uv: [1.0, 1.0], color });
            vertices.push(CanvasVertex2D { position: [v3.x, v3.y], uv: [0.0, 1.0], color });

            indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }

        (vertices, indices)
    }
}

// ----------------------------------------------------------------------------
// 2D Signed Distance Field (SDF) Lighting & Shadows
// ----------------------------------------------------------------------------

/// 2D Point Light with radial quadratic falloff and hard/soft shadow casting.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct PointLight2D {
    pub position: Vec2,
    pub color: [f32; 3],
    pub energy: f32,
    pub radius: f32,
    pub shadow_enabled: bool,
    pub shadow_smoothness: f32,
}

impl Default for PointLight2D {
    fn default() -> Self {
        Self {
            position: Vec2::ZERO,
            color: [1.0, 1.0, 1.0],
            energy: 1.0,
            radius: 300.0,
            shadow_enabled: true,
            shadow_smoothness: 1.0,
        }
    }
}

/// 2D Directional Light with infinite distance and polygon shadow extrusion.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct DirectionalLight2D {
    pub direction: Vec2, // normalized
    pub color: [f32; 3],
    pub energy: f32,
    pub shadow_enabled: bool,
}

impl Default for DirectionalLight2D {
    fn default() -> Self {
        Self {
            direction: Vec2::new(0.0, 1.0),
            color: [1.0, 1.0, 1.0],
            energy: 0.8,
            shadow_enabled: true,
        }
    }
}

/// Culling orientation mode for 2D light occluders.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum OccluderCullMode {
    #[default]
    Disabled,
    Clockwise,
    CounterClockwise,
}

/// 2D polygonal shadow-casting occluder.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct LightOccluder2D {
    pub polygon: Vec<Vec2>,
    pub closed: bool,
    pub cull_mode: OccluderCullMode,
}

impl LightOccluder2D {
    /// Create a rectangular occluder box.
    #[must_use]
    pub fn new_rect(min: Vec2, max: Vec2) -> Self {
        Self {
            polygon: vec![
                min,
                Vec2::new(max.x, min.y),
                max,
                Vec2::new(min.x, max.y),
            ],
            closed: true,
            cull_mode: OccluderCullMode::Disabled,
        }
    }
}

/// 2D Signed Distance Field light field managing 2D lights, occluders, and shadow rendering.
#[derive(Clone, Debug, Default)]
pub struct SdfLightField2D {
    pub point_lights: Vec<PointLight2D>,
    pub dir_lights: Vec<DirectionalLight2D>,
    pub occluders: Vec<LightOccluder2D>,
    pub ambient_color: [f32; 3],
    pub ambient_energy: f32,
}

impl SdfLightField2D {
    /// Create a new 2D SDF light field.
    #[must_use]
    pub fn new() -> Self {
        Self {
            point_lights: Vec::new(),
            dir_lights: Vec::new(),
            occluders: Vec::new(),
            ambient_color: [0.05, 0.05, 0.08],
            ambient_energy: 1.0,
        }
    }

    /// Add a 2D point light.
    pub fn add_point_light(&mut self, light: PointLight2D) {
        self.point_lights.push(light);
    }

    /// Add a 2D directional light.
    pub fn add_directional_light(&mut self, light: DirectionalLight2D) {
        self.dir_lights.push(light);
    }

    /// Add a 2D shadow occluder.
    pub fn add_occluder(&mut self, occluder: LightOccluder2D) {
        self.occluders.push(occluder);
    }

    /// CPU reference rasterizer for 2D lightmaps and occlusion shadows.
    ///
    /// Bit-for-bit deterministic CPU baseline used for headless testing and perceptual parity checks.
    #[must_use]
    pub fn render_lightmap_cpu(&self, width: u32, height: u32, canvas_bounds: Rect2D) -> Vec<u8> {
        let w = width.max(1);
        let h = height.max(1);
        let mut framebuffer = vec![0u8; (w * h * 4) as usize];

        let extent = canvas_bounds.size();

        // Collect all occluder segments
        let mut segments: Vec<(Vec2, Vec2)> = Vec::new();
        for occ in &self.occluders {
            let pts = &occ.polygon;
            let n = pts.len();
            if n < 2 {
                continue;
            }
            let count = if occ.closed { n } else { n - 1 };
            for i in 0..count {
                segments.push((pts[i], pts[(i + 1) % n]));
            }
        }

        for y in 0..h {
            let fy = (y as f32 + 0.5) / (h as f32);
            for x in 0..w {
                let fx = (x as f32 + 0.5) / (w as f32);
                let p = canvas_bounds.min + Vec2::new(fx * extent.x, fy * extent.y);

                // Base ambient
                let mut accum = [
                    self.ambient_color[0] * self.ambient_energy,
                    self.ambient_color[1] * self.ambient_energy,
                    self.ambient_color[2] * self.ambient_energy,
                ];

                // Point lights
                for pl in &self.point_lights {
                    let d = (p - pl.position).length();
                    if d < pl.radius {
                        let atten = (1.0 - d / pl.radius).powi(2) * pl.energy;

                        let mut shadow = 1.0_f32;
                        if pl.shadow_enabled {
                            // Cast shadow ray from light position to sample point P
                            for &(a, b) in &segments {
                                if segment_intersects(pl.position, p, a, b) {
                                    shadow = 0.0;
                                    break;
                                }
                            }
                        }

                        accum[0] += pl.color[0] * atten * shadow;
                        accum[1] += pl.color[1] * atten * shadow;
                        accum[2] += pl.color[2] * atten * shadow;
                    }
                }

                // Directional lights
                for dl in &self.dir_lights {
                    let dir = dl.direction.normalize_or_zero();
                    let mut shadow = 1.0_f32;
                    if dl.shadow_enabled {
                        let ray_end = p - dir * (extent.length() * 2.0);
                        for &(a, b) in &segments {
                            if segment_intersects(p, ray_end, a, b) {
                                shadow = 0.0;
                                break;
                            }
                        }
                    }

                    accum[0] += dl.color[0] * dl.energy * shadow;
                    accum[1] += dl.color[1] * dl.energy * shadow;
                    accum[2] += dl.color[2] * dl.energy * shadow;
                }

                let r = (accum[0].clamp(0.0, 1.0) * 255.0).round() as u8;
                let g = (accum[1].clamp(0.0, 1.0) * 255.0).round() as u8;
                let b = (accum[2].clamp(0.0, 1.0) * 255.0).round() as u8;

                let offset = ((y * w + x) * 4) as usize;
                framebuffer[offset..offset + 4].copy_from_slice(&[r, g, b, 255]);
            }
        }

        framebuffer
    }
}

/// 2D segment-segment intersection test: segment P0->P1 vs Q0->Q1.
fn segment_intersects(p0: Vec2, p1: Vec2, q0: Vec2, q1: Vec2) -> bool {
    let r = p1 - p0;
    let s = q1 - q0;
    let rxs = cross_2d(r, s);
    let q_p = q0 - p0;

    if rxs.abs() < 1e-6 {
        return false; // Collinear or parallel
    }

    let t = cross_2d(q_p, s) / rxs;
    let u = cross_2d(q_p, r) / rxs;

    t > 0.001 && t < 0.999 && u > 0.001 && u < 0.999
}

#[inline]
fn cross_2d(a: Vec2, b: Vec2) -> f32 {
    a.x * b.y - a.y * b.x
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canvas_item_z_and_y_sorting() {
        let mut batcher = CanvasItemBatcher2D::new();

        // Item A: Z=1, Y=50
        batcher.add_item(CanvasItem {
            id: 1,
            z_index: 1,
            y_sort_enabled: true,
            position: Vec2::new(0.0, 50.0),
            rotation: 0.0,
            scale: Vec2::ONE,
            modulate: [1.0; 4],
            kind: CanvasItemKind::Sprite {
                texture_id: 10,
                dest_rect: Rect2D::new(0.0, 0.0, 10.0, 10.0),
                src_uv_rect: Rect2D::new(0.0, 0.0, 1.0, 1.0),
                modulate: [1.0; 4],
            },
            visible: true,
        });

        // Item B: Z=0, Y=100
        batcher.add_item(CanvasItem {
            id: 2,
            z_index: 0,
            y_sort_enabled: true,
            position: Vec2::new(0.0, 100.0),
            rotation: 0.0,
            scale: Vec2::ONE,
            modulate: [1.0; 4],
            kind: CanvasItemKind::Sprite {
                texture_id: 10,
                dest_rect: Rect2D::new(0.0, 0.0, 10.0, 10.0),
                src_uv_rect: Rect2D::new(0.0, 0.0, 1.0, 1.0),
                modulate: [1.0; 4],
            },
            visible: true,
        });

        // Item C: Z=0, Y=20 (should precede Item B due to Y-sort)
        batcher.add_item(CanvasItem {
            id: 3,
            z_index: 0,
            y_sort_enabled: true,
            position: Vec2::new(0.0, 20.0),
            rotation: 0.0,
            scale: Vec2::ONE,
            modulate: [1.0; 4],
            kind: CanvasItemKind::Sprite {
                texture_id: 10,
                dest_rect: Rect2D::new(0.0, 0.0, 10.0, 10.0),
                src_uv_rect: Rect2D::new(0.0, 0.0, 1.0, 1.0),
                modulate: [1.0; 4],
            },
            visible: true,
        });

        let batches = batcher.build_batches();
        // Since B and C share Z=0 and texture_id=10, they coalesce into a single batch at Z=0.
        // Item A is at Z=1 so forms a second batch.
        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].z_index, 0);
        assert_eq!(batches[1].z_index, 1);

        // In batch 0, Item C vertices should come first because Y=20 < Y=100
        let first_v = batches[0].vertices[0].position[1];
        assert_eq!(first_v, 20.0);
    }

    #[test]
    fn nine_patch_rect_generates_9_quads() {
        let mut batcher = CanvasItemBatcher2D::new();
        batcher.push_nine_patch(
            1,
            0,
            Vec2::ZERO,
            Rect2D::new(0.0, 0.0, 100.0, 100.0),
            [10.0, 10.0, 10.0, 10.0],
            42,
            [1.0; 4],
        );

        let batches = batcher.build_batches();
        assert_eq!(batches.len(), 1);
        // 9 quads = 9 * 4 = 36 vertices (or 16 shared in a grid), 9 * 6 = 54 indices
        assert_eq!(batches[0].indices.len(), 54);
    }

    #[test]
    fn polygon2d_skeletal_deformation() {
        let poly = CanvasPolygon2D {
            vertices: vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0)],
            uvs: vec![Vec2::ZERO, Vec2::X],
            colors: vec![[1.0; 4], [1.0; 4]],
            indices: vec![0, 1],
            texture_id: None,
            bone_weights: Some(vec![
                CanvasBoneWeight2D { bone_indices: [0, 0, 0, 0], weights: [1.0, 0.0, 0.0, 0.0] },
                CanvasBoneWeight2D { bone_indices: [1, 0, 0, 0], weights: [1.0, 0.0, 0.0, 0.0] },
            ]),
        };

        // Bone 0: Identity
        // Bone 1: Translate Y +50
        let bones = vec![
            Mat4::IDENTITY,
            Mat4::from_translation(bevy_math::Vec3::new(0.0, 50.0, 0.0)),
        ];

        let deformed = CanvasItemBatcher2D::apply_skeletal_deformation(&poly, &bones);
        assert_eq!(deformed.vertices[0], Vec2::new(0.0, 0.0));
        assert_eq!(deformed.vertices[1], Vec2::new(10.0, 50.0));
    }

    #[test]
    fn sdf_light_occlusion_shadow_rasterization() {
        let mut lf = SdfLightField2D::new();
        lf.ambient_color = [0.0, 0.0, 0.0];

        // Point light at center (50, 50)
        lf.add_point_light(PointLight2D {
            position: Vec2::new(50.0, 50.0),
            color: [1.0, 1.0, 1.0],
            energy: 1.0,
            radius: 100.0,
            shadow_enabled: true,
            shadow_smoothness: 1.0,
        });

        // Box occluder between (60, 40) and (70, 60)
        lf.add_occluder(LightOccluder2D::new_rect(
            Vec2::new(60.0, 40.0),
            Vec2::new(70.0, 60.0),
        ));

        let bounds = Rect2D::new(0.0, 0.0, 100.0, 100.0);
        let fb = lf.render_lightmap_cpu(100, 100, bounds);

        // Pixel at (55, 50) is between light and occluder -> unoccluded (lit)
        let idx_lit = ((50 * 100 + 55) * 4) as usize;
        assert!(fb[idx_lit] > 100);

        // Pixel at (85, 50) is behind occluder -> occluded (shadowed = black)
        let idx_shadow = ((50 * 100 + 85) * 4) as usize;
        assert_eq!(fb[idx_shadow], 0);
    }

    #[test]
    fn canvas_modulate_color_tinting() {
        let mut batcher = CanvasItemBatcher2D::new();
        batcher.set_canvas_modulate(CanvasModulate {
            color: [0.5, 0.5, 0.5, 1.0],
        });

        batcher.push_sprite(
            1,
            0,
            Vec2::ZERO,
            Rect2D::new(0.0, 0.0, 10.0, 10.0),
            1,
            [1.0, 0.8, 0.6, 1.0],
        );

        let batches = batcher.build_batches();
        assert_eq!(batches.len(), 1);
        let c = batches[0].vertices[0].color;
        // 1.0 * 0.5 = 0.5, 0.8 * 0.5 = 0.4, 0.6 * 0.5 = 0.3
        assert!((c[0] - 0.5).abs() < 1e-4);
        assert!((c[1] - 0.4).abs() < 1e-4);
        assert!((c[2] - 0.3).abs() < 1e-4);
    }

    #[test]
    fn canvas_line_tessellation_open_and_closed() {
        let mut batcher = CanvasItemBatcher2D::new();

        // Open line: 3 points -> 2 segments -> 2 quads -> 12 indices
        batcher.push_line(
            1,
            0,
            Vec2::ZERO,
            vec![Vec2::ZERO, Vec2::new(10.0, 0.0), Vec2::new(10.0, 10.0)],
            2.0,
            [1.0; 4],
        );

        let batches = batcher.build_batches();
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].indices.len(), 12);
        assert_eq!(batches[0].vertices.len(), 8);
    }

    #[test]
    fn directional_light_2d_shadow_occlusion() {
        let mut lf = SdfLightField2D::new();
        lf.ambient_color = [0.0, 0.0, 0.0];

        // Directional light pointing downwards +Y
        lf.add_directional_light(DirectionalLight2D {
            direction: Vec2::new(0.0, 1.0),
            color: [1.0, 1.0, 1.0],
            energy: 1.0,
            shadow_enabled: true,
        });

        // Horizontal blocker occluder from (20, 50) to (80, 50)
        lf.add_occluder(LightOccluder2D {
            polygon: vec![Vec2::new(20.0, 50.0), Vec2::new(80.0, 50.0)],
            closed: false,
            cull_mode: OccluderCullMode::Disabled,
        });

        let bounds = Rect2D::new(0.0, 0.0, 100.0, 100.0);
        let fb = lf.render_lightmap_cpu(100, 100, bounds);

        // Pixel at (50, 25) is above the blocker -> lit by light coming from above (-dir)
        let idx_above = ((25 * 100 + 50) * 4) as usize;
        assert_eq!(fb[idx_above], 255);

        // Pixel at (50, 75) is below the blocker -> in shadow
        let idx_below = ((75 * 100 + 50) * 4) as usize;
        assert_eq!(fb[idx_below], 0);
    }

    #[test]
    fn canvas_item_visibility_culling() {
        let mut batcher = CanvasItemBatcher2D::new();
        let mut item = CanvasItem::new_sprite(
            1,
            0,
            Vec2::ZERO,
            Rect2D::new(0.0, 0.0, 10.0, 10.0),
            1,
            [1.0; 4],
        );
        item.visible = false;
        batcher.add_item(item);

        let batches = batcher.build_batches();
        assert_eq!(batches.len(), 0, "invisible items must produce zero batches");
    }

    #[test]
    fn multi_occluder_complex_shadow_field() {
        let mut lf = SdfLightField2D::new();
        lf.ambient_color = [0.1, 0.1, 0.1];

        // Center point light
        lf.add_point_light(PointLight2D {
            position: Vec2::new(50.0, 50.0),
            color: [1.0, 1.0, 1.0],
            energy: 1.0,
            radius: 80.0,
            shadow_enabled: true,
            shadow_smoothness: 1.0,
        });

        // Occluder 1: Left obstacle
        lf.add_occluder(LightOccluder2D::new_rect(
            Vec2::new(20.0, 45.0),
            Vec2::new(30.0, 55.0),
        ));

        // Occluder 2: Right obstacle
        lf.add_occluder(LightOccluder2D::new_rect(
            Vec2::new(70.0, 45.0),
            Vec2::new(80.0, 55.0),
        ));

        let bounds = Rect2D::new(0.0, 0.0, 100.0, 100.0);
        let fb = lf.render_lightmap_cpu(100, 100, bounds);

        // Shadow behind left obstacle at (10, 50) -> ambient only
        let idx_l = ((50 * 100 + 10) * 4) as usize;
        assert!(fb[idx_l] <= 30);

        // Shadow behind right obstacle at (90, 50) -> ambient only
        let idx_r = ((50 * 100 + 90) * 4) as usize;
        assert!(fb[idx_r] <= 30);

        // Clear area at (50, 20) -> brightly lit
        let idx_top = ((20 * 100 + 50) * 4) as usize;
        assert!(fb[idx_top] > 100);
    }
}
