//! Native Rust dynamic library (`.dylib`/`.so`/`.dll`) hot-reload host.
//!
//! Provides a stable `#[repr(C)]` C-ABI plugin contract avoiding unstable `dyn Trait` ABIs
//! across compiler invocations, generation-stamped shadow artifacts preventing OS dynamic
//! loader cache staleness, state-preserving `IrValue` property migration across hot-reloads,
//! and invocation of `rustc` for runtime compilation.

use foreplay_ir::{IrValue, NodeId};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{CStr, CString, c_char, c_void};
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Stable C-ABI version for Foreplay Rust script dynamic plugins.
pub const FOREPLAY_RUST_SCRIPT_ABI_VERSION: u32 = 1;

/// Entry symbol exported by plugin dynamic libraries.
pub const FOREPLAY_PLUGIN_ENTRY_SYMBOL: &[u8] = b"foreplay_rust_plugin_entry\0";

/// Supported property types matching `foreplay_ir::IrValue` variants across the C-ABI.
#[repr(u32)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RustPropertyType {
    Nil = 0,
    Bool = 1,
    I64 = 2,
    F64 = 3,
    String = 4,
    Vec2 = 5,
    Vec3 = 6,
    Quat = 7,
    Color = 8,
    NodePath = 9,
    ResourcePath = 10,
    Array = 11,
    Map = 12,
}

impl RustPropertyType {
    pub const fn from_u32(val: u32) -> Option<Self> {
        match val {
            0 => Some(Self::Nil),
            1 => Some(Self::Bool),
            2 => Some(Self::I64),
            3 => Some(Self::F64),
            4 => Some(Self::String),
            5 => Some(Self::Vec2),
            6 => Some(Self::Vec3),
            7 => Some(Self::Quat),
            8 => Some(Self::Color),
            9 => Some(Self::NodePath),
            10 => Some(Self::ResourcePath),
            11 => Some(Self::Array),
            12 => Some(Self::Map),
            _ => None,
        }
    }

    pub fn matches_ir_value(&self, val: &IrValue) -> bool {
        matches!(
            (self, val),
            (Self::Nil, IrValue::Nil)
                | (Self::Bool, IrValue::Bool(_))
                | (Self::I64, IrValue::I64(_))
                | (Self::F64, IrValue::F64(_))
                | (Self::String, IrValue::String(_))
                | (Self::Vec2, IrValue::Vec2(_))
                | (Self::Vec3, IrValue::Vec3(_))
                | (Self::Quat, IrValue::Quat(_))
                | (Self::Color, IrValue::Color(_))
                | (Self::NodePath, IrValue::NodePath(_))
                | (Self::ResourcePath, IrValue::ResourcePath(_))
                | (Self::Array, IrValue::Array(_))
                | (Self::Map, IrValue::Map(_))
        )
    }
}

/// Coerces an existing `IrValue` to match the target `RustPropertyType` if clean coercion is possible.
pub fn coerce_ir_value(val: &IrValue, target: RustPropertyType) -> Option<IrValue> {
    if target.matches_ir_value(val) {
        return Some(val.clone());
    }
    match (val, target) {
        (IrValue::I64(v), RustPropertyType::F64) => Some(IrValue::F64(*v as f64)),
        (IrValue::F64(v), RustPropertyType::I64) => Some(IrValue::I64(*v as i64)),
        (IrValue::Bool(b), RustPropertyType::I64) => Some(IrValue::I64(if *b { 1 } else { 0 })),
        (IrValue::I64(v), RustPropertyType::Bool) => Some(IrValue::Bool(*v != 0)),
        _ => None,
    }
}

/// Converts an `IrValue` to a compact, C-friendly JSON string (e.g. `14.5`, `"text"`, `true`).
pub fn ir_value_to_json_compact(val: &IrValue) -> String {
    match val {
        IrValue::Nil => "null".to_string(),
        IrValue::Bool(b) => b.to_string(),
        IrValue::I64(i) => i.to_string(),
        IrValue::F64(f) => {
            if f.fract() == 0.0 {
                format!("{f:.1}")
            } else {
                format!("{f}")
            }
        }
        IrValue::String(s) => serde_json::to_string(s).unwrap_or_else(|_| format!("\"{s}\"")),
        IrValue::Vec2([x, y]) => format!("[{x}, {y}]"),
        IrValue::Vec3([x, y, z]) => format!("[{x}, {y}, {z}]"),
        IrValue::Quat([x, y, z, w]) => format!("[{x}, {y}, {z}, {w}]"),
        IrValue::Color([r, g, b, a]) => format!("[{r}, {g}, {b}, {a}]"),
        IrValue::NodePath(p) | IrValue::ResourcePath(p) => {
            serde_json::to_string(p).unwrap_or_else(|_| format!("\"{p}\""))
        }
        IrValue::Array(_) | IrValue::Map(_) => {
            serde_json::to_string(val).unwrap_or_else(|_| "{}".into())
        }
    }
}

/// Parses an `IrValue` from either standard JSON or enum-tagged JSON, guided by optional expected property type.
pub fn parse_ir_value_from_json(json_str: &str, expected_type: Option<RustPropertyType>) -> IrValue {
    let trimmed = json_str.trim();
    if trimmed.is_empty() || trimmed == "null" {
        return IrValue::Nil;
    }

    // Try direct enum-variant deserialization first
    if let Ok(val) = serde_json::from_str::<IrValue>(trimmed) {
        return val;
    }

    // Fall back to untagged JSON value parsing
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) {
        match val {
            serde_json::Value::Null => IrValue::Nil,
            serde_json::Value::Bool(b) => IrValue::Bool(b),
            serde_json::Value::Number(n) => {
                if let Some(expected) = expected_type {
                    match expected {
                        RustPropertyType::I64 => {
                            if let Some(i) = n.as_i64() {
                                return IrValue::I64(i);
                            }
                        }
                        RustPropertyType::F64 => {
                            if let Some(f) = n.as_f64() {
                                return IrValue::F64(f);
                            }
                        }
                        _ => {}
                    }
                }
                if let Some(f) = n.as_f64() {
                    IrValue::F64(f)
                } else if let Some(i) = n.as_i64() {
                    IrValue::I64(i)
                } else {
                    IrValue::Nil
                }
            }
            serde_json::Value::String(s) => IrValue::String(s),
            serde_json::Value::Array(arr) => {
                let items: Vec<IrValue> = arr
                    .into_iter()
                    .map(|v| {
                        let sub = v.to_string();
                        parse_ir_value_from_json(&sub, None)
                    })
                    .collect();
                IrValue::Array(items)
            }
            serde_json::Value::Object(map) => {
                let mut btree = BTreeMap::new();
                for (k, v) in map {
                    let sub = v.to_string();
                    btree.insert(k, parse_ir_value_from_json(&sub, None));
                }
                IrValue::Map(btree)
            }
        }
    } else {
        IrValue::Nil
    }
}

/// C-ABI property schema description exported by a native Rust plugin.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct RustPropertySchema {
    pub name: *const c_char,
    pub prop_type: u32,
    pub default_json: *const c_char,
}

// SAFETY: RustPropertySchema contains raw C string pointers intended for static descriptor arrays across C-ABI.
unsafe impl Send for RustPropertySchema {}
// SAFETY: RustPropertySchema is read-only schema metadata and safe to share across threads.
unsafe impl Sync for RustPropertySchema {}

/// C-ABI plugin descriptor containing ABI version, name, and exported property schemas.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct RustPluginDescriptor {
    pub abi_version: u32,
    pub name: *const c_char,
    pub schema_count: usize,
    pub schemas: *const RustPropertySchema,
}

// SAFETY: RustPluginDescriptor contains raw C pointers intended for static export across C-ABI.
unsafe impl Send for RustPluginDescriptor {}
// SAFETY: RustPluginDescriptor is read-only plugin descriptor metadata and safe to share across threads.
unsafe impl Sync for RustPluginDescriptor {}

/// C-ABI per-node execution context passed to script lifecycle callbacks.
#[repr(C)]
#[derive(Debug)]
pub struct RustScriptContext {
    pub node_index: u32,
    pub node_generation: u32,
    pub user_data: *mut c_void,
}

// SAFETY: RustScriptContext is a C-ABI container for passing node handle and arbitrary user instance state across the plugin boundary.
unsafe impl Send for RustScriptContext {}
// SAFETY: RustScriptContext does not provide interior mutability without synchronization.
unsafe impl Sync for RustScriptContext {}

impl RustScriptContext {
    pub fn new(node_id: NodeId) -> Self {
        Self {
            node_index: node_id.index,
            node_generation: node_id.generation.get(),
            user_data: std::ptr::null_mut(),
        }
    }

    pub fn node_id(&self) -> Option<NodeId> {
        NodeId::new(self.node_index, self.node_generation)
    }
}

/// Stable C-ABI virtual method table exported via `foreplay_rust_plugin_entry`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ForeplayRustPluginVTable {
    pub abi_version: u32,
    pub descriptor: unsafe extern "C" fn() -> RustPluginDescriptor,
    pub on_init: Option<unsafe extern "C" fn(ctx: *mut RustScriptContext) -> i32>,
    pub on_ready: Option<unsafe extern "C" fn(ctx: *mut RustScriptContext) -> i32>,
    pub on_update: Option<unsafe extern "C" fn(ctx: *mut RustScriptContext, dt: f64) -> i32>,
    pub on_physics_update: Option<unsafe extern "C" fn(ctx: *mut RustScriptContext, dt: f64) -> i32>,
    pub on_unload: Option<unsafe extern "C" fn(ctx: *mut RustScriptContext) -> i32>,
    pub get_property_json: Option<unsafe extern "C" fn(ctx: *const RustScriptContext, key: *const c_char, out_buf: *mut c_char, max_len: usize) -> i32>,
    pub set_property_json: Option<unsafe extern "C" fn(ctx: *mut RustScriptContext, key: *const c_char, val_json: *const c_char) -> i32>,
}

// SAFETY: ForeplayRustPluginVTable contains raw function pointers intended for static export across C-ABI.
unsafe impl Send for ForeplayRustPluginVTable {}
// SAFETY: ForeplayRustPluginVTable function pointers are immutable and safe to share across threads.
unsafe impl Sync for ForeplayRustPluginVTable {}

/// Entry point function signature expected in `.dylib`/`.so`/`.dll` plugins.
pub type PluginEntryFn = unsafe extern "C" fn() -> ForeplayRustPluginVTable;

/// Safe, parsed representation of a plugin property schema.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct ParsedPropertySchema {
    pub name: String,
    pub prop_type: RustPropertyType,
    pub default_value: IrValue,
}

/// Report documenting preserved, defaulted, and archived properties across a hot-reload.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct MigrationReport {
    pub preserved_keys: Vec<String>,
    pub defaulted_new_keys: Vec<String>,
    pub archived_removed_keys: BTreeMap<String, IrValue>,
    pub generation: u64,
}

/// Errors occurring during dynamic library compilation, loading, or hot-reload.
#[derive(Debug, Error)]
pub enum RustHotReloadError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("dynamic library loading error: {0}")]
    LibLoading(#[from] libloading::Error),
    #[error("ABI mismatch: expected {expected}, found {actual}")]
    AbiMismatch { expected: u32, actual: u32 },
    #[error("entry symbol '{name}' not found: {details}")]
    EntrySymbolNotFound { name: String, details: String },
    #[error("invalid descriptor: {0}")]
    InvalidDescriptor(String),
    #[error("rustc compilation failed: {stderr}")]
    CompilationFailed { stderr: String },
    #[error("JSON serialization error: {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("node not found: {0:?}")]
    NodeNotFound(NodeId),
    #[error("property error: {0}")]
    PropertyError(String),
}

/// Internal tracked instance for a bound node.
#[derive(Debug)]
pub struct PluginInstance {
    pub node_id: NodeId,
    pub context: RustScriptContext,
    pub properties: BTreeMap<String, IrValue>,
    pub archived_properties: BTreeMap<String, IrValue>,
}

/// Native dynamic library hot-reload host.
///
/// Loads compiled native dynamic libraries into unique generation-stamped shadow files,
/// preserves component state across code updates, and dispatches lifecycle events.
#[derive(Debug)]
pub struct RustHotReloadHost {
    plugin_name: String,
    temp_dir: PathBuf,
    generation: u64,
    active_shadow_path: Option<PathBuf>,
    library: Option<libloading::Library>,
    vtable: Option<ForeplayRustPluginVTable>,
    schemas: Vec<ParsedPropertySchema>,
    instances: BTreeMap<NodeId, PluginInstance>,
    migration_history: Vec<MigrationReport>,
}

impl RustHotReloadHost {
    /// Creates a new hot-reload host for the given plugin name.
    pub fn new(plugin_name: impl Into<String>, temp_dir: Option<PathBuf>) -> Self {
        let name = plugin_name.into();
        let dir = temp_dir.unwrap_or_else(std::env::temp_dir);
        Self {
            plugin_name: name,
            temp_dir: dir,
            generation: 0,
            active_shadow_path: None,
            library: None,
            vtable: None,
            schemas: Vec::new(),
            instances: BTreeMap::new(),
            migration_history: Vec::new(),
        }
    }

    pub fn plugin_name(&self) -> &str {
        &self.plugin_name
    }

    pub fn current_generation(&self) -> u64 {
        self.generation
    }

    pub fn active_shadow_path(&self) -> Option<&Path> {
        self.active_shadow_path.as_deref()
    }

    pub fn schemas(&self) -> &[ParsedPropertySchema] {
        &self.schemas
    }

    pub fn migration_history(&self) -> &[MigrationReport] {
        &self.migration_history
    }

    pub fn bound_nodes(&self) -> Vec<NodeId> {
        self.instances.keys().copied().collect()
    }

    /// Binds a `NodeId` to this plugin host with optional initial properties.
    pub fn bind_node(
        &mut self,
        node_id: NodeId,
        initial_properties: Option<BTreeMap<String, IrValue>>,
    ) {
        let mut properties = BTreeMap::new();
        // Initialize from schemas
        for schema in &self.schemas {
            properties.insert(schema.name.clone(), schema.default_value.clone());
        }
        // Override with initial properties
        if let Some(initial) = initial_properties {
            for (k, v) in initial {
                properties.insert(k, v);
            }
        }

        let mut instance = PluginInstance {
            node_id,
            context: RustScriptContext::new(node_id),
            properties,
            archived_properties: BTreeMap::new(),
        };

        // If library is currently loaded, initialize the context
        if let Some(vtable) = self.vtable {
            if let Some(on_init) = vtable.on_init {
                // SAFETY: Calling on_init callback on a newly created valid instance context.
                unsafe {
                    let _ = on_init(&mut instance.context);
                }
            }
            if let Some(set_prop) = vtable.set_property_json {
                for (k, v) in &instance.properties {
                    let json_str = ir_value_to_json_compact(v);
                    if let (Ok(c_key), Ok(c_val)) = (CString::new(k.as_str()), CString::new(json_str)) {
                        // SAFETY: Passing valid null-terminated strings and context to set_property_json.
                        unsafe {
                            let _ = set_prop(&mut instance.context, c_key.as_ptr(), c_val.as_ptr());
                        }
                    }
                }
            }
            if let Some(on_ready) = vtable.on_ready {
                // SAFETY: Calling on_ready callback on an initialized valid instance context.
                unsafe {
                    let _ = on_ready(&mut instance.context);
                }
            }
        }

        self.instances.insert(node_id, instance);
    }

    /// Unbinds a `NodeId` from this host, returning its active properties.
    pub fn unbind_node(&mut self, node_id: NodeId) -> Option<BTreeMap<String, IrValue>> {
        let mut instance = self.instances.remove(&node_id)?;
        if let Some(vtable) = self.vtable
            && let Some(on_unload) = vtable.on_unload
        {
            // SAFETY: Calling on_unload on an unbinding instance context.
            unsafe {
                let _ = on_unload(&mut instance.context);
            }
        }
        Some(instance.properties)
    }

    /// Loads or reloads a dynamic library artifact from `source_dylib_path`.
    ///
    /// Copies the artifact to a unique generation-stamped shadow file, snapshots
    /// active instance properties, unloads generation N, loads generation N + 1,
    /// performs state-preserving schema migration, and re-initializes instances.
    pub fn load_from_path(
        &mut self,
        source_dylib_path: &Path,
    ) -> Result<MigrationReport, RustHotReloadError> {
        // 1. Snapshot properties from currently running plugin instances
        self.snapshot_instances_from_vtable();

        // 2. Unload active plugin generation
        self.unload_active_plugin();

        // 3. Increment generation and formulate unique shadow path
        self.generation += 1;
        let ext = std::env::consts::DLL_EXTENSION;
        let shadow_name = format!(
            "foreplay_hotreload_{}_{}.{}",
            self.plugin_name, self.generation, ext
        );
        let shadow_path = self.temp_dir.join(shadow_name);
        std::fs::copy(source_dylib_path, &shadow_path)?;

        // 4. Load the library from the shadow path
        // SAFETY: Loading a verified dynamic library from the shadow directory.
        let lib = unsafe { libloading::Library::new(&shadow_path) }?;

        // 5. Query the entry symbol and obtain the vtable
        // SAFETY: Resolving the foreplay_rust_plugin_entry entry symbol from the loaded library.
        let entry: libloading::Symbol<PluginEntryFn> =
            unsafe { lib.get(FOREPLAY_PLUGIN_ENTRY_SYMBOL) }?;
        // SAFETY: Invoking entry symbol function to retrieve the plugin's static vtable.
        let vtable = unsafe { entry() };

        if vtable.abi_version != FOREPLAY_RUST_SCRIPT_ABI_VERSION {
            return Err(RustHotReloadError::AbiMismatch {
                expected: FOREPLAY_RUST_SCRIPT_ABI_VERSION,
                actual: vtable.abi_version,
            });
        }

        // 6. Query descriptor and parse schemas
        // SAFETY: Calling descriptor callback pointer from the verified vtable.
        let desc = unsafe { (vtable.descriptor)() };
        if desc.abi_version != FOREPLAY_RUST_SCRIPT_ABI_VERSION {
            return Err(RustHotReloadError::AbiMismatch {
                expected: FOREPLAY_RUST_SCRIPT_ABI_VERSION,
                actual: desc.abi_version,
            });
        }

        let new_schemas = parse_descriptor_schemas(&desc)?;
        self.schemas = new_schemas.clone();

        // 7. Perform state-preserving migration on all bound instances
        let mut report = MigrationReport {
            preserved_keys: Vec::new(),
            defaulted_new_keys: Vec::new(),
            archived_removed_keys: BTreeMap::new(),
            generation: self.generation,
        };

        let mut preserved_set = BTreeSet::new();
        let mut defaulted_set = BTreeSet::new();

        for instance in self.instances.values_mut() {
            let old_props = std::mem::take(&mut instance.properties);
            let mut migrated_props = BTreeMap::new();

            // Match against new schemas
            for schema in &new_schemas {
                if let Some(old_val) = old_props.get(&schema.name) {
                    if let Some(coerced) = coerce_ir_value(old_val, schema.prop_type) {
                        migrated_props.insert(schema.name.clone(), coerced);
                        preserved_set.insert(schema.name.clone());
                    } else {
                        // Incompatible type: default to schema default and archive old value
                        migrated_props.insert(schema.name.clone(), schema.default_value.clone());
                        instance
                            .archived_properties
                            .insert(schema.name.clone(), old_val.clone());
                        report
                            .archived_removed_keys
                            .insert(schema.name.clone(), old_val.clone());
                        defaulted_set.insert(schema.name.clone());
                    }
                } else {
                    // New property added in this generation
                    migrated_props.insert(schema.name.clone(), schema.default_value.clone());
                    defaulted_set.insert(schema.name.clone());
                }
            }

            // Archive dropped properties
            for (old_k, old_v) in old_props {
                if !new_schemas.iter().any(|s| s.name == old_k) {
                    instance
                        .archived_properties
                        .insert(old_k.clone(), old_v.clone());
                    report.archived_removed_keys.insert(old_k, old_v);
                }
            }

            instance.properties = migrated_props;
        }

        report.preserved_keys = preserved_set.into_iter().collect();
        report.defaulted_new_keys = defaulted_set.into_iter().collect();
        self.migration_history.push(report.clone());

        // 8. Re-initialize instances with new vtable
        for instance in self.instances.values_mut() {
            if let Some(on_init) = vtable.on_init {
                // SAFETY: Calling on_init for migrated instance context with new vtable.
                unsafe {
                    let _ = on_init(&mut instance.context);
                }
            }
            if let Some(set_prop) = vtable.set_property_json {
                for (k, v) in &instance.properties {
                    let json_str = ir_value_to_json_compact(v);
                    if let (Ok(c_key), Ok(c_val)) = (CString::new(k.as_str()), CString::new(json_str)) {
                        // SAFETY: Passing migrated property name and JSON value to plugin vtable.
                        unsafe {
                            let _ = set_prop(&mut instance.context, c_key.as_ptr(), c_val.as_ptr());
                        }
                    }
                }
            }
            if let Some(on_ready) = vtable.on_ready {
                // SAFETY: Calling on_ready on migrated instance context with new vtable.
                unsafe {
                    let _ = on_ready(&mut instance.context);
                }
            }
        }

        self.library = Some(lib);
        self.vtable = Some(vtable);
        self.active_shadow_path = Some(shadow_path);

        Ok(report)
    }

    /// Queries the current value of an exported property.
    pub fn get_property(&self, node_id: NodeId, key: &str) -> Option<IrValue> {
        let instance = self.instances.get(&node_id)?;
        if let Some(vtable) = self.vtable
            && let Some(get_prop) = vtable.get_property_json
            && let Ok(c_key) = CString::new(key)
        {
            let mut buf = vec![0u8; 4096];
            // SAFETY: Invoking get_property_json on live instance context with valid buffer.
            let res = unsafe {
                get_prop(
                    &instance.context as *const RustScriptContext,
                    c_key.as_ptr(),
                    buf.as_mut_ptr() as *mut c_char,
                    buf.len(),
                )
            };
            if res >= 0 {
                // SAFETY: Buffer contains null-terminated JSON written by plugin.
                let cstr = unsafe { CStr::from_ptr(buf.as_ptr() as *const c_char) };
                if let Ok(json_str) = cstr.to_str() {
                    let expected_type = self.schemas.iter().find(|s| s.name == key).map(|s| s.prop_type);
                    return Some(parse_ir_value_from_json(json_str, expected_type));
                }
            }
        }
        instance.properties.get(key).cloned()
    }

    /// Updates an exported property value on a bound node handle.
    pub fn set_property(
        &mut self,
        node_id: NodeId,
        key: &str,
        value: IrValue,
    ) -> Result<(), RustHotReloadError> {
        let instance = self
            .instances
            .get_mut(&node_id)
            .ok_or(RustHotReloadError::NodeNotFound(node_id))?;

        if let Some(vtable) = self.vtable
            && let Some(set_prop) = vtable.set_property_json
            && let Ok(c_key) = CString::new(key)
        {
            let json_str = ir_value_to_json_compact(&value);
            if let Ok(c_val) = CString::new(json_str) {
                // SAFETY: Calling set_property_json on live instance context.
                let res = unsafe {
                    set_prop(&mut instance.context, c_key.as_ptr(), c_val.as_ptr())
                };
                if res != 0 {
                    return Err(RustHotReloadError::PropertyError(format!(
                        "plugin rejected property '{key}' with code {res}"
                    )));
                }
            }
        }

        instance.properties.insert(key.to_string(), value);
        Ok(())
    }

    /// Returns all current active properties for a bound node.
    pub fn get_all_properties(&self, node_id: NodeId) -> Option<&BTreeMap<String, IrValue>> {
        self.instances.get(&node_id).map(|inst| &inst.properties)
    }

    /// Steps gameplay update (`on_update`) on all bound nodes.
    pub fn step_update(&mut self, dt: f64) -> Result<(), RustHotReloadError> {
        let Some(vtable) = self.vtable else {
            return Ok(());
        };
        let Some(on_update) = vtable.on_update else {
            return Ok(());
        };
        for instance in self.instances.values_mut() {
            // SAFETY: Invoking on_update on a valid instance context.
            let code = unsafe { on_update(&mut instance.context, dt) };
            if code != 0 {
                return Err(RustHotReloadError::PropertyError(format!(
                    "on_update failed on node {:?} with code {code}",
                    instance.node_id
                )));
            }
        }
        Ok(())
    }

    /// Steps physics update (`on_physics_update`) on all bound nodes.
    pub fn step_physics_update(&mut self, dt: f64) -> Result<(), RustHotReloadError> {
        let Some(vtable) = self.vtable else {
            return Ok(());
        };
        let Some(on_physics_update) = vtable.on_physics_update else {
            return Ok(());
        };
        for instance in self.instances.values_mut() {
            // SAFETY: Invoking on_physics_update on a valid instance context.
            let code = unsafe { on_physics_update(&mut instance.context, dt) };
            if code != 0 {
                return Err(RustHotReloadError::PropertyError(format!(
                    "on_physics_update failed on node {:?} with code {code}",
                    instance.node_id
                )));
            }
        }
        Ok(())
    }

    fn snapshot_instances_from_vtable(&mut self) {
        let Some(vtable) = self.vtable else {
            return;
        };
        let Some(get_prop) = vtable.get_property_json else {
            return;
        };

        for instance in self.instances.values_mut() {
            let keys: Vec<String> = instance.properties.keys().cloned().collect();
            for key in keys {
                if let Ok(c_key) = CString::new(key.as_str()) {
                    let mut buf = vec![0u8; 4096];
                    // SAFETY: Invoking get_property_json on live instance context with valid buffer.
                    let res = unsafe {
                        get_prop(
                            &instance.context as *const RustScriptContext,
                            c_key.as_ptr(),
                            buf.as_mut_ptr() as *mut c_char,
                            buf.len(),
                        )
                    };
                    if res >= 0 {
                        // SAFETY: Buffer contains null-terminated JSON written by plugin.
                        let cstr = unsafe { CStr::from_ptr(buf.as_ptr() as *const c_char) };
                        if let Ok(json_str) = cstr.to_str() {
                            let expected_type = self.schemas.iter().find(|s| s.name == key).map(|s| s.prop_type);
                            let val = parse_ir_value_from_json(json_str, expected_type);
                            instance.properties.insert(key, val);
                        }
                    }
                }
            }
        }
    }

    fn unload_active_plugin(&mut self) {
        if let Some(vtable) = self.vtable
            && let Some(on_unload) = vtable.on_unload
        {
            for instance in self.instances.values_mut() {
                // SAFETY: Calling on_unload on live instance context prior to library drop.
                unsafe {
                    let _ = on_unload(&mut instance.context);
                }
            }
        }

        self.library = None;
        self.vtable = None;
        self.schemas.clear();

        if let Some(shadow) = self.active_shadow_path.take() {
            let _ = std::fs::remove_file(shadow);
        }
    }
}

impl Drop for RustHotReloadHost {
    fn drop(&mut self) {
        self.unload_active_plugin();
    }
}

/// Helper to parse schemas from a C-ABI descriptor.
fn parse_descriptor_schemas(
    desc: &RustPluginDescriptor,
) -> Result<Vec<ParsedPropertySchema>, RustHotReloadError> {
    let mut schemas = Vec::with_capacity(desc.schema_count);
    if desc.schema_count == 0 || desc.schemas.is_null() {
        return Ok(schemas);
    }

    for i in 0..desc.schema_count {
        // SAFETY: Pointer offset within the range [0..schema_count] guaranteed by descriptor.
        let raw_schema = unsafe { *desc.schemas.add(i) };
        // SAFETY: raw_schema.name is null-terminated C string if valid descriptor.
        let name = unsafe { c_str_to_string(raw_schema.name) }.ok_or_else(|| {
            RustHotReloadError::InvalidDescriptor(format!("schema {i} missing valid name"))
        })?;
        let prop_type = RustPropertyType::from_u32(raw_schema.prop_type).ok_or_else(|| {
            RustHotReloadError::InvalidDescriptor(format!(
                "schema '{name}' invalid type {}",
                raw_schema.prop_type
            ))
        })?;
        // SAFETY: raw_schema.default_json is null-terminated C string if valid descriptor.
        let default_json = unsafe { c_str_to_string(raw_schema.default_json) }.unwrap_or_else(|| "null".into());
        let default_value = parse_ir_value_from_json(&default_json, Some(prop_type));

        schemas.push(ParsedPropertySchema {
            name,
            prop_type,
            default_value,
        });
    }

    Ok(schemas)
}

/// Helper to convert a raw C string pointer into a Rust String safely.
///
/// # Safety
/// If `ptr` is non-null, it must point to a valid null-terminated C string.
pub unsafe fn c_str_to_string(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    // SAFETY: The caller guarantees that `ptr` is either null or points to a valid null-terminated C string.
    let cstr = unsafe { CStr::from_ptr(ptr) };
    cstr.to_str().ok().map(|s| s.to_string())
}

/// Helper to write a Rust string slice into a raw C char buffer with null-termination.
///
/// # Safety
/// `out_buf` must be a valid pointer to a buffer of at least `max_len` bytes.
pub unsafe fn write_str_to_c_buf(src: &str, out_buf: *mut c_char, max_len: usize) -> i32 {
    if out_buf.is_null() || max_len == 0 {
        return -1;
    }
    let bytes = src.as_bytes();
    if bytes.len() + 1 > max_len {
        return -2;
    }
    // SAFETY: The caller guarantees `out_buf` has capacity of at least `max_len` bytes, and bytes.len() + 1 <= max_len.
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_buf as *mut u8, bytes.len());
        *out_buf.add(bytes.len()) = 0;
    }
    bytes.len() as i32
}

/// C-ABI boilerplate header automatically prepended to standalone Rust script sources
/// if they do not already define `ForeplayRustPluginVTable`.
pub const FOREPLAY_PLUGIN_C_ABI_SNIPPET: &str = r#"
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct RustPropertySchema {
    pub name: *const ::std::ffi::c_char,
    pub prop_type: u32,
    pub default_json: *const ::std::ffi::c_char,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct RustPluginDescriptor {
    pub abi_version: u32,
    pub name: *const ::std::ffi::c_char,
    pub schema_count: usize,
    pub schemas: *const RustPropertySchema,
}

#[repr(C)]
#[derive(Debug)]
pub struct RustScriptContext {
    pub node_index: u32,
    pub node_generation: u32,
    pub user_data: *mut ::std::ffi::c_void,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ForeplayRustPluginVTable {
    pub abi_version: u32,
    pub descriptor: unsafe extern "C" fn() -> RustPluginDescriptor,
    pub on_init: Option<unsafe extern "C" fn(ctx: *mut RustScriptContext) -> i32>,
    pub on_ready: Option<unsafe extern "C" fn(ctx: *mut RustScriptContext) -> i32>,
    pub on_update: Option<unsafe extern "C" fn(ctx: *mut RustScriptContext, dt: f64) -> i32>,
    pub on_physics_update: Option<unsafe extern "C" fn(ctx: *mut RustScriptContext, dt: f64) -> i32>,
    pub on_unload: Option<unsafe extern "C" fn(ctx: *mut RustScriptContext) -> i32>,
    pub get_property_json: Option<unsafe extern "C" fn(ctx: *const RustScriptContext, key: *const ::std::ffi::c_char, out_buf: *mut ::std::ffi::c_char, max_len: usize) -> i32>,
    pub set_property_json: Option<unsafe extern "C" fn(ctx: *mut RustScriptContext, key: *const ::std::ffi::c_char, val_json: *const ::std::ffi::c_char) -> i32>,
}

unsafe impl Send for RustPropertySchema {}
unsafe impl Sync for RustPropertySchema {}
unsafe impl Send for RustPluginDescriptor {}
unsafe impl Sync for RustPluginDescriptor {}
unsafe impl Send for RustScriptContext {}
unsafe impl Sync for RustScriptContext {}
unsafe impl Send for ForeplayRustPluginVTable {}
unsafe impl Sync for ForeplayRustPluginVTable {}
"#;

/// Compiles standalone Rust plugin source code into a native dynamic library (`.dylib`/`.so`/`.dll`).
///
/// Invokes `rustc --edition 2024 --crate-type cdylib -O` to produce an optimized native binary.
pub fn compile_rust_plugin_source_to_cdylib(
    rust_src: &str,
    out_dir: &Path,
    crate_name: &str,
) -> Result<PathBuf, RustHotReloadError> {
    std::fs::create_dir_all(out_dir)?;
    let src_file = out_dir.join(format!("{crate_name}.rs"));

    let full_src = if rust_src.contains("struct ForeplayRustPluginVTable") {
        rust_src.to_string()
    } else {
        format!("{FOREPLAY_PLUGIN_C_ABI_SNIPPET}\n{rust_src}")
    };

    std::fs::write(&src_file, full_src)?;

    let ext = std::env::consts::DLL_EXTENSION;
    let prefix = std::env::consts::DLL_PREFIX;
    let dylib_name = format!("{prefix}{crate_name}.{ext}");
    let dylib_path = out_dir.join(dylib_name);

    let output = std::process::Command::new("rustc")
        .arg("--edition")
        .arg("2024")
        .arg("--crate-type")
        .arg("cdylib")
        .arg("-O")
        .arg("--crate-name")
        .arg(crate_name)
        .arg("-o")
        .arg(&dylib_path)
        .arg(&src_file)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(RustHotReloadError::CompilationFailed { stderr });
    }

    Ok(dylib_path)
}
