//! UE6 / Verse Transactional Speculative Execution and Rollback Netcode (`foreplay-core::rollback`).
//!
//! Provides deterministic state snapshotting, transaction boundaries for speculative execution
//! (e.g. AI lookahead or client-side prediction), bit-exact FNV-1a state hashing,
//! and fast rewind/restoration into live `bevy_ecs` SceneTrees.

use crate::{SceneSnapshot, SceneTree};
use foreplay_ir::IrValue;
use std::collections::BTreeMap;

/// Deterministic 64-bit FNV-1a hasher for bit-exact cross-platform state hashing.
#[derive(Debug, Clone)]
pub struct DeterministicStateHasher {
    state: u64,
}

impl Default for DeterministicStateHasher {
    fn default() -> Self {
        Self {
            state: 0xcbf2_9ce4_8422_2325, // FNV offset basis
        }
    }
}

impl DeterministicStateHasher {
    pub const fn new() -> Self {
        Self {
            state: 0xcbf2_9ce4_8422_2325,
        }
    }

    #[inline]
    pub fn write_bytes(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.state ^= u64::from(b);
            self.state = self.state.wrapping_mul(0x0000_0100_0000_01b3); // FNV prime
        }
    }

    #[inline]
    pub fn write_u64(&mut self, v: u64) {
        self.write_bytes(&v.to_le_bytes());
    }

    #[inline]
    pub fn write_f64(&mut self, v: f64) {
        // Canonicalize NaN and negative zero
        let bits = if v.is_nan() {
            0x7ff8_0000_0000_0000
        } else if v == 0.0 {
            0
        } else {
            v.to_bits()
        };
        self.write_u64(bits);
    }

    #[inline]
    pub fn write_str(&mut self, s: &str) {
        self.write_u64(s.len() as u64);
        self.write_bytes(s.as_bytes());
    }

    #[inline]
    pub fn finish(&self) -> u64 {
        self.state
    }
}

/// Recursively hashes an `IrValue` in a deterministic bit-exact manner.
fn hash_ir_value(hasher: &mut DeterministicStateHasher, value: &IrValue) {
    match value {
        IrValue::Nil => {
            hasher.write_bytes(&[0]);
        }
        IrValue::Bool(b) => {
            hasher.write_bytes(&[1, if *b { 1 } else { 0 }]);
        }
        IrValue::I64(i) => {
            hasher.write_bytes(&[2]);
            hasher.write_u64(*i as u64);
        }
        IrValue::F64(f) => {
            hasher.write_bytes(&[3]);
            hasher.write_f64(*f);
        }
        IrValue::String(s) => {
            hasher.write_bytes(&[4]);
            hasher.write_str(s);
        }
        IrValue::Vec2([x, y]) => {
            hasher.write_bytes(&[5]);
            hasher.write_f64(f64::from(*x));
            hasher.write_f64(f64::from(*y));
        }
        IrValue::Vec3([x, y, z]) => {
            hasher.write_bytes(&[6]);
            hasher.write_f64(f64::from(*x));
            hasher.write_f64(f64::from(*y));
            hasher.write_f64(f64::from(*z));
        }
        IrValue::Quat([x, y, z, w]) => {
            hasher.write_bytes(&[7]);
            hasher.write_f64(f64::from(*x));
            hasher.write_f64(f64::from(*y));
            hasher.write_f64(f64::from(*z));
            hasher.write_f64(f64::from(*w));
        }
        IrValue::Color([r, g, b, a]) => {
            hasher.write_bytes(&[8]);
            hasher.write_f64(f64::from(*r));
            hasher.write_f64(f64::from(*g));
            hasher.write_f64(f64::from(*b));
            hasher.write_f64(f64::from(*a));
        }
        IrValue::NodePath(np) => {
            hasher.write_bytes(&[9]);
            hasher.write_str(np);
        }
        IrValue::ResourcePath(rp) => {
            hasher.write_bytes(&[10]);
            hasher.write_str(rp);
        }
        IrValue::Array(items) => {
            hasher.write_bytes(&[11]);
            hasher.write_u64(items.len() as u64);
            for item in items {
                hash_ir_value(hasher, item);
            }
        }
        IrValue::Map(map) => {
            hasher.write_bytes(&[12]);
            hasher.write_u64(map.len() as u64);
            // BTreeMap iteration is deterministically sorted by key
            for (k, v) in map {
                hasher.write_str(k);
                hash_ir_value(hasher, v);
            }
        }
    }
}

/// Computes a bit-exact deterministic 64-bit state hash for a `SceneTree`.
pub fn state_hash(tree: &SceneTree) -> u64 {
    let mut hasher = DeterministicStateHasher::new();

    // 1. Tick count & Scene Name
    hasher.write_u64(tree.tick_count);
    hasher.write_str(&tree.active_scene.name);

    // 2. Scene Nodes (BTreeMap iteration guarantees deterministic order)
    hasher.write_u64(tree.active_scene.nodes.len() as u64);
    for (id, node) in &tree.active_scene.nodes {
        hasher.write_u64(id.index as u64);
        hasher.write_u64(id.generation.get() as u64);
        hasher.write_str(&node.name);
        hasher.write_str(&node.class_name);
        if let Some(parent) = node.parent {
            hasher.write_bytes(&[1]);
            hasher.write_u64(parent.index as u64);
            hasher.write_u64(parent.generation.get() as u64);
        } else {
            hasher.write_bytes(&[0]);
        }
        if let Some(inst) = &node.instance_scene {
            hasher.write_bytes(&[1]);
            hasher.write_str(inst);
        } else {
            hasher.write_bytes(&[0]);
        }
        // Groups
        hasher.write_u64(node.groups.len() as u64);
        for g in &node.groups {
            hasher.write_str(g);
        }
        // Properties (BTreeMap sorted order)
        hasher.write_u64(node.properties.len() as u64);
        for (k, v) in &node.properties {
            hasher.write_str(k);
            hash_ir_value(&mut hasher, v);
        }
    }

    // 3. Signal Connections (sorted by from_node, signal_name, to_node, method_name)
    hasher.write_u64(tree.active_scene.connections.len() as u64);
    let mut sorted_connections = tree.active_scene.connections.clone();
    sorted_connections.sort();
    for conn in &sorted_connections {
        hasher.write_u64(conn.from_node.index as u64);
        hasher.write_u64(conn.from_node.generation.get() as u64);
        hasher.write_str(&conn.signal_name);
        hasher.write_u64(conn.to_node.index as u64);
        hasher.write_u64(conn.to_node.generation.get() as u64);
        hasher.write_str(&conn.method_name);
    }

    // 4. Feel Overrides (BTreeMap sorted order)
    hasher.write_u64(tree.active_scene.feel_overrides.len() as u64);
    for (k, &v) in &tree.active_scene.feel_overrides {
        hasher.write_str(k);
        hasher.write_f64(v);
    }

    hasher.finish()
}

/// A stored snapshot frame along with its precomputed state hash.
#[derive(Clone, Debug, PartialEq)]
pub struct FrameSnapshot {
    pub tick: u64,
    pub state_hash: u64,
    pub snapshot: SceneSnapshot,
}

/// Ring-buffer of speculative frames for UE6/Verse-style rollback, transaction isolation,
/// and client-side prediction.
#[derive(Clone, Debug)]
pub struct SpeculativeFrameBuffer {
    snapshots: BTreeMap<u64, FrameSnapshot>,
    max_capacity: usize,
    in_transaction: bool,
    transaction_start_tick: Option<u64>,
}

impl Default for SpeculativeFrameBuffer {
    fn default() -> Self {
        Self::new(256)
    }
}

impl SpeculativeFrameBuffer {
    /// Creates a new speculative frame buffer with a bounded maximum capacity.
    pub fn new(max_capacity: usize) -> Self {
        Self {
            snapshots: BTreeMap::new(),
            max_capacity: max_capacity.max(2),
            in_transaction: false,
            transaction_start_tick: None,
        }
    }

    /// Maximum number of historic frames kept in the buffer.
    pub fn max_capacity(&self) -> usize {
        self.max_capacity
    }

    /// Sets the maximum capacity, pruning older frames if necessary.
    pub fn set_max_capacity(&mut self, capacity: usize) {
        self.max_capacity = capacity.max(2);
        self.prune();
    }

    /// Number of recorded snapshots currently stored.
    pub fn len(&self) -> usize {
        self.snapshots.len()
    }

    /// True if the buffer holds zero snapshots.
    pub fn is_empty(&self) -> bool {
        self.snapshots.is_empty()
    }

    /// Checks if a speculative transaction is currently open.
    pub fn is_in_transaction(&self) -> bool {
        self.in_transaction
    }

    /// Begins a speculative execution transaction (e.g. before simulating speculative AI frames).
    pub fn begin_transaction(&mut self) {
        self.in_transaction = true;
        self.transaction_start_tick = self.latest_tick();
    }

    /// Records a deterministic tick snapshot into the buffer.
    pub fn record_tick(&mut self, tree: &SceneTree, tick: u64) {
        let hash = state_hash(tree);
        let snapshot = tree.capture_snapshot();
        self.snapshots.insert(
            tick,
            FrameSnapshot {
                tick,
                state_hash: hash,
                snapshot,
            },
        );
        self.prune();
    }

    /// Commits the speculative transaction, keeping all recorded speculative frames.
    pub fn commit(&mut self) -> Result<(), String> {
        if !self.in_transaction {
            return Err("no speculative transaction is currently open".to_string());
        }
        self.in_transaction = false;
        self.transaction_start_tick = None;
        Ok(())
    }

    /// Rolls back the `SceneTree` to the exact state at `target_tick`.
    ///
    /// Any frames recorded *after* `target_tick` are invalidated and purged from the buffer.
    pub fn rollback_to_tick(&mut self, tree: &mut SceneTree, target_tick: u64) -> Result<(), String> {
        let frame = self
            .snapshots
            .get(&target_tick)
            .ok_or_else(|| format!("snapshot for tick {target_tick} not found in frame buffer"))?;

        // Restore tree state
        tree.restore_snapshot(frame.snapshot.clone());

        // Prune all ticks strictly after target_tick
        self.snapshots.split_off(&(target_tick + 1));

        // Reset transaction state
        self.in_transaction = false;
        self.transaction_start_tick = None;

        Ok(())
    }

    /// Retrieves the precomputed state hash for a specific tick, if recorded.
    pub fn hash_at_tick(&self, tick: u64) -> Option<u64> {
        self.snapshots.get(&tick).map(|f| f.state_hash)
    }

    /// Retrieves the snapshot for a specific tick, if recorded.
    pub fn snapshot_at_tick(&self, tick: u64) -> Option<&SceneSnapshot> {
        self.snapshots.get(&tick).map(|f| &f.snapshot)
    }

    /// Returns the lowest (earliest) recorded tick in the buffer.
    pub fn earliest_tick(&self) -> Option<u64> {
        self.snapshots.keys().next().copied()
    }

    /// Returns the highest (latest) recorded tick in the buffer.
    pub fn latest_tick(&self) -> Option<u64> {
        self.snapshots.keys().next_back().copied()
    }

    /// Prunes frames older than `max_capacity`.
    fn prune(&mut self) {
        while self.snapshots.len() > self.max_capacity {
            if let Some(oldest) = self.snapshots.keys().next().copied() {
                self.snapshots.remove(&oldest);
            } else {
                break;
            }
        }
    }
}
