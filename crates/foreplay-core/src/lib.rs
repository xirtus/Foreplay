//! Godot-parity `SceneTree` and `Node` composition built cleanly *around* `bevy_ecs` 0.20
//! (**3JSE Principle #1:** *"Bevy is a dependency, never a fork"*).
//!
//! Mapping Godot 4 concepts directly onto Bevy 0.20 primitives:
//! - Godot `Node` base class -> `ForeplayNodeMeta` + Bevy **Required Components** (`#[require(...)]`)
//! - Godot `add_child` / parent-child tree -> Bevy **ECS Relations** (`ChildOf` / `Children`)
//! - Godot `Signal` (`emit_signal` / `connect`) -> Bevy **Entity Observers & Triggers**
//! - Godot `ClassDB` / `@export` -> `bevy_reflect::Reflect` + `foreplay_ir::FeelSpecKnob`

use bevy_ecs::hierarchy::ChildOf;
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use foreplay_ir::{IrScene, NodeId, PortArchiveMount};
use std::collections::BTreeMap;

/// Godot-parity deterministic frame lifecycle stages mapped onto Bevy `Schedule` labels.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum LifecyclePhase {
    EnterTree,
    Ready,
    PhysicsProcess,
    Process,
    ExitTree,
}

/// Core Godot-style Node identity component attached to spawned entities in `bevy_ecs::World`.
#[derive(Component, Reflect, Clone, PartialEq, Eq, Debug)]
#[reflect(Component)]
pub struct ForeplayNodeMeta {
    pub ir_id: u64,
    pub name: String,
    pub godot_class: String,
}

/// Capability-clean Virtual File System supporting `res://`, `user://`, and `port://` BYO-ROM mounts.
#[derive(Debug, Default)]
pub struct VirtualFileSystem {
    port_mounts: BTreeMap<String, Box<dyn PortArchiveMount>>,
}

impl VirtualFileSystem {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn mount_port_archive(&mut self, name: impl Into<String>, mount: Box<dyn PortArchiveMount>) {
        self.port_mounts.insert(name.into(), mount);
    }

    pub fn mounted_archives(&self) -> usize {
        self.port_mounts.len()
    }
}

/// Runtime `SceneTree` bridging canonical `IrScene` documents into a live `bevy_ecs::World`.
pub struct SceneTree {
    pub active_scene: IrScene,
    pub world: World,
    pub ir_to_entity: BTreeMap<NodeId, Entity>,
    pub root: Option<NodeId>,
    pub tick_count: u64,
}

impl std::fmt::Debug for SceneTree {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SceneTree")
            .field("scene_name", &self.active_scene.name)
            .field("entity_count", &self.ir_to_entity.len())
            .field("root", &self.root)
            .field("tick_count", &self.tick_count)
            .finish()
    }
}

impl Default for SceneTree {
    fn default() -> Self {
        Self::from_ir(IrScene::default())
    }
}

impl SceneTree {
    /// Materializes a canonical `IrScene` into a live `bevy_ecs` 0.20 `World` using native `ChildOf` relations.
    pub fn from_ir(scene: IrScene) -> Self {
        let mut world = World::new();
        let mut ir_to_entity = BTreeMap::new();
        let root = scene.nodes.values().find(|n| n.parent.is_none()).map(|n| n.id);

        // Pass 1: spawn entities with `ForeplayNodeMeta`
        for (&node_id, ir_node) in &scene.nodes {
            let packed_id = (u64::from(node_id.generation.get()) << 32) | u64::from(node_id.index);
            let entity = world
                .spawn(ForeplayNodeMeta {
                    ir_id: packed_id,
                    name: ir_node.name.clone(),
                    godot_class: ir_node.class_name.clone(),
                })
                .id();
            ir_to_entity.insert(node_id, entity);
        }

        // Pass 2: wire Bevy 0.20 `ChildOf` hierarchy relations
        for (&node_id, ir_node) in &scene.nodes {
            if let Some(parent_id) = ir_node.parent
                && let (Some(&child_ent), Some(&parent_ent)) =
                    (ir_to_entity.get(&node_id), ir_to_entity.get(&parent_id))
            {
                world.entity_mut(child_ent).insert(ChildOf(parent_ent));
            }
        }

        Self {
            active_scene: scene,
            world,
            ir_to_entity,
            root,
            tick_count: 0,
        }
    }

    /// Re-synchronizes the `bevy_ecs::World` from `active_scene` (used for instant Play-In-Editor snapshot restore).
    pub fn rebuild_world_from_ir(&mut self) {
        let rebuilt = Self::from_ir(self.active_scene.clone());
        self.world = rebuilt.world;
        self.ir_to_entity = rebuilt.ir_to_entity;
        self.root = rebuilt.root;
    }

    pub fn step_frame(&mut self) {
        self.world.clear_trackers();
        self.tick_count += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_ecs::hierarchy::Children;
    use foreplay_ir::IrNode;

    #[test]
    fn materializes_ir_hierarchy_into_bevy_ecs_relations() {
        let mut ir = IrScene::new("SkateParkChunk");
        let root_id = NodeId::new(0, 1).unwrap();
        let child_id = NodeId::new(1, 1).unwrap();

        ir.nodes.insert(
            root_id,
            IrNode {
                id: root_id,
                name: "ParkRoot".into(),
                class_name: "Node3D".into(),
                parent: None,
                instance_scene: None,
                properties: BTreeMap::new(),
                groups: Vec::new(),
            },
        );
        ir.nodes.insert(
            child_id,
            IrNode {
                id: child_id,
                name: "HalfpipeCollider".into(),
                class_name: "StaticBody3D".into(),
                parent: Some(root_id),
                instance_scene: None,
                properties: BTreeMap::new(),
                groups: Vec::new(),
            },
        );

        let tree = SceneTree::from_ir(ir);
        let root_ent = tree.ir_to_entity[&root_id];
        let child_ent = tree.ir_to_entity[&child_id];

        let child_of = tree.world.get::<ChildOf>(child_ent).expect("ChildOf relation");
        assert_eq!(child_of.parent(), root_ent);

        let children = tree.world.get::<Children>(root_ent).expect("Children relation");
        assert!(children.contains(&child_ent));
    }
}
