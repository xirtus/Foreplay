//! Compact replay-frame streaming (`skate2-rust-port` desync-proof lobby model) + lockstep netcode.

use foreplay_ir::NodeId;

/// Compact per-entity state/replay frame streamed across jittery links without global sim desync.
#[derive(Clone, PartialEq, Debug)]
pub struct CompactReplayFrame {
    pub tick: u64,
    pub entity: NodeId,
    pub position: [f32; 3],
    pub rotation_quat: [f32; 4],
    pub linear_velocity: [f32; 3],
}
