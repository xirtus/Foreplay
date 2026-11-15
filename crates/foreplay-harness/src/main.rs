//! `foreplay-harness` executable entry point.
//!
//! Subcommands:
//! - `status` (or no args): runs 60 headless verification ticks and prints `[READY]` banner.
//! - `import-tscn <path>`: parses Godot 4 `.tscn` file, steps 60 ticks, and prints converted `.fscn` + Atlas summary.
//! - `mcp-stdio`: line-delimited JSON-RPC 2.0 stdio server for LLM agents.

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
        other => {
            eprintln!("Unknown subcommand '{other}'. Valid options: status, import-tscn <path>, mcp-stdio");
            std::process::exit(1);
        }
    }
}
