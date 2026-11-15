//! JSON-RPC 2.0 MCP & `bevy_remote` (BRP)-compatible agent interface.
//!
//! Exposes scene observation, live `FeelSpec` tuning, invertible action dispatch,
//! and headless verification through standard JSON-RPC 2.0 protocols.

use crate::HarnessSession;
use foreplay_ir::{EditorCommand, IrNode, IrValue, NodeId, SignalConnection};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::num::NonZeroU32;

/// JSON-RPC 2.0 Server handling incoming requests over stdio, TCP, or WebSocket transports.
#[derive(Clone, Copy, Debug, Default)]
pub struct HarnessRpcServer;

impl HarnessRpcServer {
    pub fn new() -> Self {
        Self
    }

    /// Handles a single JSON-RPC 2.0 request string and produces a deterministic JSON-RPC response string.
    pub fn handle_json(session: &mut HarnessSession, req_json: &str) -> String {
        let parsed: Result<Value, _> = serde_json::from_str(req_json);
        let req_val = match parsed {
            Ok(val) => val,
            Err(e) => {
                return serde_json::to_string(&json!({
                    "jsonrpc": "2.0",
                    "id": Value::Null,
                    "error": {
                        "code": -32700,
                        "message": format!("Parse error: {e}")
                    }
                }))
                .unwrap_or_else(|_| "{}".into());
            }
        };

        let id = req_val.get("id").cloned().unwrap_or(Value::Null);
        let method = match req_val.get("method").and_then(|m| m.as_str()) {
            Some(m) => m,
            None => {
                return serde_json::to_string(&json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {
                        "code": -32600,
                        "message": "Invalid Request: missing 'method'"
                    }
                }))
                .unwrap_or_else(|_| "{}".into());
            }
        };

        let params = req_val.get("params").cloned().unwrap_or(Value::Null);

        let response = match Self::dispatch_method(session, method, &params) {
            Ok(result) => json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": result
            }),
            Err(rpc_err) => json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {
                    "code": rpc_err.code,
                    "message": rpc_err.message
                }
            }),
        };

        serde_json::to_string(&response).unwrap_or_else(|_| "{}".into())
    }

    fn dispatch_method(
        session: &mut HarnessSession,
        method: &str,
        params: &Value,
    ) -> Result<Value, RpcError> {
        match method {
            "foreplay/observe" => Self::handle_observe(session),
            "foreplay/insert_node" => Self::handle_insert_node(session, params),
            "foreplay/set_property" => Self::handle_set_property(session, params),
            "foreplay/remove_node" => Self::handle_remove_node(session, params),
            "foreplay/tune_feel" => Self::handle_tune_feel(session, params),
            "foreplay/rename_node" => Self::handle_rename_node(session, params),
            "foreplay/reparent_node" => Self::handle_reparent_node(session, params),
            "foreplay/connect_signal" => Self::handle_connect_signal(session, params),
            "foreplay/disconnect_signal" => Self::handle_disconnect_signal(session, params),
            "foreplay/set_groups" => Self::handle_set_groups(session, params),
            "foreplay/batch" => Self::handle_batch(session, params),
            "foreplay/run_gdscript_ticks" => Self::handle_run_gdscript_ticks(session, params),
            "foreplay/export_project" => Self::handle_export_project(session, params),
            "foreplay/undo" => Self::handle_undo(session),
            "foreplay/redo" => Self::handle_redo(session),
            "foreplay/step" => Self::handle_step(session, params),
            "foreplay/import_tscn" => Self::handle_import_tscn(session, params),
            "foreplay/export_fscn" => Self::handle_export_fscn(session),
            _ => Err(RpcError::method_not_found(method)),
        }
    }

    fn handle_observe(session: &HarnessSession) -> Result<Value, RpcError> {
        let scene = &session.tree.active_scene;
        let atlas = session.observe_atlas();

        let nodes_json: Vec<Value> = scene
            .nodes
            .values()
            .map(|n| {
                json!({
                    "id": { "index": n.id.index, "generation": n.id.generation.get() },
                    "name": n.name,
                    "class_name": n.class_name,
                    "parent": n.parent.map(|p| json!({ "index": p.index, "generation": p.generation.get() })),
                    "instance_scene": n.instance_scene,
                    "properties": n.properties,
                    "groups": n.groups
                })
            })
            .collect();

        let affected_nodes_json: Vec<Value> = atlas
            .affected_nodes
            .iter()
            .map(|id| json!({ "index": id.index, "generation": id.generation.get() }))
            .collect();

        let edges_json: Vec<Value> = atlas
            .edges
            .iter()
            .map(|e| {
                json!({
                    "from": { "index": e.from.index, "generation": e.from.generation.get() },
                    "to": { "index": e.to.index, "generation": e.to.generation.get() },
                    "relation": e.relation
                })
            })
            .collect();

        Ok(json!({
            "scene_name": scene.name,
            "node_count": scene.nodes.len(),
            "nodes": nodes_json,
            "feel_overrides": scene.feel_overrides,
            "atlas": {
                "scene_name": atlas.scene_name,
                "affected_nodes": affected_nodes_json,
                "edges": edges_json
            },
            "tick_count": session.tree.tick_count
        }))
    }

    fn handle_insert_node(session: &mut HarnessSession, params: &Value) -> Result<Value, RpcError> {
        let node_val = if params.get("node").is_some() {
            &params["node"]
        } else {
            params
        };

        let name = node_val
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("NewNode")
            .to_string();

        let class_name = node_val
            .get("class_name")
            .or_else(|| node_val.get("type"))
            .and_then(|v| v.as_str())
            .unwrap_or("Node3D")
            .to_string();

        let parent = if let Some(p_val) = node_val.get("parent") {
            parse_node_id_val(p_val)
        } else {
            None
        };

        let instance_scene = node_val
            .get("instance_scene")
            .and_then(|v| v.as_str())
            .map(str::to_string);

        let groups = if let Some(g_arr) = node_val.get("groups").and_then(|v| v.as_array()) {
            g_arr
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        } else {
            Vec::new()
        };

        let mut properties = BTreeMap::new();
        if let Some(props_obj) = node_val.get("properties").and_then(|v| v.as_object()) {
            for (k, v) in props_obj {
                properties.insert(k.clone(), convert_json_to_ir_value(v));
            }
        }

        // Allocate NodeId if not given
        let id = if let Some(id_val) = node_val.get("id") {
            parse_node_id_val(id_val).ok_or_else(|| RpcError::invalid_params("invalid 'id'"))?
        } else {
            let next_index = session
                .tree
                .active_scene
                .nodes
                .keys()
                .map(|k| k.index)
                .max()
                .unwrap_or(0)
                + 1;
            NodeId {
                index: next_index,
                generation: NonZeroU32::new(1).unwrap(),
            }
        };

        let ir_node = IrNode {
            id,
            name,
            class_name,
            parent,
            instance_scene,
            properties,
            groups,
        };

        session
            .execute(EditorCommand::InsertNode {
                node: ir_node.clone(),
            })
            .map_err(|e| RpcError::internal(format!("Failed inserting node: {e}")))?;

        session.tree.rebuild_world_from_ir();

        Ok(json!({
            "success": true,
            "node_id": { "index": id.index, "generation": id.generation.get() }
        }))
    }

    fn handle_set_property(session: &mut HarnessSession, params: &Value) -> Result<Value, RpcError> {
        let node_id_val = params
            .get("node_id")
            .ok_or_else(|| RpcError::invalid_params("missing 'node_id'"))?;
        let node_id = parse_node_id_val(node_id_val)
            .ok_or_else(|| RpcError::invalid_params("invalid 'node_id'"))?;

        let key = params
            .get("key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| RpcError::invalid_params("missing 'key'"))?
            .to_string();

        let val_json = params
            .get("value")
            .or_else(|| params.get("new_value"))
            .ok_or_else(|| RpcError::invalid_params("missing 'value'"))?;

        let new_ir_val = convert_json_to_ir_value(val_json);

        let old_value = session
            .tree
            .active_scene
            .nodes
            .get(&node_id)
            .and_then(|n| n.properties.get(&key).cloned());

        session
            .execute(EditorCommand::SetProperty {
                node_id,
                key: key.clone(),
                old_value,
                new_value: Some(new_ir_val),
            })
            .map_err(|e| RpcError::internal(format!("Failed setting property: {e}")))?;

        session.tree.rebuild_world_from_ir();

        Ok(json!({
            "success": true,
            "node_id": { "index": node_id.index, "generation": node_id.generation.get() },
            "key": key
        }))
    }

    fn handle_remove_node(session: &mut HarnessSession, params: &Value) -> Result<Value, RpcError> {
        let node_id_val = params
            .get("node_id")
            .ok_or_else(|| RpcError::invalid_params("missing 'node_id'"))?;
        let node_id = parse_node_id_val(node_id_val)
            .ok_or_else(|| RpcError::invalid_params("invalid 'node_id'"))?;

        let target_node = session
            .tree
            .active_scene
            .nodes
            .get(&node_id)
            .cloned()
            .ok_or_else(|| RpcError::invalid_params(format!("node {node_id:?} not found")))?;

        session
            .execute(EditorCommand::RemoveNode { node: target_node })
            .map_err(|e| RpcError::internal(format!("Failed removing node: {e}")))?;

        session.tree.rebuild_world_from_ir();

        Ok(json!({
            "success": true,
            "removed_node_id": { "index": node_id.index, "generation": node_id.generation.get() }
        }))
    }

    fn handle_tune_feel(session: &mut HarnessSession, params: &Value) -> Result<Value, RpcError> {
        let key = params
            .get("key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| RpcError::invalid_params("missing 'key'"))?
            .to_string();

        let val = params
            .get("value")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| RpcError::invalid_params("missing numeric 'value'"))?;

        let old_value = session.tree.active_scene.feel_overrides.get(&key).copied();

        session
            .execute(EditorCommand::TuneFeelSpec {
                key: key.clone(),
                old_value,
                new_value: Some(val),
            })
            .map_err(|e| RpcError::internal(format!("Failed tuning feel: {e}")))?;

        Ok(json!({
            "success": true,
            "key": key,
            "value": val
        }))
    }

    fn handle_undo(session: &mut HarnessSession) -> Result<Value, RpcError> {
        let undone = session
            .undo()
            .map_err(|e| RpcError::internal(format!("Undo failed: {e}")))?;
        session.tree.rebuild_world_from_ir();
        Ok(json!({ "undone": undone }))
    }

    fn handle_redo(session: &mut HarnessSession) -> Result<Value, RpcError> {
        let redone = session
            .redo()
            .map_err(|e| RpcError::internal(format!("Redo failed: {e}")))?;
        session.tree.rebuild_world_from_ir();
        Ok(json!({ "redone": redone }))
    }

    fn handle_step(session: &mut HarnessSession, params: &Value) -> Result<Value, RpcError> {
        let ticks = params.get("ticks").and_then(|v| v.as_u64()).unwrap_or(1);
        let current_ticks = session.verify_headless_ticks(ticks);
        Ok(json!({
            "tick_count": current_ticks,
            "stepped_ticks": ticks
        }))
    }

    fn handle_import_tscn(session: &mut HarnessSession, params: &Value) -> Result<Value, RpcError> {
        let source = params
            .get("source")
            .and_then(|v| v.as_str())
            .ok_or_else(|| RpcError::invalid_params("missing 'source' string"))?;

        let scene = foreplay_porthost::tscn::parse_tscn_to_ir(source)
            .map_err(|e| RpcError::internal(format!("TSCN import failed: {e}")))?;

        let node_count = scene.nodes.len();
        let conn_count = scene.connections.len();
        let scene_name = scene.name.clone();

        session.tree.active_scene = scene;
        session.tree.rebuild_world_from_ir();

        Ok(json!({
            "success": true,
            "scene_name": scene_name,
            "node_count": node_count,
            "connection_count": conn_count
        }))
    }

    fn handle_export_fscn(session: &HarnessSession) -> Result<Value, RpcError> {
        let fscn_text = session.tree.active_scene.to_fscn_string();
        Ok(json!({
            "fscn": fscn_text
        }))
    }

    fn handle_rename_node(session: &mut HarnessSession, params: &Value) -> Result<Value, RpcError> {
        let node_id_val = params
            .get("node_id")
            .ok_or_else(|| RpcError::invalid_params("missing 'node_id'"))?;
        let node_id = parse_node_id_val(node_id_val)
            .ok_or_else(|| RpcError::invalid_params("invalid 'node_id'"))?;
        let new_name = params
            .get("new_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| RpcError::invalid_params("missing 'new_name'"))?
            .to_string();

        let old_name = session
            .tree
            .active_scene
            .nodes
            .get(&node_id)
            .map(|n| n.name.clone())
            .ok_or_else(|| RpcError::invalid_params(format!("node {node_id:?} not found")))?;

        session
            .execute(EditorCommand::RenameNode {
                node_id,
                old_name,
                new_name,
            })
            .map_err(|e| RpcError::internal(format!("failed renaming node: {e}")))?;

        Ok(json!({
            "success": true,
            "node_id": { "index": node_id.index, "generation": node_id.generation.get() }
        }))
    }

    fn handle_reparent_node(session: &mut HarnessSession, params: &Value) -> Result<Value, RpcError> {
        let node_id_val = params
            .get("node_id")
            .ok_or_else(|| RpcError::invalid_params("missing 'node_id'"))?;
        let node_id = parse_node_id_val(node_id_val)
            .ok_or_else(|| RpcError::invalid_params("invalid 'node_id'"))?;
        let new_parent = params.get("new_parent").and_then(parse_node_id_val);

        let old_parent = session
            .tree
            .active_scene
            .nodes
            .get(&node_id)
            .map(|n| n.parent)
            .ok_or_else(|| RpcError::invalid_params(format!("node {node_id:?} not found")))?;

        session
            .execute(EditorCommand::ReparentNode {
                node_id,
                old_parent,
                new_parent,
            })
            .map_err(|e| RpcError::internal(format!("failed reparenting node: {e}")))?;

        Ok(json!({
            "success": true,
            "node_id": { "index": node_id.index, "generation": node_id.generation.get() }
        }))
    }

    fn handle_connect_signal(session: &mut HarnessSession, params: &Value) -> Result<Value, RpcError> {
        let from_node = params
            .get("from_node")
            .and_then(parse_node_id_val)
            .ok_or_else(|| RpcError::invalid_params("missing 'from_node'"))?;
        let signal_name = params
            .get("signal_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| RpcError::invalid_params("missing 'signal_name'"))?
            .to_string();
        let to_node = params
            .get("to_node")
            .and_then(parse_node_id_val)
            .ok_or_else(|| RpcError::invalid_params("missing 'to_node'"))?;
        let method_name = params
            .get("method_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| RpcError::invalid_params("missing 'method_name'"))?
            .to_string();

        let connection = SignalConnection {
            from_node,
            signal_name,
            to_node,
            method_name,
        };

        session
            .execute(EditorCommand::ConnectSignal { connection })
            .map_err(|e| RpcError::internal(format!("failed connecting signal: {e}")))?;

        Ok(json!({ "success": true }))
    }

    fn handle_disconnect_signal(session: &mut HarnessSession, params: &Value) -> Result<Value, RpcError> {
        let from_node = params
            .get("from_node")
            .and_then(parse_node_id_val)
            .ok_or_else(|| RpcError::invalid_params("missing 'from_node'"))?;
        let signal_name = params
            .get("signal_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| RpcError::invalid_params("missing 'signal_name'"))?
            .to_string();
        let to_node = params
            .get("to_node")
            .and_then(parse_node_id_val)
            .ok_or_else(|| RpcError::invalid_params("missing 'to_node'"))?;
        let method_name = params
            .get("method_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| RpcError::invalid_params("missing 'method_name'"))?
            .to_string();

        let connection = SignalConnection {
            from_node,
            signal_name,
            to_node,
            method_name,
        };

        session
            .execute(EditorCommand::DisconnectSignal { connection })
            .map_err(|e| RpcError::internal(format!("failed disconnecting signal: {e}")))?;

        Ok(json!({ "success": true }))
    }

    fn handle_set_groups(session: &mut HarnessSession, params: &Value) -> Result<Value, RpcError> {
        let node_id_val = params
            .get("node_id")
            .ok_or_else(|| RpcError::invalid_params("missing 'node_id'"))?;
        let node_id = parse_node_id_val(node_id_val)
            .ok_or_else(|| RpcError::invalid_params("invalid 'node_id'"))?;
        let new_groups: Vec<String> = params
            .get("groups")
            .and_then(|v| v.as_array())
            .ok_or_else(|| RpcError::invalid_params("missing 'groups' array"))?
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect();

        let old_groups = session
            .tree
            .active_scene
            .nodes
            .get(&node_id)
            .map(|n| n.groups.clone())
            .ok_or_else(|| RpcError::invalid_params(format!("node {node_id:?} not found")))?;

        session
            .execute(EditorCommand::SetGroups {
                node_id,
                old_groups,
                new_groups,
            })
            .map_err(|e| RpcError::internal(format!("failed setting groups: {e}")))?;

        Ok(json!({
            "success": true,
            "node_id": { "index": node_id.index, "generation": node_id.generation.get() }
        }))
    }

    fn handle_batch(session: &mut HarnessSession, params: &Value) -> Result<Value, RpcError> {
        let label = params
            .get("label")
            .and_then(|v| v.as_str())
            .unwrap_or("BatchCommand")
            .to_string();
        let cmds_arr = params
            .get("commands")
            .and_then(|v| v.as_array())
            .ok_or_else(|| RpcError::invalid_params("missing 'commands' array"))?;

        let mut commands = Vec::new();
        for cmd_val in cmds_arr {
            if let Ok(cmd) = serde_json::from_value::<EditorCommand>(cmd_val.clone()) {
                commands.push(cmd);
            } else {
                return Err(RpcError::invalid_params(format!("failed parsing command in batch: {cmd_val:?}")));
            }
        }

        session
            .execute(EditorCommand::Batch { label, commands })
            .map_err(|e| RpcError::internal(format!("failed executing batch command: {e}")))?;

        session.tree.rebuild_world_from_ir();

        Ok(json!({ "success": true }))
    }

    fn handle_run_gdscript_ticks(session: &mut HarnessSession, params: &Value) -> Result<Value, RpcError> {
        let node_id_val = params
            .get("node_id")
            .ok_or_else(|| RpcError::invalid_params("missing 'node_id'"))?;
        let node_id = parse_node_id_val(node_id_val)
            .ok_or_else(|| RpcError::invalid_params("invalid 'node_id'"))?;
        let ticks = params.get("ticks").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
        let dt = params.get("dt").and_then(|v| v.as_f64()).unwrap_or(1.0 / 60.0);

        let mut node = session
            .tree
            .active_scene
            .nodes
            .get(&node_id)
            .cloned()
            .ok_or_else(|| RpcError::invalid_params(format!("node {node_id:?} not found")))?;

        let source = if let Some(s) = params.get("source").and_then(|v| v.as_str()) {
            s.to_string()
        } else if let Some(IrValue::String(s)) = node.properties.get("gdscript") {
            s.clone()
        } else if let Some(IrValue::String(s)) = node.properties.get("script") {
            s.clone()
        } else {
            return Err(RpcError::invalid_params("no gdscript source found for node"));
        };

        let mut vm = foreplay_gdscript::GdVm::from_source(&source)
            .map_err(|e| RpcError::internal(format!("failed compiling gdscript: {e}")))?;

        let mut total_steps = 0;
        let mut all_logs = Vec::new();
        let mut all_signals = Vec::new();

        // Call _ready first
        if let Ok(res_ready) = vm.ready(&mut node) {
            total_steps += res_ready.steps_executed;
            all_logs.extend(res_ready.logs);
        }

        for _ in 0..ticks {
            let res = vm
                .step_physics_process(dt, &mut node)
                .map_err(|e| RpcError::internal(format!("gdscript error: {e}")))?;
            total_steps += res.steps_executed;
            all_logs.extend(res.logs);
            all_signals.extend(res.emitted_signals);
        }

        // Write updated node back to active scene
        if let Some(target) = session.tree.active_scene.nodes.get_mut(&node_id) {
            target.properties = node.properties.clone();
        }

        Ok(json!({
            "success": true,
            "steps_executed": total_steps,
            "logs": all_logs,
            "emitted_signals": all_signals.len(),
            "updated_properties": node.properties
        }))
    }

    fn handle_export_project(session: &mut HarnessSession, params: &Value) -> Result<Value, RpcError> {
        let target_str = params
            .get("target")
            .and_then(|v| v.as_str())
            .unwrap_or("LinuxSteamDeckX64");

        let target = match target_str {
            "LinuxSteamDeckX64" | "linux" | "steamdeck" => foreplay_export::ExportTarget::LinuxSteamDeckX64,
            "WindowsX64" | "windows" | "win" => foreplay_export::ExportTarget::WindowsX64,
            "MacOsAppBundle" | "macos" | "mac" | "app" => foreplay_export::ExportTarget::MacOsAppBundle,
            "WebGpuBrowserUrl" | "web" | "webgpu" | "html5" => foreplay_export::ExportTarget::WebGpuBrowserUrl,
            "SexOsPackage" | "sexos" => foreplay_export::ExportTarget::SexOsPackage,
            other => return Err(RpcError::invalid_params(format!("unknown export target '{other}'"))),
        };

        let game_title = params
            .get("game_title")
            .and_then(|v| v.as_str())
            .unwrap_or(&session.tree.active_scene.name)
            .to_string();

        let package_name = params
            .get("package_name")
            .and_then(|v| v.as_str())
            .unwrap_or("com.foreplay.game")
            .to_string();

        let scene_slug: String = session
            .tree
            .active_scene
            .name
            .chars()
            .map(|c| if c.is_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
            .collect();
        let main_scene = format!("res://scenes/{scene_slug}.fscn");

        let mut config = foreplay_export::ExportConfig::new(game_title, package_name, main_scene.clone());
        let fscn_data = session.tree.active_scene.to_fscn_string().into_bytes();
        let clean_path = main_scene.trim_start_matches("res://");
        config = config.with_file(clean_path, fscn_data);

        let output = foreplay_export::ProjectExporter::export_project(&config, target)
            .map_err(|e| RpcError::internal(format!("export failed: {e}")))?;

        Ok(json!({
            "success": true,
            "target": target.display_name(),
            "primary_artifact": output.primary_artifact,
            "bundle_file_count": output.bundle_files.len(),
            "archive_size": output.archive_bytes.len()
        }))
    }
}

fn parse_node_id_val(v: &Value) -> Option<NodeId> {
    if let Some(obj) = v.as_object() {
        let idx = obj.get("index").and_then(|v| v.as_u64())? as u32;
        let generation = obj.get("generation").and_then(|v| v.as_u64())? as u32;
        NodeId::new(idx, generation)
    } else if let Some(num) = v.as_u64() {
        NodeId::new(num as u32, 1)
    } else {
        None
    }
}

fn convert_json_to_ir_value(val: &Value) -> IrValue {
    if let Ok(direct) = serde_json::from_value::<IrValue>(val.clone()) {
        return direct;
    }
    match val {
        Value::Null => IrValue::Nil,
        Value::Bool(b) => IrValue::Bool(*b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                IrValue::I64(i)
            } else if let Some(f) = n.as_f64() {
                IrValue::F64(f)
            } else {
                IrValue::Nil
            }
        }
        Value::String(s) => IrValue::String(s.clone()),
        Value::Array(arr) => {
            let elements = arr.iter().map(convert_json_to_ir_value).collect();
            IrValue::Array(elements)
        }
        Value::Object(obj) => {
            let mut map = BTreeMap::new();
            for (k, v) in obj {
                map.insert(k.clone(), convert_json_to_ir_value(v));
            }
            IrValue::Map(map)
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
}

impl RpcError {
    pub fn method_not_found(method: &str) -> Self {
        Self {
            code: -32601,
            message: format!("Method not found: '{method}'"),
        }
    }

    pub fn invalid_params(msg: impl Into<String>) -> Self {
        Self {
            code: -32602,
            message: msg.into(),
        }
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self {
            code: -32603,
            message: msg.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use foreplay_ir::IrScene;

    #[test]
    fn rpc_observe_act_undo_roundtrip() {
        let mut session = HarnessSession::new(IrScene::new("RpcTestScene"));

        // 1. Observe empty scene
        let obs_res = HarnessRpcServer::handle_json(
            &mut session,
            r#"{"jsonrpc":"2.0","id":1,"method":"foreplay/observe","params":{}}"#,
        );
        let obs_val: Value = serde_json::from_str(&obs_res).unwrap();
        assert_eq!(obs_val["result"]["scene_name"], "RpcTestScene");
        assert_eq!(obs_val["result"]["node_count"], 0);

        // 2. Insert node
        let insert_res = HarnessRpcServer::handle_json(
            &mut session,
            r#"{
                "jsonrpc": "2.0",
                "id": 2,
                "method": "foreplay/insert_node",
                "params": {
                    "name": "SkaterPlayer",
                    "class_name": "CharacterBody3D",
                    "properties": { "max_speed": 14.5 }
                }
            }"#,
        );
        let insert_val: Value = serde_json::from_str(&insert_res).unwrap();
        assert_eq!(insert_val["result"]["success"], true);

        // 3. Observe updated scene
        let obs_res2 = HarnessRpcServer::handle_json(
            &mut session,
            r#"{"jsonrpc":"2.0","id":3,"method":"foreplay/observe","params":{}}"#,
        );
        let obs_val2: Value = serde_json::from_str(&obs_res2).unwrap();
        assert_eq!(obs_val2["result"]["node_count"], 1);

        // 4. Tune feel spec
        let tune_res = HarnessRpcServer::handle_json(
            &mut session,
            r#"{
                "jsonrpc": "2.0",
                "id": 4,
                "method": "foreplay/tune_feel",
                "params": { "key": "ollie_pop", "value": 7.5 }
            }"#,
        );
        let tune_val: Value = serde_json::from_str(&tune_res).unwrap();
        assert_eq!(tune_val["result"]["success"], true);

        // 5. Undo tune feel and insert
        let undo_res1 = HarnessRpcServer::handle_json(
            &mut session,
            r#"{"jsonrpc":"2.0","id":5,"method":"foreplay/undo","params":{}}"#,
        );
        let undo_val1: Value = serde_json::from_str(&undo_res1).unwrap();
        assert_eq!(undo_val1["result"]["undone"], true);

        let undo_res2 = HarnessRpcServer::handle_json(
            &mut session,
            r#"{"jsonrpc":"2.0","id":6,"method":"foreplay/undo","params":{}}"#,
        );
        let undo_val2: Value = serde_json::from_str(&undo_res2).unwrap();
        assert_eq!(undo_val2["result"]["undone"], true);

        // 6. Verify back to 0 nodes
        let obs_res3 = HarnessRpcServer::handle_json(
            &mut session,
            r#"{"jsonrpc":"2.0","id":7,"method":"foreplay/observe","params":{}}"#,
        );
        let obs_val3: Value = serde_json::from_str(&obs_res3).unwrap();
        assert_eq!(obs_val3["result"]["node_count"], 0);
    }

    #[test]
    fn rpc_import_tscn_export_fscn_and_step() {
        let mut session = HarnessSession::new(IrScene::new("EmptyScene"));
        let tscn_content = include_str!("../../../templates/godot-migration/skate_spot.tscn");

        // Import .tscn via RPC
        let import_req = json!({
            "jsonrpc": "2.0",
            "id": 10,
            "method": "foreplay/import_tscn",
            "params": {
                "source": tscn_content
            }
        });
        let import_res = HarnessRpcServer::handle_json(&mut session, &import_req.to_string());
        let import_val: Value = serde_json::from_str(&import_res).unwrap();
        assert_eq!(import_val["result"]["success"], true);
        assert_eq!(import_val["result"]["scene_name"], "SkateSpot");
        assert!(import_val["result"]["node_count"].as_u64().unwrap() >= 5);

        // Step 60 frames
        let step_res = HarnessRpcServer::handle_json(
            &mut session,
            r#"{"jsonrpc":"2.0","id":11,"method":"foreplay/step","params":{"ticks":60}}"#,
        );
        let step_val: Value = serde_json::from_str(&step_res).unwrap();
        assert_eq!(step_val["result"]["tick_count"], 60);

        // Export deterministic .fscn
        let export_res = HarnessRpcServer::handle_json(
            &mut session,
            r#"{"jsonrpc":"2.0","id":12,"method":"foreplay/export_fscn","params":{}}"#,
        );
        let export_val: Value = serde_json::from_str(&export_res).unwrap();
        let fscn_text = export_val["result"]["fscn"].as_str().unwrap();
        assert!(fscn_text.contains("[foreplay_scene name=\"SkateSpot\""));
        assert!(fscn_text.contains("Skater"));
    }

    #[test]
    fn rpc_editor_commands_gdscript_and_export_roundtrip() {
        let mut session = HarnessSession::new(IrScene::new("ComprehensiveRpcScene"));

        // 1. Insert Node A & B
        let insert_a = HarnessRpcServer::handle_json(
            &mut session,
            r#"{
                "jsonrpc": "2.0",
                "id": 20,
                "method": "foreplay/insert_node",
                "params": {
                    "id": { "index": 1, "generation": 1 },
                    "name": "OldName",
                    "class_name": "CharacterBody3D",
                    "properties": { "velocity": [0.0, 0.0, 0.0] }
                }
            }"#,
        );
        let val_a: Value = serde_json::from_str(&insert_a).unwrap();
        assert_eq!(val_a["result"]["success"], true);

        let insert_b = HarnessRpcServer::handle_json(
            &mut session,
            r#"{
                "jsonrpc": "2.0",
                "id": 21,
                "method": "foreplay/insert_node",
                "params": {
                    "id": { "index": 2, "generation": 1 },
                    "name": "ChildNode",
                    "class_name": "Node3D"
                }
            }"#,
        );
        let val_b: Value = serde_json::from_str(&insert_b).unwrap();
        assert_eq!(val_b["result"]["success"], true);

        // 2. Rename Node 1
        let rename_res = HarnessRpcServer::handle_json(
            &mut session,
            r#"{
                "jsonrpc": "2.0",
                "id": 22,
                "method": "foreplay/rename_node",
                "params": {
                    "node_id": { "index": 1, "generation": 1 },
                    "new_name": "SkaterRoot"
                }
            }"#,
        );
        let rename_val: Value = serde_json::from_str(&rename_res).unwrap();
        assert_eq!(rename_val["result"]["success"], true);
        assert_eq!(
            session
                .tree
                .active_scene
                .nodes
                .get(&NodeId::new(1, 1).unwrap())
                .unwrap()
                .name,
            "SkaterRoot"
        );

        // 3. Reparent Node 2 to Node 1
        let reparent_res = HarnessRpcServer::handle_json(
            &mut session,
            r#"{
                "jsonrpc": "2.0",
                "id": 23,
                "method": "foreplay/reparent_node",
                "params": {
                    "node_id": { "index": 2, "generation": 1 },
                    "new_parent": { "index": 1, "generation": 1 }
                }
            }"#,
        );
        let reparent_val: Value = serde_json::from_str(&reparent_res).unwrap();
        assert_eq!(reparent_val["result"]["success"], true);
        assert_eq!(
            session
                .tree
                .active_scene
                .nodes
                .get(&NodeId::new(2, 1).unwrap())
                .unwrap()
                .parent,
            Some(NodeId::new(1, 1).unwrap())
        );

        // 4. Connect and Disconnect Signal
        let conn_res = HarnessRpcServer::handle_json(
            &mut session,
            r#"{
                "jsonrpc": "2.0",
                "id": 24,
                "method": "foreplay/connect_signal",
                "params": {
                    "from_node": { "index": 1, "generation": 1 },
                    "signal_name": "ollie_popped",
                    "to_node": { "index": 2, "generation": 1 },
                    "method_name": "on_player_ollie"
                }
            }"#,
        );
        let conn_val: Value = serde_json::from_str(&conn_res).unwrap();
        assert_eq!(conn_val["result"]["success"], true);
        assert_eq!(session.tree.active_scene.connections.len(), 1);

        let disconn_res = HarnessRpcServer::handle_json(
            &mut session,
            r#"{
                "jsonrpc": "2.0",
                "id": 25,
                "method": "foreplay/disconnect_signal",
                "params": {
                    "from_node": { "index": 1, "generation": 1 },
                    "signal_name": "ollie_popped",
                    "to_node": { "index": 2, "generation": 1 },
                    "method_name": "on_player_ollie"
                }
            }"#,
        );
        let disconn_val: Value = serde_json::from_str(&disconn_res).unwrap();
        assert_eq!(disconn_val["result"]["success"], true);
        assert_eq!(session.tree.active_scene.connections.len(), 0);

        // 5. Set Groups
        let groups_res = HarnessRpcServer::handle_json(
            &mut session,
            r#"{
                "jsonrpc": "2.0",
                "id": 26,
                "method": "foreplay/set_groups",
                "params": {
                    "node_id": { "index": 1, "generation": 1 },
                    "groups": ["players", "skaters"]
                }
            }"#,
        );
        let groups_val: Value = serde_json::from_str(&groups_res).unwrap();
        assert_eq!(groups_val["result"]["success"], true);
        assert_eq!(
            session
                .tree
                .active_scene
                .nodes
                .get(&NodeId::new(1, 1).unwrap())
                .unwrap()
                .groups,
            vec!["players", "skaters"]
        );

        // 6. Run GDScript Ticks
        let gd_res = HarnessRpcServer::handle_json(
            &mut session,
            r#"{
                "jsonrpc": "2.0",
                "id": 27,
                "method": "foreplay/run_gdscript_ticks",
                "params": {
                    "node_id": { "index": 1, "generation": 1 },
                    "source": "extends CharacterBody3D\nvar speed = 10.0\nfunc _physics_process(delta):\n    position.x += speed * delta\n",
                    "ticks": 5,
                    "dt": 0.1
                }
            }"#,
        );
        let gd_val: Value = serde_json::from_str(&gd_res).unwrap();
        assert_eq!(gd_val["result"]["success"], true);
        assert!(gd_val["result"]["steps_executed"].as_u64().unwrap() > 0);

        // 7. Export Project
        let export_res = HarnessRpcServer::handle_json(
            &mut session,
            r#"{
                "jsonrpc": "2.0",
                "id": 28,
                "method": "foreplay/export_project",
                "params": {
                    "target": "LinuxSteamDeckX64",
                    "game_title": "TestGame"
                }
            }"#,
        );
        let export_val: Value = serde_json::from_str(&export_res).unwrap();
        assert_eq!(export_val["result"]["success"], true);
        assert_eq!(export_val["result"]["primary_artifact"], "game.x86_64");
    }
}
