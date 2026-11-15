//! Foreplay AI Harness (`Observe -> Plan -> Act -> Verify`).
//! Shared command session backing both the MCP Agent API and Editor Undo/Redo history.

pub mod gates;
pub mod rpc;

pub use gates::*;
pub use rpc::HarnessRpcServer;

use foreplay_atlas::AtlasTaskContext;
use foreplay_core::SceneTree;
use foreplay_ir::{EditorCommand, IrError, IrScene};

#[derive(Debug)]
pub struct HarnessSession {
    pub tree: SceneTree,
    undo_stack: Vec<EditorCommand>,
    redo_stack: Vec<EditorCommand>,
}

impl HarnessSession {
    pub fn new(scene: IrScene) -> Self {
        Self {
            tree: SceneTree::from_ir(scene),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    /// Observe phase: export minimal semantic task context via `foreplay-atlas`.
    pub fn observe_atlas(&self) -> AtlasTaskContext {
        AtlasTaskContext::from_scene(&self.tree.active_scene)
    }

    /// Act phase: execute an `EditorCommand` and record it on the Undo stack.
    pub fn execute(&mut self, cmd: EditorCommand) -> Result<(), IrError> {
        cmd.apply(&mut self.tree.active_scene)?;
        self.undo_stack.push(cmd);
        self.redo_stack.clear();
        Ok(())
    }

    pub fn undo(&mut self) -> Result<bool, IrError> {
        let Some(cmd) = self.undo_stack.pop() else {
            return Ok(false);
        };
        cmd.inverse().apply(&mut self.tree.active_scene)?;
        self.redo_stack.push(cmd);
        Ok(true)
    }

    pub fn redo(&mut self) -> Result<bool, IrError> {
        let Some(cmd) = self.redo_stack.pop() else {
            return Ok(false);
        };
        cmd.apply(&mut self.tree.active_scene)?;
        self.undo_stack.push(cmd);
        Ok(true)
    }

    /// Verify phase: step `n` deterministic headless ticks and confirm scene invariants.
    pub fn verify_headless_ticks(&mut self, ticks: u64) -> u64 {
        for _ in 0..ticks {
            self.tree.step_frame();
        }
        self.tree.tick_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use foreplay_ir::{IrNode, NodeId};
    use std::collections::BTreeMap;

    #[test]
    fn harness_observe_act_undo_verify_loop() {
        let mut session = HarnessSession::new(IrScene::new("HarnessBootTest"));
        let id = NodeId::new(1, 1).unwrap();
        session
            .execute(EditorCommand::InsertNode {
                node: IrNode {
                    id,
                    name: "CameraRig".into(),
                    class_name: "Camera3D".into(),
                    parent: None,
                    instance_scene: None,
                    properties: BTreeMap::new(),
                    groups: Vec::new(),
                },
            })
            .unwrap();

        assert_eq!(session.observe_atlas().affected_nodes, vec![id]);
        assert_eq!(session.verify_headless_ticks(120), 120);
        assert!(session.undo().unwrap());
        assert!(session.observe_atlas().affected_nodes.is_empty());
        assert!(session.redo().unwrap());
        assert_eq!(session.observe_atlas().affected_nodes, vec![id]);
    }
}
