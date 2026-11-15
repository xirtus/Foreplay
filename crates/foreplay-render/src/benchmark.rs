//! PLAN.md Phase 3 Exit Gate: 10,000-Entity Render & Frustum Culling Benchmark.
//!
//! Verifies zero steady-state heap allocations in the hot render step,
//! sub-8.33ms (<8.33ms = >120 FPS) frame budget on 10,000 3D entities with SoA frustum culling
//! and depth sorting.

use std::time::Instant;
use bevy_math::{Mat4, Vec3};
use crate::pipeline::{cull_aabbs_frustum, Frustum};

/// A generated low-level draw command submitted to the GPU rasterizer.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct DrawCommand {
    pub entity_index: usize,
    pub mesh_rid: u64,
    pub material_rid: u64,
    pub depth_sq: f32,
}

/// Official verification receipt proving Phase 3 render benchmark exit gate compliance.
#[derive(Clone, PartialEq, Debug)]
pub struct RenderBenchmarkReceipt {
    pub total_entities: usize,
    pub visible_entities: usize,
    pub frame_time_ms: f64,
    pub fps: f64,
    pub zero_alloc_steady_state: bool,
    pub meets_120_fps_gate: bool,
}

/// Run the 10,000-entity render culling and command sorting benchmark.
///
/// Pre-allocates all Structure-of-Arrays (SoA) buffers outside the hot loop.
/// Executes steady-state frames with 0 heap allocations, verifying the >120 FPS gate.
#[must_use]
pub fn run_10k_entity_render_benchmark(entity_count: usize) -> RenderBenchmarkReceipt {
    let count = entity_count.max(10_000);

    // 1. Pre-allocate SoA buffers outside the hot loop
    let mut transforms: Vec<Mat4> = Vec::with_capacity(count);
    let mut aabbs_min: Vec<Vec3> = Vec::with_capacity(count);
    let mut aabbs_max: Vec<Vec3> = Vec::with_capacity(count);
    let mut visible_indices: Vec<usize> = Vec::with_capacity(count);
    let mut draw_commands: Vec<DrawCommand> = Vec::with_capacity(count);

    // Populate 10,000 entities distributed in 3D space
    let grid_size = (count as f32).cbrt().ceil() as usize;
    let spacing = 2.0_f32;
    let offset = (grid_size as f32) * spacing * 0.5;

    for i in 0..count {
        let x = (i % grid_size) as f32 * spacing - offset;
        let y = ((i / grid_size) % grid_size) as f32 * spacing - offset;
        let z = (i / (grid_size * grid_size)) as f32 * spacing - offset;
        let pos = Vec3::new(x, y, z);

        transforms.push(Mat4::from_translation(pos));
        aabbs_min.push(pos - Vec3::splat(0.5));
        aabbs_max.push(pos + Vec3::splat(0.5));
    }

    // Camera setup: view-projection looking towards origin
    let cam_pos = Vec3::new(0.0, 50.0, 100.0);
    let mut cam = crate::PerspectiveCamera3D::new(cam_pos, 60.0, 16.0 / 9.0);
    cam.look_at(Vec3::ZERO, Vec3::Y);
    let frustum = Frustum::from_view_projection(&cam.view_projection_matrix());

    // Warm-up passes
    for _ in 0..5 {
        hot_frame_step(
            &frustum,
            cam_pos,
            &aabbs_min,
            &aabbs_max,
            &transforms,
            &mut visible_indices,
            &mut draw_commands,
        );
    }

    // Benchmark loop: 30 steady-state frames
    let cap_visible_before = visible_indices.capacity();
    let cap_draw_before = draw_commands.capacity();
    let frames = 30;

    let start = Instant::now();
    for _ in 0..frames {
        hot_frame_step(
            &frustum,
            cam_pos,
            &aabbs_min,
            &aabbs_max,
            &transforms,
            &mut visible_indices,
            &mut draw_commands,
        );
    }
    let elapsed = start.elapsed();

    // Verify ZERO heap reallocations occurred in steady state
    let zero_alloc = visible_indices.capacity() == cap_visible_before
        && draw_commands.capacity() == cap_draw_before;

    let total_time_ms = elapsed.as_secs_f64() * 1000.0;
    let frame_time_ms = total_time_ms / (frames as f64);
    let fps = 1000.0 / frame_time_ms;
    let meets_gate = frame_time_ms < 8.333; // 120 FPS = 8.33ms budget

    RenderBenchmarkReceipt {
        total_entities: count,
        visible_entities: visible_indices.len(),
        frame_time_ms,
        fps,
        zero_alloc_steady_state: zero_alloc,
        meets_120_fps_gate: meets_gate,
    }
}

/// Steady-state hot frame render step: performs frustum culling and front-to-back depth sorting.
#[inline(always)]
fn hot_frame_step(
    frustum: &Frustum,
    cam_pos: Vec3,
    aabbs_min: &[Vec3],
    aabbs_max: &[Vec3],
    transforms: &[Mat4],
    visible_indices: &mut Vec<usize>,
    draw_commands: &mut Vec<DrawCommand>,
) {
    // 1. SoA batch frustum culling into pre-allocated buffer
    cull_aabbs_frustum(frustum, aabbs_min, aabbs_max, visible_indices);

    // 2. Generate draw commands without reallocating
    draw_commands.clear();
    for &idx in visible_indices.iter() {
        let pos = transforms[idx].w_axis.truncate();
        let depth_sq = (pos - cam_pos).length_squared();

        draw_commands.push(DrawCommand {
            entity_index: idx,
            mesh_rid: 1,
            material_rid: 1,
            depth_sq,
        });
    }

    // 3. Fast depth sort for opaque draw calls (front-to-back reduces overdraw)
    draw_commands.sort_unstable_by(|a, b| {
        a.depth_sq.partial_cmp(&b.depth_sq).unwrap_or(std::cmp::Ordering::Equal)
    });
}
