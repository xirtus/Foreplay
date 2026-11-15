//! Godot-parity `SceneTree` and `Node` composition built cleanly *around* `bevy_ecs` 0.20
//! (**3JSE Principle #1:** *"Bevy is a dependency, never a fork"*).
//!
//! Mapping Godot 4 concepts directly onto Bevy 0.20 primitives:
//! - Godot `Node` base class -> `ForeplayNodeMeta` + Bevy **Required Components** (`#[require(...)]`)
//! - Godot `add_child` / parent-child tree -> Bevy **ECS Relations** (`ChildOf` / `Children`)
//! - Godot `Signal` (`emit_signal` / `connect`) -> Bevy **Entity Observers & Triggers**
//! - Godot `ClassDB` / `@export` -> `bevy_reflect::Reflect` + `foreplay_ir::FeelSpecKnob`

pub mod animation;
pub mod ik_motion;
pub mod navigation;
pub mod pcg;
pub mod rollback;
pub mod ui_layout;

pub use animation::*;
pub use ik_motion::*;
pub use navigation::*;
pub use pcg::*;
pub use rollback::*;
pub use ui_layout::*;

use bevy_ecs::hierarchy::ChildOf;
use bevy_ecs::prelude::*;
use bevy_math::{Quat, Vec3};
use bevy_reflect::Reflect;
use foreplay_ir::{IrScene, IrValue, NodeId, PortArchiveMount};
use std::collections::BTreeMap;
use std::sync::Arc;

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

/// Godot-style global/world 3D transform component.
#[derive(Component, Reflect, Clone, Copy, Debug, PartialEq)]
#[reflect(Component)]
pub struct ForeplayGlobalTransform3D {
    pub translation: Vec3,
    pub rotation_quat: Quat,
    pub scale: Vec3,
}

impl Default for ForeplayGlobalTransform3D {
    fn default() -> Self {
        Self {
            translation: Vec3::ZERO,
            rotation_quat: Quat::IDENTITY,
            scale: Vec3::ONE,
        }
    }
}

/// Godot-style local 3D transform component.
#[derive(Component, Reflect, Clone, Copy, Debug, PartialEq)]
#[reflect(Component)]
#[require(ForeplayGlobalTransform3D)]
pub struct ForeplayTransform3D {
    pub translation: Vec3,
    pub rotation_quat: Quat,
    pub scale: Vec3,
}

impl Default for ForeplayTransform3D {
    fn default() -> Self {
        Self {
            translation: Vec3::ZERO,
            rotation_quat: Quat::IDENTITY,
            scale: Vec3::ONE,
        }
    }
}

impl ForeplayTransform3D {
    pub const fn from_translation(translation: Vec3) -> Self {
        Self {
            translation,
            rotation_quat: Quat::IDENTITY,
            scale: Vec3::ONE,
        }
    }

    pub const fn from_xyz(x: f32, y: f32, z: f32) -> Self {
        Self {
            translation: Vec3::new(x, y, z),
            rotation_quat: Quat::IDENTITY,
            scale: Vec3::ONE,
        }
    }
}

/// Godot-style visibility component.
#[derive(Component, Reflect, Clone, Copy, Debug, PartialEq, Eq)]
#[reflect(Component)]
pub struct ForeplayVisibility {
    pub visible: bool,
}

impl Default for ForeplayVisibility {
    fn default() -> Self {
        Self { visible: true }
    }
}

/// Godot-style 3D Node marker automatically requiring spatial and visibility components.
#[derive(Component, Reflect, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[reflect(Component)]
#[require(ForeplayTransform3D, ForeplayVisibility)]
pub struct ForeplayNode3D;

/// Errors arising from PackedScene registration and instantiation.
#[derive(Clone, PartialEq, Eq, Debug, thiserror::Error)]
pub enum PackedSceneError {
    #[error("scene prefab not found in registry: {0}")]
    PrefabNotFound(String),
    #[error("prefab '{0}' has no root node")]
    PrefabHasNoRoot(String),
}

/// Registry of named `res://...` `IrScene` prefabs (PackedScenes).
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct PackedSceneRegistry {
    scenes: BTreeMap<String, IrScene>,
}

impl PackedSceneRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, path: impl Into<String>, scene: IrScene) {
        self.scenes.insert(path.into(), scene);
    }

    pub fn get(&self, path: &str) -> Option<&IrScene> {
        self.scenes.get(path)
    }

    pub fn remove(&mut self, path: &str) -> Option<IrScene> {
        self.scenes.remove(path)
    }

    pub fn contains(&self, path: &str) -> bool {
        self.scenes.contains_key(path)
    }

    pub fn len(&self) -> usize {
        self.scenes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.scenes.is_empty()
    }
}

/// Godot-parity signal event triggered on entities.
#[derive(EntityEvent, Clone, PartialEq, Debug)]
pub struct SignalDispatched {
    pub entity: Entity,
    pub from_node: NodeId,
    pub signal_name: String,
    pub to_node: NodeId,
    pub method_name: String,
    pub payload: IrValue,
}

/// Play-In-Editor snapshot for zero-rebuild Play/Stop restoration.
#[derive(Clone, Debug, PartialEq)]
pub struct SceneSnapshot {
    pub scene: IrScene,
    pub tick_count: u64,
    pub registry: PackedSceneRegistry,
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

/// Determines whether a class name represents a 3D spatial node.
fn is_node_3d(class_name: &str) -> bool {
    class_name == "Node3D" || class_name.ends_with("3D")
}

/// Applies recognized spatial properties from an `IrNode` property map onto spawned entity components.
fn apply_spatial_properties(world: &mut World, entity: Entity, props: &BTreeMap<String, IrValue>) {
    let mut transform = world.get::<ForeplayTransform3D>(entity).copied().unwrap_or_default();
    let mut vis = world.get::<ForeplayVisibility>(entity).copied().unwrap_or_default();
    let mut modified_transform = false;
    let mut modified_vis = false;

    for (k, v) in props {
        match k.as_str() {
            "translation" | "position" => {
                if let IrValue::Vec3([x, y, z]) = v {
                    transform.translation = Vec3::new(*x, *y, *z);
                    modified_transform = true;
                }
            }
            "rotation_quat" | "rotation" => {
                if let IrValue::Quat([x, y, z, w]) = v {
                    transform.rotation_quat = Quat::from_xyzw(*x, *y, *z, *w);
                    modified_transform = true;
                }
            }
            "scale" => {
                if let IrValue::Vec3([x, y, z]) = v {
                    transform.scale = Vec3::new(*x, *y, *z);
                    modified_transform = true;
                }
            }
            "visible" => {
                if let IrValue::Bool(b) = v {
                    vis.visible = *b;
                    modified_vis = true;
                }
            }
            _ => {}
        }
    }

    if modified_transform
        && let Some(mut t) = world.get_mut::<ForeplayTransform3D>(entity)
    {
        *t = transform;
    }
    if modified_vis
        && let Some(mut v) = world.get_mut::<ForeplayVisibility>(entity)
    {
        *v = vis;
    }
}

/// Runtime `SceneTree` bridging canonical `IrScene` documents into a live `bevy_ecs::World`.
pub struct SceneTree {
    pub active_scene: IrScene,
    pub world: World,
    pub ir_to_entity: BTreeMap<NodeId, Entity>,
    pub root: Option<NodeId>,
    pub tick_count: u64,
    pub signal_history: Vec<SignalDispatched>,
    #[allow(clippy::type_complexity)]
    signal_callbacks: BTreeMap<(NodeId, String), Vec<Arc<dyn Fn(&SignalDispatched) + Send + Sync>>>,
}

impl std::fmt::Debug for SceneTree {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SceneTree")
            .field("scene_name", &self.active_scene.name)
            .field("entity_count", &self.ir_to_entity.len())
            .field("root", &self.root)
            .field("tick_count", &self.tick_count)
            .field("dispatched_signals_count", &self.signal_history.len())
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
        Self::from_ir_with_registry(scene, &PackedSceneRegistry::default())
    }

    /// Materializes a canonical `IrScene` with access to a `PackedSceneRegistry` for instancing.
    pub fn from_ir_with_registry(scene: IrScene, registry: &PackedSceneRegistry) -> Self {
        let mut world = World::new();
        world.insert_resource(registry.clone());
        let mut ir_to_entity = BTreeMap::new();
        let root = scene.nodes.values().find(|n| n.parent.is_none()).map(|n| n.id);

        // Pass 1: spawn entities with `ForeplayNodeMeta` and spatial components
        for (&node_id, ir_node) in &scene.nodes {
            let packed_id = (u64::from(node_id.generation.get()) << 32) | u64::from(node_id.index);

            if let Some(res_path) = &ir_node.instance_scene
                && let Some(prefab) = registry.get(res_path)
            {
                let prefab_root = prefab.nodes.values().find(|n| n.parent.is_none());

                    // Start with prefab root's properties and layer local overrides
                    let mut effective_props = prefab_root
                        .map(|r| r.properties.clone())
                        .unwrap_or_default();
                    effective_props.extend(ir_node.properties.clone());

                    let class_name = if ir_node.class_name.is_empty() {
                        prefab_root
                            .map(|r| r.class_name.clone())
                            .unwrap_or_else(|| "Node".to_string())
                    } else {
                        ir_node.class_name.clone()
                    };

                    let mut entity_cmds = world.spawn(ForeplayNodeMeta {
                        ir_id: packed_id,
                        name: ir_node.name.clone(),
                        godot_class: class_name.clone(),
                    });

                    if is_node_3d(&class_name) {
                        entity_cmds.insert(ForeplayNode3D);
                    }
                    let host_entity = entity_cmds.id();
                    apply_spatial_properties(&mut world, host_entity, &effective_props);
                    ir_to_entity.insert(node_id, host_entity);

                    // Instantiate prefab internal descendants
                    if let Some(root_p) = prefab_root {
                        let mut prefab_to_entity = BTreeMap::new();
                        prefab_to_entity.insert(root_p.id, host_entity);

                        for pn in prefab.nodes.values() {
                            if pn.id != root_p.id {
                                let p_packed_id =
                                    (u64::from(pn.id.generation.get()) << 32) | u64::from(pn.id.index);
                                let mut pe_cmds = world.spawn(ForeplayNodeMeta {
                                    ir_id: p_packed_id,
                                    name: pn.name.clone(),
                                    godot_class: pn.class_name.clone(),
                                });
                                if is_node_3d(&pn.class_name) {
                                    pe_cmds.insert(ForeplayNode3D);
                                }
                                let pe_id = pe_cmds.id();
                                apply_spatial_properties(&mut world, pe_id, &pn.properties);
                                prefab_to_entity.insert(pn.id, pe_id);
                            }
                        }

                        // Wire prefab descendant parent-child relations
                        for pn in prefab.nodes.values() {
                            if pn.id != root_p.id
                                && let Some(parent_pid) = pn.parent
                                && let (Some(&pchild_ent), Some(&pparent_ent)) =
                                    (prefab_to_entity.get(&pn.id), prefab_to_entity.get(&parent_pid))
                            {
                                world.entity_mut(pchild_ent).insert(ChildOf(pparent_ent));
                            }
                        }
                    }
                    continue;
                }

            // Standard (non-instanced) node spawning
            let mut entity_cmds = world.spawn(ForeplayNodeMeta {
                ir_id: packed_id,
                name: ir_node.name.clone(),
                godot_class: ir_node.class_name.clone(),
            });

            if is_node_3d(&ir_node.class_name) {
                entity_cmds.insert(ForeplayNode3D);
            }
            let entity = entity_cmds.id();
            apply_spatial_properties(&mut world, entity, &ir_node.properties);
            ir_to_entity.insert(node_id, entity);
        }

        // Pass 2: wire Bevy 0.20 `ChildOf` hierarchy relations for host scene
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
            signal_history: Vec::new(),
            signal_callbacks: BTreeMap::new(),
        }
    }

    /// Re-synchronizes the `bevy_ecs::World` from `active_scene`.
    pub fn rebuild_world_from_ir(&mut self) {
        let registry = self
            .world
            .get_resource::<PackedSceneRegistry>()
            .cloned()
            .unwrap_or_default();
        let rebuilt = Self::from_ir_with_registry(self.active_scene.clone(), &registry);
        self.world = rebuilt.world;
        self.ir_to_entity = rebuilt.ir_to_entity;
        self.root = rebuilt.root;
        self.signal_history.clear();
    }

    /// Advances the engine lifecycle by one frame.
    pub fn step_frame(&mut self) {
        self.world.clear_trackers();
        self.tick_count += 1;
    }

    /// Emits a signal routing along `active_scene.connections` and triggering Bevy observers/callbacks.
    pub fn emit_signal(&mut self, from: NodeId, signal_name: &str, payload: IrValue) {
        let matching_connections: Vec<_> = self
            .active_scene
            .connections
            .iter()
            .filter(|c| c.from_node == from && c.signal_name == signal_name)
            .cloned()
            .collect();

        for conn in matching_connections {
            let target_entity = self
                .ir_to_entity
                .get(&conn.to_node)
                .copied()
                .unwrap_or(Entity::PLACEHOLDER);

            let event = SignalDispatched {
                entity: target_entity,
                from_node: from,
                signal_name: signal_name.to_string(),
                to_node: conn.to_node,
                method_name: conn.method_name.clone(),
                payload: payload.clone(),
            };

            self.signal_history.push(event.clone());

            if let Some(callbacks) = self
                .signal_callbacks
                .get(&(conn.to_node, conn.method_name.clone()))
            {
                for cb in callbacks {
                    cb(&event);
                }
            }

            if target_entity != Entity::PLACEHOLDER {
                self.world.trigger(event);
            }
        }
    }

    /// Connects a direct Rust observer callback to a node's method invocation.
    pub fn connect_signal_callback(
        &mut self,
        to_node: NodeId,
        method_name: impl Into<String>,
        callback: impl Fn(&SignalDispatched) + Send + Sync + 'static,
    ) {
        self.signal_callbacks
            .entry((to_node, method_name.into()))
            .or_default()
            .push(Arc::new(callback));
    }

    /// Returns a slice of all signals dispatched through `emit_signal`.
    pub fn dispatched_signals(&self) -> &[SignalDispatched] {
        &self.signal_history
    }

    /// Clears recorded dispatched signal history.
    pub fn clear_dispatched_signals(&mut self) {
        self.signal_history.clear();
    }

    /// Captures a Play-In-Editor snapshot.
    pub fn capture_snapshot(&self) -> SceneSnapshot {
        let registry = self
            .world
            .get_resource::<PackedSceneRegistry>()
            .cloned()
            .unwrap_or_default();
        SceneSnapshot {
            scene: self.active_scene.clone(),
            tick_count: self.tick_count,
            registry,
        }
    }

    /// Restores a Play-In-Editor snapshot for zero-rebuild Play/Stop restoration.
    pub fn restore_snapshot(&mut self, snapshot: SceneSnapshot) {
        self.active_scene = snapshot.scene;
        self.tick_count = snapshot.tick_count;
        let rebuilt = Self::from_ir_with_registry(self.active_scene.clone(), &snapshot.registry);
        self.world = rebuilt.world;
        self.ir_to_entity = rebuilt.ir_to_entity;
        self.root = rebuilt.root;
        self.signal_history.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_ecs::hierarchy::Children;
    use bevy_ecs::observer::On;
    use foreplay_ir::{IrNode, SignalConnection};
    use std::sync::atomic::{AtomicUsize, Ordering};

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

    #[test]
    fn required_components_auto_inserted_for_node3d() {
        let mut world = World::new();
        let ent = world.spawn(ForeplayNode3D).id();

        // Required components must be automatically attached by Bevy 0.20
        assert!(world.get::<ForeplayTransform3D>(ent).is_some());
        assert!(world.get::<ForeplayGlobalTransform3D>(ent).is_some());
        assert!(world.get::<ForeplayVisibility>(ent).is_some());
        assert!(world.get::<ForeplayVisibility>(ent).unwrap().visible);
    }

    #[test]
    fn packed_scene_instantiation_and_property_override_layering() {
        let mut registry = PackedSceneRegistry::new();

        // Create prefab "res://prefabs/hero.fscn"
        let mut hero_prefab = IrScene::new("HeroPrefab");
        let prefab_root_id = NodeId::new(100, 1).unwrap();
        let prefab_mesh_id = NodeId::new(101, 1).unwrap();

        let mut prefab_props = BTreeMap::new();
        prefab_props.insert("speed".into(), IrValue::F64(10.0));
        prefab_props.insert("health".into(), IrValue::F64(100.0));
        prefab_props.insert("translation".into(), IrValue::Vec3([0.0, 0.0, 0.0]));

        hero_prefab.nodes.insert(
            prefab_root_id,
            IrNode {
                id: prefab_root_id,
                name: "HeroPrefabRoot".into(),
                class_name: "CharacterBody3D".into(),
                parent: None,
                instance_scene: None,
                properties: prefab_props,
                groups: vec!["heroes".into()],
            },
        );

        let mut mesh_props = BTreeMap::new();
        mesh_props.insert("mesh_path".into(), IrValue::String("res://skater.glb".into()));
        hero_prefab.nodes.insert(
            prefab_mesh_id,
            IrNode {
                id: prefab_mesh_id,
                name: "HeroMesh".into(),
                class_name: "MeshInstance3D".into(),
                parent: Some(prefab_root_id),
                instance_scene: None,
                properties: mesh_props,
                groups: vec![],
            },
        );

        registry.register("res://prefabs/hero.fscn", hero_prefab);

        // Host scene instantiates "res://prefabs/hero.fscn" with property override speed=25.0
        let mut host_scene = IrScene::new("LevelOne");
        let world_root_id = NodeId::new(1, 1).unwrap();
        let hero_instance_id = NodeId::new(2, 1).unwrap();

        host_scene.nodes.insert(
            world_root_id,
            IrNode {
                id: world_root_id,
                name: "WorldRoot".into(),
                class_name: "Node3D".into(),
                parent: None,
                instance_scene: None,
                properties: BTreeMap::new(),
                groups: vec![],
            },
        );

        let mut override_props = BTreeMap::new();
        override_props.insert("speed".into(), IrValue::F64(25.0)); // Overridden!
        override_props.insert("translation".into(), IrValue::Vec3([5.0, 1.0, -3.0]));

        host_scene.nodes.insert(
            hero_instance_id,
            IrNode {
                id: hero_instance_id,
                name: "PlayerHero".into(),
                class_name: "CharacterBody3D".into(),
                parent: Some(world_root_id),
                instance_scene: Some("res://prefabs/hero.fscn".into()),
                properties: override_props,
                groups: vec!["player".into()],
            },
        );

        let tree = SceneTree::from_ir_with_registry(host_scene, &registry);
        let hero_ent = tree.ir_to_entity[&hero_instance_id];
        let root_ent = tree.ir_to_entity[&world_root_id];

        // Verify hierarchy: hero is child of root
        let child_of = tree.world.get::<ChildOf>(hero_ent).expect("ChildOf root");
        assert_eq!(child_of.parent(), root_ent);

        // Verify hero has spatial transform applied from overrides
        let transform = tree
            .world
            .get::<ForeplayTransform3D>(hero_ent)
            .expect("Transform on hero");
        assert_eq!(transform.translation, Vec3::new(5.0, 1.0, -3.0));

        // Verify hero has children from prefab (HeroMesh)
        let children = tree.world.get::<Children>(hero_ent).expect("Hero children");
        assert_eq!(children.len(), 1);
        let mesh_ent = children[0];
        let mesh_meta = tree
            .world
            .get::<ForeplayNodeMeta>(mesh_ent)
            .expect("Mesh meta");
        assert_eq!(mesh_meta.name, "HeroMesh");
        assert_eq!(mesh_meta.godot_class, "MeshInstance3D");
    }

    #[test]
    fn signal_dispatched_via_connections_and_bevy_observers() {
        let mut scene = IrScene::new("SignalTestScene");
        let emitter_id = NodeId::new(1, 1).unwrap();
        let receiver_id = NodeId::new(2, 1).unwrap();

        scene.nodes.insert(
            emitter_id,
            IrNode {
                id: emitter_id,
                name: "ButtonEmitter".into(),
                class_name: "Node".into(),
                parent: None,
                instance_scene: None,
                properties: BTreeMap::new(),
                groups: vec![],
            },
        );
        scene.nodes.insert(
            receiver_id,
            IrNode {
                id: receiver_id,
                name: "GateReceiver".into(),
                class_name: "Node".into(),
                parent: None,
                instance_scene: None,
                properties: BTreeMap::new(),
                groups: vec![],
            },
        );
        scene.connections.push(SignalConnection {
            from_node: emitter_id,
            signal_name: "pressed".into(),
            to_node: receiver_id,
            method_name: "_on_button_pressed".into(),
        });

        let mut tree = SceneTree::from_ir(scene);

        // Track Bevy observer trigger invocation
        static OBSERVER_CALLED: AtomicUsize = AtomicUsize::new(0);
        tree.world.add_observer(|_event: On<SignalDispatched>| {
            OBSERVER_CALLED.fetch_add(1, Ordering::SeqCst);
        });

        // Track direct callback invocation
        static CALLBACK_CALLED: AtomicUsize = AtomicUsize::new(0);
        tree.connect_signal_callback(receiver_id, "_on_button_pressed", |_| {
            CALLBACK_CALLED.fetch_add(1, Ordering::SeqCst);
        });

        // Emit signal
        tree.emit_signal(emitter_id, "pressed", IrValue::I64(42));

        assert_eq!(tree.dispatched_signals().len(), 1);
        assert_eq!(tree.dispatched_signals()[0].signal_name, "pressed");
        assert_eq!(tree.dispatched_signals()[0].payload, IrValue::I64(42));
        assert_eq!(CALLBACK_CALLED.load(Ordering::SeqCst), 1);
        assert_eq!(OBSERVER_CALLED.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn play_in_editor_scene_snapshot_restore() {
        let mut scene = IrScene::new("PietSnapshotTest");
        let node_id = NodeId::new(1, 1).unwrap();
        scene.nodes.insert(
            node_id,
            IrNode {
                id: node_id,
                name: "Player".into(),
                class_name: "CharacterBody3D".into(),
                parent: None,
                instance_scene: None,
                properties: BTreeMap::new(),
                groups: vec![],
            },
        );

        let mut tree = SceneTree::from_ir(scene);
        let snapshot = tree.capture_snapshot();

        // Simulate play mode: step 60 frames and mutate tree
        for _ in 0..60 {
            tree.step_frame();
        }
        assert_eq!(tree.tick_count, 60);

        // Restore snapshot (Stop play mode)
        tree.restore_snapshot(snapshot);
        assert_eq!(tree.tick_count, 0);
        assert_eq!(tree.active_scene.name, "PietSnapshotTest");
        assert!(tree.ir_to_entity.contains_key(&node_id));
    }

    #[test]
    fn animation_player_interpolation_looping_and_evaluation() {
        let mut scene = IrScene::new("AnimScene");
        let skater_id = NodeId::new(1, 1).unwrap();
        scene.nodes.insert(
            skater_id,
            IrNode {
                id: skater_id,
                name: "Skater".into(),
                class_name: "CharacterBody3D".into(),
                parent: None,
                instance_scene: None,
                properties: BTreeMap::new(),
                groups: vec![],
            },
        );

        let mut clip = AnimationClip::new("OllieJump", 1.0).with_loop(LoopMode::None);
        clip.add_track(AnimationTrack::Position3DTrack {
            node_path: "Skater".into(),
            keyframes: vec![
                Keyframe::new(0.0, [0.0, 0.0, 0.0]),
                Keyframe::new(0.5, [0.0, 2.0, 0.0]),
                Keyframe::new(1.0, [0.0, 0.0, 0.0]),
            ],
            interpolation: InterpolationMode::Linear,
        });
        clip.add_track(AnimationTrack::MethodCallTrack {
            node_path: "Skater".into(),
            keyframes: vec![MethodKeyframe::new(0.5, "pop_particles", vec![IrValue::I64(10)])],
        });

        let mut player = AnimationPlayer::new();
        player.add_clip(clip);
        assert!(player.play("OllieJump"));

        // Step 0.5s: peak of jump
        let (finished, methods) = player.evaluate_into_scene(&mut scene, 0.5);
        assert!(!finished);
        assert_eq!(methods.len(), 1);
        assert_eq!(methods[0].1, "pop_particles");

        let skater = &scene.nodes[&skater_id];
        assert_eq!(
            skater.properties.get("translation"),
            Some(&IrValue::Vec3([0.0, 2.0, 0.0]))
        );

        // Step another 0.5s: land jump and finish
        let (finished, _) = player.evaluate_into_scene(&mut scene, 0.5);
        assert!(finished);
        let skater = &scene.nodes[&skater_id];
        assert_eq!(
            skater.properties.get("translation"),
            Some(&IrValue::Vec3([0.0, 0.0, 0.0]))
        );
    }

    #[test]
    fn animation_tree_bfs_travel_and_blendspace2d() {
        // Test state machine BFS travel
        let mut sm = AnimationNodeStateMachine::new();
        sm.add_state("Idle", "idle_clip");
        sm.add_state("Pushing", "push_clip");
        sm.add_state("Ollie", "ollie_clip");
        sm.add_state("Grind", "grind_clip");

        sm.add_transition(StateTransition::new("Idle", "Pushing", 0.2));
        sm.add_transition(StateTransition::new("Pushing", "Ollie", 0.1));
        sm.add_transition(StateTransition::new("Ollie", "Grind", 0.1));

        let path = sm.travel("Grind").expect("path to Grind");
        assert_eq!(path, vec!["Idle", "Pushing", "Ollie", "Grind"]);
        assert_eq!(sm.current_state, "Grind");

        // Test BlendSpace2D normalized weights
        let mut bs2d = BlendSpace2D::new();
        bs2d.add_point([0.0, 0.0], "idle");
        bs2d.add_point([0.0, 1.0], "run_fwd");
        bs2d.add_point([0.0, -1.0], "run_bwd");
        bs2d.add_point([1.0, 0.0], "strafe_right");
        bs2d.add_point([-1.0, 0.0], "strafe_left");

        // Exact match
        let exact = bs2d.sample([0.0, 1.0]);
        assert_eq!(exact, vec![("run_fwd".to_string(), 1.0)]);

        // Intermediate sample: weights must sum to 1.0
        let blended = bs2d.sample([0.5, 0.5]);
        let total_weight: f32 = blended.iter().map(|(_, w)| *w).sum();
        assert!((total_weight - 1.0).abs() < 1e-4);
    }

    #[test]
    fn ui_control_layout_solver_hbox_vbox_grid_margin_center() {
        let viewport = ControlRect::new(0.0, 0.0, 800.0, 600.0);

        // 1. FullRect anchor preset
        let root_id = NodeId::new(0, 1).unwrap();
        let mut root = ControlLayoutNode::new(root_id, ContainerKind::Control)
            .with_preset(AnchorPreset::FullRect);

        // 2. MarginContainer
        let margin_id = NodeId::new(1, 1).unwrap();
        let mut margin_container = ControlLayoutNode::new(
            margin_id,
            ContainerKind::MarginContainer {
                margins: [10.0, 20.0, 10.0, 20.0],
            },
        )
        .with_preset(AnchorPreset::FullRect);

        // 3. HBoxContainer with two children
        let hbox_id = NodeId::new(2, 1).unwrap();
        let mut hbox = ControlLayoutNode::new(
            hbox_id,
            ContainerKind::HBoxContainer { separation: 10.0 },
        )
        .with_preset(AnchorPreset::FullRect);

        let child1_id = NodeId::new(3, 1).unwrap();
        let child1 = ControlLayoutNode::new(child1_id, ContainerKind::Control)
            .with_min_size(100.0, 50.0)
            .with_size_flags(SizeFlags::fill(), SizeFlags::fill());

        let child2_id = NodeId::new(4, 1).unwrap();
        let child2 = ControlLayoutNode::new(child2_id, ContainerKind::Control)
            .with_min_size(100.0, 50.0)
            .with_size_flags(SizeFlags::expand_fill(), SizeFlags::fill());

        hbox.add_child(child1);
        hbox.add_child(child2);
        margin_container.add_child(hbox);
        root.add_child(margin_container);

        let layout = solve_control_hierarchy(viewport, &root);

        let root_rect = layout[&root_id];
        assert_eq!(root_rect, ControlRect::new(0.0, 0.0, 800.0, 600.0));

        let margin_rect = layout[&margin_id];
        assert_eq!(margin_rect, ControlRect::new(0.0, 0.0, 800.0, 600.0));

        // HBox inside MarginContainer: x = 10, y = 20, width = 780, height = 560
        let hbox_rect = layout[&hbox_id];
        assert_eq!(hbox_rect, ControlRect::new(10.0, 20.0, 780.0, 560.0));

        // Child 1 has min_size 100, no expand
        let c1_rect = layout[&child1_id];
        assert_eq!(c1_rect.x, 10.0);
        assert_eq!(c1_rect.width, 100.0);

        // Child 2 expands into remaining width: (780 - 10 separation - 100 c1_width) = 670
        let c2_rect = layout[&child2_id];
        assert_eq!(c2_rect.x, 10.0 + 100.0 + 10.0);
        assert_eq!(c2_rect.width, 670.0);
    }

    #[test]
    fn navigation_servers_2d_and_3d_pathfinding() {
        // 2D AStarGrid
        let mut grid = AStarGrid2D::new([0, 0], [10, 10], [1.0, 1.0]);
        // Block obstacle wall at x = 5, y = 0..8 (leaving a gap at y = 9)
        for y in 0..8 {
            grid.set_point_solid([5, y], true);
        }

        let path_2d = grid.find_path([0, 0], [10, 0]).expect("path around wall");
        assert!(path_2d.len() > 10);
        assert_eq!(*path_2d.first().unwrap(), [0, 0]);
        assert_eq!(*path_2d.last().unwrap(), [10, 0]);

        // Autotile bitmask lookup
        assert_eq!(TileMapLayer::autotile_lookup(0b1111), [3, 3]); // full cross
        assert_eq!(TileMapLayer::autotile_lookup(0b0000), [0, 0]); // isolated

        // 3D NavMesh
        let mut navmesh = NavMesh3D::new();
        // Poly 0: x: 0..5, z: 0..5 (y=0)
        navmesh.add_polygon(vec![
            [0.0, 0.0, 0.0],
            [5.0, 0.0, 0.0],
            [5.0, 0.0, 5.0],
            [0.0, 0.0, 5.0],
        ]);
        // Poly 1: x: 5..10, z: 0..5 (shares edge [5,0,0] to [5,0,5] with Poly 0)
        navmesh.add_polygon(vec![
            [5.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [10.0, 0.0, 5.0],
            [5.0, 0.0, 5.0],
        ]);

        let path_3d = navmesh
            .find_path([1.0, 0.0, 1.0], [9.0, 0.0, 4.0])
            .expect("3D navmesh path");
        assert_eq!(*path_3d.first().unwrap(), [1.0, 0.0, 1.0]);
        assert_eq!(*path_3d.last().unwrap(), [9.0, 0.0, 4.0]);
    }

    #[test]
    fn speculative_frame_buffer_and_state_hashing_exactness() {
        let mut scene = IrScene::new("RollbackPlaza");
        let skater_id = NodeId::new(1, 1).unwrap();
        scene.nodes.insert(
            skater_id,
            foreplay_ir::IrNode {
                id: skater_id,
                name: "Skater".into(),
                class_name: "CharacterBody3D".into(),
                parent: None,
                instance_scene: None,
                properties: BTreeMap::new(),
                groups: vec!["skaters".into()],
            },
        );

        let mut tree = SceneTree::from_ir(scene);
        let mut buffer = SpeculativeFrameBuffer::new(64);

        // Record base tick 0
        buffer.record_tick(&tree, 0);
        let hash0 = buffer.hash_at_tick(0).expect("hash at tick 0");

        // Speculative branch: begin transaction
        buffer.begin_transaction();
        assert!(buffer.is_in_transaction());

        // Step 10 ticks and mutate tree in speculative branch
        for t in 1..=10 {
            tree.step_frame();
            if let Some(skater_node) = tree.active_scene.nodes.get_mut(&skater_id) {
                skater_node
                    .properties
                    .insert("speed".into(), IrValue::F64(t as f64 * 2.0));
            }
            buffer.record_tick(&tree, t);
        }

        assert_eq!(tree.tick_count, 10);
        assert_ne!(buffer.hash_at_tick(10).unwrap(), hash0);

        // Roll back to tick 0
        buffer.rollback_to_tick(&mut tree, 0).expect("rollback to tick 0");
        assert_eq!(tree.tick_count, 0);
        assert!(!buffer.is_in_transaction());
        assert_eq!(state_hash(&tree), hash0);
        assert!(!tree.active_scene.nodes[&skater_id].properties.contains_key("speed"));
    }

    #[test]
    fn catmull_rom_spline_pcg_extruder_and_scatter_grid() {
        let points = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(10.0, 1.0, 0.0),
            Vec3::new(20.0, 1.5, 5.0),
            Vec3::new(30.0, 0.0, 10.0),
        ];

        let spline = CatmullRomSpline3D::new(points, false);
        assert!(spline.total_length() > 30.0);

        // Sample frame at mid-distance
        let frame = spline.sample_frame(spline.total_length() * 0.5);
        assert!(frame.tangent.is_normalized());
        assert!(frame.normal.is_normalized());
        assert!(frame.binormal.is_normalized());
        assert!(frame.tangent.dot(frame.normal).abs() < 1e-4);

        // Extrude skate handrail
        let config = PcgExtrusionConfig {
            profile: PcgProfile::RoundRail {
                radius: 0.04,
                radial_segments: 8,
            },
            step_distance: 0.5,
            uv_scale: [1.0, 1.0],
            generate_caps: true,
        };
        let mesh = PcgSplineExtruder::extrude(&spline, &config);
        assert!(!mesh.positions.is_empty());
        assert!(!mesh.indices.is_empty());
        assert!(!mesh.collision_segments.is_empty());
        assert_eq!(mesh.positions.len(), mesh.normals.len());
        assert_eq!(mesh.positions.len(), mesh.uvs.len());

        // Scatter street lamps/props
        let grid = PcgScatterGrid {
            bounds_min: Vec3::new(-20.0, 0.0, -20.0),
            bounds_max: Vec3::new(20.0, 10.0, 20.0),
            cell_size: 5.0,
            seed: 42,
            min_distance: 3.0,
            min_height: -5.0,
            max_height: 20.0,
            max_slope_rad: std::f32::consts::FRAC_PI_4,
            align_to_normal: true,
        };
        let instances = grid.scatter(|x, z| (0.1 * (x + z), Vec3::Y));
        assert!(!instances.is_empty());
        for inst in &instances {
            assert!(inst.position.x >= -20.0 && inst.position.x <= 20.0);
            assert!(inst.position.z >= -20.0 && inst.position.z <= 20.0);
        }
    }

    #[test]
    fn two_bone_ik_and_motion_matching_search() {
        // Analytical Two-Bone IK test
        let root = Vec3::new(0.0, 1.0, 0.0);
        let mid = Vec3::new(0.0, 0.5, 0.0);
        let tip = Vec3::new(0.0, 0.0, 0.0);
        let target = Vec3::new(0.3, 0.1, 0.2);
        let pole = Vec3::new(0.0, 0.5, 1.0); // Knee forward

        let (new_mid, new_tip) = solve_two_bone_ik(root, mid, tip, target, pole);
        let l1_orig = root.distance(mid);
        let l2_orig = mid.distance(tip);
        let l1_new = root.distance(new_mid);
        let l2_new = new_mid.distance(new_tip);

        assert!((l1_orig - l1_new).abs() < 1e-4, "Upper leg length must be preserved");
        assert!((l2_orig - l2_new).abs() < 1e-4, "Lower leg length must be preserved");
        assert!(new_tip.distance(target) < 1e-3, "Foot tip must reach target");

        // FABRIK Chain IK test
        let mut chain = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 2.0, 0.0),
            Vec3::new(0.0, 3.0, 0.0),
        ];
        let chain_target = Vec3::new(1.0, 2.0, 0.5);
        let reached = solve_fabrik_chain(&mut chain, chain_target, 0.01, 20);
        assert!(reached);
        assert!(chain[3].distance(chain_target) <= 0.01);

        // Motion Matching feature database test
        let mut db = PoseSearchDatabase::new();
        db.add_sample(MotionSample {
            sample_id: 1,
            anim_id: "OllieStart".into(),
            time: 0.1,
            velocity: Vec3::new(5.0, 0.0, 0.0),
            future_trajectory_200ms: Vec3::new(6.0, 0.2, 0.0),
            left_foot_offset: Vec3::new(-0.2, -0.5, 0.1),
            right_foot_offset: Vec3::new(0.2, -0.5, -0.1),
        });
        db.add_sample(MotionSample {
            sample_id: 2,
            anim_id: "PushHard".into(),
            time: 0.3,
            velocity: Vec3::new(8.0, 0.0, 0.0),
            future_trajectory_200ms: Vec3::new(9.6, 0.0, 0.0),
            left_foot_offset: Vec3::new(-0.2, -0.5, 0.3),
            right_foot_offset: Vec3::new(0.2, -0.2, -0.4),
        });

        let query = MotionQuery {
            target_velocity: Vec3::new(8.1, 0.0, 0.0),
            target_trajectory_200ms: Vec3::new(9.5, 0.0, 0.0),
            current_left_foot: Vec3::new(-0.2, -0.5, 0.3),
            current_right_foot: Vec3::new(0.2, -0.2, -0.4),
        };

        let weights = MotionCostWeights::default();
        let (best_sample, cost) = db.best_match(&query, &weights).expect("match found");
        assert_eq!(best_sample.sample_id, 2);
        assert!(cost < 0.1);

        // Inertial blend test
        let mut blend = InertialBlendState::new(Vec3::new(0.1, 0.0, 0.0), Vec3::ZERO, 0.2);
        let offset0 = blend.evaluate(0.0);
        assert!((offset0.x - 0.1).abs() < 1e-4);
        blend.evaluate(0.21);
        assert!(blend.is_finished());
    }
}
