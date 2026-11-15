//! Loopback TCP JSON-Lines Remote Play-In-Editor (PIE) Debug Protocol.
//!
//! Provides two-way communication between the live standalone player and editor/dev tools:
//! - Breakpoint suspension and inspection
//! - Node property inspection and mutation
//! - Feel knob live tuning
//! - Dynamic GDScript hot reloading
//! - Real-time runtime telemetry (tick, FPS, physics bodies, coroutines, GPU/audio state)

use foreplay_ir::IrValue;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};

/// Command sent from debug client / editor to the running game player.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum DebugCommand {
    Ping,
    GetSceneTree,
    SetBreakpoint {
        script_path: String,
        line: usize,
        enabled: bool,
    },
    ClearBreakpoints,
    Pause,
    Resume,
    StepFrame,
    SetNodeProperty {
        node_id: u64,
        key: String,
        value: IrValue,
    },
    SetFeelKnob {
        name: String,
        value: f64,
    },
    HotReloadGdScript {
        node_id: u64,
        source: String,
    },
    GetTelemetry,
    Shutdown,
}

/// Lightweight summary of a scene node sent across the debug IPC.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DebugNodeSummary {
    pub id: u64,
    pub name: String,
    pub class_name: String,
    pub parent_id: Option<u64>,
    pub child_count: usize,
    pub properties: BTreeMap<String, IrValue>,
}

/// Event or reply sent from the running game player to the debug client.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum DebugEvent {
    Pong,
    SceneTreeSnapshot {
        scene_name: String,
        node_count: usize,
        nodes: Vec<DebugNodeSummary>,
    },
    PausedAtBreakpoint {
        node_id: u64,
        script_path: String,
        line: usize,
        locals: BTreeMap<String, IrValue>,
        call_stack: Vec<String>,
    },
    Telemetry {
        tick: u64,
        paused: bool,
        fps_estimate: f32,
        active_physics_bodies: usize,
        active_coroutines: usize,
        gpu_adapter: String,
        audio_backend: String,
        gdext_classes_registered: usize,
    },
    Ack {
        message: String,
    },
    Error {
        message: String,
    },
}

/// Errors produced during debug IPC communication.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DebugIpcError {
    #[error("I/O error: {0}")]
    Io(String),

    #[error("serialization error: {0}")]
    Serialization(String),

    #[error("disconnected from debug peer")]
    Disconnected,

    #[error("protocol error: {0}")]
    Protocol(String),
}

/// Non-blocking TCP server hosting the PIE debug protocol inside the player runtime.
#[derive(Debug)]
pub struct DebugIpcServer {
    listener: TcpListener,
    active_client: Option<TcpStream>,
    read_buffer: Vec<u8>,
}

impl DebugIpcServer {
    /// Binds a TCP debug server to the given address (e.g. `"127.0.0.1:0"` for an ephemeral port).
    pub fn bind(addr: &str) -> Result<Self, DebugIpcError> {
        let listener = TcpListener::bind(addr)
            .map_err(|e| DebugIpcError::Io(format!("failed to bind listener at {addr}: {e}")))?;
        listener
            .set_nonblocking(true)
            .map_err(|e| DebugIpcError::Io(format!("failed to set nonblocking on listener: {e}")))?;

        Ok(Self {
            listener,
            active_client: None,
            read_buffer: Vec::new(),
        })
    }

    /// Returns the local socket address this server is listening on.
    pub fn local_addr(&self) -> Result<SocketAddr, DebugIpcError> {
        self.listener
            .local_addr()
            .map_err(|e| DebugIpcError::Io(format!("failed to get local_addr: {e}")))
    }

    /// Polls for incoming client connections or incoming commands from connected client.
    ///
    /// Non-blocking: returns `Ok(None)` if no command is ready yet.
    pub fn poll_command(&mut self) -> Result<Option<DebugCommand>, DebugIpcError> {
        // 1. Check for new connection if none active or current disconnected
        match self.listener.accept() {
            Ok((stream, _)) => {
                let _ = stream.set_nonblocking(true);
                let _ = stream.set_nodelay(true);
                self.active_client = Some(stream);
                self.read_buffer.clear();
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // No incoming connection
            }
            Err(e) => {
                return Err(DebugIpcError::Io(format!("listener accept error: {e}")));
            }
        }

        // 2. Read from active client if present
        let Some(stream) = self.active_client.as_mut() else {
            return Ok(None);
        };

        let mut chunk = [0u8; 4096];
        match stream.read(&mut chunk) {
            Ok(0) => {
                // Client disconnected
                self.active_client = None;
                self.read_buffer.clear();
                return Ok(None);
            }
            Ok(n) => {
                self.read_buffer.extend_from_slice(&chunk[..n]);
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // No new data
            }
            Err(e) => {
                self.active_client = None;
                self.read_buffer.clear();
                return Err(DebugIpcError::Io(format!("client read error: {e}")));
            }
        }

        // 3. Check for newline-delimited JSON command in read_buffer
        if let Some(newline_pos) = self.read_buffer.iter().position(|&b| b == b'\n') {
            let line_bytes = self.read_buffer.drain(..=newline_pos).collect::<Vec<u8>>();
            let line_str = std::str::from_utf8(&line_bytes)
                .map_err(|e| DebugIpcError::Protocol(format!("invalid utf-8 command: {e}")))?
                .trim();

            if line_str.is_empty() {
                return Ok(None);
            }

            let cmd = serde_json::from_str::<DebugCommand>(line_str)
                .map_err(|e| DebugIpcError::Serialization(format!("failed to parse command: {e}")))?;
            return Ok(Some(cmd));
        }

        Ok(None)
    }

    /// Sends a newline-delimited JSON `DebugEvent` reply to the active client.
    pub fn send_event(&mut self, event: &DebugEvent) -> Result<(), DebugIpcError> {
        let Some(stream) = self.active_client.as_mut() else {
            return Ok(()); // Dropped if no client connected
        };

        let json_line = serde_json::to_string(event)
            .map_err(|e| DebugIpcError::Serialization(format!("failed to serialize event: {e}")))?;

        stream
            .write_all(json_line.as_bytes())
            .and_then(|_| stream.write_all(b"\n"))
            .and_then(|_| stream.flush())
            .map_err(|e| DebugIpcError::Io(format!("failed to send event: {e}")))?;

        Ok(())
    }

    /// Returns true if a client is currently connected.
    pub fn is_client_connected(&self) -> bool {
        self.active_client.is_some()
    }
}

/// Synchronous debug client used by editor tools or self-tests to drive the PIE session.
#[derive(Debug)]
pub struct DebugIpcClient {
    stream: TcpStream,
    reader: BufReader<TcpStream>,
}

impl DebugIpcClient {
    /// Connects to a running `DebugIpcServer` at the specified socket address.
    pub fn connect(addr: SocketAddr) -> Result<Self, DebugIpcError> {
        let stream = TcpStream::connect(addr)
            .map_err(|e| DebugIpcError::Io(format!("failed to connect to {addr}: {e}")))?;
        let _ = stream.set_nodelay(true);
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .map_err(|e| DebugIpcError::Io(format!("failed to set read timeout: {e}")))?;

        let reader_stream = stream
            .try_clone()
            .map_err(|e| DebugIpcError::Io(format!("failed to clone stream: {e}")))?;

        Ok(Self {
            stream,
            reader: BufReader::new(reader_stream),
        })
    }

    /// Sends a `DebugCommand` as a newline-delimited JSON string.
    pub fn send_command(&mut self, cmd: &DebugCommand) -> Result<(), DebugIpcError> {
        let json_line = serde_json::to_string(cmd)
            .map_err(|e| DebugIpcError::Serialization(format!("failed to serialize command: {e}")))?;

        self.stream
            .write_all(json_line.as_bytes())
            .and_then(|_| self.stream.write_all(b"\n"))
            .and_then(|_| self.stream.flush())
            .map_err(|e| DebugIpcError::Io(format!("failed to send command: {e}")))?;

        Ok(())
    }

    /// Reads a newline-delimited `DebugEvent` from the server.
    pub fn read_event(&mut self) -> Result<DebugEvent, DebugIpcError> {
        let mut line = String::new();
        let bytes_read = self
            .reader
            .read_line(&mut line)
            .map_err(|e| DebugIpcError::Io(format!("failed to read line: {e}")))?;

        if bytes_read == 0 {
            return Err(DebugIpcError::Disconnected);
        }

        let trimmed = line.trim();
        let event = serde_json::from_str::<DebugEvent>(trimmed)
            .map_err(|e| DebugIpcError::Serialization(format!("failed to deserialize event '{trimmed}': {e}")))?;

        Ok(event)
    }

    /// Alias for `read_event`.
    pub fn recv_event(&mut self) -> Result<DebugEvent, DebugIpcError> {
        self.read_event()
    }

    /// Sends a command and awaits its immediate reply.
    pub fn request(&mut self, cmd: &DebugCommand) -> Result<DebugEvent, DebugIpcError> {
        self.send_command(cmd)?;
        self.read_event()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_ping_pong_ipc() {
        let mut server = DebugIpcServer::bind("127.0.0.1:0").expect("server binds");
        let addr = server.local_addr().expect("valid local addr");

        let mut client = DebugIpcClient::connect(addr).expect("client connects");

        client.send_command(&DebugCommand::Ping).expect("client sends ping");

        // Server receives command
        let mut received = None;
        for _ in 0..100 {
            if let Some(cmd) = server.poll_command().expect("poll succeeds") {
                received = Some(cmd);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        assert_eq!(received, Some(DebugCommand::Ping));

        // Server replies with Pong
        server.send_event(&DebugEvent::Pong).expect("server sends pong");

        // Client reads Pong
        let reply = client.read_event().expect("client reads pong");
        assert_eq!(reply, DebugEvent::Pong);
    }

    #[test]
    fn command_event_serialization_roundtrip() {
        let cmd = DebugCommand::SetBreakpoint {
            script_path: "res://scripts/player.gd".into(),
            line: 42,
            enabled: true,
        };
        let serialized = serde_json::to_string(&cmd).unwrap();
        let deserialized: DebugCommand = serde_json::from_str(&serialized).unwrap();
        assert_eq!(cmd, deserialized);

        let event = DebugEvent::PausedAtBreakpoint {
            node_id: 101,
            script_path: "res://scripts/player.gd".into(),
            line: 42,
            locals: BTreeMap::from([
                ("speed".into(), IrValue::F64(12.5)),
                ("on_floor".into(), IrValue::Bool(true)),
            ]),
            call_stack: vec!["_physics_process:42".into(), "_process:10".into()],
        };
        let event_serialized = serde_json::to_string(&event).unwrap();
        let event_deserialized: DebugEvent = serde_json::from_str(&event_serialized).unwrap();
        assert_eq!(event, event_deserialized);
    }

    #[test]
    fn multi_command_stream_buffering() {
        let mut server = DebugIpcServer::bind("127.0.0.1:0").expect("server binds");
        let addr = server.local_addr().expect("valid local addr");
        let mut client = DebugIpcClient::connect(addr).expect("client connects");

        // Send two commands back-to-back
        client.send_command(&DebugCommand::Pause).expect("send pause");
        client.send_command(&DebugCommand::Resume).expect("send resume");

        let mut received = Vec::new();
        for _ in 0..100 {
            while let Some(cmd) = server.poll_command().expect("poll succeeds") {
                received.push(cmd);
            }
            if received.len() >= 2 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        assert_eq!(received.len(), 2);
        assert_eq!(received[0], DebugCommand::Pause);
        assert_eq!(received[1], DebugCommand::Resume);
    }

    #[test]
    fn client_reconnect_lifecycle() {
        let mut server = DebugIpcServer::bind("127.0.0.1:0").expect("server binds");
        let addr = server.local_addr().expect("valid local addr");

        // Client 1 connects and disconnects
        {
            let mut client1 = DebugIpcClient::connect(addr).expect("client1 connects");
            client1.send_command(&DebugCommand::Ping).expect("client1 pings");
            for _ in 0..100 {
                if let Some(cmd) = server.poll_command().expect("poll succeeds") {
                    assert_eq!(cmd, DebugCommand::Ping);
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        } // client1 dropped

        // Server detects drop or accepts new client
        std::thread::sleep(std::time::Duration::from_millis(10));
        let _ = server.poll_command();

        // Client 2 connects to the same server
        let mut client2 = DebugIpcClient::connect(addr).expect("client2 connects");
        client2.send_command(&DebugCommand::GetTelemetry).expect("client2 sends cmd");

        let mut received2 = None;
        for _ in 0..100 {
            if let Some(cmd) = server.poll_command().expect("poll succeeds") {
                received2 = Some(cmd);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        assert_eq!(received2, Some(DebugCommand::GetTelemetry));
    }
}
