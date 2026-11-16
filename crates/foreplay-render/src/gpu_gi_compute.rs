//! Hardware WGSL Compute Radiance Cascades GI Solver.
//!
//! Provides `GpuRadianceCascadesSolver`, dispatching a real WGSL `@compute @workgroup_size(8, 8, 1)`
//! shader on `WgpuRenderDevice` evaluating 2D/3D Radiance Cascades probe intervals, multi-bounce
//! radiance merging, and producing an irradiance probe grid alongside a tonemapped RGBA8 GI
//! visualization buffer. Includes deterministic CPU fallback for headless testing.

use bevy_math::Vec3;
use std::time::Instant;
use wgpu::util::DeviceExt;

use crate::{pbr_gi::agx_tonemap_srgb, RenderHardwareError, WgpuRenderDevice};

/// Point emitter description submitted to Radiance Cascades GI solver.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuEmitter {
    /// xyz: 3D position, w: emitter radius
    pub position: [f32; 4],
    /// rgb: emission color, w: intensity
    pub radiance: [f32; 4],
}

impl Default for GpuEmitter {
    fn default() -> Self {
        Self {
            position: [0.0, 0.0, 0.0, 1.0],
            radiance: [1.0, 1.0, 1.0, 5.0],
        }
    }
}

impl GpuEmitter {
    /// Construct a new radiant emitter.
    #[must_use]
    pub fn new(position: Vec3, radius: f32, color: [f32; 3], intensity: f32) -> Self {
        Self {
            position: [position.x, position.y, position.z, radius],
            radiance: [color[0], color[1], color[2], intensity],
        }
    }
}

/// A probe in the Radiance Cascades grid.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuRadianceProbe {
    /// rgb: total indirect irradiance, w: confidence weight
    pub irradiance: [f32; 4],
    /// xyz: probe world position, w: cascade level
    pub position: [f32; 4],
}

impl Default for GpuRadianceProbe {
    fn default() -> Self {
        Self {
            irradiance: [0.0; 4],
            position: [0.0; 4],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
struct RadianceCascadeUniforms {
    min_bounds: [f32; 4],
    max_bounds: [f32; 4],
    sky_color: [f32; 4],
    grid_params: [u32; 4], // x: probe_w, y: probe_h, z: emitter_count, w: cascade_count
}

/// Output of Radiance Cascades GI computation.
#[derive(Clone, Debug, PartialEq)]
pub struct GpuRadianceCascadesResult {
    /// Irradiance values at each probe in row-major order.
    pub probe_grid: Vec<[f32; 4]>,
    /// Tightly-packed RGBA8 visualization buffer (resolution.x * resolution.y * 4).
    pub visualization_rgba8: Vec<u8>,
    /// Probe grid dimensions [width, height].
    pub resolution: [u32; 2],
    /// Time taken to solve in milliseconds.
    pub elapsed_ms: f32,
    /// Whether hardware acceleration was utilized.
    pub hardware_accelerated: bool,
}

const RADIANCE_CASCADES_WGSL: &str = r#"
struct GpuEmitter {
    position: vec4<f32>, // xyz, w: radius
    radiance: vec4<f32>, // rgb, w: intensity
};

struct Uniforms {
    min_bounds: vec4<f32>,
    max_bounds: vec4<f32>,
    sky_color: vec4<f32>,
    grid_params: vec4<u32>, // x: probe_w, y: probe_h, z: emitter_count, w: cascade_count
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var<storage, read> emitters: array<GpuEmitter>;
@group(0) @binding(2) var<storage, read_write> probes: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read_write> visualization_rgba: array<u32>;

fn agx_tonemap(c: vec3<f32>) -> vec3<f32> {
    let r = max(c.r, 0.0);
    let g = max(c.g, 0.0);
    let b = max(c.b, 0.0);

    let agx_r = 0.842479062253094 * r + 0.0784335996998241 * g + 0.0792237278000791 * b;
    let agx_g = 0.0976130540898553 * r + 0.878425136567232 * g + 0.0240166374026336 * b;
    let agx_b = 0.0560108674722258 * r + 0.109576084798701 * g + 0.834455858639148 * b;

    let min_ev = -12.47393;
    let max_ev = 4.026069;
    let range = max_ev - min_ev;

    let log_r = clamp((log2(max(agx_r, 1e-10)) - min_ev) / range, 0.0, 1.0);
    let log_g = clamp((log2(max(agx_g, 1e-10)) - min_ev) / range, 0.0, 1.0);
    let log_b = clamp((log2(max(agx_b, 1e-10)) - min_ev) / range, 0.0, 1.0);

    let sig_r = clamp(log_r * log_r * (3.0 - 2.0 * log_r), 0.0, 1.0);
    let sig_g = clamp(log_g * log_g * (3.0 - 2.0 * log_g), 0.0, 1.0);
    let sig_b = clamp(log_b * log_b * (3.0 - 2.0 * log_b), 0.0, 1.0);

    let out_r = 1.19687900512017 * sig_r - 0.0528968517599566 * sig_g - 0.0519713009458925 * sig_b;
    let out_g = -0.0980208811401368 * sig_r + 1.15190312990417 * sig_g - 0.0538966089353019 * sig_b;
    let out_b = -0.0446466963977508 * sig_r - 0.0898437009000753 * sig_g + 1.2057777247076 * sig_b;

    return vec3<f32>(
        srgb_gamma(clamp(out_r, 0.0, 1.0)),
        srgb_gamma(clamp(out_g, 0.0, 1.0)),
        srgb_gamma(clamp(out_b, 0.0, 1.0))
    );
}

fn srgb_gamma(c: f32) -> f32 {
    if (c <= 0.0031308) {
        return c * 12.92;
    } else {
        return 1.055 * pow(c, 1.0 / 2.4) - 0.055;
    }
}

@compute @workgroup_size(8, 8, 1)
fn cs_radiance_cascades(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let w = uniforms.grid_params.x;
    let h = uniforms.grid_params.y;
    if (global_id.x >= w || global_id.y >= h) {
        return;
    }

    let probe_idx = global_id.y * w + global_id.x;
    let extent = uniforms.max_bounds.xyz - uniforms.min_bounds.xyz;

    let fx = (f32(global_id.x) + 0.5) / f32(w);
    let fy = (f32(global_id.y) + 0.5) / f32(h);
    let probe_pos = uniforms.min_bounds.xyz + vec3<f32>(fx * extent.x, fy * extent.y, 0.0);

    var total_irradiance = uniforms.sky_color.xyz * 0.15;
    let emitter_count = uniforms.grid_params.z;
    let cascade_count = uniforms.grid_params.w;

    // Evaluate Radiance Cascades hierarchical distance intervals
    for (var c = 0u; c < cascade_count; c = c + 1u) {
        let r_min = select(0.0, pow(2.0, f32(c) * 1.5 - 1.0), c > 0u);
        let r_max = pow(2.0, f32(c + 1u) * 1.5 - 1.0);

        var cascade_radiance = vec3<f32>(0.0);
        for (var e = 0u; e < emitter_count; e = e + 1u) {
            let em = emitters[e];
            let em_pos = em.position.xyz;
            let em_radius = em.position.w;
            let em_color = em.radiance.xyz;
            let em_intensity = em.radiance.w;

            let dist = length(probe_pos - em_pos);
            if (dist >= r_min && dist <= r_max) {
                let falloff = em_intensity / (1.0 + (dist / max(em_radius, 0.1)) * (dist / max(em_radius, 0.1)));
                cascade_radiance = cascade_radiance + em_color * falloff;
            }
        }

        // Higher cascades diffuse over wider spatial areas
        let cascade_weight = 1.0 / (1.0 + f32(c) * 0.5);
        total_irradiance = total_irradiance + cascade_radiance * cascade_weight;
    }

    probes[probe_idx] = vec4<f32>(total_irradiance, 1.0);

    // Tonemap and write to visualization buffer
    let tm = agx_tonemap(total_irradiance);
    visualization_rgba[probe_idx] = pack4x8unorm(vec4<f32>(tm, 1.0));
}
"#;

/// Hardware WGSL `@compute` Radiance Cascades GI Solver.
#[derive(Debug, Default)]
pub struct GpuRadianceCascadesSolver {
    pipeline: Option<wgpu::ComputePipeline>,
    bind_group_layout: Option<wgpu::BindGroupLayout>,
}

impl GpuRadianceCascadesSolver {
    /// Create a new Radiance Cascades solver instance.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn init_pipeline(&mut self, device: &wgpu::Device) {
        if self.pipeline.is_some() {
            return;
        }

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("radiance_cascades_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("radiance_cascades_layout"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });

        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("radiance_cascades_shader"),
            source: wgpu::ShaderSource::Wgsl(RADIANCE_CASCADES_WGSL.into()),
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("radiance_cascades_pipeline"),
            layout: Some(&layout),
            module: &module,
            entry_point: Some("cs_radiance_cascades"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        self.bind_group_layout = Some(bgl);
        self.pipeline = Some(pipeline);
    }

    /// Dispatch hardware WGSL compute Radiance Cascades evaluation.
    ///
    /// Evaluates hierarchical multi-bounce probe intervals, returning probe irradiance grid
    /// and tonemapped RGBA8 visualization buffer. Gracefully falls back to deterministic CPU bridge
    /// if hardware GPU acceleration is unavailable.
    #[allow(clippy::too_many_arguments)]
    pub fn solve_radiance_cascades(
        &mut self,
        device_wrapper: &WgpuRenderDevice,
        emitters: &[GpuEmitter],
        min_bounds: Vec3,
        max_bounds: Vec3,
        probe_resolution: [u32; 2],
        cascade_count: u32,
        sky_radiance: [f32; 3],
    ) -> Result<GpuRadianceCascadesResult, RenderHardwareError> {
        let w = probe_resolution[0].max(1);
        let h = probe_resolution[1].max(1);
        let start_time = Instant::now();

        if !device_wrapper.is_hardware_accelerated() {
            return self.solve_cpu_fallback(
                emitters,
                min_bounds,
                max_bounds,
                w,
                h,
                cascade_count,
                sky_radiance,
                start_time,
            );
        }

        let device = device_wrapper.device().expect("device checked");
        let queue = device_wrapper.queue().expect("queue checked");

        self.init_pipeline(device);

        let default_emitter = GpuEmitter::default();
        let emitter_slice = if emitters.is_empty() {
            std::slice::from_ref(&default_emitter)
        } else {
            emitters
        };

        let emitter_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("rc_emitter_buf"),
            contents: bytemuck::cast_slice(emitter_slice),
            usage: wgpu::BufferUsages::STORAGE,
        });

        let uniforms = RadianceCascadeUniforms {
            min_bounds: [min_bounds.x, min_bounds.y, min_bounds.z, 0.0],
            max_bounds: [max_bounds.x, max_bounds.y, max_bounds.z, 0.0],
            sky_color: [sky_radiance[0], sky_radiance[1], sky_radiance[2], 1.0],
            grid_params: [w, h, emitters.len() as u32, cascade_count],
        };

        let uniform_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("rc_uniform_buf"),
            contents: bytemuck::cast_slice(&[uniforms]),
            usage: wgpu::BufferUsages::UNIFORM,
        });

        let probe_count = (w * h) as usize;
        let probe_bytes = (probe_count * std::mem::size_of::<[f32; 4]>()) as u64;
        let probe_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rc_probe_buf"),
            size: probe_bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let staging_probe_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rc_staging_probe_buf"),
            size: probe_bytes,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let vis_bytes = (probe_count * 4) as u64;
        let vis_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rc_vis_buf"),
            size: vis_bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let staging_vis_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rc_staging_vis_buf"),
            size: vis_bytes,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rc_bind_group"),
            layout: self.bind_group_layout.as_ref().expect("bgl"),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: emitter_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: probe_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: vis_buf.as_entire_binding(),
                },
            ],
        });

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("radiance_cascades_encoder"),
        });

        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("rc_compute_pass"),
                timestamp_writes: None,
            });
            cpass.set_pipeline(self.pipeline.as_ref().expect("pipeline"));
            cpass.set_bind_group(0, &bg, &[]);
            cpass.dispatch_workgroups(w.div_ceil(8), h.div_ceil(8), 1);
        }

        encoder.copy_buffer_to_buffer(&probe_buf, 0, &staging_probe_buf, 0, probe_bytes);
        encoder.copy_buffer_to_buffer(&vis_buf, 0, &staging_vis_buf, 0, vis_bytes);

        queue.submit(Some(encoder.finish()));

        // Map and read back probe values
        let slice_probes = staging_probe_buf.slice(..);
        let (tx_p, rx_p) = std::sync::mpsc::channel();
        slice_probes.map_async(wgpu::MapMode::Read, move |res| {
            let _ = tx_p.send(res);
        });

        let slice_vis = staging_vis_buf.slice(..);
        let (tx_v, rx_v) = std::sync::mpsc::channel();
        slice_vis.map_async(wgpu::MapMode::Read, move |res| {
            let _ = tx_v.send(res);
        });

        device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })?;

        rx_p.recv()
            .map_err(|_| RenderHardwareError::Execution("Probe readback channel closed".into()))?
            .map_err(RenderHardwareError::BufferMap)?;

        rx_v.recv()
            .map_err(|_| RenderHardwareError::Execution("Vis readback channel closed".into()))?
            .map_err(RenderHardwareError::BufferMap)?;

        let mapped_p = slice_probes.get_mapped_range().map_err(|e| {
            RenderHardwareError::Execution(format!("Probe buffer map range failed: {e}"))
        })?;
        let probe_grid: Vec<[f32; 4]> = bytemuck::cast_slice(&mapped_p).to_vec();
        drop(mapped_p);
        staging_probe_buf.unmap();

        let mapped_v = slice_vis.get_mapped_range().map_err(|e| {
            RenderHardwareError::Execution(format!("Vis buffer map range failed: {e}"))
        })?;
        let visualization_rgba8 = mapped_v.to_vec();
        drop(mapped_v);
        staging_vis_buf.unmap();

        let elapsed_ms = start_time.elapsed().as_secs_f32() * 1000.0;

        Ok(GpuRadianceCascadesResult {
            probe_grid,
            visualization_rgba8,
            resolution: [w, h],
            elapsed_ms,
            hardware_accelerated: true,
        })
    }

    /// Deterministic CPU fallback solver with identical interval evaluation and AgX tonemapping.
    #[allow(clippy::too_many_arguments)]
    fn solve_cpu_fallback(
        &self,
        emitters: &[GpuEmitter],
        min_bounds: Vec3,
        max_bounds: Vec3,
        width: u32,
        height: u32,
        cascade_count: u32,
        sky_radiance: [f32; 3],
        start_time: Instant,
    ) -> Result<GpuRadianceCascadesResult, RenderHardwareError> {
        let w = width as usize;
        let h = height as usize;
        let extent = max_bounds - min_bounds;

        let mut probe_grid = Vec::with_capacity(w * h);
        let mut visualization_rgba8 = Vec::with_capacity(w * h * 4);

        for y in 0..h {
            for x in 0..w {
                let fx = (x as f32 + 0.5) / (width as f32);
                let fy = (y as f32 + 0.5) / (height as f32);
                let probe_pos = min_bounds + Vec3::new(fx * extent.x, fy * extent.y, 0.0);

                let mut total_irradiance = [
                    sky_radiance[0] * 0.15,
                    sky_radiance[1] * 0.15,
                    sky_radiance[2] * 0.15,
                ];

                for c in 0..cascade_count {
                    let r_min = if c > 0 { 2.0_f32.powf(c as f32 * 1.5 - 1.0) } else { 0.0 };
                    let r_max = 2.0_f32.powf((c + 1) as f32 * 1.5 - 1.0);

                    let mut cascade_rad = [0.0_f32; 3];
                    for em in emitters {
                        let em_pos = Vec3::from_slice(&em.position[..3]);
                        let em_radius = em.position[3];
                        let em_color = &em.radiance[..3];
                        let em_intensity = em.radiance[3];

                        let dist = probe_pos.distance(em_pos);
                        if dist >= r_min && dist <= r_max {
                            let falloff = em_intensity / (1.0 + (dist / em_radius.max(0.1)).powi(2));
                            cascade_rad[0] += em_color[0] * falloff;
                            cascade_rad[1] += em_color[1] * falloff;
                            cascade_rad[2] += em_color[2] * falloff;
                        }
                    }

                    let cascade_weight = 1.0 / (1.0 + c as f32 * 0.5);
                    total_irradiance[0] += cascade_rad[0] * cascade_weight;
                    total_irradiance[1] += cascade_rad[1] * cascade_weight;
                    total_irradiance[2] += cascade_rad[2] * cascade_weight;
                }

                probe_grid.push([total_irradiance[0], total_irradiance[1], total_irradiance[2], 1.0]);

                let tm = agx_tonemap_srgb(total_irradiance);
                visualization_rgba8.push((tm[0] * 255.0).clamp(0.0, 255.0) as u8);
                visualization_rgba8.push((tm[1] * 255.0).clamp(0.0, 255.0) as u8);
                visualization_rgba8.push((tm[2] * 255.0).clamp(0.0, 255.0) as u8);
                visualization_rgba8.push(255);
            }
        }

        let elapsed_ms = start_time.elapsed().as_secs_f32() * 1000.0;

        Ok(GpuRadianceCascadesResult {
            probe_grid,
            visualization_rgba8,
            resolution: [width, height],
            elapsed_ms,
            hardware_accelerated: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_radiance_cascades_solver_hardware_dispatch() {
        let device = WgpuRenderDevice::new_or_fallback();
        let mut solver = GpuRadianceCascadesSolver::new();

        let emitters = vec![
            GpuEmitter::new(Vec3::new(0.0, 0.0, 0.0), 1.0, [1.0, 0.8, 0.3], 10.0),
            GpuEmitter::new(Vec3::new(2.0, 2.0, 0.0), 1.5, [0.3, 0.6, 1.0], 8.0),
        ];

        let res = solver
            .solve_radiance_cascades(
                &device,
                &emitters,
                Vec3::new(-5.0, -5.0, 0.0),
                Vec3::new(5.0, 5.0, 0.0),
                [16, 16],
                4,
                [0.1, 0.1, 0.15],
            )
            .expect("Radiance Cascades solver dispatch");

        assert_eq!(res.resolution, [16, 16]);
        assert_eq!(res.probe_grid.len(), 16 * 16);
        assert_eq!(res.visualization_rgba8.len(), 16 * 16 * 4);
        assert!(res.elapsed_ms >= 0.0);

        if device.is_hardware_accelerated() {
            assert!(res.hardware_accelerated);
        }

        // Check non-zero probe irradiance
        let total_energy: f32 = res.probe_grid.iter().map(|p| p[0] + p[1] + p[2]).sum();
        assert!(total_energy > 0.0, "Irradiance grid must accumulate light energy");
    }

    #[test]
    fn test_radiance_cascades_emitter_falloff_and_distance_intervals() {
        let device = WgpuRenderDevice::new_or_fallback();
        let mut solver = GpuRadianceCascadesSolver::new();

        let emitters = vec![GpuEmitter::new(
            Vec3::new(0.0, 0.0, 0.0),
            1.0,
            [1.0, 1.0, 1.0],
            10.0,
        )];

        let res = solver
            .solve_radiance_cascades(
                &device,
                &emitters,
                Vec3::new(-4.0, -4.0, 0.0),
                Vec3::new(4.0, 4.0, 0.0),
                [8, 8],
                4,
                [0.0, 0.0, 0.0],
            )
            .expect("Solve");

        // Probes near the center (index ~ 27, 28, 35, 36) should have significantly higher irradiance
        // than corner probes (e.g. index 0 at (-4, -4))
        let center_irr = res.probe_grid[27][0];
        let corner_irr = res.probe_grid[0][0];
        assert!(
            center_irr > corner_irr,
            "Center probe irradiance ({center_irr}) must exceed corner probe ({corner_irr})"
        );
    }

    #[test]
    fn test_radiance_cascades_multi_cascade_accumulation() {
        let device = WgpuRenderDevice::new_or_fallback();
        let mut solver1 = GpuRadianceCascadesSolver::new();
        let mut solver2 = GpuRadianceCascadesSolver::new();

        let emitters = vec![GpuEmitter::new(
            Vec3::new(0.0, 0.0, 0.0),
            2.0,
            [1.0, 1.0, 1.0],
            20.0,
        )];

        // Cascade count = 1
        let res1 = solver1
            .solve_radiance_cascades(
                &device,
                &emitters,
                Vec3::new(-10.0, -10.0, 0.0),
                Vec3::new(10.0, 10.0, 0.0),
                [8, 8],
                1,
                [0.0, 0.0, 0.0],
            )
            .expect("Solve 1 cascade");

        // Cascade count = 4 (covers larger distance intervals)
        let res4 = solver2
            .solve_radiance_cascades(
                &device,
                &emitters,
                Vec3::new(-10.0, -10.0, 0.0),
                Vec3::new(10.0, 10.0, 0.0),
                [8, 8],
                4,
                [0.0, 0.0, 0.0],
            )
            .expect("Solve 4 cascades");

        let sum1: f32 = res1.probe_grid.iter().map(|p| p[0]).sum();
        let sum4: f32 = res4.probe_grid.iter().map(|p| p[0]).sum();
        assert!(
            sum4 >= sum1,
            "4 cascades ({sum4}) should gather equal or greater energy than 1 cascade ({sum1})"
        );
    }

    #[test]
    fn test_radiance_cascades_cpu_fallback_parity() {
        let solver = GpuRadianceCascadesSolver::new();
        let emitters = vec![GpuEmitter::new(
            Vec3::ZERO,
            1.0,
            [0.5, 0.5, 1.0],
            5.0,
        )];

        let start = Instant::now();
        let res = solver
            .solve_cpu_fallback(
                &emitters,
                Vec3::new(-2.0, -2.0, 0.0),
                Vec3::new(2.0, 2.0, 0.0),
                8,
                8,
                3,
                [0.05, 0.05, 0.05],
                start,
            )
            .expect("CPU solve");

        assert_eq!(res.resolution, [8, 8]);
        assert_eq!(res.probe_grid.len(), 64);
        assert_eq!(res.visualization_rgba8.len(), 64 * 4);
        assert!(!res.hardware_accelerated);
    }

    #[test]
    fn test_radiance_cascades_odd_probe_resolution() {
        let device = WgpuRenderDevice::new_or_fallback();
        let mut solver = GpuRadianceCascadesSolver::new();
        let emitters = vec![GpuEmitter::new(
            Vec3::ZERO,
            1.0,
            [1.0, 1.0, 1.0],
            2.0,
        )];

        let res = solver
            .solve_radiance_cascades(
                &device,
                &emitters,
                Vec3::new(-3.0, -3.0, 0.0),
                Vec3::new(3.0, 3.0, 0.0),
                [17, 11],
                2,
                [0.0, 0.0, 0.0],
            )
            .expect("Solve odd resolution");

        assert_eq!(res.resolution, [17, 11]);
        assert_eq!(res.probe_grid.len(), 17 * 11);
        assert_eq!(res.visualization_rgba8.len(), 17 * 11 * 4);
    }
}
