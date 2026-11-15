//! Compact replay-frame streaming (`skate2-rust-port` desync-proof lobby model)
//! + lockstep netcode, zero-panic binary serialization, and jitter buffer interpolation.

use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use foreplay_ir::NodeId;

/// Maximum simultaneous player limit in a multiplayer session.
pub const MAX_PLAYERS: usize = 8;
/// Maximum message history in the lobby chat ring.
pub const MAX_CHAT_MESSAGES: usize = 64;

/// Compact per-entity state/replay frame streamed across jittery links without global sim desync.
#[derive(Clone, PartialEq, Debug)]
pub struct CompactReplayFrame {
    pub tick: u64,
    pub entity: NodeId,
    pub position: [f32; 3],
    pub rotation_quat: [f32; 4],
    pub linear_velocity: [f32; 3],
}

/// Errors occurring during binary replay frame wire decoding or lobby operations.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ReplayNetError {
    BufferTooShort,
    InvalidMagic,
    UnsupportedVersion(u16),
    InvalidNodeGeneration,
    NonFiniteFloat,
    InvalidRotationQuaternion,
}

impl fmt::Display for ReplayNetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BufferTooShort => write!(f, "Wire buffer too short for replay frame"),
            Self::InvalidMagic => write!(f, "Invalid replay frame header magic"),
            Self::UnsupportedVersion(v) => write!(f, "Unsupported replay protocol version: {v}"),
            Self::InvalidNodeGeneration => write!(f, "NodeId generation must be non-zero"),
            Self::NonFiniteFloat => write!(f, "Replay frame rejected non-finite float (NaN or Inf)"),
            Self::InvalidRotationQuaternion => write!(f, "Replay quaternion is degenerate or zero-length"),
        }
    }
}

impl std::error::Error for ReplayNetError {}

impl CompactReplayFrame {
    /// Zero-panic binary wire encoding:
    /// Magic (4B) + Version (2B) + Tick (8B) + NodeIndex (4B) + NodeGen (4B) + Pos (12B) + Quat (16B) + Vel (12B) = 62 Bytes.
    #[must_use]
    pub fn encode_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(62);
        // Magic
        bytes.extend_from_slice(b"FPRF");
        // Version 1 LE
        bytes.extend_from_slice(&1u16.to_le_bytes());
        // Tick LE
        bytes.extend_from_slice(&self.tick.to_le_bytes());
        // Entity NodeId
        bytes.extend_from_slice(&self.entity.index.to_le_bytes());
        bytes.extend_from_slice(&self.entity.generation.get().to_le_bytes());
        // Position
        for c in self.position {
            bytes.extend_from_slice(&c.to_le_bytes());
        }
        // Rotation Quaternion
        for c in self.rotation_quat {
            bytes.extend_from_slice(&c.to_le_bytes());
        }
        // Linear Velocity
        for c in self.linear_velocity {
            bytes.extend_from_slice(&c.to_le_bytes());
        }
        bytes
    }

    /// Zero-panic binary wire decoding with endian safety, non-zero generation validation,
    /// and strict rejection of NaN / Inf coordinates.
    pub fn decode_bytes(bytes: &[u8]) -> Result<Self, ReplayNetError> {
        if bytes.len() < 62 {
            return Err(ReplayNetError::BufferTooShort);
        }

        if &bytes[0..4] != b"FPRF" {
            return Err(ReplayNetError::InvalidMagic);
        }

        let version = u16::from_le_bytes([bytes[4], bytes[5]]);
        if version != 1 {
            return Err(ReplayNetError::UnsupportedVersion(version));
        }

        let tick = u64::from_le_bytes([
            bytes[6], bytes[7], bytes[8], bytes[9],
            bytes[10], bytes[11], bytes[12], bytes[13],
        ]);

        let node_index = u32::from_le_bytes([bytes[14], bytes[15], bytes[16], bytes[17]]);
        let node_gen = u32::from_le_bytes([bytes[18], bytes[19], bytes[20], bytes[21]]);
        let entity = NodeId::new(node_index, node_gen).ok_or(ReplayNetError::InvalidNodeGeneration)?;

        let mut offset = 22;
        let mut read_f32 = || {
            let f = f32::from_le_bytes([
                bytes[offset],
                bytes[offset + 1],
                bytes[offset + 2],
                bytes[offset + 3],
            ]);
            offset += 4;
            if f.is_finite() {
                Ok(f)
            } else {
                Err(ReplayNetError::NonFiniteFloat)
            }
        };

        let pos = [read_f32()?, read_f32()?, read_f32()?];
        let quat = [read_f32()?, read_f32()?, read_f32()?, read_f32()?];
        let vel = [read_f32()?, read_f32()?, read_f32()?];

        let quat_len_sq = quat[0] * quat[0] + quat[1] * quat[1] + quat[2] * quat[2] + quat[3] * quat[3];
        if quat_len_sq <= 1e-7 {
            return Err(ReplayNetError::InvalidRotationQuaternion);
        }

        Ok(Self {
            tick,
            entity,
            position: pos,
            rotation_quat: quat,
            linear_velocity: vel,
        })
    }
}

/// Jitter buffer storing recent `CompactReplayFrame` snapshots per entity,
/// performing smooth position Lerp and normalized Quaternion Slerp across packet loss.
#[derive(Clone, Debug, Default)]
pub struct ReplayInterpolationBuffer {
    history: BTreeMap<NodeId, Vec<CompactReplayFrame>>,
    max_history_frames: usize,
}

impl ReplayInterpolationBuffer {
    /// Create a replay buffer retaining up to `max_history_frames` per entity.
    #[must_use]
    pub fn new(max_history_frames: usize) -> Self {
        Self {
            history: BTreeMap::new(),
            max_history_frames: max_history_frames.max(4),
        }
    }

    /// Record an incoming replay frame, keeping frames ordered by tick.
    pub fn push_frame(&mut self, frame: CompactReplayFrame) {
        let entry = self.history.entry(frame.entity).or_default();
        if let Some(pos) = entry.iter().position(|f| f.tick == frame.tick) {
            entry[pos] = frame;
        } else {
            let idx = entry.binary_search_by_key(&frame.tick, |f| f.tick).unwrap_or_else(|i| i);
            entry.insert(idx, frame);
        }

        if entry.len() > self.max_history_frames {
            entry.remove(0);
        }
    }

    /// Sample interpolated entity state at a continuous floating-point render tick.
    ///
    /// Performs linear position/velocity interpolation and spherical quaternion slerp.
    /// Bridges packet drops and tick jitter smoothly.
    #[must_use]
    pub fn sample_at_tick(&self, entity: NodeId, render_tick: f64) -> Option<CompactReplayFrame> {
        let frames = self.history.get(&entity)?;
        if frames.is_empty() {
            return None;
        }

        if frames.len() == 1 || render_tick <= frames[0].tick as f64 {
            return Some(frames[0].clone());
        }

        if render_tick >= frames[frames.len() - 1].tick as f64 {
            return Some(frames[frames.len() - 1].clone());
        }

        // Find surrounding interval
        let mut idx = 0;
        while idx + 1 < frames.len() && (frames[idx + 1].tick as f64) < render_tick {
            idx += 1;
        }

        let f0 = &frames[idx];
        let f1 = &frames[idx + 1];

        let dt = (f1.tick - f0.tick) as f64;
        let t = if dt > 0.0 {
            ((render_tick - f0.tick as f64) / dt).clamp(0.0, 1.0) as f32
        } else {
            0.0
        };

        let pos = [
            f0.position[0] + (f1.position[0] - f0.position[0]) * t,
            f0.position[1] + (f1.position[1] - f0.position[1]) * t,
            f0.position[2] + (f1.position[2] - f0.position[2]) * t,
        ];

        let vel = [
            f0.linear_velocity[0] + (f1.linear_velocity[0] - f0.linear_velocity[0]) * t,
            f0.linear_velocity[1] + (f1.linear_velocity[1] - f0.linear_velocity[1]) * t,
            f0.linear_velocity[2] + (f1.linear_velocity[2] - f0.linear_velocity[2]) * t,
        ];

        let quat = slerp4(f0.rotation_quat, f1.rotation_quat, t);

        Some(CompactReplayFrame {
            tick: render_tick.round() as u64,
            entity,
            position: pos,
            rotation_quat: quat,
            linear_velocity: vel,
        })
    }

    /// Prune all frames older than `min_tick`.
    pub fn prune_older_than(&mut self, min_tick: u64) {
        for frames in self.history.values_mut() {
            frames.retain(|f| f.tick >= min_tick);
        }
    }
}

/// Normalized spherical linear quaternion interpolation.
fn slerp4(q0: [f32; 4], mut q1: [f32; 4], t: f32) -> [f32; 4] {
    let mut dot = q0[0] * q1[0] + q0[1] * q1[1] + q0[2] * q1[2] + q0[3] * q1[3];
    if dot < 0.0 {
        q1[0] = -q1[0];
        q1[1] = -q1[1];
        q1[2] = -q1[2];
        q1[3] = -q1[3];
        dot = -dot;
    }

    if dot > 0.9995 {
        let out = [
            q0[0] + (q1[0] - q0[0]) * t,
            q0[1] + (q1[1] - q0[1]) * t,
            q0[2] + (q1[2] - q0[2]) * t,
            q0[3] + (q1[3] - q0[3]) * t,
        ];
        normalize4(out)
    } else {
        let theta = dot.clamp(-1.0, 1.0).acos();
        let sin_theta = theta.sin();
        let w0 = ((1.0 - t) * theta).sin() / sin_theta;
        let w1 = (t * theta).sin() / sin_theta;
        let out = [
            q0[0] * w0 + q1[0] * w1,
            q0[1] * w0 + q1[1] * w1,
            q0[2] * w0 + q1[2] * w1,
            q0[3] * w0 + q1[3] * w1,
        ];
        normalize4(out)
    }
}

fn normalize4(q: [f32; 4]) -> [f32; 4] {
    let len_sq = q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3];
    if len_sq > 1e-8 {
        let inv = 1.0 / len_sq.sqrt();
        [q[0] * inv, q[1] * inv, q[2] * inv, q[3] * inv]
    } else {
        [0.0, 0.0, 0.0, 1.0]
    }
}

/// Participation role of a connected client in the lobby.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PlayerRole {
    ActiveSkater,
    SpectatorGhost,
}

/// Connected lobby player metadata.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LobbyPlayer {
    pub peer_id: u8,
    pub username: String,
    pub role: PlayerRole,
    pub ping_ms: u32,
}

/// Chat ring entry.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ChatMessage {
    pub sender_id: u8,
    pub sender_name: String,
    pub text: String,
    pub timestamp_ms: u64,
}

/// Headless server match lifecycle state.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LobbyState {
    WaitingForPlayers,
    Warmup { ticks_remaining: u32 },
    MatchInProgress { tick: u64 },
    MatchOver { winner_peer_id: Option<u8> },
}

/// Multiplayer session manager for up to 8 players with spectator ghosts,
/// chat ring buffer, and dedicated headless `--server` tick state machine.
#[derive(Debug)]
pub struct LobbySession {
    pub server_name: String,
    pub tick_rate_hz: u32,
    players: Vec<LobbyPlayer>,
    chat_ring: VecDeque<ChatMessage>,
    state: LobbyState,
}

impl LobbySession {
    /// Create a new lobby session.
    #[must_use]
    pub fn new(server_name: String, tick_rate_hz: u32) -> Self {
        Self {
            server_name,
            tick_rate_hz: tick_rate_hz.max(1),
            players: Vec::new(),
            chat_ring: VecDeque::with_capacity(MAX_CHAT_MESSAGES),
            state: LobbyState::WaitingForPlayers,
        }
    }

    /// Add a player to the session (up to `MAX_PLAYERS = 8`).
    pub fn add_player(&mut self, username: String, role: PlayerRole) -> Result<u8, &'static str> {
        if self.players.len() >= MAX_PLAYERS {
            return Err("Lobby is full (maximum 8 players)");
        }

        // Find lowest free peer_id
        let mut id = 0u8;
        while self.players.iter().any(|p| p.peer_id == id) {
            id += 1;
        }

        self.players.push(LobbyPlayer {
            peer_id: id,
            username,
            role,
            ping_ms: 0,
        });

        Ok(id)
    }

    /// Remove a player by peer ID.
    pub fn remove_player(&mut self, peer_id: u8) -> bool {
        let len_before = self.players.len();
        self.players.retain(|p| p.peer_id != peer_id);
        self.players.len() < len_before
    }

    /// Update player role (e.g. switch to SpectatorGhost).
    pub fn set_player_role(&mut self, peer_id: u8, role: PlayerRole) -> bool {
        if let Some(p) = self.players.iter_mut().find(|p| p.peer_id == peer_id) {
            p.role = role;
            true
        } else {
            false
        }
    }

    /// Number of active skaters (non-spectators).
    #[must_use]
    pub fn active_skater_count(&self) -> usize {
        self.players.iter().filter(|p| p.role == PlayerRole::ActiveSkater).count()
    }

    /// Total connected players and spectators.
    #[must_use]
    pub fn total_players(&self) -> usize {
        self.players.len()
    }

    /// Broadcast a chat message to the lobby ring buffer.
    pub fn send_chat(&mut self, sender_id: u8, text: String, timestamp_ms: u64) -> bool {
        let sender_name = match self.players.iter().find(|p| p.peer_id == sender_id) {
            Some(p) => p.username.clone(),
            None => return false,
        };

        if self.chat_ring.len() >= MAX_CHAT_MESSAGES {
            self.chat_ring.pop_front();
        }

        self.chat_ring.push_back(ChatMessage {
            sender_id,
            sender_name,
            text,
            timestamp_ms,
        });

        true
    }

    /// Access chat history.
    #[must_use]
    pub fn chat_messages(&self) -> &VecDeque<ChatMessage> {
        &self.chat_ring
    }

    /// Current server lifecycle state.
    #[must_use]
    pub const fn state(&self) -> LobbyState {
        self.state
    }

    /// Advance server tick state machine.
    pub fn tick(&mut self) {
        match self.state {
            LobbyState::WaitingForPlayers => {
                if self.active_skater_count() >= 2 {
                    self.state = LobbyState::Warmup {
                        ticks_remaining: self.tick_rate_hz * 3, // 3-second warmup
                    };
                }
            }
            LobbyState::Warmup { ticks_remaining } => {
                if ticks_remaining <= 1 {
                    self.state = LobbyState::MatchInProgress { tick: 0 };
                } else {
                    self.state = LobbyState::Warmup {
                        ticks_remaining: ticks_remaining - 1,
                    };
                }
            }
            LobbyState::MatchInProgress { tick } => {
                self.state = LobbyState::MatchInProgress { tick: tick + 1 };
            }
            LobbyState::MatchOver { .. } => {}
        }
    }

    /// Forcefully conclude the match and declare a winner.
    pub fn finish_match(&mut self, winner_peer_id: Option<u8>) {
        self.state = LobbyState::MatchOver { winner_peer_id };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replay_frame_binary_roundtrip() {
        let frame = CompactReplayFrame {
            tick: 1337,
            entity: NodeId::new(42, 3).unwrap(),
            position: [1.25, -2.5, 3.75],
            rotation_quat: [0.0, std::f32::consts::FRAC_1_SQRT_2, 0.0, std::f32::consts::FRAC_1_SQRT_2],
            linear_velocity: [10.0, 0.0, -5.0],
        };

        let encoded = frame.encode_bytes();
        assert_eq!(encoded.len(), 62);

        let decoded = CompactReplayFrame::decode_bytes(&encoded).expect("decode succeeds");
        assert_eq!(decoded.tick, frame.tick);
        assert_eq!(decoded.entity, frame.entity);
        assert_eq!(decoded.position, frame.position);
        assert_eq!(decoded.rotation_quat, frame.rotation_quat);
        assert_eq!(decoded.linear_velocity, frame.linear_velocity);
    }

    #[test]
    fn replay_frame_nan_and_degenerate_rejection() {
        let mut frame = CompactReplayFrame {
            tick: 1,
            entity: NodeId::new(1, 1).unwrap(),
            position: [f32::NAN, 0.0, 0.0],
            rotation_quat: [0.0, 0.0, 0.0, 1.0],
            linear_velocity: [0.0, 0.0, 0.0],
        };

        let encoded = frame.encode_bytes();
        assert_eq!(CompactReplayFrame::decode_bytes(&encoded), Err(ReplayNetError::NonFiniteFloat));

        // Zero-length quaternion
        frame.position = [0.0, 0.0, 0.0];
        frame.rotation_quat = [0.0, 0.0, 0.0, 0.0];
        let enc2 = frame.encode_bytes();
        assert_eq!(CompactReplayFrame::decode_bytes(&enc2), Err(ReplayNetError::InvalidRotationQuaternion));
    }

    #[test]
    fn jitter_buffer_interpolation_and_packet_loss() {
        let mut buffer = ReplayInterpolationBuffer::new(32);
        let id = NodeId::new(10, 1).unwrap();

        // Frame at tick 10: position (0, 0, 0), quat identity
        buffer.push_frame(CompactReplayFrame {
            tick: 10,
            entity: id,
            position: [0.0, 0.0, 0.0],
            rotation_quat: [0.0, 0.0, 0.0, 1.0],
            linear_velocity: [10.0, 0.0, 0.0],
        });

        // Frame at tick 20 (packet loss across ticks 11..19!): position (10, 0, 0)
        buffer.push_frame(CompactReplayFrame {
            tick: 20,
            entity: id,
            position: [10.0, 0.0, 0.0],
            rotation_quat: [0.0, 0.0, 0.0, 1.0],
            linear_velocity: [10.0, 0.0, 0.0],
        });

        // Sample at halfway tick 15.0
        let sample = buffer.sample_at_tick(id, 15.0).expect("interpolated sample");
        assert_eq!(sample.tick, 15);
        assert!((sample.position[0] - 5.0).abs() < 1e-4);
        assert!((sample.position[1] - 0.0).abs() < 1e-4);
        assert!((sample.linear_velocity[0] - 10.0).abs() < 1e-4);
    }

    #[test]
    fn lobby_session_lifecycle_and_spectator() {
        let mut lobby = LobbySession::new("SanVanelonaPark".into(), 60);
        assert_eq!(lobby.state(), LobbyState::WaitingForPlayers);

        let p1 = lobby.add_player("SkaterOne".into(), PlayerRole::ActiveSkater).unwrap();
        let _p2 = lobby.add_player("SkaterTwo".into(), PlayerRole::ActiveSkater).unwrap();
        let p3 = lobby.add_player("GhostCam".into(), PlayerRole::SpectatorGhost).unwrap();

        assert_eq!(lobby.total_players(), 3);
        assert_eq!(lobby.active_skater_count(), 2);

        // Chat ring
        assert!(lobby.send_chat(p1, "Kickflip the gap!".into(), 1000));
        assert_eq!(lobby.chat_messages().len(), 1);
        assert_eq!(lobby.chat_messages()[0].sender_name, "SkaterOne");

        // Tick: with 2 active skaters, transitions to Warmup
        lobby.tick();
        assert!(matches!(lobby.state(), LobbyState::Warmup { .. }));

        // Removing a player
        assert!(lobby.remove_player(p3));
        assert_eq!(lobby.total_players(), 2);
    }
}
