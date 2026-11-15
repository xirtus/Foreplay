//! Visual Graph evaluation and compilation to 3JSE Atlas semantic meaning graphs.

use crate::SemanticEdge;
use foreplay_ir::{IrScene, IrValue, NodeId};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// Math operators supported by visual scripting nodes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MathOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Min,
    Max,
    Clamp,
}

/// Visual Graph node kinds matching godot visual script and feel graph paradigms.
#[derive(Clone, PartialEq, Debug)]
pub enum VisualGraphNodeKind {
    OnReady,
    OnProcess,
    OnSignal { signal_name: String },
    Branch,
    Sequence,
    MathOp { op: MathOperator },
    GetProperty { node_id: NodeId, property: String },
    SetProperty { node_id: NodeId, property: String },
    EmitSignal { from_node: NodeId, signal_name: String },
    TuneFeel { key: String },
}

/// A node within a visual scripting / feel graph.
#[derive(Clone, PartialEq, Debug)]
pub struct VisualGraphNode {
    pub id: u32,
    pub kind: VisualGraphNodeKind,
    pub position: [f32; 2],
    pub properties: BTreeMap<String, IrValue>,
}

impl VisualGraphNode {
    pub fn new(id: u32, kind: VisualGraphNodeKind, position: [f32; 2]) -> Self {
        Self {
            id,
            kind,
            position,
            properties: BTreeMap::new(),
        }
    }

    pub fn with_property(mut self, name: impl Into<String>, value: IrValue) -> Self {
        self.properties.insert(name.into(), value);
        self
    }
}

/// Directed connection between an output pin and an input pin.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct VisualGraphConnection {
    pub from_node: u32,
    pub from_pin: String,
    pub to_node: u32,
    pub to_pin: String,
}

impl VisualGraphConnection {
    pub fn new(
        from_node: u32,
        from_pin: impl Into<String>,
        to_node: u32,
        to_pin: impl Into<String>,
    ) -> Self {
        Self {
            from_node,
            from_pin: from_pin.into(),
            to_node,
            to_pin: to_pin.into(),
        }
    }
}

/// Visual scripting and feel flow graph.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct VisualGraph {
    pub id: String,
    pub name: String,
    pub nodes: BTreeMap<u32, VisualGraphNode>,
    pub connections: Vec<VisualGraphConnection>,
}

/// Trigger event for executing a visual graph.
#[derive(Clone, PartialEq, Debug)]
pub enum VisualGraphTrigger {
    OnReady,
    OnProcess { delta: f64 },
    OnSignal { signal_name: String },
}

/// Report detailing effects produced during a visual graph execution tick.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct VisualGraphExecutionReport {
    pub executed_nodes: Vec<u32>,
    pub mutated_properties: Vec<(NodeId, String, IrValue)>,
    pub emitted_signals: Vec<(NodeId, String)>,
    pub tuned_feel: Vec<(String, f64)>,
}

impl VisualGraph {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            nodes: BTreeMap::new(),
            connections: Vec::new(),
        }
    }

    pub fn add_node(&mut self, node: VisualGraphNode) -> u32 {
        let id = node.id;
        self.nodes.insert(id, node);
        id
    }

    pub fn connect(
        &mut self,
        from_node: u32,
        from_pin: impl Into<String>,
        to_node: u32,
        to_pin: impl Into<String>,
    ) {
        self.connections.push(VisualGraphConnection::new(
            from_node, from_pin, to_node, to_pin,
        ));
    }

    pub fn disconnect(
        &mut self,
        from_node: u32,
        from_pin: &str,
        to_node: u32,
        to_pin: &str,
    ) -> bool {
        let len_before = self.connections.len();
        self.connections.retain(|c| {
            !(c.from_node == from_node
                && c.from_pin == from_pin
                && c.to_node == to_node
                && c.to_pin == to_pin)
        });
        self.connections.len() < len_before
    }

    /// Compiles semantic dependency and flow edges between scene nodes referenced in this graph.
    pub fn compile_to_semantic_edges(&self) -> Vec<SemanticEdge> {
        let mut edges = Vec::new();
        let mut seen = BTreeSet::new();

        // Adjacency for connections in the visual graph
        let mut adj: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
        for conn in &self.connections {
            adj.entry(conn.from_node).or_default().push(conn.to_node);
        }

        // For each node that associates with a source NodeId
        for (&start_id, start_node) in &self.nodes {
            let (source_node_id, source_desc) = match &start_node.kind {
                VisualGraphNodeKind::GetProperty { node_id, property } => {
                    (*node_id, format!("get:{}", property))
                }
                VisualGraphNodeKind::EmitSignal { from_node, signal_name } => {
                    (*from_node, format!("signal:{}", signal_name))
                }
                _ => continue,
            };

            // Reachability traversal in the visual graph
            let mut visited = BTreeSet::new();
            let mut queue = VecDeque::new();
            queue.push_back(start_id);
            visited.insert(start_id);

            while let Some(curr) = queue.pop_front() {
                if curr != start_id
                    && let Some(target_node) = self.nodes.get(&curr)
                {
                    let target_info = match &target_node.kind {
                        VisualGraphNodeKind::SetProperty { node_id, property } => {
                            Some((*node_id, format!("set:{}", property)))
                        }
                        VisualGraphNodeKind::EmitSignal { from_node, signal_name } => {
                            Some((*from_node, format!("signal:{}", signal_name)))
                        }
                        _ => None,
                    };

                    if let Some((target_node_id, target_desc)) = target_info {
                        let rel = format!("{} -> {}", source_desc, target_desc);
                        let key = (source_node_id, target_node_id, rel.clone());
                        if seen.insert(key) {
                            edges.push(SemanticEdge {
                                from: source_node_id,
                                to: target_node_id,
                                relation: rel,
                            });
                        }
                    }
                }

                if let Some(neighbors) = adj.get(&curr) {
                    for &next in neighbors {
                        if visited.insert(next) {
                            queue.push_back(next);
                        }
                    }
                }
            }
        }

        edges
    }

    /// Evaluates the visual graph against an `IrScene` under the specified trigger.
    pub fn evaluate(
        &self,
        trigger: &VisualGraphTrigger,
        scene: &mut IrScene,
    ) -> VisualGraphExecutionReport {
        let mut report = VisualGraphExecutionReport::default();
        let mut pin_values: BTreeMap<(u32, String), IrValue> = BTreeMap::new();
        let mut flow_queue: VecDeque<u32> = VecDeque::new();

        // 1. Identify entry points matching trigger
        for (&id, node) in &self.nodes {
            match (trigger, &node.kind) {
                (VisualGraphTrigger::OnReady, VisualGraphNodeKind::OnReady) => {
                    flow_queue.push_back(id);
                    pin_values.insert((id, "exec".into()), IrValue::Bool(true));
                    pin_values.insert((id, "out".into()), IrValue::Bool(true));
                }
                (VisualGraphTrigger::OnProcess { delta }, VisualGraphNodeKind::OnProcess) => {
                    flow_queue.push_back(id);
                    pin_values.insert((id, "delta".into()), IrValue::F64(*delta));
                    pin_values.insert((id, "exec".into()), IrValue::Bool(true));
                    pin_values.insert((id, "out".into()), IrValue::Bool(true));
                }
                (
                    VisualGraphTrigger::OnSignal { signal_name },
                    VisualGraphNodeKind::OnSignal {
                        signal_name: expected,
                    },
                ) if signal_name == expected => {
                    flow_queue.push_back(id);
                    pin_values.insert((id, "exec".into()), IrValue::Bool(true));
                    pin_values.insert((id, "out".into()), IrValue::Bool(true));
                }
                _ => {}
            }
        }

        let mut steps = 0;
        let max_steps = 1000;

        while let Some(curr_id) = flow_queue.pop_front() {
            if steps >= max_steps {
                break;
            }
            steps += 1;

            let Some(node) = self.nodes.get(&curr_id) else {
                continue;
            };
            report.executed_nodes.push(curr_id);

            match &node.kind {
                VisualGraphNodeKind::OnReady | VisualGraphNodeKind::OnProcess | VisualGraphNodeKind::OnSignal { .. } => {
                    self.forward_flow(curr_id, "exec", &mut flow_queue);
                    self.forward_flow(curr_id, "out", &mut flow_queue);
                }
                VisualGraphNodeKind::GetProperty { node_id, property } => {
                    let val = scene
                        .nodes
                        .get(node_id)
                        .and_then(|n| n.properties.get(property))
                        .cloned()
                        .unwrap_or(IrValue::Nil);
                    pin_values.insert((curr_id, "value".into()), val.clone());
                    pin_values.insert((curr_id, "out".into()), val);
                    self.forward_flow(curr_id, "exec", &mut flow_queue);
                }
                VisualGraphNodeKind::SetProperty { node_id, property } => {
                    let val = self
                        .resolve_input(curr_id, "value", &mut pin_values, scene)
                        .or_else(|| self.resolve_input(curr_id, "in", &mut pin_values, scene))
                        .or_else(|| node.properties.get("value").cloned())
                        .unwrap_or(IrValue::Nil);

                    if let Some(target) = scene.nodes.get_mut(node_id) {
                        target.properties.insert(property.clone(), val.clone());
                    }
                    report.mutated_properties.push((*node_id, property.clone(), val));

                    self.forward_flow(curr_id, "exec", &mut flow_queue);
                    self.forward_flow(curr_id, "out", &mut flow_queue);
                }
                VisualGraphNodeKind::EmitSignal { from_node, signal_name } => {
                    report.emitted_signals.push((*from_node, signal_name.clone()));
                    self.forward_flow(curr_id, "exec", &mut flow_queue);
                    self.forward_flow(curr_id, "out", &mut flow_queue);
                }
                VisualGraphNodeKind::TuneFeel { key } => {
                    let val = self
                        .resolve_input(curr_id, "value", &mut pin_values, scene)
                        .or_else(|| node.properties.get("value").cloned());

                    if let Some(IrValue::F64(num)) = val {
                        scene.feel_overrides.insert(key.clone(), num);
                        report.tuned_feel.push((key.clone(), num));
                    }
                    self.forward_flow(curr_id, "exec", &mut flow_queue);
                    self.forward_flow(curr_id, "out", &mut flow_queue);
                }
                VisualGraphNodeKind::Branch => {
                    let cond = self
                        .resolve_input(curr_id, "condition", &mut pin_values, scene)
                        .or_else(|| node.properties.get("condition").cloned())
                        .unwrap_or(IrValue::Bool(false));

                    let branch_pin = if is_truthy(&cond) { "true" } else { "false" };
                    self.forward_flow(curr_id, branch_pin, &mut flow_queue);
                }
                VisualGraphNodeKind::Sequence => {
                    // Forward across sequential output pins: "0", "1", "2"... or pins starting with "out_"
                    let mut seq_conns: Vec<&VisualGraphConnection> = self
                        .connections
                        .iter()
                        .filter(|c| c.from_node == curr_id)
                        .collect();
                    seq_conns.sort_by_key(|c| c.from_pin.clone());
                    for conn in seq_conns {
                        flow_queue.push_back(conn.to_node);
                    }
                }
                VisualGraphNodeKind::MathOp { op } => {
                    let a = self
                        .resolve_input(curr_id, "a", &mut pin_values, scene)
                        .or_else(|| node.properties.get("a").cloned());
                    let b = self
                        .resolve_input(curr_id, "b", &mut pin_values, scene)
                        .or_else(|| node.properties.get("b").cloned());
                    let c = self
                        .resolve_input(curr_id, "c", &mut pin_values, scene)
                        .or_else(|| node.properties.get("c").cloned());

                    if let Some(res) = evaluate_math_op(*op, a.as_ref(), b.as_ref(), c.as_ref()) {
                        pin_values.insert((curr_id, "result".into()), res.clone());
                        pin_values.insert((curr_id, "out".into()), res);
                    }
                    self.forward_flow(curr_id, "exec", &mut flow_queue);
                }
            }
        }

        report
    }

    fn forward_flow(&self, from_node: u32, from_pin: &str, flow_queue: &mut VecDeque<u32>) {
        for conn in &self.connections {
            if conn.from_node == from_node && conn.from_pin == from_pin {
                flow_queue.push_back(conn.to_node);
            }
        }
    }

    fn resolve_input(
        &self,
        node_id: u32,
        pin: &str,
        pin_values: &mut BTreeMap<(u32, String), IrValue>,
        scene: &IrScene,
    ) -> Option<IrValue> {
        let conn = self
            .connections
            .iter()
            .find(|c| c.to_node == node_id && c.to_pin == pin)?;

        if let Some(val) = pin_values.get(&(conn.from_node, conn.from_pin.clone())) {
            return Some(val.clone());
        }

        // If the source node is a data producer that hasn't evaluated yet, evaluate it
        let src_node = self.nodes.get(&conn.from_node)?;
        match &src_node.kind {
            VisualGraphNodeKind::GetProperty {
                node_id: prop_node,
                property,
            } => {
                let val = scene
                    .nodes
                    .get(prop_node)
                    .and_then(|n| n.properties.get(property))
                    .cloned()
                    .unwrap_or(IrValue::Nil);
                pin_values.insert((conn.from_node, conn.from_pin.clone()), val.clone());
                Some(val)
            }
            VisualGraphNodeKind::MathOp { op } => {
                let a = self
                    .resolve_input(conn.from_node, "a", pin_values, scene)
                    .or_else(|| src_node.properties.get("a").cloned());
                let b = self
                    .resolve_input(conn.from_node, "b", pin_values, scene)
                    .or_else(|| src_node.properties.get("b").cloned());
                let c = self
                    .resolve_input(conn.from_node, "c", pin_values, scene)
                    .or_else(|| src_node.properties.get("c").cloned());

                let res = evaluate_math_op(*op, a.as_ref(), b.as_ref(), c.as_ref())?;
                pin_values.insert((conn.from_node, conn.from_pin.clone()), res.clone());
                Some(res)
            }
            _ => None,
        }
    }
}

fn is_truthy(val: &IrValue) -> bool {
    match val {
        IrValue::Bool(b) => *b,
        IrValue::F64(f) => *f != 0.0 && !f.is_nan(),
        IrValue::String(s) => !s.is_empty(),
        IrValue::Array(a) => !a.is_empty(),
        IrValue::Map(m) => !m.is_empty(),
        IrValue::Nil => false,
        _ => true,
    }
}

fn evaluate_math_op(
    op: MathOperator,
    a: Option<&IrValue>,
    b: Option<&IrValue>,
    c: Option<&IrValue>,
) -> Option<IrValue> {
    match (a, b) {
        (Some(IrValue::F64(va)), Some(IrValue::F64(vb))) => {
            let res = match op {
                MathOperator::Add => va + vb,
                MathOperator::Subtract => va - vb,
                MathOperator::Multiply => va * vb,
                MathOperator::Divide => {
                    if vb.abs() > 1e-9 {
                        va / vb
                    } else {
                        0.0
                    }
                }
                MathOperator::Min => va.min(*vb),
                MathOperator::Max => va.max(*vb),
                MathOperator::Clamp => {
                    let vc = if let Some(IrValue::F64(v)) = c {
                        *v
                    } else {
                        1.0
                    };
                    va.clamp(*vb, vc)
                }
            };
            Some(IrValue::F64(res))
        }
        (Some(IrValue::Vec3(va)), Some(IrValue::Vec3(vb))) => {
            let res = match op {
                MathOperator::Add => [va[0] + vb[0], va[1] + vb[1], va[2] + vb[2]],
                MathOperator::Subtract => [va[0] - vb[0], va[1] - vb[1], va[2] - vb[2]],
                MathOperator::Multiply => [va[0] * vb[0], va[1] * vb[1], va[2] * vb[2]],
                MathOperator::Divide => [
                    if vb[0].abs() > 1e-9 { va[0] / vb[0] } else { 0.0 },
                    if vb[1].abs() > 1e-9 { va[1] / vb[1] } else { 0.0 },
                    if vb[2].abs() > 1e-9 { va[2] / vb[2] } else { 0.0 },
                ],
                MathOperator::Min => [va[0].min(vb[0]), va[1].min(vb[1]), va[2].min(vb[2])],
                MathOperator::Max => [va[0].max(vb[0]), va[1].max(vb[1]), va[2].max(vb[2])],
                MathOperator::Clamp => {
                    let vc = if let Some(IrValue::Vec3(v)) = c {
                        *v
                    } else {
                        [1.0, 1.0, 1.0]
                    };
                    [
                        va[0].clamp(vb[0], vc[0]),
                        va[1].clamp(vb[1], vc[1]),
                        va[2].clamp(vb[2], vc[2]),
                    ]
                }
            };
            Some(IrValue::Vec3(res))
        }
        (Some(IrValue::Vec3(va)), Some(IrValue::F64(vb))) => {
            let s = *vb as f32;
            let res = match op {
                MathOperator::Add => [va[0] + s, va[1] + s, va[2] + s],
                MathOperator::Subtract => [va[0] - s, va[1] - s, va[2] - s],
                MathOperator::Multiply => [va[0] * s, va[1] * s, va[2] * s],
                MathOperator::Divide => {
                    if s.abs() > 1e-6 {
                        [va[0] / s, va[1] / s, va[2] / s]
                    } else {
                        *va
                    }
                }
                _ => *va,
            };
            Some(IrValue::Vec3(res))
        }
        (Some(val), None) => Some(val.clone()),
        _ => None,
    }
}
