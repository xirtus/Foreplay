//! Foreplay Editor (`foreplay-editor`) — Godot-parity dockable workspaces built over `HarnessSession`.

use foreplay_harness::HarnessSession;
use foreplay_ir::IrScene;

/// Top-bar workspace tabs matching Godot 4 + 3JSE Atlas + RustPorts Differential Inspector.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EditorWorkspace {
    Workspace2D,
    Workspace3D,
    Script,
    AtlasGraph,
    PortDiff,
    AssetLib,
}

/// Dockable editor shell state (Track C Gemini 4 Argon attaches `egui` + `egui_dock` panels here).
#[derive(Debug)]
pub struct ForeplayEditorApp {
    pub active_workspace: EditorWorkspace,
    pub session: HarnessSession,
    pub play_in_editor_running: bool,
}

impl ForeplayEditorApp {
    pub fn new(project_name: impl Into<String>) -> Self {
        Self {
            active_workspace: EditorWorkspace::Workspace3D,
            session: HarnessSession::new(IrScene::new(project_name)),
            play_in_editor_running: false,
        }
    }
}
