//! Foreplay Atlas (`foreplay-atlas`) — Semantic layer modeled on `@3jse/atlas`.
//! Compiles `IrScene` + declared contracts into human- and agent-legible meaning graphs.

pub mod timetravel;
pub mod visual_graph;

pub use timetravel::{
    FeelSpecLiveChange, HistoricalFrameRecord, LeaseError, SignalPulse, SubtreeLease,
    SubtreeLeaseManager, TimeTravelAtlasRecorder, TimeTravelScrubView,
};
pub use visual_graph::{
    MathOperator, VisualGraph, VisualGraphConnection, VisualGraphExecutionReport, VisualGraphNode,
    VisualGraphNodeKind, VisualGraphTrigger,
};

use foreplay_ir::{FeelSpecKnob, IrScene, NodeId};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// Semantic edge showing gameplay meaning (`Skater -> Movement -> Jump -> Camera`).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SemanticEdge {
    pub from: NodeId,
    pub to: NodeId,
    pub relation: String,
}

/// Blast-radius task slice exported for Carbon/Argon subagents to minimize context tokens.
#[derive(Clone, PartialEq, Debug)]
pub struct AtlasTaskContext {
    pub scene_name: String,
    pub affected_nodes: Vec<NodeId>,
    pub edges: Vec<SemanticEdge>,
}

impl AtlasTaskContext {
    pub fn from_scene(scene: &IrScene) -> Self {
        let affected_nodes = scene.nodes.keys().copied().collect();
        let edges = scene
            .connections
            .iter()
            .map(|c| SemanticEdge {
                from: c.from_node,
                to: c.to_node,
                relation: format!("{} -> {}", c.signal_name, c.method_name),
            })
            .collect();
        Self {
            scene_name: scene.name.clone(),
            affected_nodes,
            edges,
        }
    }
}

/// Semantic directed graph representing scene dataflow and hierarchy with topological layer assignment.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AtlasGraph {
    pub layers: BTreeMap<NodeId, usize>,
    pub max_layer: usize,
    pub edges: Vec<SemanticEdge>,
}

impl AtlasGraph {
    /// Computes deterministic topological layer assignment from scene hierarchy and signal flow.
    pub fn from_scene(scene: &IrScene) -> Self {
        let mut layers: BTreeMap<NodeId, usize> = scene.nodes.keys().map(|&id| (id, 0)).collect();
        let mut edges = Vec::new();

        // 1. Hierarchy edges (parent -> child)
        let mut adj_edges: Vec<(NodeId, NodeId)> = Vec::new();
        for (&child_id, node) in &scene.nodes {
            if let Some(parent_id) = node.parent
                && scene.nodes.contains_key(&parent_id)
            {
                adj_edges.push((parent_id, child_id));
                edges.push(SemanticEdge {
                    from: parent_id,
                    to: child_id,
                    relation: "parent_of".to_string(),
                });
            }
        }

        // 2. Signal connection edges (from_node -> to_node)
        for conn in &scene.connections {
            if scene.nodes.contains_key(&conn.from_node) && scene.nodes.contains_key(&conn.to_node) {
                adj_edges.push((conn.from_node, conn.to_node));
                edges.push(SemanticEdge {
                    from: conn.from_node,
                    to: conn.to_node,
                    relation: format!("{} -> {}", conn.signal_name, conn.method_name),
                });
            }
        }

        // 3. Deterministic longest-path layer relaxation with cycle bounding
        let node_count = scene.nodes.len();
        for _ in 0..node_count {
            let mut changed = false;
            for &(u, v) in &adj_edges {
                let u_layer = layers.get(&u).copied().unwrap_or(0);
                let v_layer = layers.get(&v).copied().unwrap_or(0);
                if u_layer + 1 > v_layer {
                    layers.insert(v, u_layer + 1);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        let max_layer = layers.values().copied().max().unwrap_or(0);

        Self {
            layers,
            max_layer,
            edges,
        }
    }

    /// Returns the computed column layer for a given node.
    pub fn layer_of(&self, node: NodeId) -> Option<usize> {
        self.layers.get(&node).copied()
    }

    /// Returns all node IDs assigned to the given layout layer in deterministic sorted order.
    pub fn nodes_in_layer(&self, layer: usize) -> Vec<NodeId> {
        self.layers
            .iter()
            .filter(|&(_, &l)| l == layer)
            .map(|(&id, _)| id)
            .collect()
    }
}

/// Warnings emitted when inspecting scene `FeelSpec` tuning overrides against spec knobs.
#[derive(Clone, PartialEq, Debug)]
pub enum FeelSpecWarning {
    OutOfRange {
        key: String,
        value: f64,
        min: f64,
        max: f64,
    },
    UnknownKnob {
        key: String,
        value: f64,
    },
}

/// Validates and clamps `IrScene` feel overrides against registered `FeelSpecKnob` bounds.
#[derive(Debug, Clone, Copy, Default)]
pub struct FeelSpecValidator;

impl FeelSpecValidator {
    /// Validates `scene.feel_overrides` against registered knobs and reports warnings for out-of-range or unknown keys.
    pub fn validate(scene: &IrScene, knobs: &[FeelSpecKnob]) -> Vec<FeelSpecWarning> {
        let knob_map: BTreeMap<&str, &FeelSpecKnob> =
            knobs.iter().map(|k| (k.key.as_str(), k)).collect();
        let mut warnings = Vec::new();

        for (key, &val) in &scene.feel_overrides {
            if let Some(knob) = knob_map.get(key.as_str()) {
                if val < knob.min || val > knob.max {
                    warnings.push(FeelSpecWarning::OutOfRange {
                        key: key.clone(),
                        value: val,
                        min: knob.min,
                        max: knob.max,
                    });
                }
            } else {
                warnings.push(FeelSpecWarning::UnknownKnob {
                    key: key.clone(),
                    value: val,
                });
            }
        }

        warnings
    }

    /// Clamps all `scene.feel_overrides` to their registered `[knob.min, knob.max]` ranges.
    /// Returns the number of feel override values clamped.
    pub fn clamp_overrides(scene: &mut IrScene, knobs: &[FeelSpecKnob]) -> usize {
        let knob_map: BTreeMap<&str, &FeelSpecKnob> =
            knobs.iter().map(|k| (k.key.as_str(), k)).collect();
        let mut clamped_count = 0;

        for (key, val) in &mut scene.feel_overrides {
            if let Some(knob) = knob_map.get(key.as_str()) {
                let clamped = val.clamp(knob.min, knob.max);
                if (clamped - *val).abs() > f64::EPSILON {
                    *val = clamped;
                    clamped_count += 1;
                }
            }
        }

        clamped_count
    }
}

/// Computes the minimal blast-radius subgraph surrounding a focal node within `max_hops`.
pub fn blast_radius_subgraph(scene: &IrScene, focal: NodeId, max_hops: usize) -> AtlasTaskContext {
    if !scene.nodes.contains_key(&focal) {
        return AtlasTaskContext {
            scene_name: scene.name.clone(),
            affected_nodes: Vec::new(),
            edges: Vec::new(),
        };
    }

    // Build undirected adjacency graph across hierarchy and signal connections
    let mut adj: BTreeMap<NodeId, BTreeSet<NodeId>> = BTreeMap::new();
    for (&node_id, node) in &scene.nodes {
        adj.entry(node_id).or_default();
        if let Some(parent) = node.parent
            && scene.nodes.contains_key(&parent)
        {
            adj.entry(node_id).or_default().insert(parent);
            adj.entry(parent).or_default().insert(node_id);
        }
    }
    for conn in &scene.connections {
        if scene.nodes.contains_key(&conn.from_node) && scene.nodes.contains_key(&conn.to_node) {
            adj.entry(conn.from_node).or_default().insert(conn.to_node);
            adj.entry(conn.to_node).or_default().insert(conn.from_node);
        }
    }

    // Breadth-First Search up to max_hops
    let mut visited: BTreeSet<NodeId> = BTreeSet::new();
    let mut queue: VecDeque<(NodeId, usize)> = VecDeque::new();

    visited.insert(focal);
    queue.push_back((focal, 0));

    while let Some((curr, hops)) = queue.pop_front() {
        if hops < max_hops
            && let Some(neighbors) = adj.get(&curr)
        {
            for &neighbor in neighbors {
                if visited.insert(neighbor) {
                    queue.push_back((neighbor, hops + 1));
                }
            }
        }
    }

    let affected_nodes: Vec<NodeId> = visited.iter().copied().collect();

    // Collect all semantic edges where both endpoints belong to the blast radius
    let mut edges = Vec::new();
    for conn in &scene.connections {
        if visited.contains(&conn.from_node) && visited.contains(&conn.to_node) {
            edges.push(SemanticEdge {
                from: conn.from_node,
                to: conn.to_node,
                relation: format!("{} -> {}", conn.signal_name, conn.method_name),
            });
        }
    }

    AtlasTaskContext {
        scene_name: scene.name.clone(),
        affected_nodes,
        edges,
    }
}

/// Semantic health summary combining feel specification warnings and headless tick evidence.
#[derive(Clone, PartialEq, Debug)]
pub struct AtlasHealthReport {
    pub scene_name: String,
    pub is_healthy: bool,
    pub warning_count: usize,
    pub warnings: Vec<FeelSpecWarning>,
    pub headless_ticks_verified: u64,
    pub node_count: usize,
    pub connection_count: usize,
}

impl AtlasHealthReport {
    /// Pure function computing semantic health from validation warnings and headless tick evidence.
    pub fn generate(scene: &IrScene, knobs: &[FeelSpecKnob], headless_ticks_verified: u64) -> Self {
        let warnings = FeelSpecValidator::validate(scene, knobs);
        let warning_count = warnings.len();
        let is_healthy = warning_count == 0 && headless_ticks_verified > 0;

        Self {
            scene_name: scene.name.clone(),
            is_healthy,
            warning_count,
            warnings,
            headless_ticks_verified,
            node_count: scene.nodes.len(),
            connection_count: scene.connections.len(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use foreplay_ir::{IrNode, SignalConnection};

    #[test]
    fn topological_layer_assignment_computes_columns_from_flow() {
        let mut scene = IrScene::new("FlowLayerTest");
        let n0 = NodeId::new(0, 1).unwrap();
        let n1 = NodeId::new(1, 1).unwrap();
        let n2 = NodeId::new(2, 1).unwrap();
        let n3 = NodeId::new(3, 1).unwrap();

        // Hierarchy: n0 -> n1
        scene.nodes.insert(
            n0,
            IrNode {
                id: n0,
                name: "Root".into(),
                class_name: "Node".into(),
                parent: None,
                instance_scene: None,
                properties: BTreeMap::new(),
                groups: Vec::new(),
            },
        );
        scene.nodes.insert(
            n1,
            IrNode {
                id: n1,
                name: "Child".into(),
                class_name: "Node".into(),
                parent: Some(n0),
                instance_scene: None,
                properties: BTreeMap::new(),
                groups: Vec::new(),
            },
        );
        // Signal: n1 -> n2 -> n3
        scene.nodes.insert(
            n2,
            IrNode {
                id: n2,
                name: "Handler".into(),
                class_name: "Node".into(),
                parent: None,
                instance_scene: None,
                properties: BTreeMap::new(),
                groups: Vec::new(),
            },
        );
        scene.nodes.insert(
            n3,
            IrNode {
                id: n3,
                name: "Consumer".into(),
                class_name: "Node".into(),
                parent: None,
                instance_scene: None,
                properties: BTreeMap::new(),
                groups: Vec::new(),
            },
        );

        scene.connections.push(SignalConnection {
            from_node: n1,
            signal_name: "event_a".into(),
            to_node: n2,
            method_name: "on_event_a".into(),
        });
        scene.connections.push(SignalConnection {
            from_node: n2,
            signal_name: "event_b".into(),
            to_node: n3,
            method_name: "on_event_b".into(),
        });

        let graph = AtlasGraph::from_scene(&scene);

        // n0 is layer 0
        assert_eq!(graph.layer_of(n0), Some(0));
        // n1 is child of n0 -> layer 1
        assert_eq!(graph.layer_of(n1), Some(1));
        // n2 receives signal from n1 -> layer 2
        assert_eq!(graph.layer_of(n2), Some(2));
        // n3 receives signal from n2 -> layer 3
        assert_eq!(graph.layer_of(n3), Some(3));
        assert_eq!(graph.max_layer, 3);
        assert_eq!(graph.nodes_in_layer(1), vec![n1]);
    }

    #[test]
    fn feel_spec_validator_detects_out_of_range_and_clamps() {
        let mut scene = IrScene::new("FeelTuningTest");
        scene.feel_overrides.insert("jump_force".into(), 15.0); // max is 10.0
        scene.feel_overrides.insert("gravity".into(), -9.8); // min is 0.0
        scene.feel_overrides.insert("unknown_knob".into(), 42.0); // unspec

        let knobs = vec![
            FeelSpecKnob {
                key: "jump_force".into(),
                intent: "Ollie height".into(),
                unit: "m/s".into(),
                min: 0.0,
                max: 10.0,
                default_value: 5.0,
            },
            FeelSpecKnob {
                key: "gravity".into(),
                intent: "World gravity".into(),
                unit: "m/s^2".into(),
                min: 0.0,
                max: 20.0,
                default_value: 9.8,
            },
        ];

        let warnings = FeelSpecValidator::validate(&scene, &knobs);
        assert_eq!(warnings.len(), 3);

        // Clamping should bound jump_force to 10.0 and gravity to 0.0
        let clamped_count = FeelSpecValidator::clamp_overrides(&mut scene, &knobs);
        assert_eq!(clamped_count, 2);
        assert_eq!(scene.feel_overrides["jump_force"], 10.0);
        assert_eq!(scene.feel_overrides["gravity"], 0.0);

        // Re-validating after clamping should only report unknown_knob
        let new_warnings = FeelSpecValidator::validate(&scene, &knobs);
        assert_eq!(new_warnings.len(), 1);
        matches!(&new_warnings[0], FeelSpecWarning::UnknownKnob { .. });
    }

    #[test]
    fn blast_radius_subgraph_expands_by_hops() {
        let mut scene = IrScene::new("BlastRadiusTest");
        let n0 = NodeId::new(0, 1).unwrap();
        let n1 = NodeId::new(1, 1).unwrap();
        let n2 = NodeId::new(2, 1).unwrap();
        let n3 = NodeId::new(3, 1).unwrap();

        for &id in &[n0, n1, n2, n3] {
            scene.nodes.insert(
                id,
                IrNode {
                    id,
                    name: format!("Node_{}", id.index),
                    class_name: "Node".into(),
                    parent: None,
                    instance_scene: None,
                    properties: BTreeMap::new(),
                    groups: Vec::new(),
                },
            );
        }

        // Chain: n0 -> n1 -> n2 -> n3
        scene.connections.push(SignalConnection {
            from_node: n0,
            signal_name: "sig0".into(),
            to_node: n1,
            method_name: "meth0".into(),
        });
        scene.connections.push(SignalConnection {
            from_node: n1,
            signal_name: "sig1".into(),
            to_node: n2,
            method_name: "meth1".into(),
        });
        scene.connections.push(SignalConnection {
            from_node: n2,
            signal_name: "sig2".into(),
            to_node: n3,
            method_name: "meth2".into(),
        });

        // 0 hops from n1: only n1
        let ctx0 = blast_radius_subgraph(&scene, n1, 0);
        assert_eq!(ctx0.affected_nodes, vec![n1]);
        assert!(ctx0.edges.is_empty());

        // 1 hop from n1: n0, n1, n2
        let ctx1 = blast_radius_subgraph(&scene, n1, 1);
        assert_eq!(ctx1.affected_nodes, vec![n0, n1, n2]);
        assert_eq!(ctx1.edges.len(), 2);

        // 2 hops from n1: all 4 nodes
        let ctx2 = blast_radius_subgraph(&scene, n1, 2);
        assert_eq!(ctx2.affected_nodes, vec![n0, n1, n2, n3]);
        assert_eq!(ctx2.edges.len(), 3);
    }

    #[test]
    fn atlas_health_report_generation() {
        let mut scene = IrScene::new("HealthReportTest");
        let n0 = NodeId::new(0, 1).unwrap();
        scene.nodes.insert(
            n0,
            IrNode {
                id: n0,
                name: "Root".into(),
                class_name: "Node".into(),
                parent: None,
                instance_scene: None,
                properties: BTreeMap::new(),
                groups: Vec::new(),
            },
        );

        let knobs = vec![];

        // Healthy when 0 warnings and > 0 headless ticks verified
        let report_healthy = AtlasHealthReport::generate(&scene, &knobs, 120);
        assert!(report_healthy.is_healthy);
        assert_eq!(report_healthy.warning_count, 0);
        assert_eq!(report_healthy.headless_ticks_verified, 120);

        // Unhealthy if tick verification is 0
        let report_unverified = AtlasHealthReport::generate(&scene, &knobs, 0);
        assert!(!report_unverified.is_healthy);

        // Unhealthy if there are warnings
        scene.feel_overrides.insert("unknown_knob".into(), 1.0);
        let report_warn = AtlasHealthReport::generate(&scene, &knobs, 60);
        assert!(!report_warn.is_healthy);
        assert_eq!(report_warn.warning_count, 1);
    }

    #[test]
    fn visual_graph_evaluation_dataflow_and_math() {
        use foreplay_ir::IrValue;

        let mut scene = IrScene::new("VisualGraphScene");
        let skater_id = NodeId::new(1, 1).unwrap();
        let hud_id = NodeId::new(2, 1).unwrap();

        let mut skater_node = IrNode {
            id: skater_id,
            name: "Skater".into(),
            class_name: "CharacterBody3D".into(),
            parent: None,
            instance_scene: None,
            properties: BTreeMap::new(),
            groups: vec!["player".into()],
        };
        skater_node
            .properties
            .insert("speed".into(), IrValue::F64(10.0));
        scene.nodes.insert(skater_id, skater_node);

        let hud_node = IrNode {
            id: hud_id,
            name: "HUD".into(),
            class_name: "Control".into(),
            parent: None,
            instance_scene: None,
            properties: BTreeMap::new(),
            groups: vec!["ui".into()],
        };
        scene.nodes.insert(hud_id, hud_node);

        let mut graph = VisualGraph::new("vg_speed", "SpeedTracker");
        graph.add_node(VisualGraphNode::new(
            1,
            VisualGraphNodeKind::OnProcess,
            [0.0, 0.0],
        ));
        graph.add_node(VisualGraphNode::new(
            2,
            VisualGraphNodeKind::GetProperty {
                node_id: skater_id,
                property: "speed".into(),
            },
            [100.0, 0.0],
        ));
        graph.add_node(
            VisualGraphNode::new(
                3,
                VisualGraphNodeKind::MathOp {
                    op: MathOperator::Multiply,
                },
                [200.0, 0.0],
            )
            .with_property("b", IrValue::F64(2.0)),
        );
        graph.add_node(VisualGraphNode::new(
            4,
            VisualGraphNodeKind::SetProperty {
                node_id: hud_id,
                property: "display_speed".into(),
            },
            [300.0, 0.0],
        ));
        graph.add_node(VisualGraphNode::new(
            5,
            VisualGraphNodeKind::TuneFeel {
                key: "speed_scale".into(),
            },
            [400.0, 0.0],
        ));

        graph.connect(1, "exec", 4, "exec");
        graph.connect(2, "value", 3, "a");
        graph.connect(3, "result", 4, "value");
        graph.connect(4, "exec", 5, "exec");
        graph.connect(3, "result", 5, "value");

        let report = graph.evaluate(&VisualGraphTrigger::OnProcess { delta: 0.016 }, &mut scene);

        assert!(report.executed_nodes.contains(&1));
        assert!(report.executed_nodes.contains(&4));
        assert!(report.executed_nodes.contains(&5));

        assert_eq!(
            scene.nodes[&hud_id].properties.get("display_speed"),
            Some(&IrValue::F64(20.0))
        );
        assert_eq!(scene.feel_overrides.get("speed_scale"), Some(&20.0));
    }

    #[test]
    fn visual_graph_compiles_to_semantic_edges() {
        let skater_id = NodeId::new(1, 1).unwrap();
        let hud_id = NodeId::new(2, 1).unwrap();
        let camera_id = NodeId::new(3, 1).unwrap();

        let mut graph = VisualGraph::new("vg_feel", "FeelFlow");
        graph.add_node(VisualGraphNode::new(
            1,
            VisualGraphNodeKind::GetProperty {
                node_id: skater_id,
                property: "velocity".into(),
            },
            [0.0, 0.0],
        ));
        graph.add_node(VisualGraphNode::new(
            2,
            VisualGraphNodeKind::SetProperty {
                node_id: hud_id,
                property: "hud_velocity".into(),
            },
            [100.0, 0.0],
        ));
        graph.add_node(VisualGraphNode::new(
            3,
            VisualGraphNodeKind::EmitSignal {
                from_node: skater_id,
                signal_name: "trick_landed".into(),
            },
            [200.0, 0.0],
        ));
        graph.add_node(VisualGraphNode::new(
            4,
            VisualGraphNodeKind::SetProperty {
                node_id: camera_id,
                property: "shake_trauma".into(),
            },
            [300.0, 0.0],
        ));

        graph.connect(1, "value", 2, "value");
        graph.connect(3, "exec", 4, "exec");

        let edges = graph.compile_to_semantic_edges();
        assert_eq!(edges.len(), 2);

        let edge_dataflow = edges.iter().find(|e| e.from == skater_id && e.to == hud_id);
        assert!(edge_dataflow.is_some());
        assert!(edge_dataflow.unwrap().relation.contains("velocity"));

        let edge_signal = edges.iter().find(|e| e.from == skater_id && e.to == camera_id);
        assert!(edge_signal.is_some());
        assert!(edge_signal.unwrap().relation.contains("trick_landed"));
    }

    #[test]
    fn time_travel_recorder_scrubbing_and_causal_chain_tracing() {
        use foreplay_ir::IrValue;

        let n1 = NodeId::new(1, 1).unwrap();
        let n2 = NodeId::new(2, 1).unwrap();
        let n3 = NodeId::new(3, 1).unwrap();

        let mut recorder = TimeTravelAtlasRecorder::new(600);

        // Frame 10: pulse 1 (n1 -> n2)
        recorder.record_frame(
            10,
            0x1010,
            vec![SignalPulse {
                pulse_id: 1,
                tick: 10,
                from_node: n1,
                signal_name: "trick_initiated".into(),
                to_node: n2,
                method_name: "handle_trick".into(),
                payload: IrValue::String("kickflip".into()),
                caused_by: None,
            }],
            vec![],
        );

        // Frame 11: pulse 2 caused by pulse 1 (n2 -> n3)
        recorder.record_frame(
            11,
            0x1111,
            vec![SignalPulse {
                pulse_id: 2,
                tick: 11,
                from_node: n2,
                signal_name: "score_awarded".into(),
                to_node: n3,
                method_name: "add_score".into(),
                payload: IrValue::I64(500),
                caused_by: Some(1),
            }],
            vec![FeelSpecLiveChange {
                tick: 11,
                key: "camera_shake".into(),
                old_value: Some(0.0),
                new_value: 0.8,
            }],
        );

        // Scrub tick 11
        let view = recorder.scrub_to_tick(11).expect("scrub tick 11");
        assert_eq!(view.state_hash, 0x1111);
        assert_eq!(view.active_pulses.len(), 1);
        assert_eq!(view.active_pulses[0].method_name, "add_score");
        assert_eq!(view.active_feel_changes.len(), 1);
        assert_eq!(view.active_feel_changes[0].key, "camera_shake");

        // Active wires at tick 10
        let wires = recorder.active_wires_at_tick(10);
        assert_eq!(wires, vec![(n1, n2, "trick_initiated".to_string())]);

        // Causal trace backwards from pulse 2
        let trace = recorder.causal_trace(2);
        assert_eq!(trace.len(), 2);
        assert_eq!(trace[0].pulse_id, 1);
        assert_eq!(trace[1].pulse_id, 2);

        // Causal chain for node n3
        let n3_chain = recorder.causal_chain_for_node(n3, 0, 20);
        assert_eq!(n3_chain.len(), 1);
        assert_eq!(n3_chain[0].pulse_id, 2);
    }

    #[test]
    fn multi_agent_subtree_lease_isolation_and_conflict_prevention() {
        use foreplay_ir::{EditorCommand, IrNode};

        let mut scene = IrScene::new("MultiAgentPark");
        let root_id = NodeId::new(0, 1).unwrap();
        let skater_root = NodeId::new(1, 1).unwrap();
        let skater_board = NodeId::new(2, 1).unwrap();
        let prop_root = NodeId::new(3, 1).unwrap();
        let prop_lamp = NodeId::new(4, 1).unwrap();

        scene.nodes.insert(
            root_id,
            IrNode {
                id: root_id,
                name: "ParkRoot".into(),
                class_name: "Node3D".into(),
                parent: None,
                instance_scene: None,
                properties: BTreeMap::new(),
                groups: vec![],
            },
        );
        scene.nodes.insert(
            skater_root,
            IrNode {
                id: skater_root,
                name: "Skater".into(),
                class_name: "CharacterBody3D".into(),
                parent: Some(root_id),
                instance_scene: None,
                properties: BTreeMap::new(),
                groups: vec![],
            },
        );
        scene.nodes.insert(
            skater_board,
            IrNode {
                id: skater_board,
                name: "Deck".into(),
                class_name: "Node3D".into(),
                parent: Some(skater_root),
                instance_scene: None,
                properties: BTreeMap::new(),
                groups: vec![],
            },
        );
        scene.nodes.insert(
            prop_root,
            IrNode {
                id: prop_root,
                name: "StreetFurniture".into(),
                class_name: "Node3D".into(),
                parent: Some(root_id),
                instance_scene: None,
                properties: BTreeMap::new(),
                groups: vec![],
            },
        );
        scene.nodes.insert(
            prop_lamp,
            IrNode {
                id: prop_lamp,
                name: "LampPost".into(),
                class_name: "Node3D".into(),
                parent: Some(prop_root),
                instance_scene: None,
                properties: BTreeMap::new(),
                groups: vec![],
            },
        );

        let mut mgr = SubtreeLeaseManager::new();

        // Agent A acquires lease on Skater subtree
        let lease_a = mgr
            .acquire_lease("agent_physics", skater_root, &scene)
            .expect("agent_physics lease");

        // Agent B acquires lease on StreetFurniture subtree (disjoint! should succeed)
        let lease_b = mgr
            .acquire_lease("agent_environment", prop_root, &scene)
            .expect("agent_environment lease");

        assert_ne!(lease_a, lease_b);

        // Agent C tries to acquire lease on ParkRoot (ancestor of both!) -> must fail with conflict
        let conflict_root = mgr.acquire_lease("agent_director", root_id, &scene);
        assert!(matches!(conflict_root, Err(LeaseError::SubtreeConflict { .. })));

        // Agent B tries to acquire lease on Deck (inside Agent A's lease!) -> must fail
        let conflict_deck = mgr.acquire_lease("agent_environment", skater_board, &scene);
        assert!(matches!(conflict_deck, Err(LeaseError::SubtreeConflict { .. })));

        // Check command allowance: Agent A mutating skater_board
        let cmd_ok = EditorCommand::SetProperty {
            node_id: skater_board,
            key: "grip_tape_color".into(),
            old_value: None,
            new_value: Some(foreplay_ir::IrValue::String("black".into())),
        };
        assert!(mgr.check_command_allowed("agent_physics", &cmd_ok, &scene).is_ok());

        // Agent A trying to mutate lamp post -> unauthorized!
        let cmd_bad = EditorCommand::SetProperty {
            node_id: prop_lamp,
            key: "light_energy".into(),
            old_value: None,
            new_value: Some(foreplay_ir::IrValue::F64(10.0)),
        };
        assert!(matches!(
            mgr.check_command_allowed("agent_physics", &cmd_bad, &scene),
            Err(LeaseError::UnauthorizedMutation { .. })
        ));

        // Release lease
        mgr.release_lease("agent_physics", lease_a).expect("release lease");
        // Now agent_director can acquire skater_root
        assert!(mgr.acquire_lease("agent_director", skater_root, &scene).is_ok());
    }
}

