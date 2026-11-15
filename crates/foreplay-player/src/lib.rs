//! Standalone native Foreplay Game Engine runtime player library & IPC debug server.

pub mod debug_ipc;
pub mod embedded_pack;
pub mod runtime;

pub use debug_ipc::{
    DebugCommand, DebugEvent, DebugIpcClient, DebugIpcError, DebugIpcServer, DebugNodeSummary,
};
pub use embedded_pack::{
    detect_current_exe_embedded_pack, embed_pack_trailer, extract_embedded_pack_from_bytes,
    extract_embedded_pack_from_path, fnv1a_64, EmbeddedPackError, EMBEDDED_TRAILER_MAGIC_PREFIX,
    EMBEDDED_TRAILER_MAGIC_SUFFIX, EMBEDDED_TRAILER_SIZE,
};
pub use runtime::{PlayerRuntime, PlayerTickSummary};

/// Runtime boot mode for `foreplay-player`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlayerBootMode {
    Headless { ticks: u32 },
    EmbeddedFpck,
    ExternalFpck(String),
}
