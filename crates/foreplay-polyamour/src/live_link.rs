//! Real-Time `poly://` (`PLNK`) Bidirectional Live-Link Server, Client & Binary Delta Codec
//! (`foreplay-polyamour::live_link`).

use crate::mesh::{PolyFace, PolyMesh};
use crate::selection_raycast::SubObjectSelection;
use bevy_math::Vec3;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::time::Duration;

/// Magic bytes identifying the Polyamour live-link wire frame protocol (`"PLNK"`).
pub const POLY_LINK_MAGIC: &[u8; 4] = b"PLNK";

/// Current wire protocol version.
pub const POLY_LINK_VERSION: u32 = 1;

/// Maximum allowable frame size (64 MiB) to guard against corrupted or hostile payload allocations.
pub const MAX_FRAME_SIZE: usize = 64 * 1024 * 1024;

/// Header size: Magic (4) + Version (4) + Payload Length (4) + FNV-1a Checksum (8) = 20 bytes.
pub const HEADER_SIZE: usize = 20;

/// FNV-1a 64-bit offset basis.
pub const FNV1A_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
/// FNV-1a 64-bit prime.
pub const FNV1A_PRIME: u64 = 0x100000001b3;

/// Computes the standard 64-bit FNV-1a hash of a byte slice.
#[inline]
pub fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut hash = FNV1A_OFFSET_BASIS;
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(FNV1A_PRIME);
    }
    hash
}

/// Errors occurring during live-link framing, encoding, or network transport.
#[derive(Debug, thiserror::Error)]
pub enum LiveLinkError {
    #[error("Invalid live-link frame magic: expected b\"PLNK\", got {0:?}")]
    InvalidMagic([u8; 4]),

    #[error("Unsupported live-link protocol version: {0} (supported: {POLY_LINK_VERSION})")]
    UnsupportedVersion(u32),

    #[error("Frame size {0} bytes exceeds maximum allowed frame limit of {MAX_FRAME_SIZE} bytes")]
    FrameTooLarge(usize),

    #[error("FNV-1a checksum mismatch: expected {expected:#018x}, computed {computed:#018x}")]
    ChecksumMismatch { expected: u64, computed: u64 },

    #[error("Serialization / deserialization error: {0}")]
    Serialization(String),

    #[error("I/O transport error: {0}")]
    Io(String),
}

/// Serializable mesh snapshot for transmission across live-link network channels.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PolyMeshSnapshot {
    pub positions: Vec<[f32; 3]>,
    pub faces: Vec<Vec<u32>>,
    pub uvs: Option<Vec<Vec<[f32; 2]>>>,
    pub loose_edges: Option<Vec<[u32; 2]>>,
}

impl PolyMeshSnapshot {
    /// Creates a snapshot from a `PolyMesh`.
    pub fn from_mesh(mesh: &PolyMesh) -> Self {
        let positions = mesh.positions.iter().map(|p| [p.x, p.y, p.z]).collect();
        let faces = mesh.faces.iter().map(|f| f.verts.clone()).collect();
        let uvs = Some(mesh.faces.iter().map(|f| f.uvs.clone()).collect());
        let loose_edges = if mesh.loose_edges.is_empty() {
            None
        } else {
            Some(mesh.loose_edges.clone())
        };

        Self {
            positions,
            faces,
            uvs,
            loose_edges,
        }
    }

    /// Reconstructs a full `PolyMesh` from this snapshot.
    pub fn to_mesh(&self) -> PolyMesh {
        let mut mesh = PolyMesh::new();
        for &p in &self.positions {
            mesh.add_vertex(Vec3::new(p[0], p[1], p[2]));
        }

        let uvs_ref = self.uvs.as_ref();
        for (f_idx, verts) in self.faces.iter().enumerate() {
            let uvs = uvs_ref
                .and_then(|u| u.get(f_idx))
                .cloned()
                .unwrap_or_else(|| vec![[0.0, 0.0]; verts.len()]);
            mesh.add_face(verts.clone(), uvs, false, 0);
        }

        if let Some(ref loose) = self.loose_edges {
            mesh.loose_edges = loose.clone();
        }

        mesh
    }
}

/// High-level messages communicated across the Polyamour Live-Link TCP protocol.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum PolyLinkMessage {
    /// Initial handshake greeting.
    Hello {
        app_name: String,
        protocol_version: u32,
    },
    /// Direct command to open or replace an entire mesh in the Polyamour edit session.
    OpenMeshInPolyamour {
        node_id: u64,
        node_name: String,
        mesh: PolyMeshSnapshot,
        sub_selection: SubObjectSelection,
    },
    /// High-frequency in-place vertex & topology streaming delta.
    MeshDeltaSync {
        node_id: u64,
        revision: u64,
        updated_vertices: Vec<(u32, [f32; 3])>,
        replaced_faces: Option<Vec<Vec<u32>>>,
        edge_tags: Vec<([u32; 2], String)>,
        face_tags: Vec<(u32, String)>,
    },
    /// Synchronizes sub-object discrete selections (vertices/edges/faces).
    SelectionSync {
        node_id: u64,
        selection: SubObjectSelection,
    },
    /// Records an undo checkpoint marker with human-readable label.
    UndoMarker {
        label: String,
        revision: u64,
    },
    /// Acknowledges receipt of a specific revision.
    Ack {
        revision: u64,
    },
}

/// Encodes a `PolyLinkMessage` into a binary frame prefixed with magic, version, length, and FNV-1a checksum.
pub fn encode_frame(msg: &PolyLinkMessage) -> Result<Vec<u8>, LiveLinkError> {
    let payload =
        serde_json::to_vec(msg).map_err(|e| LiveLinkError::Serialization(e.to_string()))?;

    if payload.len() > MAX_FRAME_SIZE {
        return Err(LiveLinkError::FrameTooLarge(payload.len()));
    }

    let payload_len = payload.len() as u32;
    let checksum = fnv1a_64(&payload);

    let total_len = HEADER_SIZE
        .checked_add(payload.len())
        .ok_or(LiveLinkError::FrameTooLarge(usize::MAX))?;

    let mut frame = Vec::with_capacity(total_len);
    frame.extend_from_slice(POLY_LINK_MAGIC);
    frame.extend_from_slice(&POLY_LINK_VERSION.to_le_bytes());
    frame.extend_from_slice(&payload_len.to_le_bytes());
    frame.extend_from_slice(&checksum.to_le_bytes());
    frame.extend_from_slice(&payload);

    Ok(frame)
}

/// Attempts to decode a single `PolyLinkMessage` from the beginning of `buffer`.
/// Returns `Ok(Some((message, bytes_consumed)))` on success, `Ok(None)` if more data is needed,
/// or `Err` if corrupted/hostile.
pub fn decode_frame(buffer: &[u8]) -> Result<Option<(PolyLinkMessage, usize)>, LiveLinkError> {
    if buffer.len() < HEADER_SIZE {
        return Ok(None);
    }

    if &buffer[0..4] != POLY_LINK_MAGIC {
        let mut magic = [0u8; 4];
        magic.copy_from_slice(&buffer[0..4]);
        return Err(LiveLinkError::InvalidMagic(magic));
    }

    let version = u32::from_le_bytes(buffer[4..8].try_into().unwrap());
    if version != POLY_LINK_VERSION {
        return Err(LiveLinkError::UnsupportedVersion(version));
    }

    let payload_len = u32::from_le_bytes(buffer[8..12].try_into().unwrap()) as usize;
    if payload_len > MAX_FRAME_SIZE {
        return Err(LiveLinkError::FrameTooLarge(payload_len));
    }

    let total_len = match HEADER_SIZE.checked_add(payload_len) {
        Some(len) => len,
        None => return Err(LiveLinkError::FrameTooLarge(usize::MAX)),
    };

    if buffer.len() < total_len {
        return Ok(None); // Wait for remaining frame bytes
    }

    let expected_checksum = u64::from_le_bytes(buffer[12..20].try_into().unwrap());
    let payload = &buffer[HEADER_SIZE..total_len];
    let computed_checksum = fnv1a_64(payload);

    if expected_checksum != computed_checksum {
        return Err(LiveLinkError::ChecksumMismatch {
            expected: expected_checksum,
            computed: computed_checksum,
        });
    }

    let msg: PolyLinkMessage = serde_json::from_slice(payload)
        .map_err(|e| LiveLinkError::Serialization(e.to_string()))?;

    Ok(Some((msg, total_len)))
}

/// Applies a streaming vertex/topology delta update to `mesh` in-place.
/// Returns `true` if any mesh modifications were performed.
pub fn apply_mesh_delta(mesh: &mut PolyMesh, delta: &PolyLinkMessage) -> bool {
    match delta {
        PolyLinkMessage::MeshDeltaSync {
            updated_vertices,
            replaced_faces,
            ..
        } => {
            let mut changed = false;

            // Stream vertex position updates
            for &(idx, coords) in updated_vertices {
                let v_idx = idx as usize;
                let new_pos = Vec3::new(coords[0], coords[1], coords[2]);

                if v_idx < mesh.positions.len() {
                    mesh.positions[v_idx] = new_pos;
                    changed = true;
                } else if v_idx == mesh.positions.len() {
                    mesh.positions.push(new_pos);
                    changed = true;
                } else {
                    // Extend buffer if needed
                    mesh.positions.resize(v_idx + 1, Vec3::ZERO);
                    mesh.positions[v_idx] = new_pos;
                    changed = true;
                }
            }

            // Replace face topology if requested
            if let Some(faces) = replaced_faces {
                mesh.faces.clear();
                for verts in faces {
                    mesh.faces.push(PolyFace::new(verts.clone()));
                }
                changed = true;
            }

            changed
        }
        PolyLinkMessage::OpenMeshInPolyamour { mesh: snapshot, .. } => {
            *mesh = snapshot.to_mesh();
            true
        }
        _ => false,
    }
}

/// An active client connection on the `PolyamourLiveLinkServer`.
#[derive(Debug)]
struct ServerClient {
    stream: TcpStream,
    recv_buf: Vec<u8>,
}

/// Non-blocking loopback TCP Live-Link Server.
#[derive(Debug)]
pub struct PolyamourLiveLinkServer {
    listener: TcpListener,
    clients: Vec<ServerClient>,
}

impl PolyamourLiveLinkServer {
    /// Binds the server to an address (e.g. `"127.0.0.1:0"` for an ephemeral port).
    pub fn bind(addr: &str) -> Result<Self, LiveLinkError> {
        let listener = TcpListener::bind(addr).map_err(|e| LiveLinkError::Io(e.to_string()))?;
        listener
            .set_nonblocking(true)
            .map_err(|e| LiveLinkError::Io(e.to_string()))?;

        Ok(Self {
            listener,
            clients: Vec::new(),
        })
    }

    /// Returns the local socket address this server is listening on.
    pub fn local_addr(&self) -> Result<SocketAddr, LiveLinkError> {
        self.listener
            .local_addr()
            .map_err(|e| LiveLinkError::Io(e.to_string()))
    }

    /// Non-blocking poll for incoming connections and messages from all connected clients.
    pub fn poll_messages(&mut self) -> Vec<PolyLinkMessage> {
        // 1. Accept new incoming client connections
        while let Ok((stream, _)) = self.listener.accept() {
            let _ = stream.set_nonblocking(true);
            let _ = stream.set_read_timeout(Some(Duration::from_millis(50)));
            let _ = stream.set_write_timeout(Some(Duration::from_millis(50)));
            self.clients.push(ServerClient {
                stream,
                recv_buf: Vec::new(),
            });
        }

        let mut messages = Vec::new();
        let mut tmp_buf = [0u8; 8192];
        let mut disconnected = Vec::new();

        // 2. Read from each client
        for (idx, client) in self.clients.iter_mut().enumerate() {
            loop {
                match client.stream.read(&mut tmp_buf) {
                    Ok(0) => {
                        // EOF -> client disconnected
                        disconnected.push(idx);
                        break;
                    }
                    Ok(n) => {
                        client.recv_buf.extend_from_slice(&tmp_buf[..n]);
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        break;
                    }
                    Err(_) => {
                        disconnected.push(idx);
                        break;
                    }
                }
            }

            // Decode frames from client's accumulated buffer
            loop {
                match decode_frame(&client.recv_buf) {
                    Ok(Some((msg, consumed))) => {
                        messages.push(msg);
                        client.recv_buf.drain(0..consumed);
                    }
                    Ok(None) => break,
                    Err(_) => {
                        disconnected.push(idx);
                        break;
                    }
                }
            }
        }

        // 3. Remove disconnected clients
        if !disconnected.is_empty() {
            let mut keep = Vec::with_capacity(self.clients.len());
            for (idx, client) in self.clients.drain(..).enumerate() {
                if !disconnected.contains(&idx) {
                    keep.push(client);
                }
            }
            self.clients = keep;
        }

        messages
    }

    /// Broadcasts a message to all connected clients. Returns the count of successfully sent clients.
    pub fn broadcast(&mut self, msg: &PolyLinkMessage) -> Result<usize, LiveLinkError> {
        let frame = encode_frame(msg)?;
        let mut sent_count = 0;
        let mut dead_indices = Vec::new();

        for (idx, client) in self.clients.iter_mut().enumerate() {
            // Momentarily switch to blocking with timeout to ensure complete delivery
            let _ = client.stream.set_nonblocking(false);
            let res = client.stream.write_all(&frame);
            let _ = client.stream.set_nonblocking(true);

            if res.is_ok() {
                sent_count += 1;
            } else {
                dead_indices.push(idx);
            }
        }

        if !dead_indices.is_empty() {
            let mut keep = Vec::with_capacity(self.clients.len());
            for (idx, client) in self.clients.drain(..).enumerate() {
                if !dead_indices.contains(&idx) {
                    keep.push(client);
                }
            }
            self.clients = keep;
        }

        Ok(sent_count)
    }

    /// Returns the number of currently active connected clients.
    pub fn client_count(&self) -> usize {
        self.clients.len()
    }
}

/// Non-blocking loopback TCP Live-Link Client.
#[derive(Debug)]
pub struct PolyamourLiveLinkClient {
    stream: TcpStream,
    recv_buf: Vec<u8>,
}

impl PolyamourLiveLinkClient {
    /// Connects to a server with a specified connection timeout, setting non-blocking mode
    /// and read/write timeouts to guarantee that callers never deadlock.
    pub fn connect_timeout(addr: SocketAddr, timeout: Duration) -> Result<Self, LiveLinkError> {
        let stream =
            TcpStream::connect_timeout(&addr, timeout).map_err(|e| LiveLinkError::Io(e.to_string()))?;

        stream
            .set_read_timeout(Some(Duration::from_millis(100)))
            .map_err(|e| LiveLinkError::Io(e.to_string()))?;
        stream
            .set_write_timeout(Some(Duration::from_millis(100)))
            .map_err(|e| LiveLinkError::Io(e.to_string()))?;
        stream
            .set_nonblocking(true)
            .map_err(|e| LiveLinkError::Io(e.to_string()))?;

        Ok(Self {
            stream,
            recv_buf: Vec::new(),
        })
    }

    /// Sends a message across the TCP connection.
    pub fn send_message(&mut self, msg: &PolyLinkMessage) -> Result<(), LiveLinkError> {
        let frame = encode_frame(msg)?;
        // Temporarily configure blocking with timeout to ensure atomic frame write
        let _ = self.stream.set_nonblocking(false);
        let res = self
            .stream
            .write_all(&frame)
            .map_err(|e| LiveLinkError::Io(e.to_string()));
        let _ = self.stream.set_nonblocking(true);
        res
    }

    /// Non-blocking poll for any pending messages received from the server.
    pub fn try_recv_messages(&mut self) -> Result<Vec<PolyLinkMessage>, LiveLinkError> {
        let mut tmp_buf = [0u8; 8192];
        loop {
            match self.stream.read(&mut tmp_buf) {
                Ok(0) => {
                    return Err(LiveLinkError::Io("Server connection closed (EOF)".into()));
                }
                Ok(n) => {
                    self.recv_buf.extend_from_slice(&tmp_buf[..n]);
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    break;
                }
                Err(e) => {
                    return Err(LiveLinkError::Io(e.to_string()));
                }
            }
        }

        let mut messages = Vec::new();
        while let Some((msg, consumed)) = decode_frame(&self.recv_buf)? {
            messages.push(msg);
            self.recv_buf.drain(0..consumed);
        }

        Ok(messages)
    }

    /// Returns local socket address.
    pub fn local_addr(&self) -> Result<SocketAddr, LiveLinkError> {
        self.stream
            .local_addr()
            .map_err(|e| LiveLinkError::Io(e.to_string()))
    }

    /// Returns peer socket address.
    pub fn peer_addr(&self) -> Result<SocketAddr, LiveLinkError> {
        self.stream
            .peer_addr()
            .map_err(|e| LiveLinkError::Io(e.to_string()))
    }
}
