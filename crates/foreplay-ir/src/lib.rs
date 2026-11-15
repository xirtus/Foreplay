//! Canonical Gameplay Intermediate Representation (`foreplay-ir`).
//!
//! Unites Foreplay's four frontends (Visual Editor, Rust/TS/GDScript code,
//! Visual Graph, and the AI Harness) around one deterministic, git-diffable,
//! borrow-safe data model.

use std::collections::BTreeMap;
use std::fmt;
use std::marker::PhantomData;
use std::num::NonZeroU32;

/// Generational handle identifying a `Node` in a `SceneTree` or `IrScene`.
/// Mirrors Godot's `ObjectID` without raw pointer aliasing or `RefCell` cycles.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
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
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
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
#[derive(Clone, PartialEq, Debug)]
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
#[derive(Clone, PartialEq, Debug)]
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
#[derive(Clone, PartialEq, Debug)]
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
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct SignalConnection {
    pub from_node: NodeId,
    pub signal_name: String,
    pub to_node: NodeId,
    pub method_name: String,
}

/// Canonical serializable scene representation shared by Editor, Code, Graph, and AI Harness.
#[derive(Clone, PartialEq, Debug, Default)]
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
        let mut out = format!("[foreplay_scene name=\"{}\" format=1]\n", self.name);
        for (key, val) in &self.feel_overrides {
            out.push_str(&format!("[feel_spec key=\"{key}\" value={val:.6}]\n"));
        }
        for node in self.nodes.values() {
            out.push_str(&format!(
                "\n[node id=\"{}:{}\" name=\"{}\" type=\"{}\"",
                node.id.index, node.id.generation, node.name, node.class_name
            ));
            if let Some(parent) = node.parent {
                out.push_str(&format!(" parent=\"{}:{}\"", parent.index, parent.generation));
            }
            if let Some(inst) = &node.instance_scene {
                out.push_str(&format!(" instance=\"{inst}\""));
            }
            out.push_str("]\n");
            for (k, v) in &node.properties {
                out.push_str(&format!("{k} = {v:?}\n"));
            }
        }
        for conn in &self.connections {
            out.push_str(&format!(
                "\n[connection signal=\"{}\" from=\"{}:{}\" to=\"{}:{}\" method=\"{}\"]\n",
                conn.signal_name,
                conn.from_node.index,
                conn.from_node.generation,
                conn.to_node.index,
                conn.to_node.generation,
                conn.method_name
            ));
        }
        out
    }
}

/// Errors produced by `IrScene` mutations or `EditorCommand` replay.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum IrError {
    NodeAlreadyExists(NodeId),
    NodeNotFound(NodeId),
    PropertyNotFound(String),
    InvalidHierarchyCycle(NodeId),
}

impl fmt::Display for IrError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NodeAlreadyExists(id) => write!(f, "node already exists: {id:?}"),
            Self::NodeNotFound(id) => write!(f, "node not found: {id:?}"),
            Self::PropertyNotFound(prop) => write!(f, "property not found: {prop}"),
            Self::InvalidHierarchyCycle(id) => write!(f, "cycle detected reparenting {id:?}"),
        }
    }
}

impl std::error::Error for IrError {}

/// Invertible command bus shared by `foreplay-editor` (Undo/Redo) and `foreplay-harness` (MCP Agent API).
#[derive(Clone, PartialEq, Debug)]
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
}
