//! Canonical Gameplay Intermediate Representation (`foreplay-ir`).
//!
//! Unites Foreplay's four frontends (Visual Editor, Rust/TS/GDScript code,
//! Visual Graph, and the AI Harness) around one deterministic, git-diffable,
//! borrow-safe data model.

pub mod catalog;
pub mod fscn;
pub mod project;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::marker::PhantomData;
use std::num::NonZeroU32;

pub use catalog::{
    builtin_node_catalog, get_node_class, is_subclass_of, NodeCategory, NodeClassSpec, SignalSpec,
};
pub use fscn::FscnParseError;
pub use project::{InputBinding, InputMapIr, ProjectSettingsIr};

/// Generational handle identifying a `Node` in a `SceneTree` or `IrScene`.
/// Mirrors Godot's `ObjectID` without raw pointer aliasing or `RefCell` cycles.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub struct NodeId {
    pub index: u32,
    pub generation: NonZeroU32,
}

impl NodeId {
    pub const fn new(index: u32, generation: u32) -> Option<Self> {
        match NonZeroU32::new(generation) {
            Some(gen_nz) => Some(Self {
                index,
                generation: gen_nz,
            }),
            None => None,
        }
    }
}

/// Generational handle identifying an immutable or COW `Resource`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub struct ResourceId {
    pub index: u32,
    pub generation: NonZeroU32,
}

/// Typed low-level Server Resource ID (`RenderingServer`, `PhysicsServer3D`, `AudioServer`).
/// Mirrors Godot's `RID` with compile-time server-domain tagging.
#[derive(PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Rid<Tag> {
    pub raw: u64,
    _tag: PhantomData<fn() -> Tag>,
}

impl<Tag> Clone for Rid<Tag> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<Tag> Copy for Rid<Tag> {}

impl<Tag> fmt::Debug for Rid<Tag> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Rid({})", self.raw)
    }
}

impl<Tag> Rid<Tag> {
    pub const fn from_raw(raw: u64) -> Self {
        Self {
            raw,
            _tag: PhantomData,
        }
    }
}

/// Canonical dynamically-typed value for Inspector drawers, `.fscn`/`.tscn` serialization,
/// `FeelSpec` live tuning, and AI Agent JSON-RPC inspection.
/// Mirrors Godot's `Variant`.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub enum IrValue {
    Nil,
    Bool(bool),
    I64(i64),
    F64(f64),
    String(String),
    Vec2([f32; 2]),
    Vec3([f32; 3]),
    Quat([f32; 4]),
    Color([f32; 4]),
    NodePath(String),
    ResourcePath(String),
    Array(Vec<IrValue>),
    Map(BTreeMap<String, IrValue>),
}

/// 3JSE-inspired `FeelSpec` tuning knob bound directly to a component/node property.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct FeelSpecKnob {
    pub key: String,
    pub intent: String,
    pub unit: String,
    pub min: f64,
    pub max: f64,
    pub default_value: f64,
}

/// Reflection contract implemented via `#[derive(ForeplayNode)]` in Phase 1.
pub trait ReflectNode {
    fn class_name(&self) -> &'static str;
    fn feel_specs(&self) -> &[FeelSpecKnob] {
        &[]
    }
    fn export_properties(&self) -> BTreeMap<String, IrValue>;
    fn apply_property(&mut self, name: &str, value: IrValue) -> Result<(), IrError>;
}

/// A single node inside an `IrScene` (`BTreeMap` guarantees deterministic `.fscn` git diffs).
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct IrNode {
    pub id: NodeId,
    pub name: String,
    pub class_name: String,
    pub parent: Option<NodeId>,
    /// Optional `res://` path to an instanced `PackedScene` (`instance=ExtResource(...)`).
    pub instance_scene: Option<String>,
    pub properties: BTreeMap<String, IrValue>,
    pub groups: Vec<String>,
}

/// Typed signal wire (`Mirrors SceneState::ConnectionData` in Godot).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize, Deserialize)]
pub struct SignalConnection {
    pub from_node: NodeId,
    pub signal_name: String,
    pub to_node: NodeId,
    pub method_name: String,
}

/// Canonical serializable scene representation shared by Editor, Code, Graph, and AI Harness.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct IrScene {
    pub name: String,
    pub nodes: BTreeMap<NodeId, IrNode>,
    pub connections: Vec<SignalConnection>,
    pub feel_overrides: BTreeMap<String, f64>,
}

impl IrScene {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Self::default()
        }
    }

    /// Deterministic `.fscn` text emission (sorted keys guarantee clean git merges).
    pub fn to_fscn_string(&self) -> String {
        let mut out = format!("[foreplay_scene name=\"{}\" format=1]\n", fscn::escape_str(&self.name));
        for (key, val) in &self.feel_overrides {
            out.push_str(&format!("[feel_spec key=\"{}\" value={}]\n", fscn::escape_str(key), fscn::format_f64(*val)));
        }
        for node in self.nodes.values() {
            out.push_str(&format!(
                "\n[node id=\"{}:{}\" name=\"{}\" type=\"{}\"",
                node.id.index, node.id.generation, fscn::escape_str(&node.name), fscn::escape_str(&node.class_name)
            ));
            if let Some(parent) = node.parent {
                out.push_str(&format!(" parent=\"{}:{}\"", parent.index, parent.generation));
            }
            if let Some(inst) = &node.instance_scene {
                out.push_str(&format!(" instance=\"{}\"", fscn::escape_str(inst)));
            }
            if !node.groups.is_empty() {
                out.push_str(" groups=[");
                for (i, g) in node.groups.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    out.push_str(&format!("\"{}\"", fscn::escape_str(g)));
                }
                out.push(']');
            }
            out.push_str("]\n");
            for (k, v) in &node.properties {
                out.push_str(&format!("{k} = {}\n", fscn::serialize_ir_value(v)));
            }
        }
        for conn in &self.connections {
            out.push_str(&format!(
                "\n[connection signal=\"{}\" from=\"{}:{}\" to=\"{}:{}\" method=\"{}\"]\n",
                fscn::escape_str(&conn.signal_name),
                conn.from_node.index,
                conn.from_node.generation,
                conn.to_node.index,
                conn.to_node.generation,
                fscn::escape_str(&conn.method_name)
            ));
        }
        out
    }

    /// Lossless, zero-panic parser converting `.fscn` document text into `IrScene`.
    pub fn from_fscn_str(input: &str) -> Result<Self, FscnParseError> {
        fscn::parse_fscn(input)
    }

    /// Checks if setting `child`'s parent to `candidate_parent` would produce a cycle in the hierarchy.
    pub fn would_create_cycle(&self, child: NodeId, candidate_parent: Option<NodeId>) -> bool {
        let Some(parent) = candidate_parent else {
            return false;
        };
        if child == parent {
            return true;
        }
        let ancestors = self.ancestors_of(parent);
        ancestors.contains(&child)
    }

    /// Returns the sequence of ancestors from the node's immediate parent up to the root.
    pub fn ancestors_of(&self, node: NodeId) -> Vec<NodeId> {
        let mut ancestors = Vec::new();
        let mut current = node;
        let mut visited = std::collections::BTreeSet::new();
        visited.insert(current);
        while let Some(n) = self.nodes.get(&current) {
            if let Some(parent) = n.parent {
                if !visited.insert(parent) {
                    break;
                }
                ancestors.push(parent);
                current = parent;
            } else {
                break;
            }
        }
        ancestors
    }
}

/// Errors produced by `IrScene` mutations or `EditorCommand` replay.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum IrError {
    NodeAlreadyExists(NodeId),
    NodeNotFound(NodeId),
    PropertyNotFound(String),
    InvalidHierarchyCycle(NodeId),
    ConnectionAlreadyExists(SignalConnection),
    ConnectionNotFound(SignalConnection),
}

impl fmt::Display for IrError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NodeAlreadyExists(id) => write!(f, "node already exists: {id:?}"),
            Self::NodeNotFound(id) => write!(f, "node not found: {id:?}"),
            Self::PropertyNotFound(prop) => write!(f, "property not found: {prop}"),
            Self::InvalidHierarchyCycle(id) => write!(f, "cycle detected reparenting {id:?}"),
            Self::ConnectionAlreadyExists(c) => write!(f, "signal connection already exists: {c:?}"),
            Self::ConnectionNotFound(c) => write!(f, "signal connection not found: {c:?}"),
        }
    }
}

impl std::error::Error for IrError {}

/// Invertible command bus shared by `foreplay-editor` (Undo/Redo) and `foreplay-harness` (MCP Agent API).
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub enum EditorCommand {
    InsertNode {
        node: IrNode,
    },
    RemoveNode {
        node: IrNode,
    },
    SetProperty {
        node_id: NodeId,
        key: String,
        old_value: Option<IrValue>,
        new_value: Option<IrValue>,
    },
    TuneFeelSpec {
        key: String,
        old_value: Option<f64>,
        new_value: Option<f64>,
    },
    RenameNode {
        node_id: NodeId,
        old_name: String,
        new_name: String,
    },
    ReparentNode {
        node_id: NodeId,
        old_parent: Option<NodeId>,
        new_parent: Option<NodeId>,
    },
    ConnectSignal {
        connection: SignalConnection,
    },
    DisconnectSignal {
        connection: SignalConnection,
    },
    SetGroups {
        node_id: NodeId,
        old_groups: Vec<String>,
        new_groups: Vec<String>,
    },
    Batch {
        label: String,
        commands: Vec<EditorCommand>,
    },
}

impl EditorCommand {
    /// Returns the exact inverse command so Undo/Redo is mathematically invertible.
    pub fn inverse(&self) -> Self {
        match self {
            Self::InsertNode { node } => Self::RemoveNode { node: node.clone() },
            Self::RemoveNode { node } => Self::InsertNode { node: node.clone() },
            Self::SetProperty {
                node_id,
                key,
                old_value,
                new_value,
            } => Self::SetProperty {
                node_id: *node_id,
                key: key.clone(),
                old_value: new_value.clone(),
                new_value: old_value.clone(),
            },
            Self::TuneFeelSpec {
                key,
                old_value,
                new_value,
            } => Self::TuneFeelSpec {
                key: key.clone(),
                old_value: *new_value,
                new_value: *old_value,
            },
            Self::RenameNode {
                node_id,
                old_name,
                new_name,
            } => Self::RenameNode {
                node_id: *node_id,
                old_name: new_name.clone(),
                new_name: old_name.clone(),
            },
            Self::ReparentNode {
                node_id,
                old_parent,
                new_parent,
            } => Self::ReparentNode {
                node_id: *node_id,
                old_parent: *new_parent,
                new_parent: *old_parent,
            },
            Self::ConnectSignal { connection } => Self::DisconnectSignal {
                connection: connection.clone(),
            },
            Self::DisconnectSignal { connection } => Self::ConnectSignal {
                connection: connection.clone(),
            },
            Self::SetGroups {
                node_id,
                old_groups,
                new_groups,
            } => Self::SetGroups {
                node_id: *node_id,
                old_groups: new_groups.clone(),
                new_groups: old_groups.clone(),
            },
            Self::Batch { label, commands } => Self::Batch {
                label: label.clone(),
                commands: commands.iter().rev().map(Self::inverse).collect(),
            },
        }
    }

    /// Applies this command deterministically to an `IrScene`.
    pub fn apply(&self, scene: &mut IrScene) -> Result<(), IrError> {
        match self {
            Self::InsertNode { node } => {
                if scene.nodes.contains_key(&node.id) {
                    return Err(IrError::NodeAlreadyExists(node.id));
                }
                scene.nodes.insert(node.id, node.clone());
                Ok(())
            }
            Self::RemoveNode { node } => {
                scene
                    .nodes
                    .remove(&node.id)
                    .ok_or(IrError::NodeNotFound(node.id))?;
                Ok(())
            }
            Self::SetProperty {
                node_id,
                key,
                new_value,
                ..
            } => {
                let target = scene
                    .nodes
                    .get_mut(node_id)
                    .ok_or(IrError::NodeNotFound(*node_id))?;
                match new_value {
                    Some(val) => {
                        target.properties.insert(key.clone(), val.clone());
                    }
                    None => {
                        target.properties.remove(key);
                    }
                }
                Ok(())
            }
            Self::TuneFeelSpec { key, new_value, .. } => {
                match new_value {
                    Some(v) => {
                        scene.feel_overrides.insert(key.clone(), *v);
                    }
                    None => {
                        scene.feel_overrides.remove(key);
                    }
                }
                Ok(())
            }
            Self::RenameNode {
                node_id, new_name, ..
            } => {
                let target = scene
                    .nodes
                    .get_mut(node_id)
                    .ok_or(IrError::NodeNotFound(*node_id))?;
                target.name = new_name.clone();
                Ok(())
            }
            Self::ReparentNode {
                node_id,
                new_parent,
                ..
            } => {
                if !scene.nodes.contains_key(node_id) {
                    return Err(IrError::NodeNotFound(*node_id));
                }
                if let Some(parent_id) = new_parent
                    && !scene.nodes.contains_key(parent_id)
                {
                    return Err(IrError::NodeNotFound(*parent_id));
                }
                if scene.would_create_cycle(*node_id, *new_parent) {
                    return Err(IrError::InvalidHierarchyCycle(*node_id));
                }
                let target = scene
                    .nodes
                    .get_mut(node_id)
                    .ok_or(IrError::NodeNotFound(*node_id))?;
                target.parent = *new_parent;
                Ok(())
            }
            Self::ConnectSignal { connection } => {
                if !scene.nodes.contains_key(&connection.from_node) {
                    return Err(IrError::NodeNotFound(connection.from_node));
                }
                if !scene.nodes.contains_key(&connection.to_node) {
                    return Err(IrError::NodeNotFound(connection.to_node));
                }
                if scene.connections.contains(connection) {
                    return Err(IrError::ConnectionAlreadyExists(connection.clone()));
                }
                scene.connections.push(connection.clone());
                Ok(())
            }
            Self::DisconnectSignal { connection } => {
                let pos = scene
                    .connections
                    .iter()
                    .position(|c| c == connection)
                    .ok_or_else(|| IrError::ConnectionNotFound(connection.clone()))?;
                scene.connections.remove(pos);
                Ok(())
            }
            Self::SetGroups {
                node_id,
                new_groups,
                ..
            } => {
                let target = scene
                    .nodes
                    .get_mut(node_id)
                    .ok_or(IrError::NodeNotFound(*node_id))?;
                target.groups = new_groups.clone();
                Ok(())
            }
            Self::Batch { commands, .. } => {
                let mut staged = scene.clone();
                for sub_cmd in commands {
                    sub_cmd.apply(&mut staged)?;
                }
                *scene = staged;
                Ok(())
            }
        }
    }
}

/// Hotswappable physics contract allowing live A/B switching (e.g. Rapier vs. Skate 2 / Skate 3 port physics).
pub trait PhysicsBackend: Send + Sync + fmt::Debug {
    fn backend_id(&self) -> &'static str;
    fn step(&mut self, dt_seconds: f32);
}

/// Read-only BYO-ROM / Retail Game Archive VFS mount (`port://`) for `skate2-rust-port`-style projects.
pub trait PortArchiveMount: Send + Sync + fmt::Debug {
    fn scheme_prefix(&self) -> &str;
    fn list_entries(&self) -> Vec<String>;
    fn read_bytes(&self, virtual_path: &str) -> Result<Vec<u8>, String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_commands_are_strictly_invertible() {
        let mut scene = IrScene::new("SanVanelonaPlaza");
        let initial = scene.clone();

        let root_id = NodeId::new(0, 1).expect("non-zero generation");
        let root_node = IrNode {
            id: root_id,
            name: "SkaterRoot".into(),
            class_name: "CharacterBody3D".into(),
            parent: None,
            instance_scene: None,
            properties: BTreeMap::new(),
            groups: vec!["players".into()],
        };

        let insert_cmd = EditorCommand::InsertNode { node: root_node };
        insert_cmd.apply(&mut scene).unwrap();

        let prop_cmd = EditorCommand::SetProperty {
            node_id: root_id,
            key: "max_speed".into(),
            old_value: None,
            new_value: Some(IrValue::F64(14.5)),
        };
        prop_cmd.apply(&mut scene).unwrap();

        let feel_cmd = EditorCommand::TuneFeelSpec {
            key: "ollie_pop_impulse".into(),
            old_value: None,
            new_value: Some(6.8),
        };
        feel_cmd.apply(&mut scene).unwrap();

        assert!(scene.to_fscn_string().contains("SkaterRoot"));
        assert_eq!(scene.feel_overrides.get("ollie_pop_impulse"), Some(&6.8));

        // Undo in reverse order must restore exact initial state
        feel_cmd.inverse().apply(&mut scene).unwrap();
        prop_cmd.inverse().apply(&mut scene).unwrap();
        insert_cmd.inverse().apply(&mut scene).unwrap();

        assert_eq!(scene, initial);
    }

    #[test]
    fn fscn_lossless_roundtrip_all_ir_value_types() {
        let mut scene = IrScene::new("ComprehensiveRoundTripTest");
        scene.feel_overrides.insert("gravity".into(), 9.81);
        scene.feel_overrides.insert("air_resistance".into(), 0.05);

        let root_id = NodeId::new(1, 1).unwrap();
        let child_id = NodeId::new(2, 1).unwrap();

        let mut root_props = BTreeMap::new();
        root_props.insert("nil_prop".into(), IrValue::Nil);
        root_props.insert("bool_true".into(), IrValue::Bool(true));
        root_props.insert("bool_false".into(), IrValue::Bool(false));
        root_props.insert("int_pos".into(), IrValue::I64(42));
        root_props.insert("int_neg".into(), IrValue::I64(-1337));
        root_props.insert("float_val".into(), IrValue::F64(123.456));
        root_props.insert("escaped_str".into(), IrValue::String("hello \"world\"\nline2\ttab".into()));
        root_props.insert("vec2_val".into(), IrValue::Vec2([1.5, -2.5]));
        root_props.insert("vec3_val".into(), IrValue::Vec3([0.0, 10.0, -5.25]));
        root_props.insert("quat_val".into(), IrValue::Quat([0.0, 0.7, 0.0, 0.7]));
        root_props.insert("color_val".into(), IrValue::Color([1.0, 0.5, 0.25, 0.8]));
        root_props.insert("nodepath_val".into(), IrValue::NodePath("SkaterRoot/Camera3D".into()));
        root_props.insert("resourcepath_val".into(), IrValue::ResourcePath("res://models/skater.glb".into()));

        let mut nested_map = BTreeMap::new();
        nested_map.insert("sub_bool".into(), IrValue::Bool(true));
        nested_map.insert("sub_vec3".into(), IrValue::Vec3([1.0, 2.0, 3.0]));
        nested_map.insert("sub_arr".into(), IrValue::Array(vec![IrValue::I64(10), IrValue::Nil]));

        root_props.insert(
            "array_val".into(),
            IrValue::Array(vec![
                IrValue::I64(1),
                IrValue::F64(2.5),
                IrValue::String("element".into()),
                IrValue::Map(nested_map.clone()),
            ]),
        );
        root_props.insert("map_val".into(), IrValue::Map(nested_map));

        let root_node = IrNode {
            id: root_id,
            name: "WorldRoot".into(),
            class_name: "Node3D".into(),
            parent: None,
            instance_scene: None,
            properties: root_props,
            groups: vec!["root_group".into(), "visible".into()],
        };

        let mut child_props = BTreeMap::new();
        child_props.insert("speed".into(), IrValue::F64(24.0));
        let child_node = IrNode {
            id: child_id,
            name: "InstancedHero".into(),
            class_name: "CharacterBody3D".into(),
            parent: Some(root_id),
            instance_scene: Some("res://prefabs/hero.fscn".into()),
            properties: child_props,
            groups: vec![],
        };

        scene.nodes.insert(root_id, root_node);
        scene.nodes.insert(child_id, child_node);

        scene.connections.push(SignalConnection {
            from_node: child_id,
            signal_name: "jumped".into(),
            to_node: root_id,
            method_name: "_on_hero_jumped".into(),
        });

        let serialized = scene.to_fscn_string();
        let parsed = IrScene::from_fscn_str(&serialized).expect("parse must succeed");

        assert_eq!(parsed, scene, "roundtrip scene must be identical");
    }

    #[test]
    fn hierarchy_cycle_detection() {
        let mut scene = IrScene::new("CycleTest");
        let n1 = NodeId::new(1, 1).unwrap();
        let n2 = NodeId::new(2, 1).unwrap();
        let n3 = NodeId::new(3, 1).unwrap();

        // Tree: n1 -> n2 -> n3
        scene.nodes.insert(
            n1,
            IrNode {
                id: n1,
                name: "Node1".into(),
                class_name: "Node".into(),
                parent: None,
                instance_scene: None,
                properties: BTreeMap::new(),
                groups: vec![],
            },
        );
        scene.nodes.insert(
            n2,
            IrNode {
                id: n2,
                name: "Node2".into(),
                class_name: "Node".into(),
                parent: Some(n1),
                instance_scene: None,
                properties: BTreeMap::new(),
                groups: vec![],
            },
        );
        scene.nodes.insert(
            n3,
            IrNode {
                id: n3,
                name: "Node3".into(),
                class_name: "Node".into(),
                parent: Some(n2),
                instance_scene: None,
                properties: BTreeMap::new(),
                groups: vec![],
            },
        );

        assert_eq!(scene.ancestors_of(n3), vec![n2, n1]);
        assert_eq!(scene.ancestors_of(n2), vec![n1]);
        assert_eq!(scene.ancestors_of(n1), Vec::<NodeId>::new());

        // Self parent is cycle
        assert!(scene.would_create_cycle(n1, Some(n1)));
        // Making n1 child of n3 creates a cycle (n1 -> n2 -> n3 -> n1)
        assert!(scene.would_create_cycle(n1, Some(n3)));
        // Making n1 child of n2 creates a cycle (n1 -> n2 -> n1)
        assert!(scene.would_create_cycle(n1, Some(n2)));
        // Making n3 child of n1 is safe
        assert!(!scene.would_create_cycle(n3, Some(n1)));
        // Making root has no cycle
        assert!(!scene.would_create_cycle(n3, None));
    }

    #[test]
    fn serde_json_derives_work() {
        let node_id = NodeId::new(42, 7).unwrap();
        let json = serde_json::to_string(&node_id).unwrap();
        let deserialized: NodeId = serde_json::from_str(&json).unwrap();
        assert_eq!(node_id, deserialized);

        let mut scene = IrScene::new("SerdeJsonScene");
        scene.feel_overrides.insert("tuning".into(), 1.0);
        let scene_json = serde_json::to_string(&scene).unwrap();
        let scene_deser: IrScene = serde_json::from_str(&scene_json).unwrap();
        assert_eq!(scene, scene_deser);
    }

    #[test]
    fn builtin_catalog_covers_48_plus_classes_and_inheritance() {
        let catalog = builtin_node_catalog();
        assert!(catalog.len() >= 48, "expected at least 48 classes, got {}", catalog.len());

        // Check essential Godot 4 classes exist
        assert!(catalog.contains_key("Node"));
        assert!(catalog.contains_key("Node2D"));
        assert!(catalog.contains_key("CharacterBody2D"));
        assert!(catalog.contains_key("Node3D"));
        assert!(catalog.contains_key("CharacterBody3D"));
        assert!(catalog.contains_key("Control"));
        assert!(catalog.contains_key("Button"));
        assert!(catalog.contains_key("AnimationPlayer"));
        assert!(catalog.contains_key("AudioStreamPlayer3D"));

        // Check inheritance chains
        assert!(is_subclass_of("CharacterBody3D", "Node3D"));
        assert!(is_subclass_of("CharacterBody3D", "Node"));
        assert!(is_subclass_of("Button", "Control"));
        assert!(is_subclass_of("CheckBox", "Control"));
        assert!(!is_subclass_of("Node3D", "Node2D"));

        // Check signals
        let button_spec = get_node_class("Button").unwrap();
        assert!(button_spec.built_in_signals.iter().any(|s| s.name == "pressed"));

        // Check badge colors
        assert_eq!(NodeCategory::Node3D.badge_color(), [0.95, 0.45, 0.40, 1.0]);
    }

    #[test]
    fn input_map_and_project_settings_roundtrip() {
        let mut settings = ProjectSettingsIr::default_project("SanVanelona");
        settings.physics_ticks_per_second = 120;
        settings.input_map.bind("special_trick", InputBinding::key("F"));
        settings.input_map.bind("special_trick", InputBinding::joy_button(4));

        let toml_str = settings.to_toml_string();
        let parsed = ProjectSettingsIr::from_toml_str(&toml_str).expect("parse toml project settings");

        assert_eq!(parsed.name, "SanVanelona");
        assert_eq!(parsed.physics_ticks_per_second, 120);
        assert_eq!(parsed.input_map.bindings_for("special_trick").len(), 2);
        assert_eq!(
            parsed.input_map.bindings_for("special_trick")[0],
            InputBinding::Key { keycode: "F".into() }
        );
    }
}
