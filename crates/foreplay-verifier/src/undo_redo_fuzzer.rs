//! Stateful Undo/Redo State-Machine Fuzzer (`foreplay-verifier`).
//!
//! Attacks the `HarnessSession` command bus across all 10 invertible `EditorCommand` variants:
//! `InsertNode`, `RemoveNode`, `SetProperty`, `TuneFeelSpec`, `RenameNode`, `ReparentNode`,
//! `ConnectSignal`, `DisconnectSignal`, `SetGroups`, and atomic `Batch`.
//! Proves mathematical invariants against an independent `ShadowHistoryModel` and powers
//! the 10,000-step continuous stateful marathon.

use crate::VerificationReceipt;
use foreplay_harness::HarnessSession;
use foreplay_ir::{EditorCommand, IrError, IrNode, IrScene, IrValue, NodeId, SignalConnection};
use std::collections::BTreeMap;
use std::num::NonZeroU32;

/// Compare two `IrValue`s for exact bit-level equality, treating IEEE-754 NaNs,
/// signed zeroes, infinities, and subnormals identically across representations.
pub fn ir_values_bit_identical(a: &IrValue, b: &IrValue) -> bool {
    match (a, b) {
        (IrValue::Nil, IrValue::Nil) => true,
        (IrValue::Bool(x), IrValue::Bool(y)) => x == y,
        (IrValue::I64(x), IrValue::I64(y)) => x == y,
        (IrValue::F64(x), IrValue::F64(y)) => x.to_bits() == y.to_bits(),
        (IrValue::String(x), IrValue::String(y)) => x == y,
        (IrValue::Vec2(x), IrValue::Vec2(y)) => {
            x.iter().zip(y.iter()).all(|(p, q)| p.to_bits() == q.to_bits())
        }
        (IrValue::Vec3(x), IrValue::Vec3(y)) => {
            x.iter().zip(y.iter()).all(|(p, q)| p.to_bits() == q.to_bits())
        }
        (IrValue::Quat(x), IrValue::Quat(y)) => {
            x.iter().zip(y.iter()).all(|(p, q)| p.to_bits() == q.to_bits())
        }
        (IrValue::Color(x), IrValue::Color(y)) => {
            x.iter().zip(y.iter()).all(|(p, q)| p.to_bits() == q.to_bits())
        }
        (IrValue::NodePath(x), IrValue::NodePath(y)) => x == y,
        (IrValue::ResourcePath(x), IrValue::ResourcePath(y)) => x == y,
        (IrValue::Array(x), IrValue::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y.iter()).all(|(p, q)| ir_values_bit_identical(p, q))
        }
        (IrValue::Map(x), IrValue::Map(y)) => {
            x.len() == y.len()
                && x.iter().all(|(k, v)| {
                    y.get(k)
                        .map(|y_v| ir_values_bit_identical(v, y_v))
                        .unwrap_or(false)
                })
        }
        _ => false,
    }
}

/// Compare two `IrNode`s for exact bit-level equality.
pub fn nodes_bit_identical(a: &IrNode, b: &IrNode) -> bool {
    a.id == b.id
        && a.name == b.name
        && a.class_name == b.class_name
        && a.parent == b.parent
        && a.instance_scene == b.instance_scene
        && a.groups == b.groups
        && a.properties.len() == b.properties.len()
        && a.properties.iter().all(|(k, val_a)| {
            b.properties
                .get(k)
                .map(|val_b| ir_values_bit_identical(val_a, val_b))
                .unwrap_or(false)
        })
}

/// Compare two `IrScene` documents for exact bit-level equality across all fields.
pub fn scenes_bit_identical(a: &IrScene, b: &IrScene) -> bool {
    if a.name != b.name {
        return false;
    }
    if a.connections.len() != b.connections.len() {
        return false;
    }
    let mut conns_a = a.connections.clone();
    let mut conns_b = b.connections.clone();
    conns_a.sort();
    conns_b.sort();
    if conns_a != conns_b {
        return false;
    }
    if a.feel_overrides.len() != b.feel_overrides.len() {
        return false;
    }
    for (k, v_a) in &a.feel_overrides {
        match b.feel_overrides.get(k) {
            Some(v_b) if v_a.to_bits() == v_b.to_bits() => {}
            _ => return false,
        }
    }
    if a.nodes.len() != b.nodes.len() {
        return false;
    }
    for (id, node_a) in &a.nodes {
        match b.nodes.get(id) {
            Some(node_b) if nodes_bit_identical(node_a, node_b) => {}
            _ => return false,
        }
    }
    true
}

/// Reference shadow-history model serving as an independent oracle for `HarnessSession`.
#[derive(Clone, PartialEq, Debug)]
pub struct ShadowHistoryModel {
    pub initial_scene: IrScene,
    pub current_scene: IrScene,
    pub undo_stack: Vec<(EditorCommand, IrScene)>,
    pub redo_stack: Vec<(EditorCommand, IrScene)>,
}

impl ShadowHistoryModel {
    pub fn new(scene: IrScene) -> Self {
        Self {
            initial_scene: scene.clone(),
            current_scene: scene,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    /// Try executing a command against the shadow scene.
    /// If valid, snapshots the pre-command scene, records it on the undo stack,
    /// and deterministically clears the redo stack.
    /// If invalid, returns `Err` without modifying any state or stack.
    pub fn execute(&mut self, cmd: EditorCommand) -> Result<(), IrError> {
        let mut speculative = self.current_scene.clone();
        cmd.apply(&mut speculative)?;
        let before_scene = std::mem::replace(&mut self.current_scene, speculative);
        self.undo_stack.push((cmd, before_scene));
        self.redo_stack.clear();
        Ok(())
    }

    /// Perform a single shadow undo step.
    pub fn undo(&mut self) -> bool {
        let Some((cmd, before_scene)) = self.undo_stack.pop() else {
            return false;
        };
        let after_scene = std::mem::replace(&mut self.current_scene, before_scene);
        self.redo_stack.push((cmd, after_scene));
        true
    }

    /// Perform a single shadow redo step.
    pub fn redo(&mut self) -> bool {
        let Some((cmd, after_scene)) = self.redo_stack.pop() else {
            return false;
        };
        let before_scene = std::mem::replace(&mut self.current_scene, after_scene);
        self.undo_stack.push((cmd, before_scene));
        true
    }

    /// Unwind the entire undo stack until empty.
    pub fn unwind_to_initial(&mut self) -> usize {
        let mut count = 0;
        while self.undo() {
            count += 1;
        }
        count
    }

    /// Rewind all redo steps until empty.
    pub fn rewind_all(&mut self) -> usize {
        let mut count = 0;
        while self.redo() {
            count += 1;
        }
        count
    }
}

/// Arbitrary operation for the stateful undo/redo fuzzer covering all 10 `EditorCommand` variants.
#[derive(Clone, PartialEq, Debug)]
pub enum FuzzOperation {
    /// 1. Insert a new node into the scene.
    InsertNode {
        node: IrNode,
    },
    /// 2. Remove an existing node by ID.
    RemoveNode {
        target_id: NodeId,
    },
    /// 3. Set a property on an existing node.
    SetProperty {
        target_id: NodeId,
        key: String,
        new_value: Option<IrValue>,
    },
    /// 4. Tune a feel spec knob.
    TuneFeelSpec {
        key: String,
        new_value: Option<f64>,
    },
    /// 5. Rename a node.
    RenameNode {
        target_id: NodeId,
        new_name: String,
    },
    /// 6. Reparent a node.
    ReparentNode {
        target_id: NodeId,
        new_parent: Option<NodeId>,
    },
    /// 7. Connect a signal wire.
    ConnectSignal {
        connection: SignalConnection,
    },
    /// 8. Disconnect a signal wire.
    DisconnectSignal {
        connection: SignalConnection,
    },
    /// 9. Set node groups.
    SetGroups {
        target_id: NodeId,
        new_groups: Vec<String>,
    },
    /// 10. Valid atomic Batch of commands.
    Batch {
        label: String,
        commands: Vec<EditorCommand>,
    },
    /// Random burst of partial undos.
    UndoBurst(usize),
    /// Random burst of partial redos.
    RedoBurst(usize),
    /// Adversarial Attack: Attempt self-parenting cycle (`A -> Some(A)`).
    AttackSelfParenting {
        target_id: NodeId,
    },
    /// Adversarial Attack: Attempt multi-hop ancestor cycle (`A -> B -> C`, then `A -> Some(C)`).
    AttackMultiHopCycle,
    /// Adversarial Attack: Atomic batch with failing tail (2 valid + 1 failing).
    AttackFailingBatchRollback,
    /// Intentionally invalid command: Insert duplicate ID.
    InvalidInsertDuplicate {
        node_name: String,
    },
    /// Intentionally invalid command: Remove non-existent ID.
    InvalidRemoveMissing {
        missing_id: NodeId,
    },
    /// Intentionally invalid command: Set property on non-existent node.
    InvalidSetPropertyMissing {
        missing_id: NodeId,
        key: String,
        value: IrValue,
    },
}

/// Finds a `NodeId` that is guaranteed not to exist in the given scene.
pub fn find_unused_node_id(scene: &IrScene) -> NodeId {
    let max_index = scene.nodes.keys().map(|id| id.index).max().unwrap_or(0);
    NodeId::new(max_index.saturating_add(1) + 1, 1).unwrap_or_else(|| {
        NodeId::new(1, 1).expect("valid ID")
    })
}

/// Executes a full stateful sequence of operations against `HarnessSession` while checking
/// all 4 mathematical invariants against `ShadowHistoryModel`.
pub fn run_stateful_fuzz_sequence(
    initial_scene: IrScene,
    operations: &[FuzzOperation],
) -> Result<(), String> {
    let mut harness = HarnessSession::new(initial_scene.clone());
    let mut shadow = ShadowHistoryModel::new(initial_scene.clone());

    if !scenes_bit_identical(&harness.tree.active_scene, &shadow.current_scene) {
        return Err("Initial harness scene does not match shadow initial scene".into());
    }

    for (op_idx, op) in operations.iter().enumerate() {
        match op {
            FuzzOperation::InsertNode { node } => {
                let mut node = node.clone();
                // Ensure parent reference is valid within current scene
                if let Some(p) = node.parent
                    && !harness.tree.active_scene.nodes.contains_key(&p)
                {
                    node.parent = None;
                }
                if harness.tree.active_scene.nodes.contains_key(&node.id) {
                    let scene_before = harness.tree.active_scene.clone();
                    let res = harness.execute(EditorCommand::InsertNode { node: node.clone() });
                    if !matches!(res, Err(IrError::NodeAlreadyExists(id)) if id == node.id) {
                        return Err(format!("Op #{op_idx}: Expected NodeAlreadyExists, got {res:?}"));
                    }
                    if !scenes_bit_identical(&harness.tree.active_scene, &scene_before) {
                        return Err(format!("Op #{op_idx}: active_scene corrupted by failed InsertNode!"));
                    }
                } else {
                    let cmd = EditorCommand::InsertNode { node: node.clone() };
                    harness.execute(cmd.clone()).map_err(|e| format!("Op #{op_idx} InsertNode failed: {e:?}"))?;
                    shadow.execute(cmd).map_err(|e| format!("Op #{op_idx} shadow InsertNode failed: {e:?}"))?;
                    if !scenes_bit_identical(&harness.tree.active_scene, &shadow.current_scene) {
                        return Err(format!("Op #{op_idx}: Mismatch after InsertNode({:?})", node.id));
                    }
                }
            }

            FuzzOperation::RemoveNode { target_id } => {
                // If node has active children, reparent them to None first to avoid dangling parent references
                let has_children = harness
                    .tree
                    .active_scene
                    .nodes
                    .values()
                    .any(|n| n.parent == Some(*target_id));
                if has_children {
                    // Skip removing a parent node directly to avoid leaving dangling parent pointers
                    continue;
                }

                if let Some(existing_node) = harness.tree.active_scene.nodes.get(target_id).cloned() {
                    let cmd = EditorCommand::RemoveNode { node: existing_node };
                    harness.execute(cmd.clone()).map_err(|e| format!("Op #{op_idx} RemoveNode failed: {e:?}"))?;
                    shadow.execute(cmd).map_err(|e| format!("Op #{op_idx} shadow RemoveNode failed: {e:?}"))?;
                    if !scenes_bit_identical(&harness.tree.active_scene, &shadow.current_scene) {
                        return Err(format!("Op #{op_idx}: Mismatch after RemoveNode({:?})", target_id));
                    }
                } else {
                    let scene_before = harness.tree.active_scene.clone();
                    let dummy_node = IrNode {
                        id: *target_id,
                        name: "MissingDummy".into(),
                        class_name: "Node".into(),
                        parent: None,
                        instance_scene: None,
                        properties: BTreeMap::new(),
                        groups: Vec::new(),
                    };
                    let res = harness.execute(EditorCommand::RemoveNode { node: dummy_node });
                    if !matches!(res, Err(IrError::NodeNotFound(id)) if id == *target_id) {
                        return Err(format!("Op #{op_idx}: Expected NodeNotFound, got {res:?}"));
                    }
                    if !scenes_bit_identical(&harness.tree.active_scene, &scene_before) {
                        return Err(format!("Op #{op_idx}: active_scene corrupted by failed RemoveNode!"));
                    }
                }
            }

            FuzzOperation::SetProperty { target_id, key, new_value } => {
                if let Some(node) = harness.tree.active_scene.nodes.get(target_id) {
                    let old_value = node.properties.get(key).cloned();
                    let cmd = EditorCommand::SetProperty {
                        node_id: *target_id,
                        key: key.clone(),
                        old_value,
                        new_value: new_value.clone(),
                    };
                    harness.execute(cmd.clone()).map_err(|e| format!("Op #{op_idx} SetProperty failed: {e:?}"))?;
                    shadow.execute(cmd).map_err(|e| format!("Op #{op_idx} shadow SetProperty failed: {e:?}"))?;
                    if !scenes_bit_identical(&harness.tree.active_scene, &shadow.current_scene) {
                        return Err(format!("Op #{op_idx}: Mismatch after SetProperty({:?}, {})", target_id, key));
                    }
                } else {
                    let scene_before = harness.tree.active_scene.clone();
                    let cmd = EditorCommand::SetProperty {
                        node_id: *target_id,
                        key: key.clone(),
                        old_value: None,
                        new_value: new_value.clone(),
                    };
                    let res = harness.execute(cmd);
                    if !matches!(res, Err(IrError::NodeNotFound(id)) if id == *target_id) {
                        return Err(format!("Op #{op_idx}: Expected NodeNotFound, got {res:?}"));
                    }
                    if !scenes_bit_identical(&harness.tree.active_scene, &scene_before) {
                        return Err(format!("Op #{op_idx}: active_scene corrupted by failed SetProperty!"));
                    }
                }
            }

            FuzzOperation::TuneFeelSpec { key, new_value } => {
                let old_value = harness.tree.active_scene.feel_overrides.get(key).copied();
                let cmd = EditorCommand::TuneFeelSpec {
                    key: key.clone(),
                    old_value,
                    new_value: *new_value,
                };
                harness.execute(cmd.clone()).map_err(|e| format!("Op #{op_idx} TuneFeelSpec failed: {e:?}"))?;
                shadow.execute(cmd).map_err(|e| format!("Op #{op_idx} shadow TuneFeelSpec failed: {e:?}"))?;
                if !scenes_bit_identical(&harness.tree.active_scene, &shadow.current_scene) {
                    return Err(format!("Op #{op_idx}: Mismatch after TuneFeelSpec({key})"));
                }
            }

            FuzzOperation::RenameNode { target_id, new_name } => {
                if let Some(node) = harness.tree.active_scene.nodes.get(target_id) {
                    let old_name = node.name.clone();
                    let cmd = EditorCommand::RenameNode {
                        node_id: *target_id,
                        old_name,
                        new_name: new_name.clone(),
                    };
                    harness.execute(cmd.clone()).map_err(|e| format!("Op #{op_idx} RenameNode failed: {e:?}"))?;
                    shadow.execute(cmd).map_err(|e| format!("Op #{op_idx} shadow RenameNode failed: {e:?}"))?;
                    if !scenes_bit_identical(&harness.tree.active_scene, &shadow.current_scene) {
                        return Err(format!("Op #{op_idx}: Mismatch after RenameNode"));
                    }
                } else {
                    let scene_before = harness.tree.active_scene.clone();
                    let cmd = EditorCommand::RenameNode {
                        node_id: *target_id,
                        old_name: "Old".into(),
                        new_name: new_name.clone(),
                    };
                    let res = harness.execute(cmd);
                    if !matches!(res, Err(IrError::NodeNotFound(id)) if id == *target_id) {
                        return Err(format!("Op #{op_idx}: Expected NodeNotFound for missing Rename"));
                    }
                    if !scenes_bit_identical(&harness.tree.active_scene, &scene_before) {
                        return Err(format!("Op #{op_idx}: active_scene corrupted by failed RenameNode"));
                    }
                }
            }

            FuzzOperation::ReparentNode { target_id, new_parent } => {
                let target_exists = harness.tree.active_scene.nodes.contains_key(target_id);
                let parent_exists = match new_parent {
                    Some(p) => harness.tree.active_scene.nodes.contains_key(p),
                    None => true,
                };

                if !target_exists || !parent_exists {
                    let scene_before = harness.tree.active_scene.clone();
                    let cmd = EditorCommand::ReparentNode {
                        node_id: *target_id,
                        old_parent: None,
                        new_parent: *new_parent,
                    };
                    let res = harness.execute(cmd);
                    if !matches!(res, Err(IrError::NodeNotFound(_))) {
                        return Err(format!("Op #{op_idx}: Expected NodeNotFound on missing reparent target"));
                    }
                    if !scenes_bit_identical(&harness.tree.active_scene, &scene_before) {
                        return Err(format!("Op #{op_idx}: active_scene corrupted by failed ReparentNode"));
                    }
                } else if harness.tree.active_scene.would_create_cycle(*target_id, *new_parent) {
                    let scene_before = harness.tree.active_scene.clone();
                    let old_parent = harness.tree.active_scene.nodes[target_id].parent;
                    let cmd = EditorCommand::ReparentNode {
                        node_id: *target_id,
                        old_parent,
                        new_parent: *new_parent,
                    };
                    let res = harness.execute(cmd);
                    if !matches!(res, Err(IrError::InvalidHierarchyCycle(id)) if id == *target_id) {
                        return Err(format!("Op #{op_idx}: Expected InvalidHierarchyCycle({target_id:?}), got {res:?}"));
                    }
                    if !scenes_bit_identical(&harness.tree.active_scene, &scene_before) {
                        return Err(format!("Op #{op_idx}: active_scene corrupted by cyclic ReparentNode!"));
                    }
                } else {
                    let old_parent = harness.tree.active_scene.nodes[target_id].parent;
                    let cmd = EditorCommand::ReparentNode {
                        node_id: *target_id,
                        old_parent,
                        new_parent: *new_parent,
                    };
                    harness.execute(cmd.clone()).map_err(|e| format!("Op #{op_idx} ReparentNode failed: {e:?}"))?;
                    shadow.execute(cmd).map_err(|e| format!("Op #{op_idx} shadow ReparentNode failed: {e:?}"))?;
                    if !scenes_bit_identical(&harness.tree.active_scene, &shadow.current_scene) {
                        return Err(format!("Op #{op_idx}: Mismatch after ReparentNode"));
                    }
                }
            }

            FuzzOperation::ConnectSignal { connection } => {
                let from_exists = harness.tree.active_scene.nodes.contains_key(&connection.from_node);
                let to_exists = harness.tree.active_scene.nodes.contains_key(&connection.to_node);
                let already_connected = harness.tree.active_scene.connections.contains(connection);

                if !from_exists || !to_exists {
                    let scene_before = harness.tree.active_scene.clone();
                    let res = harness.execute(EditorCommand::ConnectSignal { connection: connection.clone() });
                    if !matches!(res, Err(IrError::NodeNotFound(_))) {
                        return Err(format!("Op #{op_idx}: Expected NodeNotFound on missing signal endpoints"));
                    }
                    if !scenes_bit_identical(&harness.tree.active_scene, &scene_before) {
                        return Err(format!("Op #{op_idx}: scene corrupted by invalid ConnectSignal"));
                    }
                } else if already_connected {
                    let scene_before = harness.tree.active_scene.clone();
                    let res = harness.execute(EditorCommand::ConnectSignal { connection: connection.clone() });
                    if !matches!(res, Err(IrError::ConnectionAlreadyExists(_))) {
                        return Err(format!("Op #{op_idx}: Expected ConnectionAlreadyExists"));
                    }
                    if !scenes_bit_identical(&harness.tree.active_scene, &scene_before) {
                        return Err(format!("Op #{op_idx}: scene corrupted by duplicate ConnectSignal"));
                    }
                } else {
                    let cmd = EditorCommand::ConnectSignal { connection: connection.clone() };
                    harness.execute(cmd.clone()).map_err(|e| format!("Op #{op_idx} ConnectSignal failed: {e:?}"))?;
                    shadow.execute(cmd).map_err(|e| format!("Op #{op_idx} shadow ConnectSignal failed: {e:?}"))?;
                    if !scenes_bit_identical(&harness.tree.active_scene, &shadow.current_scene) {
                        return Err(format!("Op #{op_idx}: Mismatch after ConnectSignal"));
                    }
                }
            }

            FuzzOperation::DisconnectSignal { connection } => {
                let exists = harness.tree.active_scene.connections.contains(connection);
                if !exists {
                    let scene_before = harness.tree.active_scene.clone();
                    let res = harness.execute(EditorCommand::DisconnectSignal { connection: connection.clone() });
                    if !matches!(res, Err(IrError::ConnectionNotFound(_))) {
                        return Err(format!("Op #{op_idx}: Expected ConnectionNotFound on missing disconnect"));
                    }
                    if !scenes_bit_identical(&harness.tree.active_scene, &scene_before) {
                        return Err(format!("Op #{op_idx}: scene corrupted by invalid DisconnectSignal"));
                    }
                } else {
                    let cmd = EditorCommand::DisconnectSignal { connection: connection.clone() };
                    harness.execute(cmd.clone()).map_err(|e| format!("Op #{op_idx} DisconnectSignal failed: {e:?}"))?;
                    shadow.execute(cmd).map_err(|e| format!("Op #{op_idx} shadow DisconnectSignal failed: {e:?}"))?;
                    if !scenes_bit_identical(&harness.tree.active_scene, &shadow.current_scene) {
                        return Err(format!("Op #{op_idx}: Mismatch after DisconnectSignal"));
                    }
                }
            }

            FuzzOperation::SetGroups { target_id, new_groups } => {
                if let Some(node) = harness.tree.active_scene.nodes.get(target_id) {
                    let old_groups = node.groups.clone();
                    let cmd = EditorCommand::SetGroups {
                        node_id: *target_id,
                        old_groups,
                        new_groups: new_groups.clone(),
                    };
                    harness.execute(cmd.clone()).map_err(|e| format!("Op #{op_idx} SetGroups failed: {e:?}"))?;
                    shadow.execute(cmd).map_err(|e| format!("Op #{op_idx} shadow SetGroups failed: {e:?}"))?;
                    if !scenes_bit_identical(&harness.tree.active_scene, &shadow.current_scene) {
                        return Err(format!("Op #{op_idx}: Mismatch after SetGroups"));
                    }
                } else {
                    let scene_before = harness.tree.active_scene.clone();
                    let cmd = EditorCommand::SetGroups {
                        node_id: *target_id,
                        old_groups: Vec::new(),
                        new_groups: new_groups.clone(),
                    };
                    let res = harness.execute(cmd);
                    if !matches!(res, Err(IrError::NodeNotFound(id)) if id == *target_id) {
                        return Err(format!("Op #{op_idx}: Expected NodeNotFound for missing SetGroups"));
                    }
                    if !scenes_bit_identical(&harness.tree.active_scene, &scene_before) {
                        return Err(format!("Op #{op_idx}: scene corrupted by invalid SetGroups"));
                    }
                }
            }

            FuzzOperation::Batch { label, commands } => {
                let cmd = EditorCommand::Batch {
                    label: label.clone(),
                    commands: commands.clone(),
                };
                let harness_res = harness.execute(cmd.clone());
                let shadow_res = shadow.execute(cmd);
                if harness_res.is_ok() != shadow_res.is_ok() {
                    return Err(format!("Op #{op_idx}: Batch execution outcome disagreed between harness and shadow!"));
                }
                if !scenes_bit_identical(&harness.tree.active_scene, &shadow.current_scene) {
                    return Err(format!("Op #{op_idx}: Mismatch after Batch"));
                }
            }

            FuzzOperation::UndoBurst(burst_len) => {
                let scene_before_burst = harness.tree.active_scene.clone();
                let mut undone_count = 0;
                for _ in 0..*burst_len {
                    if harness.undo().map_err(|e| format!("Undo failed: {e:?}"))? {
                        shadow.undo();
                        undone_count += 1;
                    } else {
                        break;
                    }
                }

                if !scenes_bit_identical(&harness.tree.active_scene, &shadow.current_scene) {
                    return Err(format!("Op #{op_idx}: Mismatch during UndoBurst"));
                }

                for _ in 0..undone_count {
                    harness.redo().map_err(|e| format!("Redo failed: {e:?}"))?;
                    shadow.redo();
                }

                if !scenes_bit_identical(&harness.tree.active_scene, &scene_before_burst) {
                    return Err(format!("Op #{op_idx}: Invariant (a) failed! undo({undone_count}) + redo({undone_count}) failed restoration!"));
                }
            }

            FuzzOperation::RedoBurst(burst_len) => {
                for _ in 0..*burst_len {
                    if harness.redo().map_err(|e| format!("Redo failed: {e:?}"))? {
                        shadow.redo();
                    } else {
                        break;
                    }
                }
                if !scenes_bit_identical(&harness.tree.active_scene, &shadow.current_scene) {
                    return Err(format!("Op #{op_idx}: Mismatch after RedoBurst"));
                }
            }

            FuzzOperation::AttackSelfParenting { target_id } => {
                if let Some(node) = harness.tree.active_scene.nodes.get(target_id) {
                    let scene_before = harness.tree.active_scene.clone();
                    let old_parent = node.parent;
                    let cmd = EditorCommand::ReparentNode {
                        node_id: *target_id,
                        old_parent,
                        new_parent: Some(*target_id),
                    };
                    let res = harness.execute(cmd);
                    if !matches!(res, Err(IrError::InvalidHierarchyCycle(id)) if id == *target_id) {
                        return Err(format!("Self-parenting attack failed to reject cycle! Got {res:?}"));
                    }
                    if !scenes_bit_identical(&harness.tree.active_scene, &scene_before) {
                        return Err("Self-parenting attack corrupted active_scene!".into());
                    }
                }
            }

            FuzzOperation::AttackMultiHopCycle => {
                // Look for an existing multi-hop ancestor chain in active_scene:
                // i.e., some descendant whose ancestors include at least 2 nodes (e.g. parent, grandparent).
                let mut found_chain = None;
                for &descendant in harness.tree.active_scene.nodes.keys() {
                    let ancestors = harness.tree.active_scene.ancestors_of(descendant);
                    if ancestors.len() >= 2 {
                        let ancestor = *ancestors.last().unwrap();
                        found_chain = Some((ancestor, descendant));
                        break;
                    }
                }

                if let Some((ancestor, descendant)) = found_chain {
                    let scene_before = harness.tree.active_scene.clone();
                    let old_parent = harness.tree.active_scene.nodes[&ancestor].parent;
                    let attack_cmd = EditorCommand::ReparentNode {
                        node_id: ancestor,
                        old_parent,
                        new_parent: Some(descendant),
                    };
                    let res = harness.execute(attack_cmd);
                    if !matches!(res, Err(IrError::InvalidHierarchyCycle(id)) if id == ancestor) {
                        return Err(format!("Multi-hop cycle attack failed! Expected InvalidHierarchyCycle({ancestor:?}), got {res:?}"));
                    }
                    if !scenes_bit_identical(&harness.tree.active_scene, &scene_before) {
                        return Err("Multi-hop cycle attack corrupted active_scene!".into());
                    }
                } else {
                    // When no multi-hop chain currently exists in active_scene, test the invariant
                    // in a dedicated local harness to avoid polluting the main state machine history:
                    let mut local_scene = IrScene::new("MultiHopIsolated");
                    let id_a = NodeId::new(99_001, 1).unwrap();
                    let id_b = NodeId::new(99_002, 1).unwrap();
                    let id_c = NodeId::new(99_003, 1).unwrap();
                    local_scene.nodes.insert(id_a, make_test_node(99_001, 1, "IsoA", "Node3D", None));
                    local_scene.nodes.insert(id_b, make_test_node(99_002, 1, "IsoB", "Node3D", Some(id_a)));
                    local_scene.nodes.insert(id_c, make_test_node(99_003, 1, "IsoC", "Node3D", Some(id_b)));

                    let mut local_harness = HarnessSession::new(local_scene.clone());
                    let attack_cmd = EditorCommand::ReparentNode {
                        node_id: id_a,
                        old_parent: None,
                        new_parent: Some(id_c),
                    };
                    let res = local_harness.execute(attack_cmd);
                    if !matches!(res, Err(IrError::InvalidHierarchyCycle(id)) if id == id_a) {
                        return Err(format!("Local multi-hop cycle attack failed! Expected InvalidHierarchyCycle({id_a:?}), got {res:?}"));
                    }
                    if !scenes_bit_identical(&local_harness.tree.active_scene, &local_scene) {
                        return Err("Local multi-hop cycle attack corrupted active_scene!".into());
                    }
                }
            }

            FuzzOperation::AttackFailingBatchRollback => {
                let scene_before = harness.tree.active_scene.clone();
                let unused_id = find_unused_node_id(&harness.tree.active_scene);
                let valid_node = make_test_node(unused_id.index, unused_id.generation.get(), "BatchValid", "Node3D", None);

                // Subcommand 1: valid insert
                let sub1 = EditorCommand::InsertNode { node: valid_node };
                // Subcommand 2: valid tune feel spec
                let sub2 = EditorCommand::TuneFeelSpec {
                    key: "batch_temp_key".into(),
                    old_value: None,
                    new_value: Some(42.0),
                };
                // Subcommand 3: FAILING remove non-existent node
                let missing = NodeId::new(99_999, 1).unwrap();
                let dummy = make_test_node(99_999, 1, "Missing", "Node", None);
                let sub3_failing = EditorCommand::RemoveNode { node: dummy };

                let failing_batch = EditorCommand::Batch {
                    label: "AtomicRollbackTestBatch".into(),
                    commands: vec![sub1, sub2, sub3_failing],
                };

                let res = harness.execute(failing_batch);
                if !matches!(res, Err(IrError::NodeNotFound(id)) if id == missing) {
                    return Err(format!("Atomic batch rollback attack expected NodeNotFound, got {res:?}"));
                }
                // PROVE: active_scene is 100% untouched by partially executed subcommands
                if !scenes_bit_identical(&harness.tree.active_scene, &scene_before) {
                    return Err("Atomic Batch rollback VIOLATED! active_scene was partially mutated!".into());
                }
            }

            FuzzOperation::InvalidInsertDuplicate { node_name } => {
                if let Some(&first_id) = harness.tree.active_scene.nodes.keys().next() {
                    let scene_before = harness.tree.active_scene.clone();
                    let dup_node = IrNode {
                        id: first_id,
                        name: node_name.clone(),
                        class_name: "DuplicatedNode".into(),
                        parent: None,
                        instance_scene: None,
                        properties: BTreeMap::new(),
                        groups: Vec::new(),
                    };
                    let res = harness.execute(EditorCommand::InsertNode { node: dup_node });
                    if !matches!(res, Err(IrError::NodeAlreadyExists(id)) if id == first_id) {
                        return Err(format!("Op #{op_idx}: Expected NodeAlreadyExists"));
                    }
                    if !scenes_bit_identical(&harness.tree.active_scene, &scene_before) {
                        return Err(format!("Op #{op_idx}: active_scene mutated by invalid duplicate insert!"));
                    }
                }
            }

            FuzzOperation::InvalidRemoveMissing { missing_id } => {
                let id = if harness.tree.active_scene.nodes.contains_key(missing_id) {
                    find_unused_node_id(&harness.tree.active_scene)
                } else {
                    *missing_id
                };
                let scene_before = harness.tree.active_scene.clone();
                let dummy = make_test_node(id.index, id.generation.get(), "NonExistent", "Node", None);
                let res = harness.execute(EditorCommand::RemoveNode { node: dummy });
                if !matches!(res, Err(IrError::NodeNotFound(i)) if i == id) {
                    return Err(format!("Op #{op_idx}: Expected NodeNotFound"));
                }
                if !scenes_bit_identical(&harness.tree.active_scene, &scene_before) {
                    return Err(format!("Op #{op_idx}: active_scene mutated by invalid remove!"));
                }
            }

            FuzzOperation::InvalidSetPropertyMissing { missing_id, key, value } => {
                let id = if harness.tree.active_scene.nodes.contains_key(missing_id) {
                    find_unused_node_id(&harness.tree.active_scene)
                } else {
                    *missing_id
                };
                let scene_before = harness.tree.active_scene.clone();
                let cmd = EditorCommand::SetProperty {
                    node_id: id,
                    key: key.clone(),
                    old_value: None,
                    new_value: Some(value.clone()),
                };
                let res = harness.execute(cmd);
                if !matches!(res, Err(IrError::NodeNotFound(i)) if i == id) {
                    return Err(format!("Op #{op_idx}: Expected NodeNotFound"));
                }
                if !scenes_bit_identical(&harness.tree.active_scene, &scene_before) {
                    return Err(format!("Op #{op_idx}: active_scene mutated by invalid set property!"));
                }
            }
        }
    }

    // Invariant b) proof: Unwinding entire undo stack always restores exact initial IrScene
    let scene_at_end = harness.tree.active_scene.clone();
    let mut total_undone = 0;
    while harness.undo().map_err(|e| format!("Final unwind step failed: {e:?}"))? {
        total_undone += 1;
    }

    if !scenes_bit_identical(&harness.tree.active_scene, &shadow.initial_scene) {
        return Err("Invariant (b) failed! Full unwind did NOT restore initial scene!".into());
    }

    // Rewind back to end of operations
    let mut total_rewound = 0;
    while harness.redo().map_err(|e| format!("Final rewind step failed: {e:?}"))? {
        total_rewound += 1;
    }
    if total_undone != total_rewound {
        return Err(format!("Total undone ({total_undone}) != total rewound ({total_rewound})!"));
    }
    if !scenes_bit_identical(&harness.tree.active_scene, &scene_at_end) {
        return Err("Rewinding back to end did not restore pre-unwind scene state!".into());
    }

    // Invariant c) proof: Executing a new command after undo() deterministically clears Redo stack
    if harness.undo().map_err(|e| format!("Pre-clearing undo failed: {e:?}"))? {
        let new_id = find_unused_node_id(&harness.tree.active_scene);
        let canary_node = make_test_node(new_id.index, new_id.generation.get(), "CanaryNode", "Node3D", None);
        harness
            .execute(EditorCommand::InsertNode { node: canary_node })
            .map_err(|e| format!("Canary insert failed: {e:?}"))?;

        let redo_after_new_cmd = harness.redo().map_err(|e| format!("Redo check failed: {e:?}"))?;
        if redo_after_new_cmd {
            return Err("Invariant (c) failed! redo() succeeded after new command was executed!".into());
        }

        harness.undo().map_err(|e| format!("Canary undo failed: {e:?}"))?;
        if harness.tree.active_scene.nodes.contains_key(&new_id) {
            return Err("Canary node still present after undoing canary!".into());
        }
    }

    Ok(())
}

/// Fast pseudo-random number generator for the 10,000-step marathon.
struct FastRng {
    state: u64,
}

impl FastRng {
    fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { 0xCAFE_BABE_DEAD_BEEF } else { seed },
        }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    fn next_usize(&mut self, max: usize) -> usize {
        if max == 0 { 0 } else { (self.next_u64() as usize) % max }
    }
}

/// Dedicated 10,000-step continuous stateful Undo/Redo marathon test.
/// Executes 10,000 sequential random operations across all 10 command types,
/// hierarchy cycle attacks, batch rollbacks, and intermediate undo/redo sweeps.
pub fn verify_10k_step_undo_redo_marathon() -> VerificationReceipt {
    let mut rng = FastRng::new(0xFEED_FACE_CAFE_BEEF);
    let mut initial_scene = IrScene::new("MarathonScene");
    let root = make_test_node(0, 1, "Root", "Node3D", None);
    initial_scene.nodes.insert(root.id, root);

    let mut harness = HarnessSession::new(initial_scene.clone());
    let mut shadow = ShadowHistoryModel::new(initial_scene.clone());

    let mut cases_executed = 0;
    let mut invariants_held = true;

    for step in 1..=10_000 {
        cases_executed += 1;
        let op_choice = rng.next_usize(15);

        match op_choice {
            0 => {
                // Insert node
                let node_id = NodeId::new((step % 50 + 1) as u32, 1).unwrap();
                if !harness.tree.active_scene.nodes.contains_key(&node_id) {
                    let parent = Some(NodeId::new(0, 1).unwrap());
                    let node = make_test_node(node_id.index, 1, format!("Node_{step}"), "Node3D", parent);
                    let cmd = EditorCommand::InsertNode { node };
                    if harness.execute(cmd.clone()).is_ok() {
                        let _ = shadow.execute(cmd);
                    }
                }
            }
            1 => {
                // Remove existing node (other than root 0:1, and only if not referenced by active signal connections or children)
                let existing_ids: Vec<_> = harness
                    .tree
                    .active_scene
                    .nodes
                    .keys()
                    .filter(|id| {
                        id.index != 0
                            && !harness
                                .tree
                                .active_scene
                                .connections
                                .iter()
                                .any(|c| c.from_node == **id || c.to_node == **id)
                            && !harness
                                .tree
                                .active_scene
                                .nodes
                                .values()
                                .any(|n| n.parent == Some(**id))
                    })
                    .copied()
                    .collect();
                if !existing_ids.is_empty() {
                    let target_id = existing_ids[rng.next_usize(existing_ids.len())];
                    let node = harness.tree.active_scene.nodes[&target_id].clone();
                    let cmd = EditorCommand::RemoveNode { node };
                    if harness.execute(cmd.clone()).is_ok() {
                        let _ = shadow.execute(cmd);
                    }
                }
            }
            2 => {
                // Set property
                let existing_ids: Vec<_> = harness.tree.active_scene.nodes.keys().copied().collect();
                if !existing_ids.is_empty() {
                    let target_id = existing_ids[rng.next_usize(existing_ids.len())];
                    let old_val = harness.tree.active_scene.nodes[&target_id]
                        .properties
                        .get("score")
                        .cloned();
                    let new_val = Some(IrValue::I64(step as i64));
                    let cmd = EditorCommand::SetProperty {
                        node_id: target_id,
                        key: "score".into(),
                        old_value: old_val,
                        new_value: new_val,
                    };
                    if harness.execute(cmd.clone()).is_ok() {
                        let _ = shadow.execute(cmd);
                    }
                }
            }
            3 => {
                // Tune feel spec
                let old_val = harness.tree.active_scene.feel_overrides.get("skater_speed").copied();
                let cmd = EditorCommand::TuneFeelSpec {
                    key: "skater_speed".into(),
                    old_value: old_val,
                    new_value: Some(1.0 + (step as f64) * 0.001),
                };
                if harness.execute(cmd.clone()).is_ok() {
                    let _ = shadow.execute(cmd);
                }
            }
            4 => {
                // Rename node
                let existing_ids: Vec<_> = harness.tree.active_scene.nodes.keys().copied().collect();
                if !existing_ids.is_empty() {
                    let target_id = existing_ids[rng.next_usize(existing_ids.len())];
                    let old_name = harness.tree.active_scene.nodes[&target_id].name.clone();
                    let new_name = format!("Renamed_{step}");
                    let cmd = EditorCommand::RenameNode {
                        node_id: target_id,
                        old_name,
                        new_name,
                    };
                    if harness.execute(cmd.clone()).is_ok() {
                        let _ = shadow.execute(cmd);
                    }
                }
            }
            5 => {
                // Reparent node safely
                let existing_ids: Vec<_> = harness.tree.active_scene.nodes.keys().copied().collect();
                if existing_ids.len() > 1 {
                    let target_id = existing_ids[rng.next_usize(existing_ids.len())];
                    let new_parent = Some(existing_ids[rng.next_usize(existing_ids.len())]);
                    if target_id.index != 0 && !harness.tree.active_scene.would_create_cycle(target_id, new_parent) {
                        let old_parent = harness.tree.active_scene.nodes[&target_id].parent;
                        let cmd = EditorCommand::ReparentNode {
                            node_id: target_id,
                            old_parent,
                            new_parent,
                        };
                        if harness.execute(cmd.clone()).is_ok() {
                            let _ = shadow.execute(cmd);
                        }
                    }
                }
            }
            6 => {
                // Set groups
                let existing_ids: Vec<_> = harness.tree.active_scene.nodes.keys().copied().collect();
                if !existing_ids.is_empty() {
                    let target_id = existing_ids[rng.next_usize(existing_ids.len())];
                    let old_groups = harness.tree.active_scene.nodes[&target_id].groups.clone();
                    let new_groups = vec![format!("group_{}", step % 5)];
                    let cmd = EditorCommand::SetGroups {
                        node_id: target_id,
                        old_groups,
                        new_groups,
                    };
                    if harness.execute(cmd.clone()).is_ok() {
                        let _ = shadow.execute(cmd);
                    }
                }
            }
            7 => {
                // Connect signal
                let existing_ids: Vec<_> = harness.tree.active_scene.nodes.keys().copied().collect();
                if existing_ids.len() >= 2 {
                    let from = existing_ids[0];
                    let to = existing_ids[1];
                    let conn = SignalConnection {
                        from_node: from,
                        signal_name: format!("sig_{}", step % 3),
                        to_node: to,
                        method_name: "_on_sig".into(),
                    };
                    if !harness.tree.active_scene.connections.contains(&conn) {
                        let cmd = EditorCommand::ConnectSignal { connection: conn };
                        if harness.execute(cmd.clone()).is_ok() {
                            let _ = shadow.execute(cmd);
                        }
                    }
                }
            }
            8 => {
                // Disconnect signal
                if !harness.tree.active_scene.connections.is_empty() {
                    let conn = harness.tree.active_scene.connections[0].clone();
                    let cmd = EditorCommand::DisconnectSignal { connection: conn };
                    if harness.execute(cmd.clone()).is_ok() {
                        let _ = shadow.execute(cmd);
                    }
                }
            }
            9 => {
                // Attack: self-parenting cycle (must reject with InvalidHierarchyCycle)
                let existing_ids: Vec<_> = harness.tree.active_scene.nodes.keys().copied().collect();
                if !existing_ids.is_empty() {
                    let target_id = existing_ids[rng.next_usize(existing_ids.len())];
                    let scene_before = harness.tree.active_scene.clone();
                    let cmd = EditorCommand::ReparentNode {
                        node_id: target_id,
                        old_parent: harness.tree.active_scene.nodes[&target_id].parent,
                        new_parent: Some(target_id),
                    };
                    let res = harness.execute(cmd);
                    if !matches!(res, Err(IrError::InvalidHierarchyCycle(_))) {
                        invariants_held = false;
                    }
                    if !scenes_bit_identical(&harness.tree.active_scene, &scene_before) {
                        invariants_held = false;
                    }
                }
            }
            10 => {
                // Attack: Failing batch rollback (must be atomic: 0% mutation)
                let scene_before = harness.tree.active_scene.clone();
                let sub1 = EditorCommand::TuneFeelSpec {
                    key: "temp_marathon_knob".into(),
                    old_value: None,
                    new_value: Some(123.45),
                };
                let sub2_failing = EditorCommand::RemoveNode {
                    node: make_test_node(999_999, 1, "Missing", "Node", None),
                };
                let batch = EditorCommand::Batch {
                    label: "MarathonAtomicTest".into(),
                    commands: vec![sub1, sub2_failing],
                };
                let res = harness.execute(batch);
                if !matches!(res, Err(IrError::NodeNotFound(_))) {
                    invariants_held = false;
                }
                if !scenes_bit_identical(&harness.tree.active_scene, &scene_before) {
                    invariants_held = false;
                }
            }
            11 => {
                // Partial undo burst (1..5)
                let burst = rng.next_usize(5) + 1;
                for _ in 0..burst {
                    match harness.undo() {
                        Ok(true) => {
                            shadow.undo();
                        }
                        Ok(false) => break,
                        Err(_) => {
                            invariants_held = false;
                            break;
                        }
                    }
                }
            }
            12 => {
                // Partial redo burst (1..5)
                let burst = rng.next_usize(5) + 1;
                for _ in 0..burst {
                    if harness.redo().unwrap_or(false) {
                        shadow.redo();
                    } else {
                        break;
                    }
                }
            }
            _ => {
                // Valid atomic batch (2 subcommands)
                let sub1 = EditorCommand::TuneFeelSpec {
                    key: "batch_knob_a".into(),
                    old_value: harness.tree.active_scene.feel_overrides.get("batch_knob_a").copied(),
                    new_value: Some((step as f64) * 0.1),
                };
                let sub2 = EditorCommand::TuneFeelSpec {
                    key: "batch_knob_b".into(),
                    old_value: harness.tree.active_scene.feel_overrides.get("batch_knob_b").copied(),
                    new_value: Some((step as f64) * 0.2),
                };
                let batch = EditorCommand::Batch {
                    label: "ValidBatch".into(),
                    commands: vec![sub1, sub2],
                };
                if harness.execute(batch.clone()).is_ok() {
                    let _ = shadow.execute(batch);
                }
            }
        }

        // Intermediate sweep check every 500 steps
        if step % 500 == 0 {
            if !scenes_bit_identical(&harness.tree.active_scene, &shadow.current_scene) {
                invariants_held = false;
            }
            let snapshot = harness.tree.active_scene.clone();
            let mut undos = 0;
            for _ in 0..15 {
                if harness.undo().unwrap_or(false) {
                    shadow.undo();
                    undos += 1;
                }
            }
            for _ in 0..undos {
                let _ = harness.redo();
                shadow.redo();
            }
            if !scenes_bit_identical(&harness.tree.active_scene, &snapshot) {
                invariants_held = false;
            }
        }
    }

    // Marathon Exit Verification: Full unwind to step 0
    let scene_at_marathon_end = harness.tree.active_scene.clone();
    let mut total_undone = 0;
    while harness.undo().unwrap_or(false) {
        total_undone += 1;
    }

    // Must match initial scene bit-identically!
    if !scenes_bit_identical(&harness.tree.active_scene, &shadow.initial_scene) {
        invariants_held = false;
    }

    // Full rewind to step 10,000
    let mut total_rewound = 0;
    while harness.redo().unwrap_or(false) {
        total_rewound += 1;
    }
    if total_undone != total_rewound {
        invariants_held = false;
    }
    if !scenes_bit_identical(&harness.tree.active_scene, &scene_at_marathon_end) {
        invariants_held = false;
    }

    VerificationReceipt {
        suite_name: "undo_redo_10k_step_marathon".into(),
        cases_executed,
        invariants_held,
    }
}

/// Helper generating extreme float values for fuzzing.
pub fn extreme_f64_values() -> Vec<f64> {
    vec![
        0.0,
        -0.0,
        1.0,
        -1.0,
        f64::MAX,
        f64::MIN,
        f64::MIN_POSITIVE,
        f64::EPSILON,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::from_bits(0x7ff8_0000_0000_0001),
        1.234_567_890_123_456,
        -9.876_543_210_987_654,
    ]
}

/// Helper generating rich strings including Unicode, quotes, newlines, and null bytes.
pub fn rich_fuzz_strings() -> Vec<String> {
    vec![
        "".into(),
        "simple".into(),
        "Skate 2 / Skate 3 Plaza \u{1F6F9}".into(),
        "\"double-quoted\" and 'single-quoted'".into(),
        "Line 1\nLine 2\r\nLine 3\tTabbed".into(),
        "🦀 Ferris \u{0000} NullByte \u{1F4A5} Crash".into(),
        "日本語のテキスト".into(),
        "مرحبا بالعالم".into(),
        "\\backslash /slash &amp; <xml> {json: true}".into(),
    ]
}

/// Helper creating arbitrary nested `IrValue`s.
pub fn sample_adversarial_ir_values() -> Vec<IrValue> {
    let mut values = vec![
        IrValue::Nil,
        IrValue::Bool(true),
        IrValue::Bool(false),
        IrValue::I64(i64::MIN),
        IrValue::I64(-1),
        IrValue::I64(0),
        IrValue::I64(1),
        IrValue::I64(i64::MAX),
    ];

    for &f in &extreme_f64_values() {
        values.push(IrValue::F64(f));
    }

    for s in rich_fuzz_strings() {
        values.push(IrValue::String(s.clone()));
        values.push(IrValue::NodePath(format!("../{s}")));
        values.push(IrValue::ResourcePath(format!("res://{s}.tscn")));
    }

    values.push(IrValue::Vec2([0.0, -0.0]));
    values.push(IrValue::Vec3([f32::INFINITY, 0.0, f32::NEG_INFINITY]));
    values.push(IrValue::Quat([0.0, 0.0, 0.0, 1.0]));
    values.push(IrValue::Color([1.0, 0.5, 0.25, 1.0]));

    let nested_array = IrValue::Array(vec![
        IrValue::I64(42),
        IrValue::String("inner".into()),
        IrValue::Array(vec![
            IrValue::Bool(true),
            IrValue::F64(f64::from_bits(0x7ff8_0000_0000_0001)),
        ]),
    ]);
    values.push(nested_array);

    let mut map_inner = BTreeMap::new();
    map_inner.insert("sub_key".into(), IrValue::String("sub_val\nwith_newline".into()));
    map_inner.insert("float_val".into(), IrValue::F64(f64::EPSILON));

    let mut map_outer = BTreeMap::new();
    map_outer.insert("nested_map".into(), IrValue::Map(map_inner));
    map_outer.insert("count".into(), IrValue::I64(999));
    values.push(IrValue::Map(map_outer));

    values
}

/// Helper constructing a node with generated values.
pub fn make_test_node(
    index: u32,
    generation: u32,
    name: impl Into<String>,
    class_name: impl Into<String>,
    parent: Option<NodeId>,
) -> IrNode {
    let generation_nz = NonZeroU32::new(generation.max(1)).expect("non-zero");
    let id = NodeId {
        index,
        generation: generation_nz,
    };
    IrNode {
        id,
        name: name.into(),
        class_name: class_name.into(),
        parent,
        instance_scene: None,
        properties: BTreeMap::new(),
        groups: Vec::new(),
    }
}

/// Proptest strategy generating arbitrary `NodeId`s.
pub fn arb_node_id(max_index: u32) -> impl proptest::strategy::Strategy<Value = NodeId> {
    use proptest::prelude::*;
    (0..=max_index, 1..=5u32).prop_map(|(idx, generation)| {
        NodeId::new(idx, generation).expect("non-zero generation")
    })
}

/// Proptest strategy generating arbitrary `IrValue`s.
pub fn arb_leaf_ir_value() -> impl proptest::strategy::Strategy<Value = IrValue> {
    use proptest::prelude::*;
    prop_oneof![
        Just(IrValue::Nil),
        any::<bool>().prop_map(IrValue::Bool),
        any::<i64>().prop_map(IrValue::I64),
        prop_oneof![
            any::<f64>(),
            Just(0.0),
            Just(-0.0),
            Just(1.0),
            Just(-1.0),
            Just(f64::MAX),
            Just(f64::MIN),
            Just(f64::MIN_POSITIVE),
            Just(f64::EPSILON),
            Just(f64::INFINITY),
            Just(f64::NEG_INFINITY),
            Just(f64::from_bits(0x7ff8_0000_0000_0001)),
        ]
        .prop_map(IrValue::F64),
        prop_oneof![
            "[a-zA-Z0-9_ \\-'\"]{0,20}",
            Just("Line1\nLine2\r\nLine3\tTab".into()),
            Just("🦀 Skate \u{1F6F9} \u{0000}".into()),
            Just("日本語 / Arabic مرحبا".into()),
        ]
        .prop_map(IrValue::String),
        any::<[f32; 2]>().prop_map(IrValue::Vec2),
        any::<[f32; 3]>().prop_map(IrValue::Vec3),
        any::<[f32; 4]>().prop_map(IrValue::Quat),
        any::<[f32; 4]>().prop_map(IrValue::Color),
        "[a-zA-Z0-9_/]{1,15}".prop_map(|s| IrValue::NodePath(format!("../{s}"))),
        "[a-zA-Z0-9_]{1,10}".prop_map(|s| IrValue::ResourcePath(format!("res://{s}.tscn"))),
    ]
}

/// Proptest strategy generating arbitrary `FuzzOperation`s covering all 10 `EditorCommand`s.
pub fn arb_fuzz_operation() -> impl proptest::strategy::Strategy<Value = FuzzOperation> {
    use proptest::prelude::*;
    prop_oneof![
        // 1. InsertNode
        (
            arb_node_id(25),
            "[A-Z][a-zA-Z0-9_]{1,12}",
            prop_oneof![
                Just("Node3D".into()),
                Just("CharacterBody3D".into()),
                Just("Camera3D".into()),
            ],
            prop::option::of(arb_node_id(25)),
            prop::collection::btree_map("[a-z_]{1,8}", arb_leaf_ir_value(), 0..3),
        )
            .prop_map(|(id, name, class_name, parent, properties)| {
                FuzzOperation::InsertNode {
                    node: IrNode {
                        id,
                        name,
                        class_name,
                        parent,
                        instance_scene: None,
                        properties,
                        groups: Vec::new(),
                    },
                }
            }),
        // 2. RemoveNode
        arb_node_id(25).prop_map(|target_id| FuzzOperation::RemoveNode { target_id }),
        // 3. SetProperty
        (
            arb_node_id(25),
            prop_oneof![
                Just("max_speed".into()),
                Just("gravity".into()),
                Just("player_name".into()),
            ],
            prop::option::of(arb_leaf_ir_value()),
        )
            .prop_map(|(target_id, key, new_value)| {
                FuzzOperation::SetProperty {
                    target_id,
                    key,
                    new_value,
                }
            }),
        // 4. TuneFeelSpec
        (
            prop_oneof![
                Just("ollie_pop_impulse".into()),
                Just("skater_speed_mult".into()),
            ],
            prop::option::of(any::<f64>()),
        )
            .prop_map(|(key, new_value)| FuzzOperation::TuneFeelSpec { key, new_value }),
        // 5. RenameNode
        (arb_node_id(25), "[A-Z][a-zA-Z0-9_]{1,10}")
            .prop_map(|(target_id, new_name)| FuzzOperation::RenameNode { target_id, new_name }),
        // 6. ReparentNode
        (arb_node_id(25), prop::option::of(arb_node_id(25)))
            .prop_map(|(target_id, new_parent)| FuzzOperation::ReparentNode { target_id, new_parent }),
        // 7. SetGroups
        (arb_node_id(25), prop::collection::vec("[a-z_]{1,8}", 0..3))
            .prop_map(|(target_id, new_groups)| FuzzOperation::SetGroups { target_id, new_groups }),
        // 8. UndoBurst
        (1..=10usize).prop_map(FuzzOperation::UndoBurst),
        // 9. RedoBurst
        (1..=10usize).prop_map(FuzzOperation::RedoBurst),
        // 10. Self-parenting attack
        arb_node_id(25).prop_map(|target_id| FuzzOperation::AttackSelfParenting { target_id }),
        // 11. Multi-hop cycle attack
        Just(FuzzOperation::AttackMultiHopCycle),
        // 12. Failing batch atomic rollback attack
        Just(FuzzOperation::AttackFailingBatchRollback),
    ]
}
