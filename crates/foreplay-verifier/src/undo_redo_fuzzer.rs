//! Stateful Undo/Redo State-Machine Fuzzer (`foreplay-verifier`).
//!
//! Attacks the `HarnessSession` command bus with arbitrary sequences of valid and
//! invalid commands, bursts of partial `undo()` / `redo()`, and verifies mathematical
//! invariants against an independent `ShadowHistoryModel`.

use foreplay_harness::HarnessSession;
use foreplay_ir::{EditorCommand, IrError, IrNode, IrScene, IrValue, NodeId};
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
    if a.connections != b.connections {
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

/// Arbitrary operation for the stateful undo/redo fuzzer.
#[derive(Clone, PartialEq, Debug)]
pub enum FuzzOperation {
    /// Insert a new node into the scene.
    InsertNode {
        node: IrNode,
    },
    /// Remove an existing node by ID.
    RemoveNode {
        target_id: NodeId,
    },
    /// Set a property on an existing node.
    SetProperty {
        target_id: NodeId,
        key: String,
        new_value: Option<IrValue>,
    },
    /// Tune a feel spec knob.
    TuneFeelSpec {
        key: String,
        new_value: Option<f64>,
    },
    /// Random burst of partial undos.
    UndoBurst(usize),
    /// Random burst of partial redos.
    RedoBurst(usize),
    /// Intentionally invalid command: Insert a node whose ID already exists.
    InvalidInsertDuplicate {
        node_name: String,
    },
    /// Intentionally invalid command: Remove a node with an ID guaranteed not in the scene.
    InvalidRemoveMissing {
        missing_id: NodeId,
    },
    /// Intentionally invalid command: Set a property on a node not in the scene.
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
                if harness.tree.active_scene.nodes.contains_key(&node.id) {
                    // Node already exists: execution must cleanly fail with Err(IrError::NodeAlreadyExists)
                    let scene_before = harness.tree.active_scene.clone();
                    let res = harness.execute(EditorCommand::InsertNode { node: node.clone() });
                    if !matches!(res, Err(IrError::NodeAlreadyExists(id)) if id == node.id) {
                        return Err(format!(
                            "Op #{op_idx}: Expected NodeAlreadyExists({:?}), got {:?}",
                            node.id, res
                        ));
                    }
                    if !scenes_bit_identical(&harness.tree.active_scene, &scene_before) {
                        return Err(format!(
                            "Op #{op_idx}: active_scene corrupted by failed InsertNode!"
                        ));
                    }
                } else {
                    let cmd = EditorCommand::InsertNode { node: node.clone() };
                    harness
                        .execute(cmd.clone())
                        .map_err(|e| format!("Op #{op_idx} harness InsertNode failed: {e:?}"))?;
                    shadow
                        .execute(cmd)
                        .map_err(|e| format!("Op #{op_idx} shadow InsertNode failed: {e:?}"))?;
                    if !scenes_bit_identical(&harness.tree.active_scene, &shadow.current_scene) {
                        return Err(format!(
                            "Op #{op_idx}: Mismatch after InsertNode({:?})",
                            node.id
                        ));
                    }
                }
            }

            FuzzOperation::RemoveNode { target_id } => {
                if let Some(existing_node) = harness.tree.active_scene.nodes.get(target_id).cloned() {
                    let cmd = EditorCommand::RemoveNode { node: existing_node };
                    harness
                        .execute(cmd.clone())
                        .map_err(|e| format!("Op #{op_idx} harness RemoveNode failed: {e:?}"))?;
                    shadow
                        .execute(cmd)
                        .map_err(|e| format!("Op #{op_idx} shadow RemoveNode failed: {e:?}"))?;
                    if !scenes_bit_identical(&harness.tree.active_scene, &shadow.current_scene) {
                        return Err(format!(
                            "Op #{op_idx}: Mismatch after RemoveNode({:?})",
                            target_id
                        ));
                    }
                } else {
                    // Node does not exist: must cleanly fail with Err(IrError::NodeNotFound)
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
                        return Err(format!(
                            "Op #{op_idx}: Expected NodeNotFound({:?}), got {:?}",
                            target_id, res
                        ));
                    }
                    if !scenes_bit_identical(&harness.tree.active_scene, &scene_before) {
                        return Err(format!(
                            "Op #{op_idx}: active_scene corrupted by failed RemoveNode!"
                        ));
                    }
                }
            }

            FuzzOperation::SetProperty {
                target_id,
                key,
                new_value,
            } => {
                if let Some(node) = harness.tree.active_scene.nodes.get(target_id) {
                    let old_value = node.properties.get(key).cloned();
                    let cmd = EditorCommand::SetProperty {
                        node_id: *target_id,
                        key: key.clone(),
                        old_value,
                        new_value: new_value.clone(),
                    };
                    harness
                        .execute(cmd.clone())
                        .map_err(|e| format!("Op #{op_idx} harness SetProperty failed: {e:?}"))?;
                    shadow
                        .execute(cmd)
                        .map_err(|e| format!("Op #{op_idx} shadow SetProperty failed: {e:?}"))?;
                    if !scenes_bit_identical(&harness.tree.active_scene, &shadow.current_scene) {
                        return Err(format!(
                            "Op #{op_idx}: Mismatch after SetProperty({:?}, {})",
                            target_id, key
                        ));
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
                        return Err(format!(
                            "Op #{op_idx}: Expected NodeNotFound({:?}), got {:?}",
                            target_id, res
                        ));
                    }
                    if !scenes_bit_identical(&harness.tree.active_scene, &scene_before) {
                        return Err(format!(
                            "Op #{op_idx}: active_scene corrupted by failed SetProperty!"
                        ));
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
                harness
                    .execute(cmd.clone())
                    .map_err(|e| format!("Op #{op_idx} harness TuneFeelSpec failed: {e:?}"))?;
                shadow
                    .execute(cmd)
                    .map_err(|e| format!("Op #{op_idx} shadow TuneFeelSpec failed: {e:?}"))?;
                if !scenes_bit_identical(&harness.tree.active_scene, &shadow.current_scene) {
                    return Err(format!("Op #{op_idx}: Mismatch after TuneFeelSpec({key})"));
                }
            }

            FuzzOperation::UndoBurst(burst_len) => {
                // Invariant a) proof:
                // undo() N times followed by redo() N times produces the exact bit-identical IrScene.
                let scene_before_burst = harness.tree.active_scene.clone();
                let mut undone_count = 0;
                for _ in 0..*burst_len {
                    let did_undo = harness
                        .undo()
                        .map_err(|e| format!("Op #{op_idx} harness undo failed: {e:?}"))?;
                    if did_undo {
                        let shadow_did_undo = shadow.undo();
                        if !shadow_did_undo {
                            return Err(format!(
                                "Op #{op_idx}: harness undid but shadow refused!"
                            ));
                        }
                        undone_count += 1;
                    } else {
                        break;
                    }
                }

                if !scenes_bit_identical(&harness.tree.active_scene, &shadow.current_scene) {
                    return Err(format!(
                        "Op #{op_idx}: Mismatch during UndoBurst after {undone_count} undos"
                    ));
                }

                // Now redo exact number of times undone: must restore scene_before_burst
                for r_idx in 0..undone_count {
                    let did_redo = harness
                        .redo()
                        .map_err(|e| format!("Op #{op_idx} redo step #{r_idx} failed: {e:?}"))?;
                    if !did_redo {
                        return Err(format!(
                            "Op #{op_idx}: harness redo refused at step {r_idx}/{undone_count}!"
                        ));
                    }
                    let shadow_did_redo = shadow.redo();
                    if !shadow_did_redo {
                        return Err(format!(
                            "Op #{op_idx}: shadow redo refused at step {r_idx}/{undone_count}!"
                        ));
                    }
                }

                if !scenes_bit_identical(&harness.tree.active_scene, &scene_before_burst) {
                    return Err(format!(
                        "Op #{op_idx}: Invariant (a) failed! undo({undone_count}) followed by redo({undone_count}) did not restore bit-identical scene!"
                    ));
                }
            }

            FuzzOperation::RedoBurst(burst_len) => {
                for _ in 0..*burst_len {
                    let did_redo = harness
                        .redo()
                        .map_err(|e| format!("Op #{op_idx} harness redo failed: {e:?}"))?;
                    if did_redo {
                        let shadow_did_redo = shadow.redo();
                        if !shadow_did_redo {
                            return Err(format!(
                                "Op #{op_idx}: harness redid but shadow refused!"
                            ));
                        }
                    } else {
                        break;
                    }
                }
                if !scenes_bit_identical(&harness.tree.active_scene, &shadow.current_scene) {
                    return Err(format!(
                        "Op #{op_idx}: Mismatch after RedoBurst({burst_len})"
                    ));
                }
            }

            FuzzOperation::InvalidInsertDuplicate { node_name } => {
                // Invariant d) proof: duplicate insert cleanly fails and doesn't pollute stacks
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
                        return Err(format!(
                            "Op #{op_idx}: Expected NodeAlreadyExists({:?}), got {:?}",
                            first_id, res
                        ));
                    }
                    if !scenes_bit_identical(&harness.tree.active_scene, &scene_before) {
                        return Err(format!(
                            "Op #{op_idx}: active_scene mutated by invalid duplicate insert!"
                        ));
                    }
                }
            }

            FuzzOperation::InvalidRemoveMissing { missing_id } => {
                // Guarantee missing_id is not in scene
                let id = if harness.tree.active_scene.nodes.contains_key(missing_id) {
                    find_unused_node_id(&harness.tree.active_scene)
                } else {
                    *missing_id
                };
                let scene_before = harness.tree.active_scene.clone();
                let dummy = IrNode {
                    id,
                    name: "NonExistent".into(),
                    class_name: "Node".into(),
                    parent: None,
                    instance_scene: None,
                    properties: BTreeMap::new(),
                    groups: Vec::new(),
                };
                let res = harness.execute(EditorCommand::RemoveNode { node: dummy });
                if !matches!(res, Err(IrError::NodeNotFound(i)) if i == id) {
                    return Err(format!(
                        "Op #{op_idx}: Expected NodeNotFound({:?}), got {:?}",
                        id, res
                    ));
                }
                if !scenes_bit_identical(&harness.tree.active_scene, &scene_before) {
                    return Err(format!(
                        "Op #{op_idx}: active_scene mutated by invalid remove!"
                    ));
                }
            }

            FuzzOperation::InvalidSetPropertyMissing {
                missing_id,
                key,
                value,
            } => {
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
                    return Err(format!(
                        "Op #{op_idx}: Expected NodeNotFound({:?}), got {:?}",
                        id, res
                    ));
                }
                if !scenes_bit_identical(&harness.tree.active_scene, &scene_before) {
                    return Err(format!(
                        "Op #{op_idx}: active_scene mutated by invalid set property!"
                    ));
                }
            }
        }
    }

    // Invariant b) proof: Unwinding the entire Undo stack always restores the exact initial IrScene.
    let scene_at_end = harness.tree.active_scene.clone();
    let mut total_undone = 0;
    while harness
        .undo()
        .map_err(|e| format!("Final unwind step failed: {e:?}"))?
    {
        total_undone += 1;
    }

    if !scenes_bit_identical(&harness.tree.active_scene, &shadow.initial_scene) {
        return Err(
            "Invariant (b) failed! Unwinding entire undo stack did NOT restore bit-identical initial scene!"
                .into(),
        );
    }

    // Rewind back to end of operations
    let mut total_rewound = 0;
    while harness
        .redo()
        .map_err(|e| format!("Final rewind step failed: {e:?}"))?
    {
        total_rewound += 1;
    }
    if total_undone != total_rewound {
        return Err(format!(
            "Total undone ({total_undone}) != total rewound ({total_rewound})!"
        ));
    }
    if !scenes_bit_identical(&harness.tree.active_scene, &scene_at_end) {
        return Err(
            "Rewinding back to end did not restore the exact pre-unwind scene state!".into(),
        );
    }

    // Invariant c) proof: Executing a new command after undo() deterministically clears the Redo stack without state corruption.
    if harness
        .undo()
        .map_err(|e| format!("Pre-clearing undo failed: {e:?}"))?
    {
        let new_id = find_unused_node_id(&harness.tree.active_scene);
        let canary_node = IrNode {
            id: new_id,
            name: "InvariantCCanary".into(),
            class_name: "Node3D".into(),
            parent: None,
            instance_scene: None,
            properties: BTreeMap::new(),
            groups: vec!["canary".into()],
        };
        harness
            .execute(EditorCommand::InsertNode {
                node: canary_node.clone(),
            })
            .map_err(|e| format!("Canary insert failed: {e:?}"))?;

        // Redo stack MUST be completely empty now
        let redo_after_new_cmd = harness
            .redo()
            .map_err(|e| format!("Redo check failed: {e:?}"))?;
        if redo_after_new_cmd {
            return Err(
                "Invariant (c) failed! redo() succeeded after new command was executed!".into(),
            );
        }

        // Undo canary node
        harness
            .undo()
            .map_err(|e| format!("Canary undo failed: {e:?}"))?;
        if harness.tree.active_scene.nodes.contains_key(&new_id) {
            return Err("Canary node still present after undoing canary!".into());
        }
    }

    Ok(())
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
        f64::from_bits(0x7ff8_0000_0000_0001), // NaN with specific payload
        1.234_567_890_123_456,
        -9.876_543_210_987_654,
    ]
}

/// Helper generating extreme f32 values.
pub fn extreme_f32_values() -> Vec<f32> {
    vec![
        0.0,
        -0.0,
        1.0,
        -1.0,
        f32::MAX,
        f32::MIN,
        f32::MIN_POSITIVE,
        f32::EPSILON,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::from_bits(0x7fc0_0001),
        0.000_001,
        -0.000_001,
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

    // Nested Array
    let nested_array = IrValue::Array(vec![
        IrValue::I64(42),
        IrValue::String("inner".into()),
        IrValue::Array(vec![
            IrValue::Bool(true),
            IrValue::F64(f64::from_bits(0x7ff8_0000_0000_0001)),
        ]),
    ]);
    values.push(nested_array);

    // Nested Map
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

/// Proptest strategy generating arbitrary `IrValue`s including nested arrays,
/// nested maps, rich strings, and extreme floats.
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

/// Recursive proptest strategy generating arbitrary nested `IrValue`s.
pub fn arb_ir_value() -> impl proptest::strategy::Strategy<Value = IrValue> {
    use proptest::prelude::*;
    let leaf = arb_leaf_ir_value();
    leaf.prop_recursive(3, 16, 4, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..4).prop_map(IrValue::Array),
            prop::collection::btree_map("[a-z_]{1,8}", inner, 0..4).prop_map(IrValue::Map),
        ]
    })
}

/// Proptest strategy generating arbitrary `FuzzOperation`s.
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
                Just("CollisionShape3D".into()),
                Just("Area3D".into()),
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
                Just("is_active".into()),
                Just("transform".into()),
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
                Just("air_control_decay".into()),
                Just("camera_fov".into()),
                Just("rebound_damping".into()),
            ],
            prop::option::of(prop_oneof![
                any::<f64>(),
                Just(0.0),
                Just(-0.0),
                Just(f64::MAX),
                Just(f64::MIN),
                Just(f64::EPSILON),
                Just(f64::INFINITY),
            ]),
        )
            .prop_map(|(key, new_value)| FuzzOperation::TuneFeelSpec { key, new_value }),
        // 5. UndoBurst
        (1..=10usize).prop_map(FuzzOperation::UndoBurst),
        // 6. RedoBurst
        (1..=10usize).prop_map(FuzzOperation::RedoBurst),
        // 7. InvalidInsertDuplicate
        "[A-Za-z0-9_]{1,10}".prop_map(|node_name| FuzzOperation::InvalidInsertDuplicate { node_name }),
        // 8. InvalidRemoveMissing
        (1000..=2000u32, 1..=3u32).prop_map(|(idx, generation)| {
            FuzzOperation::InvalidRemoveMissing {
                missing_id: NodeId::new(idx, generation).unwrap(),
            }
        }),
        // 9. InvalidSetPropertyMissing
        (
            1000..=2000u32,
            "[a-z_]{1,8}",
            arb_leaf_ir_value(),
        )
            .prop_map(|(idx, key, value)| {
                FuzzOperation::InvalidSetPropertyMissing {
                    missing_id: NodeId::new(idx, 1).unwrap(),
                    key,
                    value,
                }
            }),
    ]
}

