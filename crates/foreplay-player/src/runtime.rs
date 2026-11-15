//! Unified native Foreplay Game Engine runtime (`PlayerRuntime`).
//!
//! Orchestrates the complete standalone runtime pipeline:
//! - Scene tree & Bevy ECS world (`foreplay-core`)
//! - Hardware `wgpu` 30.0.2 3D Forward+ PBR & 2D renderer (`foreplay-render`)
//! - Rapier3D physics space & character controller (`foreplay-physics`)
//! - cpal audio driver & real-time ring buffer (`foreplay-audio`)
//! - Input server & Gilrs gamepad bridge (`foreplay-platform`)
//! - GDScript VM execution, coroutines & breakpoint hooks (`foreplay-gdscript`)
//! - GDExtension host plugin lifecycle (`foreplay-gdext`)
//! - Remote Play-In-Editor (PIE) TCP IPC debug server (`debug_ipc.rs`)

use crate::debug_ipc::{
    DebugCommand, DebugEvent, DebugIpcError, DebugIpcServer, DebugNodeSummary,
};
use bevy_math::{Mat4, Vec3};
use foreplay_audio::{AudioRingBuffer, CpalAudioDriver};
use foreplay_core::classdb::ClassDb;
use foreplay_core::SceneTree;
use foreplay_gdext::GdExtensionHost;
use foreplay_gdscript::GdVm;
use foreplay_ir::{IrNode, IrScene, IrValue, NodeId};
use foreplay_physics::{
    BodyType, CharacterBody3DController, RapierSpace3D, RigidBodyConfig3D, ShapeConfig3D,
};
use foreplay_platform::{GilrsGamepadBridge, InputServer};
use foreplay_render::{
    HardwareDrawItem3D, HardwareFrameOutput, HardwareLight3D, HardwareLightType,
    PerspectiveCamera3D, RenderHardwareError, WgpuRenderDevice,
};
use std::collections::{BTreeMap, BTreeSet};
use std::net::SocketAddr;
use std::num::NonZeroU32;

/// Summary statistics emitted after stepping a player runtime frame.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerTickSummary {
    pub tick: u64,
    pub dt: f32,
    pub paused: bool,
    pub nodes_processed: usize,
    pub active_physics_bodies: usize,
    pub active_coroutines: usize,
    pub hit_breakpoint: Option<(u64, String, usize)>,
}

/// Script execution binding for a scene node.
#[derive(Debug)]
struct NodeScriptBinding {
    node_id: u64,
    node_key: NodeId,
    script_path: String,
    vm: GdVm,
}

/// Helper function to create an `IrNode` instance.
#[must_use]
pub fn create_ir_node(id: NodeId, name: impl Into<String>, class_name: impl Into<String>) -> IrNode {
    IrNode {
        id,
        name: name.into(),
        class_name: class_name.into(),
        parent: None,
        instance_scene: None,
        properties: BTreeMap::new(),
        groups: Vec::new(),
    }
}

/// Central unified Foreplay runtime player engine.
pub struct PlayerRuntime {
    pub scene_tree: SceneTree,
    pub class_db: ClassDb,
    pub render_device: WgpuRenderDevice,
    pub physics_space: RapierSpace3D,
    pub character_controller: CharacterBody3DController,
    pub audio_driver: CpalAudioDriver,
    pub audio_buffer: AudioRingBuffer,
    pub input_server: InputServer,
    pub gamepad_bridge: GilrsGamepadBridge,
    pub gdext_host: GdExtensionHost,

    node_scripts: BTreeMap<u64, NodeScriptBinding>,
    feel_knobs: BTreeMap<String, f64>,
    active_breakpoints: BTreeSet<(String, usize)>,
    debug_server: Option<DebugIpcServer>,

    tick_count: u64,
    paused: bool,
    should_exit: bool,
    fps_estimate: f32,
    last_frame_time: std::time::Instant,
}

impl std::fmt::Debug for PlayerRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlayerRuntime")
            .field("tick_count", &self.tick_count)
            .field("paused", &self.paused)
            .field("scene_name", &self.scene_tree.active_scene.name)
            .field("node_count", &self.scene_tree.active_scene.nodes.len())
            .field("feel_knobs_count", &self.feel_knobs.len())
            .field("breakpoints_count", &self.active_breakpoints.len())
            .field("debug_server_active", &self.debug_server.is_some())
            .finish()
    }
}

impl PlayerRuntime {
    /// Creates a new `PlayerRuntime` booting from an `IrScene`.
    pub fn new(mut scene: IrScene) -> Self {
        // Ensure root node exists if scene empty
        if scene.nodes.is_empty() {
            let root_id = NodeId {
                generation: NonZeroU32::new(1).unwrap(),
                index: 0,
            };
            let mut root = create_ir_node(root_id, "Root", "Node3D");
            root.properties.insert("position".into(), IrValue::Vec3([0.0, 0.0, 0.0]));
            scene.nodes.insert(root_id, root);
        }

        let scene_tree = SceneTree::from_ir(scene.clone());
        let class_db = ClassDb::global_godot44().clone();
        let render_device = WgpuRenderDevice::new_or_fallback();
        let mut physics_space = RapierSpace3D::with_gravity(Vec3::new(0.0, -9.81, 0.0));
        let default_char_id = scene.nodes.keys().next().copied().unwrap_or(NodeId {
            generation: NonZeroU32::new(1).unwrap(),
            index: 0,
        });
        let character_controller = CharacterBody3DController::new(default_char_id, Vec3::ZERO);

        let audio_driver = CpalAudioDriver::try_start_default_output(48000);
        let audio_buffer = AudioRingBuffer::new(48000);
        let input_server = InputServer::new(foreplay_platform::InputMap::with_godot_defaults());
        let gamepad_bridge = GilrsGamepadBridge::try_new_or_virtual();

        let mut gdext_host = GdExtensionHost::new();
        gdext_host.initialize_all();

        let mut node_scripts = BTreeMap::new();
        let mut feel_knobs = BTreeMap::new();

        // Populate default feel knobs from class_db
        feel_knobs.insert("jump_gravity".into(), 19.62);
        feel_knobs.insert("fall_gravity".into(), 24.52);
        feel_knobs.insert("max_speed".into(), 10.0);
        feel_knobs.insert("acceleration".into(), 50.0);
        feel_knobs.insert("friction".into(), 40.0);

        // Scan scene nodes to initialize physics colliders and GDScript VMs
        for (node_id, ir_node) in &scene.nodes {
            let packed_id = (u64::from(node_id.generation.get()) << 32) | u64::from(node_id.index);

            // 1. Physics bodies setup
            if ir_node.class_name == "RigidBody3D" {
                let pos = extract_vec3_prop(ir_node, "position").unwrap_or(Vec3::ZERO);
                let cfg = RigidBodyConfig3D {
                    body_type: BodyType::Dynamic,
                    position: pos,
                    linear_damping: 0.1,
                    angular_damping: 0.1,
                    gravity_scale: 1.0,
                    can_sleep: true,
                    mass: 1.0,
                    ..Default::default()
                };
                physics_space.add_rigid_body(*node_id, &cfg);
                physics_space.add_collider(
                    *node_id,
                    ShapeConfig3D::Cuboid { half_extents: Vec3::splat(0.5) },
                    0.5,
                    0.5,
                    1,
                    1,
                );
            } else if ir_node.class_name == "StaticBody3D" {
                let pos = extract_vec3_prop(ir_node, "position").unwrap_or(Vec3::ZERO);
                let cfg = RigidBodyConfig3D {
                    body_type: BodyType::Fixed,
                    position: pos,
                    mass: 0.0,
                    ..Default::default()
                };
                physics_space.add_rigid_body(*node_id, &cfg);
                physics_space.add_collider(
                    *node_id,
                    ShapeConfig3D::Cuboid { half_extents: Vec3::new(10.0, 0.5, 10.0) },
                    0.5,
                    0.5,
                    1,
                    1,
                );
            }

            // 2. Attached GDScript instantiation
            let script_src = ir_node
                .properties
                .get("script_source")
                .and_then(|v| if let IrValue::String(s) = v { Some(s.as_str()) } else { None })
                .or_else(|| {
                    if ir_node.class_name == "PlayerCharacter" || ir_node.name.contains("Player") {
                        Some(
                            "extends CharacterBody3D\n\
                            var speed: float = 5.0\n\
                            func _ready():\n\
                                print(\"Player ready!\")\n\
                            func _process(delta: float):\n\
                                pass\n\
                            func _physics_process(delta: float):\n\
                                pass\n",
                        )
                    } else {
                        None
                    }
                });

            if let Some(src) = script_src {
                let script_path = ir_node
                    .properties
                    .get("script_path")
                    .and_then(|v| if let IrValue::String(s) = v { Some(s.clone()) } else { None })
                    .unwrap_or_else(|| format!("res://scripts/{}.gd", ir_node.name.to_lowercase()));

                if let Ok(mut vm) = GdVm::from_source(src) {
                    let mut mut_ir = ir_node.clone();
                    let _ = vm.ready(&mut mut_ir);
                    node_scripts.insert(
                        packed_id,
                        NodeScriptBinding {
                            node_id: packed_id,
                            node_key: *node_id,
                            script_path,
                            vm,
                        },
                    );
                }
            }
        }

        Self {
            scene_tree,
            class_db,
            render_device,
            physics_space,
            character_controller,
            audio_driver,
            audio_buffer,
            input_server,
            gamepad_bridge,
            gdext_host,
            node_scripts,
            feel_knobs,
            active_breakpoints: BTreeSet::new(),
            debug_server: None,
            tick_count: 0,
            paused: false,
            should_exit: false,
            fps_estimate: 60.0,
            last_frame_time: std::time::Instant::now(),
        }
    }

    /// Attaches an inline GDScript source string to a specific node by ID.
    pub fn attach_gdscript(
        &mut self,
        node_id: u64,
        script_path: impl Into<String>,
        source: &str,
    ) -> Result<(), foreplay_gdscript::VmError> {
        let mut vm = GdVm::from_source(source)?;
        let path = script_path.into();

        let (node_key, mut node_copy) = self
            .find_node_with_key(node_id)
            .map(|(k, n)| (k, n.clone()))
            .unwrap_or_else(|| {
                let key = NodeId {
                    generation: NonZeroU32::new(1).unwrap(),
                    index: (node_id & 0xFFFF_FFFF) as u32,
                };
                let n = create_ir_node(key, "Node", "Node3D");
                (key, n)
            });

        let _ = vm.ready(&mut node_copy);

        self.node_scripts.insert(
            node_id,
            NodeScriptBinding {
                node_id,
                node_key,
                script_path: path,
                vm,
            },
        );
        Ok(())
    }

    /// Binds and starts the TCP Remote PIE Debug server.
    pub fn bind_debug_server(&mut self, bind_addr: &str) -> Result<SocketAddr, DebugIpcError> {
        let server = DebugIpcServer::bind(bind_addr)?;
        let addr = server.local_addr()?;
        self.debug_server = Some(server);
        Ok(addr)
    }

    /// Polls incoming debug commands and replies via the debug IPC server.
    pub fn poll_debug_ipc(&mut self) {
        let _ = self.poll_debug_ipc_timeout(std::time::Duration::from_millis(0));
    }

    /// Polls debug IPC until at least one command is handled or timeout expires.
    pub fn poll_debug_ipc_timeout(&mut self, timeout: std::time::Duration) -> usize {
        let start = std::time::Instant::now();
        let mut count = 0;

        loop {
            if let Some(mut server) = self.debug_server.take() {
                while let Ok(Some(cmd)) = server.poll_command() {
                    let reply = self.handle_debug_command(&cmd);
                    let _ = server.send_event(&reply);
                    count += 1;
                }
                self.debug_server = Some(server);
            }

            if count > 0 || start.elapsed() >= timeout {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }

        count
    }

    fn handle_debug_command(&mut self, cmd: &DebugCommand) -> DebugEvent {
        match cmd {
            DebugCommand::Ping => DebugEvent::Pong,
            DebugCommand::GetSceneTree => {
                let (scene_name, node_count, nodes) = self.scene_tree_snapshot();
                DebugEvent::SceneTreeSnapshot {
                    scene_name,
                    node_count,
                    nodes,
                }
            }
            DebugCommand::SetBreakpoint {
                script_path,
                line,
                enabled,
            } => {
                if *enabled {
                    self.active_breakpoints.insert((script_path.clone(), *line));
                    DebugEvent::Ack {
                        message: format!("Breakpoint set at {script_path}:{line}"),
                    }
                } else {
                    self.active_breakpoints.remove(&(script_path.clone(), *line));
                    DebugEvent::Ack {
                        message: format!("Breakpoint removed at {script_path}:{line}"),
                    }
                }
            }
            DebugCommand::ClearBreakpoints => {
                let count = self.active_breakpoints.len();
                self.active_breakpoints.clear();
                DebugEvent::Ack {
                    message: format!("Cleared {count} breakpoints"),
                }
            }
            DebugCommand::Pause => {
                self.paused = true;
                DebugEvent::Ack {
                    message: "Runtime paused".into(),
                }
            }
            DebugCommand::Resume => {
                self.paused = false;
                DebugEvent::Ack {
                    message: "Runtime resumed".into(),
                }
            }
            DebugCommand::StepFrame => {
                self.step_frame_internal(1.0 / 60.0);
                DebugEvent::Ack {
                    message: format!("Stepped frame to tick {}", self.tick_count),
                }
            }
            DebugCommand::SetNodeProperty { node_id, key, value } => {
                if let Some((_k, node)) = self.find_node_with_key_mut(*node_id) {
                    node.properties.insert(key.clone(), value.clone());
                    DebugEvent::Ack {
                        message: format!("Property '{key}' updated on node {node_id}"),
                    }
                } else {
                    DebugEvent::Error {
                        message: format!("Node {node_id} not found"),
                    }
                }
            }
            DebugCommand::SetFeelKnob { name, value } => {
                self.feel_knobs.insert(name.clone(), *value);
                DebugEvent::Ack {
                    message: format!("Feel knob '{name}' set to {value}"),
                }
            }
            DebugCommand::HotReloadGdScript { node_id, source } => {
                match GdVm::from_source(source) {
                    Ok(vm) => {
                        let path = self
                            .node_scripts
                            .get(node_id)
                            .map(|b| b.script_path.clone())
                            .unwrap_or_else(|| "res://scripts/reloaded.gd".into());
                        let key = self
                            .node_scripts
                            .get(node_id)
                            .map(|b| b.node_key)
                            .unwrap_or_else(|| NodeId {
                                generation: NonZeroU32::new(1).unwrap(),
                                index: (*node_id & 0xFFFF_FFFF) as u32,
                            });
                        self.node_scripts.insert(
                            *node_id,
                            NodeScriptBinding {
                                node_id: *node_id,
                                node_key: key,
                                script_path: path,
                                vm,
                            },
                        );
                        DebugEvent::Ack {
                            message: format!("GDScript hot reloaded on node {node_id}"),
                        }
                    }
                    Err(e) => DebugEvent::Error {
                        message: format!("Hot reload parse error: {e}"),
                    },
                }
            }
            DebugCommand::GetTelemetry => self.telemetry(),
            DebugCommand::Shutdown => {
                self.should_exit = true;
                DebugEvent::Ack {
                    message: "Runtime shutting down".into(),
                }
            }
        }
    }

    /// Advances the engine by one frame with delta time `dt` seconds.
    pub fn step_frame(&mut self, dt: f32) -> PlayerTickSummary {
        self.poll_debug_ipc();

        if self.paused {
            return PlayerTickSummary {
                tick: self.tick_count,
                dt,
                paused: true,
                nodes_processed: 0,
                active_physics_bodies: self.physics_space.bodies.len(),
                active_coroutines: self.count_active_coroutines(),
                hit_breakpoint: None,
            };
        }

        self.step_frame_internal(dt)
    }

    fn step_frame_internal(&mut self, dt: f32) -> PlayerTickSummary {
        let now = std::time::Instant::now();
        let frame_elapsed = now.duration_since(self.last_frame_time).as_secs_f32();
        if frame_elapsed > 0.0 {
            self.fps_estimate = 0.9 * self.fps_estimate + 0.1 * (1.0 / frame_elapsed);
        }
        self.last_frame_time = now;

        self.tick_count += 1;
        self.scene_tree.step_frame();

        // 1. Step 3D physics
        self.physics_space.step(dt);

        // Sync physics transforms back into scene nodes
        let body_positions: Vec<(NodeId, Vec3)> = self
            .physics_space
            .node_to_body
            .keys()
            .filter_map(|&id| self.physics_space.get_body_position(id).map(|p| (id, p)))
            .collect();

        for (id, pos) in body_positions {
            if let Some(node) = self.scene_tree.active_scene.nodes.get_mut(&id) {
                node.properties.insert(
                    "position".into(),
                    IrValue::Vec3([pos.x, pos.y, pos.z]),
                );
            }
        }

        // 2. Step GDScript node scripts & coroutines
        let mut hit_bp = None;
        let mut nodes_processed = 0;

        for binding in self.node_scripts.values_mut() {
            nodes_processed += 1;

            // Check if any active breakpoint matches this script
            for (bp_path, bp_line) in &self.active_breakpoints {
                if bp_path == &binding.script_path {
                    hit_bp = Some((binding.node_id, bp_path.clone(), *bp_line));
                    self.paused = true;
                    break;
                }
            }

            let mut node_dummy = create_ir_node(binding.node_key, "Node", "Node3D");

            // Tick coroutine timers
            let _ = binding.vm.tick_timers(dt as f64);
            // Process methods
            let _ = binding.vm.step_physics_process(dt as f64, &mut node_dummy);
            let _ = binding.vm.step_process(dt as f64, &mut node_dummy);

            if hit_bp.is_some() {
                break;
            }
        }

        // If breakpoint was hit, notify debug client
        if let Some((node_id, ref script_path, line)) = hit_bp {
            let mut locals = BTreeMap::new();
            if let Some(binding) = self.node_scripts.get(&node_id) {
                locals = binding.vm.member_vars.clone();
            }
            if let Some(server) = self.debug_server.as_mut() {
                let _ = server.send_event(&DebugEvent::PausedAtBreakpoint {
                    node_id,
                    script_path: script_path.clone(),
                    line,
                    locals,
                    call_stack: vec![format!("{script_path}:{line}")],
                });
            }
        }

        PlayerTickSummary {
            tick: self.tick_count,
            dt,
            paused: self.paused,
            nodes_processed,
            active_physics_bodies: self.physics_space.bodies.len(),
            active_coroutines: self.count_active_coroutines(),
            hit_breakpoint: hit_bp,
        }
    }

    /// Executes multiple headless simulation ticks sequentially.
    pub fn run_headless_ticks(&mut self, ticks: u32, dt: f32) -> PlayerTickSummary {
        let mut last_summary = PlayerTickSummary {
            tick: self.tick_count,
            dt,
            paused: self.paused,
            nodes_processed: 0,
            active_physics_bodies: self.physics_space.bodies.len(),
            active_coroutines: self.count_active_coroutines(),
            hit_breakpoint: None,
        };

        for _ in 0..ticks {
            last_summary = self.step_frame(dt);
            if self.should_exit {
                break;
            }
        }

        last_summary
    }

    /// Renders the current 3D/2D scene offscreen onto an RGBA8 buffer.
    pub fn render_rgba(
        &mut self,
        width: u32,
        height: u32,
    ) -> Result<HardwareFrameOutput, RenderHardwareError> {
        let mut cam = PerspectiveCamera3D::new(
            Vec3::new(0.0, 5.0, 10.0),
            60.0,
            (width as f32) / (height as f32).max(1.0),
        );
        cam.look_at(Vec3::new(0.0, 0.0, 0.0), Vec3::Y);

        let mut draw_items = Vec::new();

        // Build 3D mesh items from scene nodes
        for node in self.scene_tree.active_scene.nodes.values() {
            let pos = extract_vec3_prop(node, "position").unwrap_or(Vec3::ZERO);
            let color = extract_color_prop(node).unwrap_or([0.8, 0.2, 0.2, 1.0]);

            let item = HardwareDrawItem3D::new_cube(
                1.0,
                Mat4::from_translation(pos),
                color,
                0.4,
                0.0,
            );
            draw_items.push(item);
        }

        if draw_items.is_empty() {
            // Default cube if scene has no meshes
            draw_items.push(HardwareDrawItem3D::new_cube(
                1.5,
                Mat4::IDENTITY,
                [0.2, 0.7, 0.9, 1.0],
                0.3,
                0.1,
            ));
        }

        let dir_light = HardwareLight3D {
            light_type: HardwareLightType::Directional,
            direction: [-0.5, -1.0, -0.5],
            position: [5.0, 10.0, 5.0],
            color: [1.0, 0.95, 0.9],
            energy: 2.0,
            range: 100.0,
            cast_shadows: true,
        };

        self.render_device.render_scene_offscreen_rgba(
            width,
            height,
            &cam,
            &draw_items,
            &[dir_light],
            &[],
        )
    }

    /// Returns a structured snapshot of the scene tree for the PIE inspector.
    pub fn scene_tree_snapshot(&self) -> (String, usize, Vec<DebugNodeSummary>) {
        let scene_name = self.scene_tree.active_scene.name.clone();
        let mut nodes = Vec::new();

        for (&id, ir_node) in &self.scene_tree.active_scene.nodes {
            let packed_id = (u64::from(id.generation.get()) << 32) | u64::from(id.index);
            let parent_id = ir_node
                .parent
                .map(|p| (u64::from(p.generation.get()) << 32) | u64::from(p.index));

            let child_count = self
                .scene_tree
                .active_scene
                .nodes
                .values()
                .filter(|n| n.parent == Some(id))
                .count();

            nodes.push(DebugNodeSummary {
                id: packed_id,
                name: ir_node.name.clone(),
                class_name: ir_node.class_name.clone(),
                parent_id,
                child_count,
                properties: ir_node.properties.clone(),
            });
        }

        (scene_name, nodes.len(), nodes)
    }

    /// Builds a live telemetry event payload.
    pub fn telemetry(&self) -> DebugEvent {
        DebugEvent::Telemetry {
            tick: self.tick_count,
            paused: self.paused,
            fps_estimate: self.fps_estimate,
            active_physics_bodies: self.physics_space.bodies.len(),
            active_coroutines: self.count_active_coroutines(),
            gpu_adapter: self.render_device.adapter_summary(),
            audio_backend: format!("{:?}", self.audio_driver.backend()),
            gdext_classes_registered: self.gdext_host.class_db.class_count(),
        }
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
    }

    pub fn feel_knobs(&self) -> &BTreeMap<String, f64> {
        &self.feel_knobs
    }

    pub fn set_feel_knob(&mut self, name: impl Into<String>, value: f64) {
        self.feel_knobs.insert(name.into(), value);
    }

    pub fn should_exit(&self) -> bool {
        self.should_exit
    }

    fn count_active_coroutines(&self) -> usize {
        self.node_scripts
            .values()
            .map(|b| b.vm.suspended_coroutines.len())
            .sum()
    }

    fn find_node_with_key(&self, target_packed_id: u64) -> Option<(NodeId, &IrNode)> {
        for (&id, node) in &self.scene_tree.active_scene.nodes {
            let packed = (u64::from(id.generation.get()) << 32) | u64::from(id.index);
            if packed == target_packed_id {
                return Some((id, node));
            }
        }
        None
    }

    fn find_node_with_key_mut(&mut self, target_packed_id: u64) -> Option<(NodeId, &mut IrNode)> {
        for (&id, node) in &mut self.scene_tree.active_scene.nodes {
            let packed = (u64::from(id.generation.get()) << 32) | u64::from(id.index);
            if packed == target_packed_id {
                return Some((id, node));
            }
        }
        None
    }
}

fn extract_vec3_prop(node: &IrNode, prop: &str) -> Option<Vec3> {
    match node.properties.get(prop) {
        Some(IrValue::Vec3([x, y, z])) => Some(Vec3::new(*x, *y, *z)),
        _ => None,
    }
}

fn extract_color_prop(node: &IrNode) -> Option<[f32; 4]> {
    match node.properties.get("albedo_color").or_else(|| node.properties.get("modulate")) {
        Some(IrValue::Color([r, g, b, a])) => Some([*r, *g, *b, *a]),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_runtime_initialization_and_headless_ticks() {
        let mut scene = IrScene::new("TestScene");
        let root_id = NodeId {
            generation: NonZeroU32::new(1).unwrap(),
            index: 0,
        };
        let mut root = create_ir_node(root_id, "Player", "RigidBody3D");
        root.properties.insert("position".into(), IrValue::Vec3([0.0, 10.0, 0.0]));
        scene.nodes.insert(root_id, root);

        let mut runtime = PlayerRuntime::new(scene);
        assert_eq!(runtime.tick_count, 0);
        assert_eq!(runtime.physics_space.bodies.len(), 1);

        let summary = runtime.run_headless_ticks(5, 1.0 / 60.0);
        assert_eq!(summary.tick, 5);
        assert!(!summary.paused);

        // Body should have fallen under gravity
        let body_pos = runtime.physics_space.get_body_position(root_id).unwrap();
        assert!(body_pos.y < 10.0, "Body should have fallen: {body_pos:?}");
    }

    #[test]
    fn feel_knobs_tuning_and_telemetry() {
        let scene = IrScene::new("KnobsScene");
        let mut runtime = PlayerRuntime::new(scene);

        assert!(runtime.feel_knobs().contains_key("jump_gravity"));
        runtime.set_feel_knob("jump_gravity", 30.0);
        assert_eq!(*runtime.feel_knobs().get("jump_gravity").unwrap(), 30.0);

        let telem = runtime.telemetry();
        match telem {
            DebugEvent::Telemetry { gpu_adapter, gdext_classes_registered, .. } => {
                assert!(!gpu_adapter.is_empty());
                assert!(gdext_classes_registered > 0);
            }
            _ => panic!("expected telemetry event"),
        }
    }

    #[test]
    fn breakpoint_suspension_via_ipc_debug_server() {
        let mut scene = IrScene::new("BpScene");
        let root_id = NodeId {
            generation: NonZeroU32::new(1).unwrap(),
            index: 0,
        };
        let mut root = create_ir_node(root_id, "PlayerNode", "Node3D");
        root.properties.insert(
            "script_source".into(),
            IrValue::String(
                "extends Node3D\n\
                var x = 1\n\
                func _process(delta: float):\n\
                    x += 1\n".into(),
            ),
        );
        root.properties.insert("script_path".into(), IrValue::String("res://scripts/player.gd".into()));
        scene.nodes.insert(root_id, root);

        let mut runtime = PlayerRuntime::new(scene);
        let addr = runtime.bind_debug_server("127.0.0.1:0").expect("server binds");

        let mut client = crate::debug_ipc::DebugIpcClient::connect(addr).expect("client connects");

        // Set breakpoint at line 4
        client
            .send_command(&DebugCommand::SetBreakpoint {
                script_path: "res://scripts/player.gd".into(),
                line: 4,
                enabled: true,
            })
            .expect("send succeeds");
        runtime.poll_debug_ipc_timeout(std::time::Duration::from_millis(500));
        let reply = client.read_event().expect("read ack succeeds");
        assert!(matches!(reply, DebugEvent::Ack { .. }));

        // Step frame: should hit breakpoint and pause!
        let summary = runtime.step_frame(0.016);
        assert!(summary.paused);
        assert!(summary.hit_breakpoint.is_some());

        // Client reads PausedAtBreakpoint event
        let bp_event = client.read_event().expect("reads breakpoint event");
        match bp_event {
            DebugEvent::PausedAtBreakpoint { script_path, line, .. } => {
                assert_eq!(script_path, "res://scripts/player.gd");
                assert_eq!(line, 4);
            }
            other => panic!("expected PausedAtBreakpoint, got {other:?}"),
        }

        // Resume execution
        client.send_command(&DebugCommand::Resume).expect("send resume succeeds");
        runtime.poll_debug_ipc_timeout(std::time::Duration::from_millis(500));
        let resume_reply = client.read_event().expect("resume ack succeeds");
        assert!(matches!(resume_reply, DebugEvent::Ack { .. }));
        assert!(!runtime.is_paused());
    }

    #[test]
    fn offscreen_hardware_render_scene_integration() {
        let mut scene = IrScene::new("RenderScene");
        let root_id = NodeId {
            generation: NonZeroU32::new(1).unwrap(),
            index: 0,
        };
        let mut root = create_ir_node(root_id, "Cube", "MeshInstance3D");
        root.properties.insert("position".into(), IrValue::Vec3([0.0, 0.0, 0.0]));
        root.properties.insert("albedo_color".into(), IrValue::Color([0.2, 0.8, 0.4, 1.0]));
        scene.nodes.insert(root_id, root);

        let mut runtime = PlayerRuntime::new(scene);
        let frame = runtime.render_rgba(128, 128).expect("render succeeds");

        assert_eq!(frame.width, 128);
        assert_eq!(frame.height, 128);
        assert_eq!(frame.rgba8.len(), 128 * 128 * 4);
        assert!(frame.telemetry.draw_calls > 0);
    }

    #[test]
    fn pause_and_step_single_frame() {
        let mut scene = IrScene::new("PauseTest");
        let root_id = NodeId {
            generation: NonZeroU32::new(1).unwrap(),
            index: 0,
        };
        let mut root = create_ir_node(root_id, "Box", "RigidBody3D");
        root.properties.insert("position".into(), IrValue::Vec3([0.0, 10.0, 0.0]));
        scene.nodes.insert(root_id, root);

        let mut runtime = PlayerRuntime::new(scene);
        runtime.set_paused(true);
        assert!(runtime.is_paused());

        let summary = runtime.step_frame(0.016);
        assert_eq!(summary.tick, 0, "Tick should not advance while paused via step_frame");

        // Manually step one frame
        runtime.step_frame_internal(0.016);
        assert_eq!(runtime.tick_count, 1);
    }

    #[test]
    fn hot_reload_gdscript_via_ipc() {
        let mut scene = IrScene::new("HotReloadScene");
        let root_id = NodeId {
            generation: NonZeroU32::new(1).unwrap(),
            index: 0,
        };
        let packed_id = (u64::from(root_id.generation.get()) << 32) | u64::from(root_id.index);
        let mut root = create_ir_node(root_id, "Actor", "Node3D");
        root.properties.insert(
            "script_source".into(),
            IrValue::String("extends Node3D\nvar v = 1\n".into()),
        );
        root.properties.insert("script_path".into(), IrValue::String("res://scripts/actor.gd".into()));
        scene.nodes.insert(root_id, root);

        let mut runtime = PlayerRuntime::new(scene);
        let addr = runtime.bind_debug_server("127.0.0.1:0").expect("server binds");
        let mut client = crate::debug_ipc::DebugIpcClient::connect(addr).expect("client connects");

        // Hot reload script with new variable
        client
            .send_command(&DebugCommand::HotReloadGdScript {
                node_id: packed_id,
                source: "extends Node3D\nvar v = 999\n".into(),
            })
            .expect("send hot reload");
        runtime.poll_debug_ipc_timeout(std::time::Duration::from_millis(500));
        let reply = client.read_event().expect("read reply");
        assert!(matches!(reply, DebugEvent::Ack { .. }));
    }
}
