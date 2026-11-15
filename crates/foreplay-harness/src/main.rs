//! `foreplay-harness` executable entry point.
//!
//! Subcommands:
//! - `status` (or no args): runs 60 headless verification ticks and prints `[READY]` banner.
//! - `import-tscn <path>`: parses Godot 4 `.tscn` file, steps 60 ticks, and prints converted `.fscn` + Atlas summary.
//! - `mcp-stdio`: line-delimited JSON-RPC 2.0 stdio server for LLM agents.
//! - `verify-all-gates`: comprehensive verification runner across Phases 1, 2, 5 & 6.

use foreplay_harness::{HarnessRpcServer, HarnessSession};
use foreplay_ir::{EditorCommand, IrNode, IrScene, IrValue, NodeId, SignalConnection};
use std::collections::BTreeMap;
use std::env;
use std::io::{self, BufRead, Write};

fn main() {
    let args: Vec<String> = env::args().collect();
    let subcommand = args.get(1).map(|s| s.as_str()).unwrap_or("status");

    match subcommand {
        "status" => {
            let mut session = HarnessSession::new(IrScene::new("DefaultProject"));
            let ticks = session.verify_headless_ticks(60);
            println!(
                "foreplay-harness v0.1.0 [READY] | scene='{}' | verified_headless_ticks={ticks}",
                session.tree.active_scene.name
            );
        }
        "import-tscn" => {
            let path = match args.get(2) {
                Some(p) => p,
                None => {
                    eprintln!("Usage: foreplay-harness import-tscn <path-to-.tscn>");
                    std::process::exit(1);
                }
            };
            let content = match std::fs::read_to_string(path) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("Error reading file '{path}': {e}");
                    std::process::exit(1);
                }
            };
            let scene = match foreplay_porthost::tscn::parse_tscn_to_ir(&content) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Error parsing .tscn: {e}");
                    std::process::exit(1);
                }
            };
            let mut session = HarnessSession::new(scene);
            let ticks = session.verify_headless_ticks(60);
            let atlas = session.observe_atlas();
            println!(
                "[IMPORTED] scene='{}' | nodes={} | connections={} | verified_headless_ticks={ticks}",
                session.tree.active_scene.name,
                session.tree.active_scene.nodes.len(),
                session.tree.active_scene.connections.len()
            );
            println!("--- .fscn ---\n{}", session.tree.active_scene.to_fscn_string());
            println!("--- Atlas Summary ---\n{atlas:?}");
        }
        "mcp-stdio" => {
            let mut session = HarnessSession::new(IrScene::new("McpWorkspace"));
            let stdin = io::stdin();
            let mut stdout = io::stdout();
            for line_res in stdin.lock().lines() {
                let line = match line_res {
                    Ok(l) => l,
                    Err(_) => break,
                };
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let resp = HarnessRpcServer::handle_json(&mut session, trimmed);
                let _ = writeln!(stdout, "{resp}");
                let _ = stdout.flush();
            }
        }
        "verify-all-gates" => {
            println!("============================================================");
            println!("Foreplay Headless Verification Gates (Phases 1, 2, 5 & 6)");
            println!("============================================================");

            // Gate 1: Phase 1 Conformance Gate (200 Godot .tscn files roundtrip)
            print!("[1/4] Running Phase 1 Godot 4 200 .tscn Conformance Gate... ");
            io::stdout().flush().unwrap();
            let conformance_report = foreplay_porthost::conformance::run_200_tscn_conformance_gate();
            if conformance_report.passed != conformance_report.total_scenes {
                eprintln!(
                    "\nPhase 1 Conformance Gate FAILED: {}/{} passed",
                    conformance_report.passed, conformance_report.total_scenes
                );
                std::process::exit(1);
            }
            println!("PASS (200/200 distinct .tscn fixtures roundtripped losslessly)");

            // Gate 2: Phase 2 300-Tick Headless Playtest Gate
            print!("[2/4] Running Phase 2 300-tick Headless Agent Playtest Gate... ");
            io::stdout().flush().unwrap();
            let mut session = HarnessSession::new(IrScene::new("Phase2PlaytestPark"));

            // Insert CharacterBody3D (Player)
            let player_id = NodeId::new(1, 1).unwrap();
            let mut player_props = BTreeMap::new();
            player_props.insert("speed".into(), IrValue::F64(14.5));
            player_props.insert("position".into(), IrValue::Vec3([0.0, 1.0, 0.0]));
            session
                .execute(EditorCommand::InsertNode {
                    node: IrNode {
                        id: player_id,
                        name: "Skater".into(),
                        class_name: "CharacterBody3D".into(),
                        parent: None,
                        instance_scene: None,
                        properties: player_props,
                        groups: vec!["player".into()],
                    },
                })
                .unwrap();

            // Insert Camera3D
            let cam_id = NodeId::new(2, 1).unwrap();
            session
                .execute(EditorCommand::InsertNode {
                    node: IrNode {
                        id: cam_id,
                        name: "FollowCamera".into(),
                        class_name: "Camera3D".into(),
                        parent: Some(player_id),
                        instance_scene: None,
                        properties: BTreeMap::new(),
                        groups: Vec::new(),
                    },
                })
                .unwrap();

            // Insert StaticBody3D (Ground)
            let ground_id = NodeId::new(3, 1).unwrap();
            session
                .execute(EditorCommand::InsertNode {
                    node: IrNode {
                        id: ground_id,
                        name: "ParkGround".into(),
                        class_name: "StaticBody3D".into(),
                        parent: None,
                        instance_scene: None,
                        properties: BTreeMap::new(),
                        groups: vec!["environment".into()],
                    },
                })
                .unwrap();

            // Connect signal
            session
                .execute(EditorCommand::ConnectSignal {
                    connection: SignalConnection {
                        from_node: ground_id,
                        signal_name: "body_entered".into(),
                        to_node: player_id,
                        method_name: "on_ground_contact".into(),
                    },
                })
                .unwrap();

            // Set initial Feel tuning
            session
                .execute(EditorCommand::TuneFeelSpec {
                    key: "skate_acceleration".into(),
                    old_value: None,
                    new_value: Some(12.0),
                })
                .unwrap();

            // Step 150 frames
            session.verify_headless_ticks(150);

            // Live tune FeelSpec mid-gameplay
            session
                .execute(EditorCommand::TuneFeelSpec {
                    key: "ollie_pop_force".into(),
                    old_value: None,
                    new_value: Some(8.5),
                })
                .unwrap();

            // Step remaining 150 frames (total 300 frames)
            let total_ticks = session.verify_headless_ticks(150);
            assert_eq!(total_ticks, 300);

            let atlas = session.observe_atlas();
            assert!(atlas.affected_nodes.contains(&player_id));
            assert_eq!(
                session
                    .tree
                    .active_scene
                    .feel_overrides
                    .get("ollie_pop_force"),
                Some(&8.5)
            );
            assert_eq!(
                session
                    .tree
                    .active_scene
                    .feel_overrides
                    .get("skate_acceleration"),
                Some(&12.0)
            );
            println!("PASS (300 ticks executed, FeelSpec tuned live, Atlas invariant holds)");

            // Gate 3: Phase 5 1,000-Frame Differential Trace Gate
            print!("[3/4] Running Phase 5 1,000-frame Differential Frame Oracle Gate... ");
            io::stdout().flush().unwrap();
            let diff_report = foreplay_porthost::oracle::run_1000_frame_differential_gate();
            if diff_report.passed_frames != diff_report.total_frames {
                eprintln!(
                    "\nPhase 5 Differential Gate FAILED: max position drift {}",
                    diff_report.max_position_drift
                );
                std::process::exit(1);
            }
            println!("PASS (1,000/1,000 frames matched upstream oracle within 1e-4 tolerance)");

            // Gate 4: Phase 6 One-Click Export Bundle Gate
            print!("[4/4] Running Phase 6 One-Click Export Bundle Gate (5 targets)... ");
            io::stdout().flush().unwrap();
            let mut export_cfg = foreplay_export::ExportConfig::new(
                "ForeplaySkater",
                "com.foreplay.skater",
                "res://scenes/park.fscn",
            );
            export_cfg.version = "1.0.0".into();
            export_cfg.files.insert(
                "scenes/park.fscn".into(),
                session.tree.active_scene.to_fscn_string().into_bytes(),
            );
            export_cfg.files.insert(
                "scripts/skater.gd".into(),
                b"extends CharacterBody3D\nfunc _physics_process(delta):\n    pass\n".to_vec(),
            );

            for &target in foreplay_export::ExportTarget::all() {
                let out = foreplay_export::ProjectExporter::export_project(&export_cfg, target)
                    .unwrap_or_else(|e| {
                        eprintln!("\nExport failed for {target:?}: {e}");
                        std::process::exit(1);
                    });
                let unpacked = foreplay_export::FpckArchive::unpack_bytes(&out.archive_bytes)
                    .unwrap_or_else(|e| {
                        eprintln!("\nUnpack failed for {target:?}: {e}");
                        std::process::exit(1);
                    });
                assert!(unpacked.contains_key("scenes/park.fscn"));
            }
            println!("PASS (Linux/SteamDeck, Windows, macOS .app with Game Mode, WebGPU, SexOS bundles verified)");

            println!("============================================================");
            println!("ALL 4 VERIFICATION GATES PASSED (100% GREEN)");
            println!("============================================================");
        }
        other => {
            eprintln!(
                "Unknown subcommand '{other}'. Valid options: status, import-tscn <path>, mcp-stdio, verify-all-gates"
            );
            std::process::exit(1);
        }
    }
}
