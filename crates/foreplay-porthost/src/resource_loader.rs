//! Godot-Parity UID Registry & Multi-Threaded `ResourceLoader`.
//!
//! Mirrors Godot 4's `ResourceUID` (`core/io/resource_uid.cpp`) and
//! `ResourceLoader` (`core/io/resource_loader.cpp`). Provides bidirectional
//! `uid://...` <-> `res://...` table resolution, `.godot/uid_cache.txt` serialization,
//! and asynchronous multi-threaded loading with deduplicated cache for `.tscn`, `.tres`,
//! `.fscn`, and `.glb` payloads.

use crate::gltf_import::{import_glb_bytes_to_ir, GlbImportedBundle};
use crate::tres::{parse_tres, TresFile};
use crate::tscn::parse_tscn_to_ir;
use foreplay_ir::IrScene;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use thiserror::Error;

/// Generates a deterministic 64-bit FNV-1a UID string for a given `res://` path.
pub fn generate_fnv1a_uid(res_path: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in res_path.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("uid://{:016x}", hash)
}

/// Bidirectional UID registry mirroring Godot 4's `ResourceUID`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UidRegistry {
    uid_to_path: BTreeMap<String, String>,
    path_to_uid: BTreeMap<String, String>,
}

impl UidRegistry {
    /// Creates an empty UID registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Generates a deterministic FNV-1a UID for a `res://` path.
    pub fn generate_uid(&self, res_path: &str) -> String {
        generate_fnv1a_uid(res_path)
    }

    /// Registers a bidirectional mapping between `uid` and `res_path`.
    pub fn register(&mut self, uid: impl Into<String>, res_path: impl Into<String>) {
        let u = uid.into();
        let p = res_path.into();

        // If path was registered under another UID, remove it
        if let Some(old_uid) = self.path_to_uid.get(&p) {
            self.uid_to_path.remove(old_uid);
        }
        // If UID was pointing to another path, remove it
        if let Some(old_path) = self.uid_to_path.get(&u) {
            self.path_to_uid.remove(old_path);
        }

        self.path_to_uid.insert(p.clone(), u.clone());
        self.uid_to_path.insert(u, p);
    }

    /// Returns the existing UID for `res_path`, or creates and registers a new FNV-1a UID.
    pub fn ensure_uid(&mut self, res_path: &str) -> String {
        if let Some(uid) = self.path_to_uid.get(res_path) {
            return uid.clone();
        }
        let uid = self.generate_uid(res_path);
        self.register(uid.clone(), res_path);
        uid
    }

    /// Resolves a `uid://...` URI to its target `res://` path.
    pub fn get_path(&self, uid: &str) -> Option<&str> {
        self.uid_to_path.get(uid).map(String::as_str)
    }

    /// Resolves a `res://` path to its corresponding `uid://...` URI.
    pub fn get_uid(&self, res_path: &str) -> Option<&str> {
        self.path_to_uid.get(res_path).map(String::as_str)
    }

    /// Removes a mapping by `res_path`.
    pub fn remove_path(&mut self, res_path: &str) -> Option<String> {
        if let Some(uid) = self.path_to_uid.remove(res_path) {
            self.uid_to_path.remove(&uid);
            Some(uid)
        } else {
            None
        }
    }

    /// Removes a mapping by `uid`.
    pub fn remove_uid(&mut self, uid: &str) -> Option<String> {
        if let Some(path) = self.uid_to_path.remove(uid) {
            self.path_to_uid.remove(&path);
            Some(path)
        } else {
            None
        }
    }

    /// Number of registered entries.
    pub fn len(&self) -> usize {
        self.uid_to_path.len()
    }

    /// True if registry contains zero mappings.
    pub fn is_empty(&self) -> bool {
        self.uid_to_path.is_empty()
    }

    /// Loads mappings from Godot 4's `.godot/uid_cache.txt` text format.
    pub fn load_cache_text(&mut self, text: &str) {
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') {
                continue;
            }

            // Godot 4 format: res://path/to/res:::uid://...
            if let Some((path, uid)) = trimmed.split_once(":::") {
                let p = path.trim();
                let u = uid.trim();
                if !p.is_empty() && !u.is_empty() {
                    self.register(u, p);
                }
            } else if let Some((first, second)) = trimmed.split_once(char::is_whitespace) {
                // Alternative whitespace-delimited formats
                let first = first.trim();
                let second = second.trim();
                if first.starts_with("uid://") {
                    self.register(first, second);
                } else if second.starts_with("uid://") {
                    self.register(second, first);
                }
            }
        }
    }

    /// Serializes mappings to Godot 4's `.godot/uid_cache.txt` text format (sorted deterministically).
    pub fn save_cache_text(&self) -> String {
        let mut out = String::new();
        for (path, uid) in &self.path_to_uid {
            out.push_str(&format!("{path}:::{uid}\n"));
        }
        out
    }
}

/// Status of an asynchronous resource load operation (Mirrors Godot 4's `ResourceLoader::ThreadLoadStatus`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ThreadLoadStatus {
    InvalidResource = 0,
    ThreadLoadInProgress = 1,
    ThreadLoadFailed = 2,
    ThreadLoadLoaded = 3,
}

/// Polymorphic container for all resource types loaded by `ThreadedResourceLoader`.
#[derive(Clone, Debug, PartialEq)]
pub enum LoadedResource {
    Scene(IrScene),
    Tres(TresFile),
    Glb(GlbImportedBundle),
    Raw(Vec<u8>),
}

impl LoadedResource {
    /// Returns a reference to the `IrScene` if this resource is a Scene or GLB.
    pub fn as_scene(&self) -> Option<&IrScene> {
        match self {
            Self::Scene(s) => Some(s),
            Self::Glb(b) => Some(&b.scene),
            _ => None,
        }
    }

    /// Returns a reference to the `TresFile` if this resource is a `.tres` file.
    pub fn as_tres(&self) -> Option<&TresFile> {
        match self {
            Self::Tres(t) => Some(t),
            _ => None,
        }
    }

    /// Returns a reference to the `GlbImportedBundle` if this resource is a GLB.
    pub fn as_glb(&self) -> Option<&GlbImportedBundle> {
        match self {
            Self::Glb(b) => Some(b),
            _ => None,
        }
    }

    /// Returns a reference to the raw bytes if this resource is binary data.
    pub fn as_raw(&self) -> Option<&[u8]> {
        match self {
            Self::Raw(r) => Some(r),
            _ => None,
        }
    }
}

/// Errors returned by `ThreadedResourceLoader`.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ResourceLoaderError {
    #[error("resource not found: {0}")]
    NotFound(String),
    #[error("resource load still in progress: {0}")]
    StillInProgress(String),
    #[error("resource load failed for '{path}': {error}")]
    LoadFailed { path: String, error: String },
    #[error("resource load was not requested: {0}")]
    NotRequested(String),
    #[error("unresolved UID URI: {0}")]
    UnresolvedUid(String),
    #[error("decode/parse error: {0}")]
    ParseError(String),
    #[error("timeout waiting for resource: {0}")]
    Timeout(String),
}

#[derive(Debug)]
struct LoaderInner {
    uid_registry: Mutex<UidRegistry>,
    virtual_files: Mutex<BTreeMap<String, Vec<u8>>>,
    cache: Mutex<BTreeMap<String, Arc<LoadedResource>>>,
    status: Mutex<BTreeMap<String, ThreadLoadStatus>>,
    errors: Mutex<BTreeMap<String, String>>,
}

/// Multi-threaded, deduplicating resource loader mirroring Godot 4's `ResourceLoader`.
#[derive(Clone, Debug)]
pub struct ThreadedResourceLoader {
    inner: Arc<LoaderInner>,
}

impl Default for ThreadedResourceLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl ThreadedResourceLoader {
    /// Creates a new `ThreadedResourceLoader` with an empty cache and UID registry.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(LoaderInner {
                uid_registry: Mutex::new(UidRegistry::new()),
                virtual_files: Mutex::new(BTreeMap::new()),
                cache: Mutex::new(BTreeMap::new()),
                status: Mutex::new(BTreeMap::new()),
                errors: Mutex::new(BTreeMap::new()),
            }),
        }
    }

    /// Creates a loader with an initialized `UidRegistry`.
    pub fn with_uid_registry(registry: UidRegistry) -> Self {
        let loader = Self::new();
        *loader.inner.uid_registry.lock().unwrap() = registry;
        loader
    }

    /// Registers an in-memory virtual file (e.g. for testing, embedded templates, or `port://` ROM streams).
    pub fn register_virtual(&self, path: impl Into<String>, bytes: Vec<u8>) {
        let p = path.into();
        self.inner.virtual_files.lock().unwrap().insert(p, bytes);
    }

    /// Registers a UID mapping in the loader's registry.
    pub fn register_uid(&self, uid: impl Into<String>, res_path: impl Into<String>) {
        self.inner
            .uid_registry
            .lock()
            .unwrap()
            .register(uid, res_path);
    }

    /// Resolves a path, translating `uid://...` to `res://...` if necessary.
    pub fn resolve_path(&self, path: &str) -> Result<String, ResourceLoaderError> {
        if path.starts_with("uid://") {
            let reg = self.inner.uid_registry.lock().unwrap();
            let target = reg
                .get_path(path)
                .ok_or_else(|| ResourceLoaderError::UnresolvedUid(path.to_string()))?;
            Ok(target.to_string())
        } else {
            Ok(path.to_string())
        }
    }

    /// Starts asynchronous background loading of the resource at `path` (deduplicated).
    ///
    /// Mirrors Godot 4's `ResourceLoader::load_threaded_request`.
    pub fn load_threaded_request(&self, path: &str) -> Result<(), ResourceLoaderError> {
        let resolved_path = self.resolve_path(path)?;

        let mut status_map = self.inner.status.lock().unwrap();
        if let Some(&current_status) = status_map.get(path)
            && (current_status == ThreadLoadStatus::ThreadLoadLoaded
                || current_status == ThreadLoadStatus::ThreadLoadInProgress)
        {
            return Ok(());
        }

        // Check if already in cache under resolved path
        {
            let cache = self.inner.cache.lock().unwrap();
            if let Some(loaded) = cache.get(&resolved_path) {
                status_map.insert(path.to_string(), ThreadLoadStatus::ThreadLoadLoaded);
                status_map.insert(resolved_path.clone(), ThreadLoadStatus::ThreadLoadLoaded);
                if path != resolved_path {
                    let mut cache_mut = self.inner.cache.lock().unwrap();
                    cache_mut.insert(path.to_string(), loaded.clone());
                }
                return Ok(());
            }
        }

        status_map.insert(path.to_string(), ThreadLoadStatus::ThreadLoadInProgress);
        status_map.insert(resolved_path.clone(), ThreadLoadStatus::ThreadLoadInProgress);
        drop(status_map);

        let inner = self.inner.clone();
        let req_path = path.to_string();
        let target_path = resolved_path;

        std::thread::spawn(move || {
            let result = load_resource_bytes(&inner, &target_path);
            match result {
                Ok(loaded) => {
                    let arc = Arc::new(loaded);
                    let mut cache = inner.cache.lock().unwrap();
                    cache.insert(req_path.clone(), arc.clone());
                    cache.insert(target_path.clone(), arc);
                    drop(cache);

                    let mut status = inner.status.lock().unwrap();
                    status.insert(req_path, ThreadLoadStatus::ThreadLoadLoaded);
                    status.insert(target_path, ThreadLoadStatus::ThreadLoadLoaded);
                }
                Err(err) => {
                    let err_str = err.to_string();
                    let mut errors = inner.errors.lock().unwrap();
                    errors.insert(req_path.clone(), err_str.clone());
                    errors.insert(target_path.clone(), err_str);
                    drop(errors);

                    let mut status = inner.status.lock().unwrap();
                    status.insert(req_path, ThreadLoadStatus::ThreadLoadFailed);
                    status.insert(target_path, ThreadLoadStatus::ThreadLoadFailed);
                }
            }
        });

        Ok(())
    }

    /// Queries the status of an asynchronous load request.
    ///
    /// Mirrors Godot 4's `ResourceLoader::load_threaded_get_status`.
    pub fn load_threaded_get_status(
        &self,
        path: &str,
        progress: Option<&mut f32>,
    ) -> ThreadLoadStatus {
        let status_map = self.inner.status.lock().unwrap();
        let s = status_map
            .get(path)
            .copied()
            .unwrap_or(ThreadLoadStatus::InvalidResource);

        if let Some(p) = progress {
            *p = match s {
                ThreadLoadStatus::ThreadLoadLoaded => 1.0,
                ThreadLoadStatus::ThreadLoadInProgress => 0.5,
                ThreadLoadStatus::ThreadLoadFailed => 0.0,
                ThreadLoadStatus::InvalidResource => 0.0,
            };
        }

        s
    }

    /// Retrieves the loaded resource from cache once loading completes.
    ///
    /// Mirrors Godot 4's `ResourceLoader::load_threaded_get`.
    pub fn load_threaded_get(&self, path: &str) -> Result<Arc<LoadedResource>, ResourceLoaderError> {
        let status = self.load_threaded_get_status(path, None);
        match status {
            ThreadLoadStatus::ThreadLoadLoaded => {
                let cache = self.inner.cache.lock().unwrap();
                if let Some(res) = cache.get(path) {
                    Ok(res.clone())
                } else if let Ok(resolved) = self.resolve_path(path) {
                    cache
                        .get(&resolved)
                        .cloned()
                        .ok_or_else(|| ResourceLoaderError::NotFound(path.to_string()))
                } else {
                    Err(ResourceLoaderError::NotFound(path.to_string()))
                }
            }
            ThreadLoadStatus::ThreadLoadInProgress => {
                Err(ResourceLoaderError::StillInProgress(path.to_string()))
            }
            ThreadLoadStatus::ThreadLoadFailed => {
                let errs = self.inner.errors.lock().unwrap();
                let msg = errs
                    .get(path)
                    .cloned()
                    .unwrap_or_else(|| "unknown load error".to_string());
                Err(ResourceLoaderError::LoadFailed {
                    path: path.to_string(),
                    error: msg,
                })
            }
            ThreadLoadStatus::InvalidResource => {
                Err(ResourceLoaderError::NotRequested(path.to_string()))
            }
        }
    }

    /// Synchronously waits until asynchronous loading finishes or times out.
    pub fn load_threaded_wait(
        &self,
        path: &str,
        timeout: Option<Duration>,
    ) -> Result<Arc<LoadedResource>, ResourceLoaderError> {
        let current_status = self.load_threaded_get_status(path, None);
        if current_status == ThreadLoadStatus::InvalidResource {
            self.load_threaded_request(path)?;
        }

        let start = Instant::now();
        let max_duration = timeout.unwrap_or(Duration::from_secs(5));

        loop {
            let status = self.load_threaded_get_status(path, None);
            match status {
                ThreadLoadStatus::ThreadLoadLoaded | ThreadLoadStatus::ThreadLoadFailed => {
                    return self.load_threaded_get(path);
                }
                ThreadLoadStatus::ThreadLoadInProgress => {
                    if start.elapsed() > max_duration {
                        return Err(ResourceLoaderError::Timeout(path.to_string()));
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
                ThreadLoadStatus::InvalidResource => {
                    return Err(ResourceLoaderError::NotRequested(path.to_string()));
                }
            }
        }
    }

    /// Checks if a resource is already loaded in the cache.
    pub fn has_cached(&self, path: &str) -> bool {
        let cache = self.inner.cache.lock().unwrap();
        if cache.contains_key(path) {
            return true;
        }
        if let Ok(resolved) = self.resolve_path(path) {
            return cache.contains_key(&resolved);
        }
        false
    }

    /// Clears the in-memory cache and status table.
    pub fn clear_cache(&self) {
        self.inner.cache.lock().unwrap().clear();
        self.inner.status.lock().unwrap().clear();
        self.inner.errors.lock().unwrap().clear();
    }
}

fn load_resource_bytes(
    inner: &LoaderInner,
    resolved_path: &str,
) -> Result<LoadedResource, ResourceLoaderError> {
    // 1. Fetch raw bytes from virtual memory store or local disk
    let bytes = {
        let virt = inner.virtual_files.lock().unwrap();
        virt.get(resolved_path).cloned()
    };

    let raw_bytes = match bytes {
        Some(b) => b,
        None => std::fs::read(resolved_path)
            .map_err(|e| ResourceLoaderError::NotFound(format!("{resolved_path}: {e}")))?,
    };

    // 2. Determine decoder based on file extension or magic bytes
    if resolved_path.ends_with(".tscn") {
        let text = std::str::from_utf8(&raw_bytes)
            .map_err(|e| ResourceLoaderError::ParseError(e.to_string()))?;
        let ir = parse_tscn_to_ir(text)
            .map_err(|e| ResourceLoaderError::ParseError(e.to_string()))?;
        Ok(LoadedResource::Scene(ir))
    } else if resolved_path.ends_with(".fscn") {
        let text = std::str::from_utf8(&raw_bytes)
            .map_err(|e| ResourceLoaderError::ParseError(e.to_string()))?;
        let ir = IrScene::from_fscn_str(text)
            .map_err(|e| ResourceLoaderError::ParseError(e.to_string()))?;
        Ok(LoadedResource::Scene(ir))
    } else if resolved_path.ends_with(".tres") {
        let text = std::str::from_utf8(&raw_bytes)
            .map_err(|e| ResourceLoaderError::ParseError(e.to_string()))?;
        let tres = parse_tres(text)
            .map_err(|e| ResourceLoaderError::ParseError(e.to_string()))?;
        Ok(LoadedResource::Tres(tres))
    } else if resolved_path.ends_with(".glb")
        || resolved_path.ends_with(".gltf")
        || raw_bytes.starts_with(b"glTF")
    {
        let bundle = import_glb_bytes_to_ir(&raw_bytes, None)
            .map_err(|e| ResourceLoaderError::ParseError(e.to_string()))?;
        Ok(LoadedResource::Glb(bundle))
    } else {
        Ok(LoadedResource::Raw(raw_bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gltf_import::build_sample_rigged_character_glb;

    #[test]
    fn uid_registry_bidirectional_and_cache_serialization() {
        let mut reg = UidRegistry::new();
        let path1 = "res://levels/level_01.tscn";
        let uid1 = reg.ensure_uid(path1);
        assert!(uid1.starts_with("uid://"));

        assert_eq!(reg.get_path(&uid1), Some(path1));
        assert_eq!(reg.get_uid(path1), Some(uid1.as_str()));

        // Ensure idempotence
        let uid1_re = reg.ensure_uid(path1);
        assert_eq!(uid1, uid1_re);

        // Save cache
        let cache_text = reg.save_cache_text();
        assert!(cache_text.contains("res://levels/level_01.tscn:::uid://"));

        // Load into fresh registry
        let mut reg2 = UidRegistry::new();
        reg2.load_cache_text(&cache_text);
        assert_eq!(reg2.get_path(&uid1), Some(path1));
        assert_eq!(reg2.get_uid(path1), Some(uid1.as_str()));
    }

    #[test]
    fn threaded_loader_loads_tscn_tres_fscn_glb() {
        let loader = ThreadedResourceLoader::new();

        // 1. Register virtual .tscn
        let tscn = "[gd_scene format=3 uid=\"uid://test_scene\"]\n[node name=\"Root\" type=\"Node3D\"]\n";
        loader.register_virtual("res://scenes/test.tscn", tscn.as_bytes().to_vec());

        // 2. Register virtual .fscn
        let fscn = "[foreplay_scene name=\"FscnTest\" format=1]\n[node id=\"1:1\" name=\"Root\" type=\"Node3D\"]\n";
        loader.register_virtual("res://scenes/test.fscn", fscn.as_bytes().to_vec());

        // 3. Register virtual .tres
        let tres = "[gd_resource type=\"BoxShape3D\" format=3 uid=\"uid://box_shape\"]\n[resource]\nsize = Vector3(1, 2, 3)\n";
        loader.register_virtual("res://shapes/box.tres", tres.as_bytes().to_vec());

        // 4. Register virtual .glb
        let glb_bytes = build_sample_rigged_character_glb();
        loader.register_virtual("res://models/character.glb", glb_bytes);

        // Request loads
        loader.load_threaded_request("res://scenes/test.tscn").unwrap();
        loader.load_threaded_request("res://scenes/test.fscn").unwrap();
        loader.load_threaded_request("res://shapes/box.tres").unwrap();
        loader.load_threaded_request("res://models/character.glb").unwrap();

        // Wait and verify .tscn
        let res_tscn = loader.load_threaded_wait("res://scenes/test.tscn", None).unwrap();
        assert!(res_tscn.as_scene().is_some());

        // Wait and verify .fscn
        let res_fscn = loader.load_threaded_wait("res://scenes/test.fscn", None).unwrap();
        assert_eq!(res_fscn.as_scene().unwrap().name, "FscnTest");

        // Wait and verify .tres
        let res_tres = loader.load_threaded_wait("res://shapes/box.tres", None).unwrap();
        assert_eq!(res_tres.as_tres().unwrap().header.res_type, "BoxShape3D");

        // Wait and verify .glb
        let res_glb = loader.load_threaded_wait("res://models/character.glb", None).unwrap();
        let bundle = res_glb.as_glb().unwrap();
        assert_eq!(bundle.scene.name, "RiggedCharacterScene");
        assert_eq!(bundle.skins.len(), 1);
    }

    #[test]
    fn threaded_loader_uid_resolution_and_deduplication() {
        let loader = ThreadedResourceLoader::new();
        let tscn = "[gd_scene format=3 uid=\"uid://portal\"]\n[node name=\"Portal\" type=\"Node3D\"]\n";
        loader.register_virtual("res://portal.tscn", tscn.as_bytes().to_vec());

        let uid = "uid://portal_12345";
        loader.register_uid(uid, "res://portal.tscn");

        // Request via UID
        loader.load_threaded_request(uid).expect("request via UID");

        // Query status with progress
        let mut progress = 0.0f32;
        let _ = loader.load_threaded_get_status(uid, Some(&mut progress));

        let res = loader.load_threaded_wait(uid, None).expect("load via UID");
        assert!(res.as_scene().is_some());

        // Verify cached under both UID and res:// path
        assert!(loader.has_cached(uid));
        assert!(loader.has_cached("res://portal.tscn"));
    }

    #[test]
    fn threaded_loader_reports_failure_for_missing_resource() {
        let loader = ThreadedResourceLoader::new();
        let missing = "res://does_not_exist.tscn";

        let _ = loader.load_threaded_request(missing);
        let res = loader.load_threaded_wait(missing, Some(Duration::from_millis(500)));
        assert!(res.is_err());

        let status = loader.load_threaded_get_status(missing, None);
        assert_eq!(status, ThreadLoadStatus::ThreadLoadFailed);
    }
}
