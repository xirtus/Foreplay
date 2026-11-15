//! Foreplay Atlas (`foreplay-atlas`) — Semantic layer modeled on `@3jse/atlas`.
//! Compiles `IrScene` + declared contracts into human- and agent-legible meaning graphs.

use foreplay_ir::{IrScene, NodeId};

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
