//! Generational `SceneTree`, `Node` lifecycle phases, and `VirtualFileSystem` (`res://`, `user://`, `port://`).

use foreplay_ir::{IrScene, NodeId, PortArchiveMount};
use std::collections::BTreeMap;

/// Godot-parity deterministic frame lifecycle stages.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LifecyclePhase {
    EnterTree,
    Ready,
    PhysicsProcess,
    Process,
    ExitTree,
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

/// Borrow-safe runtime `SceneTree` shell backed by `IrScene` in Phase 0.
#[derive(Debug, Default)]
pub struct SceneTree {
    pub active_scene: IrScene,
    pub root: Option<NodeId>,
    pub tick_count: u64,
}

impl SceneTree {
    pub fn from_ir(scene: IrScene) -> Self {
        let root = scene.nodes.values().find(|n| n.parent.is_none()).map(|n| n.id);
        Self {
            active_scene: scene,
            root,
            tick_count: 0,
        }
    }

    pub fn step_frame(&mut self) {
        self.tick_count += 1;
    }
}
