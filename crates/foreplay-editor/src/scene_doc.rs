//! Multi-Scene Document Management (`foreplay-editor`).
//!
//! Mirrors Godot 4's multi-scene document tabs above the viewport:
//! `[ ForeplayParkDemo.fscn × ] [ UI_HUD.fscn × ] [ + ]`
//! allowing multiple scenes to remain open concurrently with independent undo stacks.

use foreplay_harness::HarnessSession;
use foreplay_ir::{IrScene, NodeId};

/// An open scene document within the editor shell.
#[derive(Debug)]
pub struct OpenSceneDoc {
    pub id: usize,
    pub title: String,
    pub file_path: Option<String>,
    pub session: HarnessSession,
    pub selected_node: Option<NodeId>,
    pub is_dirty: bool,
}

impl OpenSceneDoc {
    /// Creates a new open scene document from an initial `IrScene`.
    pub fn new(id: usize, scene: IrScene, file_path: Option<String>) -> Self {
        let title = scene.name.clone();
        Self {
            id,
            title,
            file_path,
            session: HarnessSession::new(scene),
            selected_node: None,
            is_dirty: false,
        }
    }

    /// Creates a starter skate park demo scene document.
    pub fn starter_park(id: usize) -> Self {
        let mut doc = Self::new(id, IrScene::new("ForeplayParkDemo"), Some("res://scenes/park_demo.fscn".into()));
        crate::demo_scene::populate_starter_scene(&mut doc.session).ok();
        doc.selected_node = NodeId::new(1, 1);
        doc
    }

    /// Creates a secondary UI/HUD canvas scene document.
    pub fn starter_hud(id: usize) -> Self {
        let mut scene = IrScene::new("UI_HUD");
        let root_id = NodeId::new(1, 1).expect("valid generation");
        let mut props = std::collections::BTreeMap::new();
        props.insert("position".into(), foreplay_ir::IrValue::Vec2([0.0, 0.0]));
        props.insert("scale".into(), foreplay_ir::IrValue::Vec2([1280.0, 720.0]));

        let root_node = foreplay_ir::IrNode {
            id: root_id,
            name: "HUDCanvasRoot".into(),
            class_name: "Control".into(),
            parent: None,
            instance_scene: None,
            properties: props,
            groups: vec!["ui".into()],
        };
        scene.nodes.insert(root_id, root_node);

        let mut doc = Self::new(id, scene, Some("res://scenes/hud.fscn".into()));
        doc.selected_node = Some(root_id);
        doc
    }
}
