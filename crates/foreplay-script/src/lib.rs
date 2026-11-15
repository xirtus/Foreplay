//! Dual scripting host: Native Rust hot-reload + zero-compile `rustports-quickjs` (TS/JS).
//!
//! Provides script attachment registry, hot-reload revision tracking, exported property
//! synchronization with `foreplay_ir::IrNode`, and the `QuickJsBridgeSpec` for TypeScript/JS bindings.

pub mod ts_eval;
pub use ts_eval::{JsEvalError, JsExecutionResult, QuickJsComponentRunner};

use foreplay_ir::{IrNode, IrValue, NodeId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Supported scripting frontends compiling to / operating over `foreplay-ir`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum ScriptFrontend {
    NativeRustCrate,
    QuickJsTypeScript,
    GdScriptCompat,
}

/// Metadata and state for a script attached to a specific `NodeId`.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct ScriptAttachment {
    pub node_id: NodeId,
    pub frontend: ScriptFrontend,
    pub script_path: String,
    pub revision: u64,
    pub properties: BTreeMap<String, IrValue>,
}

/// Script registry managing script attachments, hot-reload revisions, and property synchronization.
#[derive(Clone, Debug, Default)]
pub struct ScriptRegistry {
    attachments: BTreeMap<NodeId, ScriptAttachment>,
    path_to_nodes: BTreeMap<String, Vec<NodeId>>,
}

impl ScriptRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Attaches a script to a node handle, initializing revision to 1.
    pub fn attach_script(
        &mut self,
        node_id: NodeId,
        frontend: ScriptFrontend,
        script_path: impl Into<String>,
    ) -> &mut ScriptAttachment {
        let path = script_path.into();

        // Evict previous registration for this node if present
        if let Some(old) = self.attachments.remove(&node_id)
            && let Some(nodes) = self.path_to_nodes.get_mut(&old.script_path)
        {
            nodes.retain(|&id| id != node_id);
        }

        self.path_to_nodes
            .entry(path.clone())
            .or_default()
            .push(node_id);

        let attachment = ScriptAttachment {
            node_id,
            frontend,
            script_path: path,
            revision: 1,
            properties: BTreeMap::new(),
        };

        self.attachments.insert(node_id, attachment);
        self.attachments.get_mut(&node_id).unwrap()
    }

    /// Detaches a script from a node.
    pub fn detach_script(&mut self, node_id: NodeId) -> Option<ScriptAttachment> {
        let attachment = self.attachments.remove(&node_id)?;
        if let Some(nodes) = self.path_to_nodes.get_mut(&attachment.script_path) {
            nodes.retain(|&id| id != node_id);
        }
        Some(attachment)
    }

    pub fn get_attachment(&self, node_id: NodeId) -> Option<&ScriptAttachment> {
        self.attachments.get(&node_id)
    }

    pub fn get_attachment_mut(&mut self, node_id: NodeId) -> Option<&mut ScriptAttachment> {
        self.attachments.get_mut(&node_id)
    }

    pub fn attachments_for_path(&self, script_path: &str) -> &[NodeId] {
        self.path_to_nodes
            .get(script_path)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Bumps revision counter for a specific node handle (e.g. after hot recompile).
    pub fn bump_revision(&mut self, node_id: NodeId) -> Option<u64> {
        let attachment = self.attachments.get_mut(&node_id)?;
        attachment.revision += 1;
        Some(attachment.revision)
    }

    /// Reloads all node attachments pointing to `script_path`, incrementing their revisions.
    pub fn reload_script(&mut self, script_path: &str) -> usize {
        let mut count = 0;
        if let Some(node_ids) = self.path_to_nodes.get(script_path).cloned() {
            for node_id in node_ids {
                if let Some(att) = self.attachments.get_mut(&node_id) {
                    att.revision += 1;
                    count += 1;
                }
            }
        }
        count
    }

    /// Copies exported script properties to target `IrNode`.
    pub fn sync_properties_to_ir(&self, node_id: NodeId, node: &mut IrNode) {
        if let Some(att) = self.attachments.get(&node_id) {
            for (k, v) in &att.properties {
                node.properties.insert(k.clone(), v.clone());
            }
        }
    }

    /// Synchronizes properties from `IrNode` into script attachment.
    pub fn sync_from_ir(&mut self, node: &IrNode) {
        if let Some(att) = self.attachments.get_mut(&node.id) {
            for (k, v) in &node.properties {
                att.properties.insert(k.clone(), v.clone());
            }
        }
    }

    pub fn attachment_count(&self) -> usize {
        self.attachments.len()
    }
}

/// Bridge specification for `rustports-quickjs` TypeScript runtime and binding generator.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct QuickJsBridgeSpec {
    pub module_name: String,
    pub script_path: String,
    pub exported_symbols: Vec<String>,
}

impl QuickJsBridgeSpec {
    pub fn new(module_name: impl Into<String>, script_path: impl Into<String>) -> Self {
        Self {
            module_name: module_name.into(),
            script_path: script_path.into(),
            exported_symbols: Vec::new(),
        }
    }

    pub fn with_symbol(mut self, symbol: impl Into<String>) -> Self {
        self.exported_symbols.push(symbol.into());
        self
    }

    /// Generates TypeScript declaration file content (`.d.ts`) from an `IrNode` and its properties.
    pub fn generate_d_ts(&self, class_name: &str, properties: &BTreeMap<String, IrValue>) -> String {
        let mut out = format!("// Generated Foreplay TypeScript definition for {class_name}\n");
        out.push_str(&format!("export declare class {class_name} {{\n"));
        for (k, v) in properties {
            let ts_type = match v {
                IrValue::Nil => "null | undefined",
                IrValue::Bool(_) => "boolean",
                IrValue::I64(_) | IrValue::F64(_) => "number",
                IrValue::String(_) | IrValue::NodePath(_) | IrValue::ResourcePath(_) => "string",
                IrValue::Vec2(_) => "[number, number]",
                IrValue::Vec3(_) => "[number, number, number]",
                IrValue::Quat(_) | IrValue::Color(_) => "[number, number, number, number]",
                IrValue::Array(_) => "any[]",
                IrValue::Map(_) => "Record<string, any>",
            };
            out.push_str(&format!("    {k}: {ts_type};\n"));
        }
        out.push_str("    ready?(): void;\n");
        out.push_str("    physicsProcess?(delta: number): void;\n");
        out.push_str("    process?(delta: number): void;\n");
        out.push_str("}\n");
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroU32;

    #[test]
    fn script_registry_attach_reload_and_sync() {
        let mut registry = ScriptRegistry::new();
        let id1 = NodeId {
            index: 1,
            generation: NonZeroU32::new(1).unwrap(),
        };

        // Attach QuickJs script
        let att = registry.attach_script(
            id1,
            ScriptFrontend::QuickJsTypeScript,
            "res://scripts/skater.ts",
        );
        att.properties
            .insert("max_speed".into(), IrValue::F64(14.5));
        att.properties
            .insert("is_grounded".into(), IrValue::Bool(true));

        assert_eq!(att.revision, 1);
        assert_eq!(
            registry.attachments_for_path("res://scripts/skater.ts"),
            &[id1]
        );

        // Sync to IrNode
        let mut node = IrNode {
            id: id1,
            name: "Skater".into(),
            class_name: "CharacterBody3D".into(),
            parent: None,
            instance_scene: None,
            properties: BTreeMap::new(),
            groups: Vec::new(),
        };
        registry.sync_properties_to_ir(id1, &mut node);
        assert_eq!(
            node.properties.get("max_speed"),
            Some(&IrValue::F64(14.5))
        );

        // Hot reload script
        let reloaded = registry.reload_script("res://scripts/skater.ts");
        assert_eq!(reloaded, 1);
        assert_eq!(registry.get_attachment(id1).unwrap().revision, 2);

        // Generate TS definition
        let bridge = QuickJsBridgeSpec::new("skater_controller", "res://scripts/skater.ts");
        let dts = bridge.generate_d_ts("SkaterController", &node.properties);
        assert!(dts.contains("max_speed: number;"));
        assert!(dts.contains("is_grounded: boolean;"));
    }
}
