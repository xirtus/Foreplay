//! `foreplay-harness` executable entry point.
//!
//! Subcommands:
//! - `status` (or no args): runs 60 headless verification ticks and prints `[READY]` banner.
//! - `import-tscn <path>`: parses Godot 4 `.tscn` file, steps 60 ticks, and prints converted `.fscn` + Atlas summary.
//! - `mcp-stdio`: line-delimited JSON-RPC 2.0 stdio server for LLM agents.
//! - `verify-all-gates`: comprehensive verification runner across Phases 1, 2, 5 & 6.

use foreplay_harness::{HarnessRpcServer, HarnessSession};
use foreplay_ir::IrScene;
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
            println!("Foreplay 8-Gate Headless Verification Suite");
            println!("============================================================");

            let all_report = foreplay_harness::run_all_verification_gates();

            for gate in &all_report.gates {
                let status_str = if gate.passed { "PASS" } else { "FAIL" };
                println!(
                    "[{}/8] {} ({status_str} in {}ms)",
                    gate.gate_number, gate.name, gate.duration_ms
                );
                println!("      Details: {}", gate.details);
            }

            println!("============================================================");
            if all_report.all_passed() {
                println!(
                    "ALL 8 VERIFICATION GATES PASSED (100% GREEN) in {}ms",
                    all_report.total_duration_ms
                );
                println!("============================================================");
            } else {
                eprintln!(
                    "VERIFICATION GATES FAILED: {}/{} passed, {} failed",
                    all_report.passed_gates, all_report.total_gates, all_report.failed_gates
                );
                println!("============================================================");
                std::process::exit(1);
            }
        }
        other => {
            eprintln!(
                "Unknown subcommand '{other}'. Valid options: status, import-tscn <path>, mcp-stdio, verify-all-gates"
            );
            std::process::exit(1);
        }
    }
}
