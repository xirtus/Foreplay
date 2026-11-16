//! Dual scripting host: Native Rust hot-reload + zero-compile `rustports-quickjs` (TS/JS),
//! plus `sexlife` pro-apps creative suite bridges (`Moan` Adaptive Audio & `Splicy` Cutscenes).
//!
//! Provides script attachment registry, hot-reload revision tracking, exported property
//! synchronization with `foreplay_ir::IrNode`, the `QuickJsBridgeSpec` for TypeScript/JS bindings,
//! native Rust dynamic library (`.dylib`/`.so`/`.dll`) hot-reloading with state-preserving migration,
//! and bidirectional `Moan` / `Splicy` manifests.

pub mod rust_hot_reload;
pub use rust_hot_reload::*;

pub mod sexlife_bridge;
pub use sexlife_bridge::*;

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

    #[test]
    fn test_c_abi_vtable_and_descriptor_layout() {
        assert_eq!(FOREPLAY_RUST_SCRIPT_ABI_VERSION, 1);
        assert_eq!(std::mem::size_of::<RustPropertyType>(), 4);
        assert_eq!(std::mem::size_of::<RustScriptContext>(), 16);
    }

    #[test]
    fn test_schema_type_conversion_and_coercion() {
        assert!(RustPropertyType::F64.matches_ir_value(&IrValue::F64(std::f64::consts::PI)));
        assert!(RustPropertyType::I64.matches_ir_value(&IrValue::I64(42)));
        assert!(RustPropertyType::Bool.matches_ir_value(&IrValue::Bool(true)));

        // Coercion from I64 to F64
        let coerced = coerce_ir_value(&IrValue::I64(10), RustPropertyType::F64);
        assert_eq!(coerced, Some(IrValue::F64(10.0)));

        // Coercion from F64 to I64
        let coerced_int = coerce_ir_value(&IrValue::F64(24.7), RustPropertyType::I64);
        assert_eq!(coerced_int, Some(IrValue::I64(24)));

        // Incompatible coercion
        let coerced_fail = coerce_ir_value(&IrValue::String("hello".into()), RustPropertyType::F64);
        assert_eq!(coerced_fail, None);
    }

    #[test]
    fn test_migration_report_preserved_defaulted_archived() {
        let mut report = MigrationReport {
            preserved_keys: vec!["speed".into()],
            defaulted_new_keys: vec!["jump_force".into()],
            archived_removed_keys: BTreeMap::new(),
            generation: 2,
        };
        report
            .archived_removed_keys
            .insert("old_unused".into(), IrValue::Bool(false));

        assert_eq!(report.generation, 2);
        assert!(report.preserved_keys.contains(&"speed".into()));
        assert!(report.defaulted_new_keys.contains(&"jump_force".into()));
        assert_eq!(
            report.archived_removed_keys.get("old_unused"),
            Some(&IrValue::Bool(false))
        );
    }

    #[test]
    fn test_migration_type_incompatibility_handling() {
        let node_id = NodeId::new(10, 1).unwrap();
        let mut host = RustHotReloadHost::new("type_test", None);
        let mut props = BTreeMap::new();
        props.insert("velocity".into(), IrValue::String("invalid_speed".into()));
        host.bind_node(node_id, Some(props));

        assert_eq!(
            host.get_property(node_id, "velocity"),
            Some(IrValue::String("invalid_speed".into()))
        );
    }

    #[test]
    fn test_shadow_library_path_and_generation_increment() {
        let temp_dir = std::env::temp_dir().join("foreplay_test_shadow_host");
        let host = RustHotReloadHost::new("skater_controller", Some(temp_dir.clone()));
        assert_eq!(host.current_generation(), 0);
        assert_eq!(host.plugin_name(), "skater_controller");
    }

    #[test]
    fn test_end_to_end_rustc_cdylib_v1_to_v2_hot_reload() {
        let temp_dir = std::env::temp_dir().join(format!("foreplay_e2e_cdylib_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        // 1. Rust source for Plugin V1
        let v1_src = r#"
static PLUGIN_NAME: &[u8] = b"SkaterPlugin\0";
static SPEED_PROP: &[u8] = b"speed\0";
static SPEED_DEF: &[u8] = b"10.0\0";

static V1_SCHEMAS: [RustPropertySchema; 1] = [
    RustPropertySchema {
        name: SPEED_PROP.as_ptr() as *const ::std::ffi::c_char,
        prop_type: 3, // F64
        default_json: SPEED_DEF.as_ptr() as *const ::std::ffi::c_char,
    },
];

unsafe extern "C" fn v1_desc() -> RustPluginDescriptor {
    RustPluginDescriptor {
        abi_version: 1,
        name: PLUGIN_NAME.as_ptr() as *const ::std::ffi::c_char,
        schema_count: 1,
        schemas: V1_SCHEMAS.as_ptr(),
    }
}

struct State {
    speed: f64,
    ticks: u32,
}

unsafe extern "C" fn v1_init(ctx: *mut RustScriptContext) -> i32 {
    let state = Box::new(State { speed: 10.0, ticks: 0 });
    // SAFETY: Initializing user_data on valid context.
    unsafe {
        (*ctx).user_data = Box::into_raw(state) as *mut ::std::ffi::c_void;
    }
    0
}

unsafe extern "C" fn v1_update(ctx: *mut RustScriptContext, _dt: f64) -> i32 {
    // SAFETY: Dereferencing valid State user_data.
    unsafe {
        let s = &mut *((*ctx).user_data as *mut State);
        s.ticks += 1;
    }
    0
}

unsafe extern "C" fn v1_set_prop(ctx: *mut RustScriptContext, key: *const ::std::ffi::c_char, val_json: *const ::std::ffi::c_char) -> i32 {
    // SAFETY: key and val_json are null-terminated strings.
    unsafe {
        let k = ::std::ffi::CStr::from_ptr(key).to_str().unwrap();
        let v = ::std::ffi::CStr::from_ptr(val_json).to_str().unwrap();
        if k == "speed" {
            let s = &mut *((*ctx).user_data as *mut State);
            if let Ok(num) = v.parse::<f64>() {
                s.speed = num;
            }
        }
    }
    0
}

unsafe extern "C" fn v1_get_prop(ctx: *const RustScriptContext, key: *const ::std::ffi::c_char, out_buf: *mut ::std::ffi::c_char, max_len: usize) -> i32 {
    // SAFETY: key and out_buf are valid.
    unsafe {
        let k = ::std::ffi::CStr::from_ptr(key).to_str().unwrap();
        let s = &*((*ctx).user_data as *const State);
        if k == "speed" {
            let json = format!("{}", s.speed);
            let b = json.as_bytes();
            if b.len() + 1 > max_len { return -2; }
            ::std::ptr::copy_nonoverlapping(b.as_ptr(), out_buf as *mut u8, b.len());
            *out_buf.add(b.len()) = 0;
            return b.len() as i32;
        }
    }
    -1
}

unsafe extern "C" fn v1_unload(ctx: *mut RustScriptContext) -> i32 {
    // SAFETY: Freeing user_data on unload.
    unsafe {
        if !(*ctx).user_data.is_null() {
            let _ = Box::from_raw((*ctx).user_data as *mut State);
            (*ctx).user_data = ::std::ptr::null_mut();
        }
    }
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn foreplay_rust_plugin_entry() -> ForeplayRustPluginVTable {
    ForeplayRustPluginVTable {
        abi_version: 1,
        descriptor: v1_desc,
        on_init: Some(v1_init),
        on_ready: None,
        on_update: Some(v1_update),
        on_physics_update: None,
        on_unload: Some(v1_unload),
        get_property_json: Some(v1_get_prop),
        set_property_json: Some(v1_set_prop),
    }
}
"#;

        // Compile V1 cdylib
        let v1_dylib = compile_rust_plugin_source_to_cdylib(v1_src, &temp_dir, "skater_v1")
            .expect("V1 cdylib compilation must succeed");
        assert!(v1_dylib.exists());

        // Create host and load V1
        let mut host = RustHotReloadHost::new("skater_hotreload", Some(temp_dir.clone()));
        let report_v1 = host.load_from_path(&v1_dylib).expect("load V1 dylib");
        assert_eq!(report_v1.generation, 1);

        // Bind node handle and verify initial property
        let node_id = NodeId::new(42, 1).unwrap();
        host.bind_node(node_id, None);
        assert_eq!(host.get_property(node_id, "speed"), Some(IrValue::F64(10.0)));

        // Mutate property live in V1 and step update frames
        host.set_property(node_id, "speed", IrValue::F64(14.5)).unwrap();
        assert_eq!(host.get_property(node_id, "speed"), Some(IrValue::F64(14.5)));
        host.step_update(0.016).unwrap();
        host.step_update(0.016).unwrap();

        // 2. Rust source for Plugin V2 (adding new property `turbo_boost = 2.5` and doubling tick speed)
        let v2_src = r#"
static PLUGIN_NAME: &[u8] = b"SkaterPlugin\0";
static SPEED_PROP: &[u8] = b"speed\0";
static SPEED_DEF: &[u8] = b"20.0\0";
static TURBO_PROP: &[u8] = b"turbo_boost\0";
static TURBO_DEF: &[u8] = b"2.5\0";

static V2_SCHEMAS: [RustPropertySchema; 2] = [
    RustPropertySchema {
        name: SPEED_PROP.as_ptr() as *const ::std::ffi::c_char,
        prop_type: 3, // F64
        default_json: SPEED_DEF.as_ptr() as *const ::std::ffi::c_char,
    },
    RustPropertySchema {
        name: TURBO_PROP.as_ptr() as *const ::std::ffi::c_char,
        prop_type: 3, // F64
        default_json: TURBO_DEF.as_ptr() as *const ::std::ffi::c_char,
    },
];

unsafe extern "C" fn v2_desc() -> RustPluginDescriptor {
    RustPluginDescriptor {
        abi_version: 1,
        name: PLUGIN_NAME.as_ptr() as *const ::std::ffi::c_char,
        schema_count: 2,
        schemas: V2_SCHEMAS.as_ptr(),
    }
}

struct StateV2 {
    speed: f64,
    turbo: f64,
    ticks: u32,
}

unsafe extern "C" fn v2_init(ctx: *mut RustScriptContext) -> i32 {
    let state = Box::new(StateV2 { speed: 20.0, turbo: 2.5, ticks: 0 });
    // SAFETY: Initializing user_data on valid context.
    unsafe {
        (*ctx).user_data = Box::into_raw(state) as *mut ::std::ffi::c_void;
    }
    0
}

unsafe extern "C" fn v2_update(ctx: *mut RustScriptContext, _dt: f64) -> i32 {
    // SAFETY: Dereferencing valid StateV2 user_data.
    unsafe {
        let s = &mut *((*ctx).user_data as *mut StateV2);
        s.ticks += 2; // Doubled in v2
    }
    0
}

unsafe extern "C" fn v2_set_prop(ctx: *mut RustScriptContext, key: *const ::std::ffi::c_char, val_json: *const ::std::ffi::c_char) -> i32 {
    // SAFETY: key and val_json are null-terminated strings.
    unsafe {
        let k = ::std::ffi::CStr::from_ptr(key).to_str().unwrap();
        let v = ::std::ffi::CStr::from_ptr(val_json).to_str().unwrap();
        let s = &mut *((*ctx).user_data as *mut StateV2);
        if k == "speed" {
            if let Ok(num) = v.parse::<f64>() { s.speed = num; }
        } else if k == "turbo_boost" {
            if let Ok(num) = v.parse::<f64>() { s.turbo = num; }
        }
    }
    0
}

unsafe extern "C" fn v2_get_prop(ctx: *const RustScriptContext, key: *const ::std::ffi::c_char, out_buf: *mut ::std::ffi::c_char, max_len: usize) -> i32 {
    // SAFETY: key and out_buf are valid.
    unsafe {
        let k = ::std::ffi::CStr::from_ptr(key).to_str().unwrap();
        let s = &*((*ctx).user_data as *const StateV2);
        let json = if k == "speed" {
            format!("{}", s.speed)
        } else if k == "turbo_boost" {
            format!("{}", s.turbo)
        } else {
            return -1;
        };
        let b = json.as_bytes();
        if b.len() + 1 > max_len { return -2; }
        ::std::ptr::copy_nonoverlapping(b.as_ptr(), out_buf as *mut u8, b.len());
        *out_buf.add(b.len()) = 0;
        return b.len() as i32;
    }
}

unsafe extern "C" fn v2_unload(ctx: *mut RustScriptContext) -> i32 {
    // SAFETY: Freeing user_data on unload.
    unsafe {
        if !(*ctx).user_data.is_null() {
            let _ = Box::from_raw((*ctx).user_data as *mut StateV2);
            (*ctx).user_data = ::std::ptr::null_mut();
        }
    }
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn foreplay_rust_plugin_entry() -> ForeplayRustPluginVTable {
    ForeplayRustPluginVTable {
        abi_version: 1,
        descriptor: v2_desc,
        on_init: Some(v2_init),
        on_ready: None,
        on_update: Some(v2_update),
        on_physics_update: None,
        on_unload: Some(v2_unload),
        get_property_json: Some(v2_get_prop),
        set_property_json: Some(v2_set_prop),
    }
}
"#;

        // Compile V2 cdylib
        let v2_dylib = compile_rust_plugin_source_to_cdylib(v2_src, &temp_dir, "skater_v2")
            .expect("V2 cdylib compilation must succeed");
        assert!(v2_dylib.exists());

        // Hot-reload into host in <0.5s!
        let t_start = std::time::Instant::now();
        let report_v2 = host.load_from_path(&v2_dylib).expect("hot-reload V2 dylib");
        let elapsed = t_start.elapsed();
        assert!(elapsed.as_secs_f64() < 0.5, "hot-reload took {:?}", elapsed);

        // Verify state preservation and migration report
        assert_eq!(report_v2.generation, 2);
        assert!(report_v2.preserved_keys.contains(&"speed".into()));
        assert!(report_v2.defaulted_new_keys.contains(&"turbo_boost".into()));

        // Crucial: V1 mutated speed (14.5) is preserved, not overwritten by V2 default (20.0)!
        assert_eq!(host.get_property(node_id, "speed"), Some(IrValue::F64(14.5)));
        // Brand new property is defaulted to declared schema default (2.5)
        assert_eq!(host.get_property(node_id, "turbo_boost"), Some(IrValue::F64(2.5)));

        // Step update in V2 executes cleanly
        host.step_update(0.016).unwrap();

        // Cleanup temp dir
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_compilation_error_reporting() {
        let temp_dir = std::env::temp_dir().join(format!("foreplay_err_test_{}", std::process::id()));
        let res = compile_rust_plugin_source_to_cdylib("this is invalid rust code !@#$", &temp_dir, "err_crate");
        assert!(res.is_err());
        match res.unwrap_err() {
            RustHotReloadError::CompilationFailed { stderr } => {
                assert!(stderr.contains("error:"));
            }
            other => panic!("expected CompilationFailed, got: {:?}", other),
        }
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_abi_version_mismatch_rejection() {
        let err = RustHotReloadError::AbiMismatch { expected: 1, actual: 99 };
        assert!(format!("{err}").contains("expected 1, found 99"));
    }

    #[test]
    fn test_moan_project_stem_manifest_json_roundtrip() {
        let link = MoanProjectStemLink::default_skate_soundtrack();
        let json_str = link.to_moan_json().expect("serialize moan manifest");
        let roundtrip = MoanProjectStemLink::from_moan_json(&json_str).expect("deserialize moan manifest");
        assert_eq!(link, roundtrip);
    }

    #[test]
    fn test_moan_default_skate_soundtrack_stems_and_busses() {
        let link = MoanProjectStemLink::default_skate_soundtrack();
        assert_eq!(link.bpm, 128.0);
        assert_eq!(link.stems.len(), 4);
        assert_eq!(link.dsp_bus.cutoff_hz, 20000.0);
        assert_eq!(link.quantize_grid, QuantizeGrid::Bar);
    }

    #[test]
    fn test_moan_adaptive_mix_evaluation_intensity_curves() {
        let link = MoanProjectStemLink::default_skate_soundtrack();

        // Idle state: Foley is high, Drums are silent
        let idle = link.evaluate_adaptive_mix(0.0, false, 0.0);
        assert_eq!(idle.intensity, 0.0);
        assert_eq!(idle.stem_gains.get("stem_drums"), Some(&0.0));
        assert_eq!(idle.stem_gains.get("stem_foley"), Some(&0.8)); // 1.0 * base_gain 0.8

        // Max intensity: Drums and Lead are at full volume, Foley is low
        let max_intensity = link.evaluate_adaptive_mix(1.0, false, 16.0);
        assert_eq!(max_intensity.intensity, 1.0);
        assert_eq!(max_intensity.stem_gains.get("stem_drums"), Some(&1.0));
        assert!(max_intensity.stem_gains.get("stem_foley").unwrap() < &0.3);
    }

    #[test]
    fn test_moan_adaptive_mix_combo_boost() {
        let link = MoanProjectStemLink::default_skate_soundtrack();
        let normal = link.evaluate_adaptive_mix(0.5, false, 4.0);
        let combo = link.evaluate_adaptive_mix(0.5, true, 4.0);

        // Combo boosts Lead track gain and DSP drive saturation
        assert!(combo.stem_gains.get("stem_lead").unwrap() > normal.stem_gains.get("stem_lead").unwrap());
        assert!(combo.dsp_sends.drive > normal.dsp_sends.drive);
    }

    #[test]
    fn test_moan_beat_quantized_grid_transitions() {
        let link = MoanProjectStemLink::default_skate_soundtrack();

        // Exactly on bar boundary: beat 0.0, 4.0, 8.0 (with 4 beats per bar)
        let bar_boundary = link.evaluate_adaptive_mix(0.5, false, 4.0);
        assert!(bar_boundary.is_quantized_transition_point);
        assert_eq!(bar_boundary.current_bar, 1);
        assert_eq!(bar_boundary.beat_in_bar, 0.0);

        // Mid-bar: beat 2.5
        let mid_bar = link.evaluate_adaptive_mix(0.5, false, 2.5);
        assert!(!mid_bar.is_quantized_transition_point);
        assert_eq!(mid_bar.current_bar, 0);
    }

    #[test]
    fn test_splicy_cutscene_edl_json_roundtrip() {
        let mut edl = SplicyCutsceneEdl::new("IntroCutscene", 15.0, 60.0);
        let cam_id = NodeId::new(1, 1).unwrap();
        edl.camera_shots.push(CameraShotClip {
            id: "shot_01".into(),
            camera_node: cam_id,
            start_time_sec: 0.0,
            end_time_sec: 5.0,
            focal_length_mm: 35.0,
            aperture_f_stop: 2.8,
            dolly_orbit_keyframes: vec![CameraKeyframe {
                time_offset_sec: 0.0,
                position: [0.0, 1.8, 5.0],
                rotation_quat: [0.0, 0.0, 0.0, 1.0],
                fov_degrees: 54.0,
            }],
        });
        edl.audio_clips.push(AudioTrackClip {
            id: "skate_grind_audio".into(),
            asset_path: "res://audio/sfx/grind.wav".into(),
            clip_start_sec: 1.0,
            clip_end_sec: 4.0,
            audio_lead_in_sec: 0.5,
            audio_tail_out_sec: 0.8,
            base_volume: 0.9,
        });

        let json = edl.to_splicy_json().expect("serialize splicy edl");
        let deser = SplicyCutsceneEdl::from_splicy_json(&json).expect("deserialize splicy edl");
        assert_eq!(edl, deser);
    }

    #[test]
    fn test_splicy_camera_shot_interpolation_and_dolly() {
        let mut edl = SplicyCutsceneEdl::new("OrbitShot", 10.0, 60.0);
        let cam_id = NodeId::new(2, 1).unwrap();
        edl.camera_shots.push(CameraShotClip {
            id: "shot_orbit".into(),
            camera_node: cam_id,
            start_time_sec: 0.0,
            end_time_sec: 10.0,
            focal_length_mm: 50.0,
            aperture_f_stop: 1.8,
            dolly_orbit_keyframes: vec![
                CameraKeyframe {
                    time_offset_sec: 0.0,
                    position: [0.0, 2.0, 10.0],
                    rotation_quat: [0.0, 0.0, 0.0, 1.0],
                    fov_degrees: 45.0,
                },
                CameraKeyframe {
                    time_offset_sec: 10.0,
                    position: [10.0, 2.0, 0.0],
                    rotation_quat: [0.0, std::f32::consts::FRAC_1_SQRT_2, 0.0, std::f32::consts::FRAC_1_SQRT_2],
                    fov_degrees: 60.0,
                },
            ],
        });

        // Evaluate halfway at t = 5.0s
        let snapshot = edl.evaluate_at(5.0);
        assert!(snapshot.active_camera.is_some());
        let cam = snapshot.active_camera.unwrap();
        assert_eq!(cam.shot_id, "shot_orbit");
        assert!((cam.position[0] - 5.0).abs() < 1e-4);
        assert!((cam.position[2] - 5.0).abs() < 1e-4);
        assert!((cam.fov_degrees - 52.5).abs() < 1e-4);
    }

    #[test]
    fn test_splicy_fcpx_j_cut_and_l_cut_audio_split_offsets() {
        let mut edl = SplicyCutsceneEdl::new("SplitEditCutscene", 20.0, 60.0);
        edl.audio_clips.push(AudioTrackClip {
            id: "dialogue_clip".into(),
            asset_path: "res://audio/dialogue.wav".into(),
            clip_start_sec: 10.0,
            clip_end_sec: 15.0,
            audio_lead_in_sec: 2.0, // J-cut: starts at 8.0s
            audio_tail_out_sec: 3.0, // L-cut: ends at 18.0s
            base_volume: 1.0,
        });

        // At t = 9.0s: in J-cut lead-in!
        let snap_j = edl.evaluate_at(9.0);
        assert_eq!(snap_j.active_audio.len(), 1);
        assert!(snap_j.active_audio[0].in_j_cut_lead);
        assert!(!snap_j.active_audio[0].in_l_cut_tail);
        assert!(snap_j.active_audio[0].effective_volume > 0.0 && snap_j.active_audio[0].effective_volume < 1.0);

        // At t = 12.0s: in main shot
        let snap_main = edl.evaluate_at(12.0);
        assert!(!snap_main.active_audio[0].in_j_cut_lead);
        assert!(!snap_main.active_audio[0].in_l_cut_tail);
        assert_eq!(snap_main.active_audio[0].effective_volume, 1.0);

        // At t = 16.5s: in L-cut tail-out!
        let snap_l = edl.evaluate_at(16.5);
        assert!(snap_l.active_audio[0].in_l_cut_tail);
        assert!(!snap_l.active_audio[0].in_j_cut_lead);
        assert!(snap_l.active_audio[0].effective_volume > 0.0 && snap_l.active_audio[0].effective_volume < 1.0);

        // At t = 19.0s: audio clip has finished
        let snap_out = edl.evaluate_at(19.0);
        assert!(snap_out.active_audio.is_empty());
    }

    #[test]
    fn test_splicy_signal_trigger_markers_evaluation() {
        let mut edl = SplicyCutsceneEdl::new("MarkerTimeline", 10.0, 60.0);
        let target_node = NodeId::new(5, 1).unwrap();
        edl.signal_markers.push(SignalTriggerMarker {
            id: "m1".into(),
            timestamp_sec: 2.5,
            target_node,
            signal_name: "play_impact_vfx".into(),
            payload: Some(IrValue::F64(1.0)),
        });

        // Triggered exactly at 2.5s
        let snap = edl.evaluate_at(2.5);
        assert_eq!(snap.triggered_markers.len(), 1);
        assert_eq!(snap.triggered_markers[0].signal_name, "play_impact_vfx");

        // Not triggered at 1.0s
        let snap_miss = edl.evaluate_at(1.0);
        assert!(snap_miss.triggered_markers.is_empty());
    }

    #[test]
    fn test_splicy_subtitle_scrub_progress() {
        let mut edl = SplicyCutsceneEdl::new("SubTimeline", 10.0, 60.0);
        edl.subtitle_tracks.push(SubtitleTrack {
            language_code: "en".into(),
            entries: vec![SubtitleEntry {
                start_sec: 1.0,
                end_sec: 3.0,
                speaker: "Coach Frank".into(),
                text: "Do a kickflip!".into(),
            }],
        });

        // Halfway through subtitle at 2.0s
        let snap = edl.evaluate_at(2.0);
        assert_eq!(snap.active_subtitles.len(), 1);
        assert_eq!(snap.active_subtitles[0].speaker, "Coach Frank");
        assert_eq!(snap.active_subtitles[0].text, "Do a kickflip!");
        assert!((snap.active_subtitles[0].progress - 0.5).abs() < 1e-3);

        // Out of range at 4.0s
        let snap_out = edl.evaluate_at(4.0);
        assert!(snap_out.active_subtitles.is_empty());
    }

    #[test]
    fn test_splicy_to_and_from_editor_commands() {
        let mut edl = SplicyCutsceneEdl::new("BossCutscene", 12.0, 30.0);
        let cam_node = NodeId::new(3, 1).unwrap();
        let target_node = NodeId::new(4, 1).unwrap();
        let root_node = NodeId::new(1, 1).unwrap();

        edl.camera_shots.push(CameraShotClip {
            id: "boss_shot".into(),
            camera_node: cam_node,
            start_time_sec: 0.0,
            end_time_sec: 12.0,
            focal_length_mm: 85.0,
            aperture_f_stop: 1.4,
            dolly_orbit_keyframes: vec![],
        });
        edl.signal_markers.push(SignalTriggerMarker {
            id: "m_roar".into(),
            timestamp_sec: 3.0,
            target_node,
            signal_name: "boss_roar".into(),
            payload: None,
        });

        let cmds = edl.to_editor_commands(root_node);
        assert!(cmds.len() >= 5);

        let parsed_edl = SplicyCutsceneEdl::from_editor_commands(root_node, &cmds).unwrap();
        assert_eq!(parsed_edl.name, "BossCutscene");
        assert_eq!(parsed_edl.duration_sec, 12.0);
        assert_eq!(parsed_edl.frame_rate, 30.0);
        assert_eq!(parsed_edl.signal_markers.len(), 1);
        assert_eq!(parsed_edl.signal_markers[0].target_node, target_node);
    }

    #[test]
    fn test_script_registry_native_rust_and_ts_coexistence() {
        let mut reg = ScriptRegistry::new();
        let id_rust = NodeId::new(100, 1).unwrap();
        let id_ts = NodeId::new(101, 1).unwrap();

        reg.attach_script(id_rust, ScriptFrontend::NativeRustCrate, "res://scripts/skater.rs");
        reg.attach_script(id_ts, ScriptFrontend::QuickJsTypeScript, "res://scripts/ui.ts");

        assert_eq!(reg.attachment_count(), 2);
        assert_eq!(reg.get_attachment(id_rust).unwrap().frontend, ScriptFrontend::NativeRustCrate);
        assert_eq!(reg.get_attachment(id_ts).unwrap().frontend, ScriptFrontend::QuickJsTypeScript);
    }
}
