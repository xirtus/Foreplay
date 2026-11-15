//! Time-Travel Pulses & Multi-Agent Subtree Leases (`foreplay-atlas::timetravel`).
//!
//! Provides:
//! - `TimeTravelAtlasRecorder`: Bounded 600-frame ring buffer recording emitted `SignalPulse`s,
//!   `FeelSpec` live changes, and per-frame state hashes with causal trace analysis.
//! - `SubtreeLeaseManager`: Optimistic subtree locking enabling multiple AI agents to
//!   concurrently and safely edit disjoint scene subtrees without lock inversion or merge conflicts.

use foreplay_ir::{EditorCommand, IrScene, IrValue, NodeId};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// A single signal pulse event recorded along a signal wire between nodes.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct SignalPulse {
    pub pulse_id: u64,
    pub tick: u64,
    pub from_node: NodeId,
    pub signal_name: String,
    pub to_node: NodeId,
    pub method_name: String,
    pub payload: IrValue,
    /// Optional causal parent pulse ID that triggered this pulse (for causal chain tracing).
    pub caused_by: Option<u64>,
}

/// A recorded live feel-spec knob value adjustment.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct FeelSpecLiveChange {
    pub tick: u64,
    pub key: String,
    pub old_value: Option<f64>,
    pub new_value: f64,
}

/// Record of all signals, feel adjustments, and the state hash for a single frame.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct HistoricalFrameRecord {
    pub tick: u64,
    pub state_hash: u64,
    pub pulses: Vec<SignalPulse>,
    pub feel_changes: Vec<FeelSpecLiveChange>,
}

/// Scrub view returned when inspecting a historical tick.
#[derive(Clone, PartialEq, Debug)]
pub struct TimeTravelScrubView<'a> {
    pub tick: u64,
    pub state_hash: u64,
    pub active_pulses: &'a [SignalPulse],
    pub active_feel_changes: &'a [FeelSpecLiveChange],
}

/// Bounded 600-frame ring buffer recorder for live engine scrubbing and causal tracing.
#[derive(Clone, Debug)]
pub struct TimeTravelAtlasRecorder {
    frames: VecDeque<HistoricalFrameRecord>,
    max_capacity: usize,
    pulse_index: BTreeMap<u64, SignalPulse>,
}

impl Default for TimeTravelAtlasRecorder {
    fn default() -> Self {
        Self::new(600)
    }
}

impl TimeTravelAtlasRecorder {
    /// Creates a recorder with a maximum frame capacity (default 600 frames = 10s at 60Hz).
    pub fn new(max_capacity: usize) -> Self {
        Self {
            frames: VecDeque::with_capacity(max_capacity),
            max_capacity: max_capacity.max(10),
            pulse_index: BTreeMap::new(),
        }
    }

    /// Number of recorded frames currently stored.
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    /// True if zero frames are recorded.
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// Maximum capacity of the ring buffer.
    pub fn max_capacity(&self) -> usize {
        self.max_capacity
    }

    /// Earliest recorded tick.
    pub fn earliest_tick(&self) -> Option<u64> {
        self.frames.front().map(|f| f.tick)
    }

    /// Latest recorded tick.
    pub fn latest_tick(&self) -> Option<u64> {
        self.frames.back().map(|f| f.tick)
    }

    /// Records a completed frame into the ring buffer, indexing pulses for fast lookup.
    pub fn record_frame(
        &mut self,
        tick: u64,
        state_hash: u64,
        pulses: Vec<SignalPulse>,
        feel_changes: Vec<FeelSpecLiveChange>,
    ) {
        // Index new pulses
        for p in &pulses {
            self.pulse_index.insert(p.pulse_id, p.clone());
        }

        // If ring buffer is full, remove oldest frame and purge its pulses
        if self.frames.len() >= self.max_capacity
            && let Some(oldest) = self.frames.pop_front()
        {
            for p in oldest.pulses {
                self.pulse_index.remove(&p.pulse_id);
            }
        }

        self.frames.push_back(HistoricalFrameRecord {
            tick,
            state_hash,
            pulses,
            feel_changes,
        });
    }

    /// Scrubs to a historical tick, returning active pulses and feel changes at that exact tick.
    pub fn scrub_to_tick(&self, tick: u64) -> Option<TimeTravelScrubView<'_>> {
        self.frames.iter().find(|f| f.tick == tick).map(|f| {
            TimeTravelScrubView {
                tick: f.tick,
                state_hash: f.state_hash,
                active_pulses: &f.pulses,
                active_feel_changes: &f.feel_changes,
            }
        })
    }

    /// Returns active signal wire connections `(from_node, to_node, signal_name)` pulsed at `tick`.
    pub fn active_wires_at_tick(&self, tick: u64) -> Vec<(NodeId, NodeId, String)> {
        self.scrub_to_tick(tick)
            .map(|view| {
                view.active_pulses
                    .iter()
                    .map(|p| (p.from_node, p.to_node, p.signal_name.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Finds all pulses involving a specific node (either as sender or receiver) within `[start_tick, end_tick]`.
    pub fn causal_chain_for_node(
        &self,
        node: NodeId,
        start_tick: u64,
        end_tick: u64,
    ) -> Vec<&SignalPulse> {
        let mut results = Vec::new();
        for frame in &self.frames {
            if frame.tick >= start_tick && frame.tick <= end_tick {
                for p in &frame.pulses {
                    if p.from_node == node || p.to_node == node {
                        results.push(p);
                    }
                }
            }
        }
        results
    }

    /// Traces the full causal dependency chain backwards from a given `final_pulse_id`.
    ///
    /// Returns the sequence of pulses leading up to `final_pulse_id` in chronological order.
    pub fn causal_trace(&self, final_pulse_id: u64) -> Vec<&SignalPulse> {
        let mut chain = Vec::new();
        let mut current_id = Some(final_pulse_id);
        let mut visited = BTreeSet::new();

        while let Some(id) = current_id {
            if !visited.insert(id) {
                break; // Cycle safeguard
            }
            if let Some(pulse) = self.pulse_index.get(&id) {
                chain.push(pulse);
                current_id = pulse.caused_by;
            } else {
                break;
            }
        }

        chain.reverse();
        chain
    }
}

/// An active lease granting an AI agent exclusive mutation rights over a subtree.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct SubtreeLease {
    pub lease_id: u64,
    pub agent_id: String,
    pub root_node: NodeId,
    pub acquired_tick: u64,
}

/// Errors arising during subtree lease operations.
#[derive(Clone, PartialEq, Eq, Debug, thiserror::Error)]
pub enum LeaseError {
    #[error("node {0:?} does not exist in scene")]
    NodeNotFound(NodeId),
    #[error("node {node:?} is already locked by agent '{holder}' under lease {lease_id}")]
    SubtreeConflict {
        node: NodeId,
        holder: String,
        lease_id: u64,
    },
    #[error("lease {0} not found")]
    LeaseNotFound(u64),
    #[error("agent '{agent_id}' does not hold a lease covering node {affected_node:?}")]
    UnauthorizedMutation {
        agent_id: String,
        affected_node: NodeId,
    },
    #[error("agent '{0}' is not the owner of lease {1}")]
    UnauthorizedRelease(String, u64),
}

/// Manages optimistic subtree locks for concurrent multi-agent scene editing.
#[derive(Clone, Debug, Default)]
pub struct SubtreeLeaseManager {
    leases: BTreeMap<u64, SubtreeLease>,
    next_lease_id: u64,
}

impl SubtreeLeaseManager {
    pub fn new() -> Self {
        Self {
            leases: BTreeMap::new(),
            next_lease_id: 1,
        }
    }

    /// All active leases.
    pub fn leases(&self) -> &BTreeMap<u64, SubtreeLease> {
        &self.leases
    }

    /// Gathers all descendant node IDs of `root` within `scene`.
    pub fn collect_subtree_nodes(&self, scene: &IrScene, root: NodeId) -> BTreeSet<NodeId> {
        let mut descendants = BTreeSet::new();
        descendants.insert(root);

        let mut queue = vec![root];
        while let Some(curr) = queue.pop() {
            for (&id, node) in &scene.nodes {
                if node.parent == Some(curr) && descendants.insert(id) {
                    queue.push(id);
                }
            }
        }
        descendants
    }

    /// Checks if a node falls within a lease's subtree in `scene`.
    fn node_in_subtree(scene: &IrScene, node: NodeId, root: NodeId) -> bool {
        if node == root {
            return true;
        }
        let ancestors = scene.ancestors_of(node);
        ancestors.contains(&root)
    }

    /// Acquires an exclusive mutation lease over `root_node` and its entire descendant subtree.
    ///
    /// Rejects if `root_node` does not exist, or if `root_node`, any of its ancestors,
    /// or any of its descendants are already leased by another agent.
    pub fn acquire_lease(
        &mut self,
        agent_id: &str,
        root_node: NodeId,
        scene: &IrScene,
    ) -> Result<u64, LeaseError> {
        if !scene.nodes.contains_key(&root_node) {
            return Err(LeaseError::NodeNotFound(root_node));
        }

        // Check if root_node is already covered by an existing lease
        for lease in self.leases.values() {
            if lease.agent_id != agent_id {
                // 1. Is root_node a descendant of an existing lease?
                if Self::node_in_subtree(scene, root_node, lease.root_node) {
                    return Err(LeaseError::SubtreeConflict {
                        node: root_node,
                        holder: lease.agent_id.clone(),
                        lease_id: lease.lease_id,
                    });
                }
                // 2. Is any existing lease a descendant of root_node?
                if Self::node_in_subtree(scene, lease.root_node, root_node) {
                    return Err(LeaseError::SubtreeConflict {
                        node: lease.root_node,
                        holder: lease.agent_id.clone(),
                        lease_id: lease.lease_id,
                    });
                }
            }
        }

        let lease_id = self.next_lease_id;
        self.next_lease_id += 1;

        self.leases.insert(
            lease_id,
            SubtreeLease {
                lease_id,
                agent_id: agent_id.to_string(),
                root_node,
                acquired_tick: 0,
            },
        );

        Ok(lease_id)
    }

    /// Releases an existing lease previously acquired by `agent_id`.
    pub fn release_lease(&mut self, agent_id: &str, lease_id: u64) -> Result<(), LeaseError> {
        let lease = self
            .leases
            .get(&lease_id)
            .ok_or(LeaseError::LeaseNotFound(lease_id))?;

        if lease.agent_id != agent_id {
            return Err(LeaseError::UnauthorizedRelease(
                agent_id.to_string(),
                lease_id,
            ));
        }

        self.leases.remove(&lease_id);
        Ok(())
    }

    /// Collects all nodes directly affected by an `EditorCommand`.
    fn collect_affected_nodes(cmd: &EditorCommand) -> Vec<NodeId> {
        match cmd {
            EditorCommand::InsertNode { node } => vec![node.id],
            EditorCommand::RemoveNode { node } => vec![node.id],
            EditorCommand::SetProperty { node_id, .. } => vec![*node_id],
            EditorCommand::TuneFeelSpec { .. } => vec![],
            EditorCommand::RenameNode { node_id, .. } => vec![*node_id],
            EditorCommand::ReparentNode {
                node_id,
                new_parent,
                ..
            } => {
                let mut v = vec![*node_id];
                if let Some(p) = new_parent {
                    v.push(*p);
                }
                v
            }
            EditorCommand::ConnectSignal { connection } => {
                vec![connection.from_node, connection.to_node]
            }
            EditorCommand::DisconnectSignal { connection } => {
                vec![connection.from_node, connection.to_node]
            }
            EditorCommand::SetGroups { node_id, .. } => vec![*node_id],
            EditorCommand::Batch { commands, .. } => {
                let mut v = Vec::new();
                for c in commands {
                    v.extend(Self::collect_affected_nodes(c));
                }
                v
            }
        }
    }

    /// Verifies whether `agent_id` is authorized to execute `cmd` against `scene`.
    ///
    /// Every node affected by `cmd` must reside within a subtree leased by `agent_id`,
    /// and no node affected by `cmd` may be leased by any other agent.
    pub fn check_command_allowed(
        &self,
        agent_id: &str,
        cmd: &EditorCommand,
        scene: &IrScene,
    ) -> Result<(), LeaseError> {
        let affected = Self::collect_affected_nodes(cmd);

        // An empty affected list (e.g. global feel tuning) is allowed
        if affected.is_empty() {
            return Ok(());
        }

        let agent_leases: Vec<&SubtreeLease> = self
            .leases
            .values()
            .filter(|l| l.agent_id == agent_id)
            .collect();

        for node_id in affected {
            // Must belong to at least one lease held by agent_id
            let covered_by_agent = agent_leases
                .iter()
                .any(|l| Self::node_in_subtree(scene, node_id, l.root_node));

            if !covered_by_agent {
                return Err(LeaseError::UnauthorizedMutation {
                    agent_id: agent_id.to_string(),
                    affected_node: node_id,
                });
            }

            // Must NOT conflict with any other agent's lease
            for other_lease in self.leases.values() {
                if other_lease.agent_id != agent_id
                    && Self::node_in_subtree(scene, node_id, other_lease.root_node)
                {
                    return Err(LeaseError::SubtreeConflict {
                        node: node_id,
                        holder: other_lease.agent_id.clone(),
                        lease_id: other_lease.lease_id,
                    });
                }
            }
        }

        Ok(())
    }
}
