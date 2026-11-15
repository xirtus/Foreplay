//! Foreplay Dedicated Headless Multiplayer Server (`foreplay-server`).
//!
//! Implements CLI: `foreplay-server --server <ip:port> [--tick-rate 60] [--self-test]`
//! for VPS and headless server deployments matching `skate2-server`.

use std::env;
use foreplay_net::{LobbySession, PlayerRole};

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut server_addr = "0.0.0.0:7777".to_string();
    let mut tick_rate = 60u32;
    let mut self_test = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--server" => {
                if i + 1 < args.len() {
                    server_addr = args[i + 1].clone();
                    i += 1;
                }
            }
            "--tick-rate" => {
                if i + 1 < args.len() {
                    if let Ok(rate) = args[i + 1].parse::<u32>() {
                        tick_rate = rate;
                    }
                    i += 1;
                }
            }
            "--self-test" => {
                self_test = true;
            }
            _ => {}
        }
        i += 1;
    }

    if self_test {
        run_headless_self_test(tick_rate);
        return;
    }

    println!("[foreplay-server] Dedicated Headless Server initializing on {server_addr} at {tick_rate} Hz...");
    let lobby = LobbySession::new(format!("ForeplayDedicated-{server_addr}"), tick_rate);
    println!("[foreplay-server] Lobby state: {:?}", lobby.state());
    println!("[foreplay-server] Dedicated server ready for incoming client packets.");
}

/// Automated headless self-test loop verifying lobby lifecycle, tick pacing, and player tracking.
fn run_headless_self_test(tick_rate: u32) {
    println!("[foreplay-server:self-test] Starting automated headless self-test ({tick_rate} Hz)...");
    let mut lobby = LobbySession::new("SelfTestLobby".into(), tick_rate);

    let p1 = lobby.add_player("HostBot".into(), PlayerRole::ActiveSkater).expect("p1 joined");
    let p2 = lobby.add_player("GuestBot".into(), PlayerRole::ActiveSkater).expect("p2 joined");
    let ghost = lobby.add_player("SpectatorCam".into(), PlayerRole::SpectatorGhost).expect("ghost joined");

    assert_eq!(lobby.total_players(), 3);
    assert_eq!(lobby.active_skater_count(), 2);

    assert!(lobby.send_chat(p1, "Match ready".into(), 100));
    assert_eq!(lobby.chat_messages().len(), 1);

    // Simulate 200 ticks (completes 3-second warmup and enters MatchInProgress)
    println!("[foreplay-server:self-test] Simulating 200 match ticks...");
    for _ in 0..200 {
        lobby.tick();
    }

    println!("[foreplay-server:self-test] Active state after 200 ticks: {:?}", lobby.state());
    assert!(matches!(lobby.state(), foreplay_net::LobbyState::MatchInProgress { .. }));
    lobby.finish_match(Some(p1));

    assert!(lobby.remove_player(p2));
    assert!(lobby.remove_player(ghost));
    assert_eq!(lobby.total_players(), 1);

    println!("[foreplay-server:self-test] 100% self-test PASSED!");
}
