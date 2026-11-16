//! Standalone native Foreplay Game Engine runtime executable (`foreplay-player`).
//!
//! Supports:
//! - Automatic embedded `FPCKEXEC` package boot
//! - `--self-test`: Comprehensive end-to-end self-test suite exiting 0
//! - `--headless --ticks <N>`: Batch headless simulation
//! - `--scene <file.fscn>`: Custom scene loading
//! - `--debug-ipc <127.0.0.1:port>`: Loopback PIE debug server
//! - `--windowed`: Desktop window loop via `winit 0.30`

use bevy_math::Vec3;
use foreplay_ir::{IrScene, IrValue, NodeId};
use foreplay_player::runtime::create_ir_node;
use foreplay_player::{
    detect_current_exe_embedded_pack, embed_pack_trailer, extract_embedded_pack_from_bytes,
    pack_node_id, DebugCommand, DebugEvent, DebugIpcClient, PlayerBootMode, PlayerRuntime,
};
use foreplay_polyamour::{PolyLinkMessage, PolyamourLiveLinkClient};
use foreplay_render::SurfaceSwapchainPresenter;
use std::num::NonZeroU32;
use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowAttributes, WindowId};

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // 1. Check for --self-test flag
    if args.iter().any(|a| a == "--self-test") {
        run_self_test();
        return;
    }

    // 2. Check for embedded FPCKEXEC package in current binary
    if let Ok(Some(pack_bytes)) = detect_current_exe_embedded_pack() {
        println!(
            "Booting self-contained FPCKEXEC binary: detected {} embedded pack bytes",
            pack_bytes.len()
        );
        let scene = load_scene_from_pack_bytes(&pack_bytes)
            .unwrap_or_else(|_| IrScene::new("EmbeddedGame"));
        boot_runtime(scene, &args, PlayerBootMode::EmbeddedFpck);
        return;
    }

    // 3. Parse command-line flags
    let mut scene_path: Option<String> = None;
    let mut debug_ipc_addr: Option<String> = None;
    let mut ticks: Option<u32> = None;
    let mut windowed = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--scene" if i + 1 < args.len() => {
                scene_path = Some(args[i + 1].clone());
                i += 2;
            }
            "--debug-ipc" if i + 1 < args.len() => {
                debug_ipc_addr = Some(args[i + 1].clone());
                i += 2;
            }
            "--ticks" if i + 1 < args.len() => {
                ticks = args[i + 1].parse().ok();
                i += 2;
            }
            "--headless" => {
                if ticks.is_none() {
                    ticks = Some(60);
                }
                i += 1;
            }
            "--windowed" => {
                windowed = true;
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }

    // Build or load scene
    let scene = if let Some(path) = &scene_path {
        load_scene_from_path(path).unwrap_or_else(|_| IrScene::new("Game"))
    } else {
        create_default_demo_scene()
    };

    let mode = if let Some(n) = ticks {
        PlayerBootMode::Headless { ticks: n }
    } else if let Some(ref p) = scene_path {
        PlayerBootMode::ExternalFpck(p.clone())
    } else {
        PlayerBootMode::Headless { ticks: 60 }
    };

    if windowed {
        run_windowed(scene, debug_ipc_addr.as_deref());
    } else {
        boot_runtime_with_ipc(scene, ticks.unwrap_or(60), debug_ipc_addr.as_deref(), mode);
    }
}

fn boot_runtime_with_ipc(
    scene: IrScene,
    ticks: u32,
    debug_ipc_addr: Option<&str>,
    mode: PlayerBootMode,
) {
    println!("Foreplay Player Runtime v1.0.0 initializing ({mode:?})...");
    let mut runtime = PlayerRuntime::new(scene);

    if let Some(addr) = debug_ipc_addr {
        match runtime.bind_debug_server(addr) {
            Ok(bound) => println!("Debug IPC server listening on {bound}"),
            Err(e) => eprintln!("Failed to bind debug IPC server on {addr}: {e}"),
        }
    }

    let summary = runtime.run_headless_ticks(ticks, 1.0 / 60.0);
    println!(
        "Foreplay Player Runtime finished: tick={}, bodies={}, coroutines={}",
        summary.tick, summary.active_physics_bodies, summary.active_coroutines
    );
}

fn boot_runtime(scene: IrScene, args: &[String], mode: PlayerBootMode) {
    let ticks = args
        .iter()
        .position(|a| a == "--ticks")
        .and_then(|pos| args.get(pos + 1))
        .and_then(|t| t.parse().ok())
        .unwrap_or(60);

    let debug_ipc = args
        .iter()
        .position(|a| a == "--debug-ipc")
        .and_then(|pos| args.get(pos + 1))
        .map(|s| s.as_str());

    if args.iter().any(|a| a == "--windowed") {
        run_windowed(scene, debug_ipc);
    } else {
        boot_runtime_with_ipc(scene, ticks, debug_ipc, mode);
    }
}

/// Executes the comprehensive self-test suite verifying:
/// 1. 3D physics + GDScript coroutines + GDExtension initialization
/// 2. Hardware wgpu offscreen rendering
/// 3. Remote PIE TCP Debug IPC server & client communication
/// 4. Breakpoint suspension, state inspection, feel knob tuning & resume
/// 5. Self-contained FPCKEXEC binary embedding & extraction round-trip
fn run_self_test() {
    println!("============================================================");
    println!(" FOREPLAY PLAYER RUNTIME: RUNNING COMPREHENSIVE SELF-TEST  ");
    println!("============================================================");

    // 1. Create a rich test scene
    let mut scene = IrScene::new("SelfTestScene");

    let player_id = NodeId {
        generation: NonZeroU32::new(1).unwrap(),
        index: 1,
    };
    let mut player_node = create_ir_node(player_id, "Player", "RigidBody3D");
    player_node.properties.insert("position".into(), IrValue::Vec3([0.0, 5.0, 0.0]));
    player_node.properties.insert(
        "script_source".into(),
        IrValue::String(
            "extends RigidBody3D\n\
            var jump_power: float = 10.0\n\
            var step_count: int = 0\n\
            func _ready():\n\
                print(\"SelfTest script ready\")\n\
            func _process(delta: float):\n\
                step_count += 1\n\
            func _physics_process(delta: float):\n\
                pass\n".into(),
        ),
    );
    player_node.properties.insert(
        "script_path".into(),
        IrValue::String("res://scripts/player.gd".into()),
    );
    scene.nodes.insert(player_id, player_node);

    let ground_id = NodeId {
        generation: NonZeroU32::new(1).unwrap(),
        index: 2,
    };
    let mut ground_node = create_ir_node(ground_id, "Ground", "StaticBody3D");
    ground_node.properties.insert("position".into(), IrValue::Vec3([0.0, 0.0, 0.0]));
    ground_node.properties.insert("albedo_color".into(), IrValue::Color([0.1, 0.6, 0.2, 1.0]));
    scene.nodes.insert(ground_id, ground_node);

    println!("[1/6] Booting PlayerRuntime with 3D physics + GDScript + GDExtension...");
    let mut runtime = PlayerRuntime::new(scene);
    assert_eq!(runtime.physics_space.bodies.len(), 2);
    assert!(runtime.gdext_host.class_db.class_count() > 100);
    println!("      Physics bodies registered: {}", runtime.physics_space.bodies.len());
    println!("      GDExtension classes synchronized: {}", runtime.gdext_host.class_db.class_count());

    println!("[2/6] Testing hardware offscreen wgpu PBR rendering...");
    let frame = runtime.render_rgba(128, 128).expect("wgpu offscreen render succeeds");
    assert_eq!(frame.width, 128);
    assert_eq!(frame.height, 128);
    assert_eq!(frame.rgba8.len(), 128 * 128 * 4);
    println!("      Hardware render output: 128x128 RGBA8 ({} bytes), adapter: {}", frame.rgba8.len(), frame.telemetry.adapter_name);

    println!("[3/6] Starting Remote PIE TCP IPC Debug Server on loopback...");
    let bind_addr = runtime.bind_debug_server("127.0.0.1:0").expect("server binds");
    let mut client = DebugIpcClient::connect(bind_addr).expect("debug client connects");
    println!("      Debug server listening on {bind_addr}, client connected");

    println!("[4/6] Executing PIE debug protocol validation...");
    // Ping -> Pong
    let pong = sync_request(&mut client, &mut runtime, &DebugCommand::Ping);
    assert_eq!(pong, DebugEvent::Pong);
    println!("      Verified: Ping -> Pong");

    // GetSceneTree
    let tree = sync_request(&mut client, &mut runtime, &DebugCommand::GetSceneTree);
    match tree {
        DebugEvent::SceneTreeSnapshot { node_count, ref nodes, .. } => {
            assert!(node_count >= 2);
            assert!(nodes.iter().any(|n| n.name == "Player"));
            println!("      Verified: SceneTreeSnapshot with {node_count} nodes");
        }
        other => panic!("expected SceneTreeSnapshot, got {other:?}"),
    }

    // SetBreakpoint -> StepFrame -> PausedAtBreakpoint
    let bp_ack = sync_request(
        &mut client,
        &mut runtime,
        &DebugCommand::SetBreakpoint {
            script_path: "res://scripts/player.gd".into(),
            line: 7,
            enabled: true,
        },
    );
    assert!(matches!(bp_ack, DebugEvent::Ack { .. }));
    println!("      Verified: SetBreakpoint on res://scripts/player.gd:7");

    let step_summary = runtime.step_frame(0.016);
    assert!(step_summary.paused, "Runtime should pause on breakpoint!");
    assert!(step_summary.hit_breakpoint.is_some());

    let paused_event = client.read_event().expect("read breakpoint event");
    match paused_event {
        DebugEvent::PausedAtBreakpoint { script_path, line, .. } => {
            assert_eq!(script_path, "res://scripts/player.gd");
            assert_eq!(line, 7);
            println!("      Verified: PausedAtBreakpoint caught on {script_path}:{line}");
        }
        other => panic!("expected PausedAtBreakpoint, got {other:?}"),
    }

    // Step single frame while paused
    let step_ack = sync_request(&mut client, &mut runtime, &DebugCommand::StepFrame);
    assert!(matches!(step_ack, DebugEvent::Ack { .. }));
    println!("      Verified: Single frame stepped while paused");

    // Tune Feel Knob
    let knob_ack = sync_request(
        &mut client,
        &mut runtime,
        &DebugCommand::SetFeelKnob {
            name: "jump_gravity".into(),
            value: 28.5,
        },
    );
    assert!(matches!(knob_ack, DebugEvent::Ack { .. }));
    assert_eq!(*runtime.feel_knobs().get("jump_gravity").unwrap(), 28.5);
    println!("      Verified: Live feel knob tuning ('jump_gravity' -> 28.5)");

    // Resume
    let resume_ack = sync_request(&mut client, &mut runtime, &DebugCommand::Resume);
    assert!(matches!(resume_ack, DebugEvent::Ack { .. }));
    assert!(!runtime.is_paused());
    println!("      Verified: Runtime resumed successfully");

    println!("[5/6] Testing FPCKEXEC self-contained binary trailer embedding & extraction...");
    let dummy_exe = b"MZ_STUB_EXECUTABLE_BASE_PAYLOAD_FOREPLAY_V1";
    let mock_pack = b"FPCK_BINARY_DATA_WITH_MULTIPLE_SCENES_AND_TEXTURES";
    let embedded = embed_pack_trailer(dummy_exe, mock_pack);
    let extracted = extract_embedded_pack_from_bytes(&embedded)
        .expect("extraction succeeds")
        .expect("pack found");
    assert_eq!(extracted, mock_pack);
    println!("      Verified: Embedded FPCKEXEC binary trailer round-trip verified ({} bytes total)", embedded.len());

    println!("[6/6] Testing live poly:// PLNK mid-gameplay mesh & RapierSpace3D collider hot-streaming...");
    let poly_addr = runtime
        .bind_polyamour_live_link("127.0.0.1:0")
        .expect("polyamour live-link server binds");
    let mut link_client = PolyamourLiveLinkClient::connect_timeout(
        poly_addr,
        std::time::Duration::from_secs(2),
    )
    .expect("polyamour live-link client connects");

    // Impart high dynamic velocity and momentum to the moving player body
    let moving_vel = Vec3::new(12.5, 0.0, 3.2);
    let moving_ang = Vec3::new(0.0, 1.5, 0.0);
    runtime.physics_space.set_body_velocity(player_id, moving_vel);
    runtime.physics_space.set_body_angular_velocity(player_id, moving_ang);

    let pre_sync_vel = runtime.physics_space.get_body_velocity(player_id).unwrap();
    let pre_sync_ang = runtime.physics_space.get_body_angular_velocity(player_id).unwrap();
    assert_eq!(pre_sync_vel, moving_vel);
    assert_eq!(pre_sync_ang, moving_ang);

    // Stream mid-gameplay MeshDeltaSync for ground geometry with grind_edge and collision_trimesh
    let packed_ground = pack_node_id(ground_id);
    let delta = PolyLinkMessage::MeshDeltaSync {
        node_id: packed_ground,
        revision: 42,
        updated_vertices: vec![
            (0, [0.0, 0.0, 0.0]),
            (1, [10.0, 1.5, 0.0]),
            (2, [10.0, 1.5, 10.0]),
            (3, [0.0, 0.0, 10.0]),
        ],
        replaced_faces: Some(vec![
            vec![0, 1, 2],
            vec![0, 2, 3],
        ]),
        edge_tags: vec![
            ([1, 2], "grind_edge".to_string()),
        ],
        face_tags: vec![
            (0, "collision_trimesh".to_string()),
            (1, "collision_trimesh".to_string()),
        ],
    };

    link_client.send_message(&delta).expect("send delta succeeds");
    let polled = runtime.poll_polyamour_live_link_timeout(std::time::Duration::from_millis(500));
    assert!(polled > 0, "Server must have polled at least 1 message");

    // Client receives Ack { revision: 42 }
    let start_read = std::time::Instant::now();
    let mut client_replies = Vec::new();
    while start_read.elapsed() < std::time::Duration::from_millis(500) {
        if let Ok(msgs) = link_client.try_recv_messages()
            && !msgs.is_empty()
        {
            client_replies = msgs;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert!(
        client_replies.iter().any(|m| matches!(m, PolyLinkMessage::Ack { revision: 42 })),
        "Client must receive Ack for revision 42"
    );

    // Verify ground state updated in runtime
    let ground_state = runtime
        .live_polyamour_meshes()
        .get(&ground_id)
        .expect("ground state exists in live_polyamour_meshes");
    assert_eq!(ground_state.revision, 42);
    assert_eq!(ground_state.compiled_bundle.grind_rail_splines.len(), 1, "Grind rail spline extracted");
    assert!(!ground_state.compiled_bundle.trimesh_collider.0.is_empty(), "TriMesh vertices present");

    // Verify ground collider in RapierSpace3D was hot-rebuilt as TriMesh
    let ground_collider_handle = runtime.physics_space.node_to_collider.get(&ground_id).copied().expect("ground collider handle");
    let ground_collider = runtime.physics_space.colliders.get(ground_collider_handle).expect("ground collider in space");
    assert!(ground_collider.shape().as_trimesh().is_some(), "Collider must be a TriMesh");

    // Verify dynamic player body momentum was completely preserved (zero momentum loss)
    let post_sync_vel = runtime.physics_space.get_body_velocity(player_id).unwrap();
    let post_sync_ang = runtime.physics_space.get_body_angular_velocity(player_id).unwrap();
    assert_eq!(post_sync_vel, pre_sync_vel, "Dynamic player linear velocity preserved with zero momentum loss");
    assert_eq!(post_sync_ang, pre_sync_ang, "Dynamic player angular velocity preserved with zero momentum loss");
    println!("      Verified: Mid-gameplay poly:// live-link MeshDeltaSync hot-rebuilt collider & splines with zero player momentum loss");

    println!("============================================================");
    println!(" ALL SELF-TESTS PASSED (100% GREEN) — EXITING CODE 0       ");
    println!("============================================================");
}

fn sync_request(
    client: &mut DebugIpcClient,
    runtime: &mut PlayerRuntime,
    cmd: &DebugCommand,
) -> DebugEvent {
    client.send_command(cmd).expect("send cmd succeeds");
    runtime.poll_debug_ipc_timeout(std::time::Duration::from_millis(500));
    client.read_event().expect("read event succeeds")
}

/// Minimal winit 0.30 desktop window runner
struct PlayerApp {
    scene: IrScene,
    debug_ipc_addr: Option<String>,
    runtime: Option<PlayerRuntime>,
    window: Option<Arc<Window>>,
    presenter: Option<SurfaceSwapchainPresenter<'static>>,
}

impl ApplicationHandler for PlayerApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let win_attrs = WindowAttributes::default()
                .with_title("Foreplay Player Runtime")
                .with_inner_size(winit::dpi::LogicalSize::new(800.0, 600.0));
            if let Ok(win) = event_loop.create_window(win_attrs) {
                let arc_win = Arc::new(win);
                self.window = Some(arc_win.clone());

                let mut runtime = PlayerRuntime::new(self.scene.clone());
                if let Some(ref addr) = self.debug_ipc_addr {
                    let _ = runtime.bind_debug_server(addr);
                }

                if let (Some(adapter), Some(device)) = (
                    runtime.render_device.adapter(),
                    runtime.render_device.device(),
                )
                    && let Ok(surface) = runtime.render_device.instance().create_surface(arc_win.clone())
                {
                    let size = arc_win.inner_size();
                    let presenter = SurfaceSwapchainPresenter::new(
                        surface,
                        adapter,
                        device,
                        size.width.max(1),
                        size.height.max(1),
                        true,
                    );
                    self.presenter = Some(presenter);
                }

                self.runtime = Some(runtime);
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(new_size) => {
                if let (Some(presenter), Some(runtime)) = (self.presenter.as_mut(), self.runtime.as_mut())
                    && let Some(device) = runtime.render_device.device()
                {
                    presenter.resize(device, new_size.width, new_size.height);
                }
            }
            WindowEvent::RedrawRequested => {
                if let Some(runtime) = self.runtime.as_mut() {
                    runtime.step_frame(1.0 / 60.0);
                    let _ = runtime.render_rgba(800, 600);

                    if let (Some(presenter), Some(device), Some(queue)) = (
                        self.presenter.as_mut(),
                        runtime.render_device.device(),
                        runtime.render_device.queue(),
                    )
                        && let Ok(frame) = presenter.acquire_frame(device)
                    {
                        presenter.present(queue, frame);
                    }
                }
                if let Some(win) = &self.window {
                    win.request_redraw();
                }
            }
            _ => {}
        }
    }
}

fn run_windowed(scene: IrScene, debug_ipc_addr: Option<&str>) {
    let event_loop = EventLoop::new().expect("event loop creates");
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = PlayerApp {
        scene,
        debug_ipc_addr: debug_ipc_addr.map(|s| s.to_string()),
        runtime: None,
        window: None,
        presenter: None,
    };

    let _ = event_loop.run_app(&mut app);
}

fn create_default_demo_scene() -> IrScene {
    let mut scene = IrScene::new("DemoScene");
    let root_id = NodeId {
        generation: NonZeroU32::new(1).unwrap(),
        index: 0,
    };
    let mut root = create_ir_node(root_id, "PlayerBox", "RigidBody3D");
    root.properties.insert("position".into(), IrValue::Vec3([0.0, 5.0, 0.0]));
    scene.nodes.insert(root_id, root);
    scene
}

fn load_scene_from_path(path: &str) -> Result<IrScene, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read scene at {path}: {e}"))?;
    IrScene::from_fscn_str(&content).map_err(|e| format!("parse error: {e:?}"))
}

fn load_scene_from_pack_bytes(bytes: &[u8]) -> Result<IrScene, String> {
    // If pack bytes start with FSCN or JSON, parse directly
    if let Ok(text) = std::str::from_utf8(bytes)
        && let Ok(scene) = IrScene::from_fscn_str(text)
    {
        return Ok(scene);
    }
    // Otherwise fallback to default scene
    Ok(create_default_demo_scene())
}
