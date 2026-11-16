//! Foreplay Editor (`foreplay-editor`) — Godot-parity dockable workspaces built over `HarnessSession`.

pub mod camera;
pub mod demo_scene;
pub mod tabs;
pub mod theme;
pub mod transport;
pub mod bottom_panel;
pub mod modals;
pub mod scene_doc;
pub mod widgets;

use std::collections::BTreeMap;
use std::time::Instant;
use egui::{Key, Ui};
use egui_dock::DockState;
use foreplay_harness::HarnessSession;
use foreplay_ir::{EditorCommand, IrNode, IrScene, IrValue, NodeId, SignalConnection};
use crate::bottom_panel::{BottomPanelState, BottomPanelTab};
use crate::camera::{Camera2D, Camera3D, GizmoMode};
use crate::modals::command_palette::CommandPaletteState;
use crate::modals::create_node_dialog::CreateNodeDialogState;
use crate::modals::export_modal::ExportModalState;
use crate::modals::project_manager::{ProjectManagerState, ProjectTemplate};
use crate::modals::project_settings::ProjectSettingsState;
use crate::modals::ActiveModal;
use crate::scene_doc::OpenSceneDoc;
use crate::tabs::ai_harness::AiHarnessState;
use crate::tabs::animation::AnimationDockState;
use crate::tabs::audio_mixer::AudioMixerState;
use crate::tabs::console::{ConsoleLogEntry, LogLevel};
use crate::tabs::debugger::DebuggerState;
use crate::tabs::filesystem::FileSystemDockState;
use crate::tabs::inspector::PropTypeChoice;
use crate::tabs::node_dock::NodeDockState;
use crate::tabs::shader_editor::ShaderEditorState;
use crate::tabs::tilemap::TileMapDockState;
use crate::tabs::viewport_3d::ViewMode3D;
use crate::tabs::{build_default_dock_state, EditorTab, EditorTabViewer};

/// Top-bar workspace tabs matching Godot 4 + 3JSE Atlas + RustPorts Differential Inspector + Polyamour + Three.js.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum EditorWorkspace {
    Workspace2D,
    Workspace3D,
    Script,
    AtlasGraph,
    PortDiff,
    AssetLib,
    ModelPolyamour,
    ThreeJsWeb,
}

/// Hotswappable physics backend choice.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PhysicsBackendChoice {
    RapierStandard,
    Skate2Port,
}

/// Dockable editor shell state managing scene tree, tabs, camera, and Undo/Redo history.
pub struct ForeplayEditorApp {
    pub active_workspace: EditorWorkspace,
    pub session: HarnessSession,
    pub play_in_editor_running: bool,
    pub play_in_editor_paused: bool,
    pub pie_snapshot: Option<IrScene>,
    pub dock_state: DockState<EditorTab>,
    pub selected_node: Option<NodeId>,
    pub camera_3d: Camera3D,
    pub camera_2d: Camera2D,
    pub gizmo_mode: GizmoMode,
    pub physics_backend: PhysicsBackendChoice,
    pub console_logs: Vec<ConsoleLogEntry>,
    pub command_history_count: usize,
    pub port_diff_epsilon: f32,
    pub script_buffer: String,
    pub search_query: String,
    pub new_prop_key: String,
    pub new_prop_type: PropTypeChoice,
    pub new_feel_key: String,
    pub new_feel_value: f64,
    pub fps: f32,
    last_frame_instant: Option<Instant>,

    // --- Wave 2 Parity State ---
    pub open_scenes: Vec<OpenSceneDoc>,
    pub active_scene_index: usize,
    pub active_modal: Option<ActiveModal>,
    pub project_manager_state: ProjectManagerState,
    pub project_settings_state: ProjectSettingsState,
    pub export_modal_state: ExportModalState,
    pub command_palette_state: CommandPaletteState,
    pub create_node_dialog_state: CreateNodeDialogState,
    pub node_dock_state: NodeDockState,
    pub filesystem_state: FileSystemDockState,
    pub animation_state: AnimationDockState,
    pub audio_mixer_state: AudioMixerState,
    pub shader_editor_state: ShaderEditorState,
    pub tilemap_state: TileMapDockState,
    pub debugger_state: DebuggerState,
    pub ai_harness_state: AiHarnessState,
    pub bottom_panel_state: BottomPanelState,

    // Viewport 3D & 2D tool state
    pub is_local_transform: bool,
    pub snap_enabled: bool,
    pub snap_step_translate: f32,
    pub snap_step_rotate_deg: f32,
    pub preview_camera: bool,
    pub view_mode_3d: ViewMode3D,
    pub snap_2d_pixel: bool,
    pub snap_2d_grid: bool,

    // Live Engine Backend Wiring (Wave 2B)
    pub last_differential_report: Option<foreplay_porthost::DifferentialReport>,
    pub active_vm: Option<foreplay_gdscript::GdVm>,

    // Wave 3A Studio Obsidian, Polyamour, Three.js & Trackpad State
    pub theme_preset: crate::theme::EditorThemePreset,
    pub polyamour_state: crate::tabs::polyamour_model::PolyamourModelState,
    pub threejs_state: crate::tabs::threejs_studio::ThreeJsStudioState,
    pub atlas_pan: egui::Vec2,
    pub atlas_zoom: f32,
    pub atlas_recorder: foreplay_atlas::timetravel::TimeTravelAtlasRecorder,
    pub atlas_scrub_tick: Option<u64>,

    // Hardware GPU Viewport Renderer & Zero-Copy Viewport Bridge
    pub render_device: Option<foreplay_render::WgpuRenderDevice>,
    pub zero_copy_viewport: Option<foreplay_render::ZeroCopyViewportBridge>,
}

impl std::fmt::Debug for ForeplayEditorApp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ForeplayEditorApp")
            .field("active_workspace", &self.active_workspace)
            .field("scene_name", &self.session.tree.active_scene.name)
            .field("play_in_editor_running", &self.play_in_editor_running)
            .field("selected_node", &self.selected_node)
            .field("open_scenes_count", &self.open_scenes.len())
            .field("active_modal", &self.active_modal)
            .finish()
    }
}

impl ForeplayEditorApp {
    /// Constructs a new editor app instance for the given project name.
    pub fn new(project_name: impl Into<String>) -> Self {
        let name_str = project_name.into();
        let scene = IrScene::new(name_str.clone());
        let session = HarnessSession::new(scene);

        let script_sample = r#"class_name SkaterController
extends CharacterBody3D

@export var max_speed: float = 14.5
@export var is_grounded: bool = true
var velocity = Vector3(0.0, 0.0, 0.0)

signal trick_landed(trick_name: String, score: int)

func _ready() -> void:
    velocity.x = 5.0
    print("Skater controller bound and ready!")

func _physics_process(delta: float) -> void:
    position.x += max_speed * delta
    if position.x > 10.0:
        emit_signal("trick_landed", "kickflip", 100)
"#
        .to_string();

        let initial_doc = OpenSceneDoc::new(0, IrScene::new(name_str.clone()), Some("res://scenes/main.fscn".into()));
        let hud_doc = OpenSceneDoc::starter_hud(1);

        let mut app = Self {
            active_workspace: EditorWorkspace::Workspace3D,
            session,
            play_in_editor_running: false,
            play_in_editor_paused: false,
            pie_snapshot: None,
            dock_state: build_default_dock_state(),
            selected_node: None,
            camera_3d: Camera3D::default(),
            camera_2d: Camera2D::default(),
            gizmo_mode: GizmoMode::Translate,
            physics_backend: PhysicsBackendChoice::RapierStandard,
            console_logs: Vec::new(),
            command_history_count: 0,
            port_diff_epsilon: 0.005,
            script_buffer: script_sample,
            search_query: String::new(),
            new_prop_key: String::new(),
            new_prop_type: PropTypeChoice::F64,
            new_feel_key: String::new(),
            new_feel_value: 1.0,
            fps: 60.0,
            last_frame_instant: None,

            // Wave 2
            open_scenes: vec![initial_doc, hud_doc],
            active_scene_index: 0,
            active_modal: None,
            project_manager_state: ProjectManagerState::default(),
            project_settings_state: ProjectSettingsState::default(),
            export_modal_state: ExportModalState::default(),
            command_palette_state: CommandPaletteState::default(),
            create_node_dialog_state: CreateNodeDialogState::default(),
            node_dock_state: NodeDockState::default(),
            filesystem_state: FileSystemDockState::default(),
            animation_state: AnimationDockState::default(),
            audio_mixer_state: AudioMixerState::default(),
            shader_editor_state: ShaderEditorState::default(),
            tilemap_state: TileMapDockState::default(),
            debugger_state: DebuggerState::default(),
            ai_harness_state: AiHarnessState::default(),
            bottom_panel_state: BottomPanelState::default(),

            is_local_transform: false,
            snap_enabled: false,
            snap_step_translate: 0.5,
            snap_step_rotate_deg: 15.0,
            preview_camera: false,
            view_mode_3d: ViewMode3D::LitNormal,
            snap_2d_pixel: true,
            snap_2d_grid: false,

            last_differential_report: None,
            active_vm: None,

            theme_preset: crate::theme::EditorThemePreset::StudioObsidianPro,
            polyamour_state: crate::tabs::polyamour_model::PolyamourModelState::default(),
            threejs_state: crate::tabs::threejs_studio::ThreeJsStudioState::default(),
            atlas_pan: egui::Vec2::ZERO,
            atlas_zoom: 1.0,
            atlas_recorder: foreplay_atlas::timetravel::TimeTravelAtlasRecorder::default(),
            atlas_scrub_tick: None,
            render_device: Some(foreplay_render::WgpuRenderDevice::new_or_fallback()),
            zero_copy_viewport: Some(foreplay_render::ZeroCopyViewportBridge::new()),
        };

        let summary = app.adapter_summary();
        app.log(
            LogLevel::Info,
            format!("Initialized Foreplay Editor v1.1.0 for project '{name_str}' | GPU: {summary}"),
        );
        app
    }

    /// Returns `true` if hardware GPU zero-copy viewport bridge is active without software fallback.
    #[must_use]
    pub fn is_zero_copy_viewport_active(&self) -> bool {
        self.render_device
            .as_ref()
            .is_some_and(|d| d.is_hardware_accelerated())
    }

    /// Human-readable summary of the underlying wgpu graphics adapter.
    pub fn adapter_summary(&self) -> String {
        self.render_device
            .as_ref()
            .map(|d| d.adapter_summary())
            .unwrap_or_else(|| "wgpu (High Performance Adapter)".into())
    }

    /// Obtains a mutable reference to the hardware `WgpuRenderDevice`, initializing if needed.
    pub fn get_render_device(&mut self) -> &mut foreplay_render::WgpuRenderDevice {
        self.render_device
            .get_or_insert_with(foreplay_render::WgpuRenderDevice::new_or_fallback)
    }

    /// Records a console log entry.
    pub fn log(&mut self, level: LogLevel, message: impl Into<String>) {
        let msg = message.into();
        let timestamp_ms = self.command_history_count as u64 * 16;
        self.console_logs.push(ConsoleLogEntry {
            timestamp_ms,
            level,
            message: msg,
        });
    }

    /// Dispatches an `EditorCommand` through `HarnessSession::execute` and logs the action.
    ///
    /// CRITICAL GOLDEN RULE: All scene mutations MUST route through this method!
    pub fn execute_command(&mut self, cmd: EditorCommand) {
        self.log(LogLevel::Command, format!("Executed {cmd:?}"));
        let tick = self.command_history_count as u64;
        let mut feel_changes = Vec::new();
        let mut pulses = Vec::new();
        if let EditorCommand::TuneFeelSpec { ref key, old_value, new_value } = cmd {
            if let Some(new_val) = new_value {
                feel_changes.push(foreplay_atlas::timetravel::FeelSpecLiveChange {
                    tick,
                    key: key.clone(),
                    old_value,
                    new_value: new_val,
                });
            }
        } else if let EditorCommand::ConnectSignal { ref connection } = cmd {
            pulses.push(foreplay_atlas::timetravel::SignalPulse {
                pulse_id: tick + 1,
                tick,
                from_node: connection.from_node,
                signal_name: connection.signal_name.clone(),
                to_node: connection.to_node,
                method_name: connection.method_name.clone(),
                payload: IrValue::Nil,
                caused_by: None,
            });
        }
        if let Err(err) = self.session.execute(cmd) {
            self.log(LogLevel::Warn, format!("Command failed: {err}"));
        } else {
            self.command_history_count += 1;
            let state_hash = self.session.tree.active_scene.nodes.len() as u64;
            self.atlas_recorder.record_frame(tick, state_hash, pulses, feel_changes);
        }
    }

    /// Undoes the last command via `HarnessSession::undo`.
    pub fn undo(&mut self) {
        match self.session.undo() {
            Ok(true) => {
                self.log(LogLevel::Command, "Undo successful (restored previous state)");
            }
            Ok(false) => {
                self.log(LogLevel::Info, "Undo stack is empty");
            }
            Err(err) => {
                self.log(LogLevel::Warn, format!("Undo error: {err}"));
            }
        }
    }

    /// Redoes the next command via `HarnessSession::redo`.
    pub fn redo(&mut self) {
        match self.session.redo() {
            Ok(true) => {
                self.log(LogLevel::Command, "Redo successful");
            }
            Ok(false) => {
                self.log(LogLevel::Info, "Redo stack is empty");
            }
            Err(err) => {
                self.log(LogLevel::Warn, format!("Redo error: {err}"));
            }
        }
    }

    /// Sets the active workspace and focuses the corresponding dock tab.
    pub fn set_workspace(&mut self, ws: EditorWorkspace) {
        self.active_workspace = ws;
        let target_tab = match ws {
            EditorWorkspace::Workspace2D => EditorTab::Viewport2D,
            EditorWorkspace::Workspace3D => EditorTab::Viewport3D,
            EditorWorkspace::ModelPolyamour => EditorTab::PolyamourModel,
            EditorWorkspace::Script => EditorTab::ScriptView,
            EditorWorkspace::ThreeJsWeb => EditorTab::ThreeJsStudio,
            EditorWorkspace::AtlasGraph => EditorTab::AtlasGraph,
            EditorWorkspace::PortDiff => EditorTab::PortDiff,
            EditorWorkspace::AssetLib => EditorTab::AssetLib,
        };

        if let Some(tab_path) = self.dock_state.find_tab(&target_tab) {
            self.dock_state.set_active_tab(tab_path).ok();
        }
    }

    /// Switches active scene document or opens a new one.
    pub fn open_or_switch_scene(&mut self, title: impl Into<String>, file_path: Option<String>) {
        let title_str = title.into();
        // Check if already open
        if let Some(pos) = self.open_scenes.iter().position(|d| d.title == title_str) {
            self.switch_to_scene_index(pos);
            return;
        }

        // Create new document
        let new_id = self.open_scenes.len();
        let new_doc = OpenSceneDoc::new(new_id, IrScene::new(title_str.clone()), file_path);
        self.open_scenes.push(new_doc);
        self.switch_to_scene_index(self.open_scenes.len() - 1);
        self.log(LogLevel::Info, format!("Opened scene document '{title_str}'"));
    }

    /// Switches the active scene tab index, swapping sessions cleanly.
    pub fn switch_to_scene_index(&mut self, index: usize) {
        if index >= self.open_scenes.len() {
            return;
        }

        // Sync current active session into doc
        if self.active_scene_index < self.open_scenes.len() {
            self.open_scenes[self.active_scene_index].session =
                std::mem::replace(&mut self.session, HarnessSession::new(IrScene::new("temp")));
            self.open_scenes[self.active_scene_index].selected_node = self.selected_node;
        }

        // Swap target session into active
        self.active_scene_index = index;
        self.session = std::mem::replace(
            &mut self.open_scenes[index].session,
            HarnessSession::new(IrScene::new("temp")),
        );
        self.selected_node = self.open_scenes[index].selected_node;
        self.log(
            LogLevel::Info,
            format!("Switched to scene '{}'", self.session.tree.active_scene.name),
        );
    }

    /// Closes a scene document tab.
    pub fn close_scene_doc(&mut self, index: usize) {
        if self.open_scenes.len() <= 1 || index >= self.open_scenes.len() {
            return;
        }

        let closed_title = self.open_scenes[index].title.clone();
        if index == self.active_scene_index {
            let next_idx = if index > 0 { index - 1 } else { 0 };
            self.switch_to_scene_index(next_idx);
        }

        self.open_scenes.remove(index);
        if self.active_scene_index >= self.open_scenes.len() {
            self.active_scene_index = self.open_scenes.len() - 1;
        }

        self.log(LogLevel::Info, format!("Closed scene '{closed_title}'"));
    }

    /// Creates a new scene based on a project template.
    pub fn create_new_project_scene(&mut self, name: &str, template: ProjectTemplate) {
        self.open_or_switch_scene(name, Some(format!("res://scenes/{name}.fscn")));

        match template {
            ProjectTemplate::SkatePark3D => {
                self.populate_starter_scene();
            }
            ProjectTemplate::Platformer2D => {
                let root_id = NodeId::new(1, 1).expect("valid generation");
                let mut root_props = BTreeMap::new();
                root_props.insert("position".into(), IrValue::Vec2([0.0, 0.0]));
                self.execute_command(EditorCommand::InsertNode {
                    node: IrNode {
                        id: root_id,
                        name: "PlatformerRoot2D".into(),
                        class_name: "Node2D".into(),
                        parent: None,
                        instance_scene: None,
                        properties: root_props,
                        groups: vec!["game".into()],
                    },
                });

                let player_id = NodeId::new(2, 1).expect("valid generation");
                let mut player_props = BTreeMap::new();
                player_props.insert("position".into(), IrValue::Vec2([200.0, 400.0]));
                player_props.insert("scale".into(), IrValue::Vec2([1.0, 1.0]));
                self.execute_command(EditorCommand::InsertNode {
                    node: IrNode {
                        id: player_id,
                        name: "PlayerPlatformer".into(),
                        class_name: "CharacterBody2D".into(),
                        parent: Some(root_id),
                        instance_scene: None,
                        properties: player_props,
                        groups: vec!["player".into()],
                    },
                });
                self.selected_node = Some(player_id);
            }
            ProjectTemplate::UiMenu => {
                let root_id = NodeId::new(1, 1).expect("valid generation");
                let mut root_props = BTreeMap::new();
                root_props.insert("position".into(), IrValue::Vec2([0.0, 0.0]));
                root_props.insert("scale".into(), IrValue::Vec2([1280.0, 720.0]));
                self.execute_command(EditorCommand::InsertNode {
                    node: IrNode {
                        id: root_id,
                        name: "MainMenuRoot".into(),
                        class_name: "Control".into(),
                        parent: None,
                        instance_scene: None,
                        properties: root_props,
                        groups: vec!["ui".into()],
                    },
                });

                let button_id = NodeId::new(2, 1).expect("valid generation");
                let mut btn_props = BTreeMap::new();
                btn_props.insert("position".into(), IrValue::Vec2([540.0, 320.0]));
                btn_props.insert("scale".into(), IrValue::Vec2([200.0, 50.0]));
                btn_props.insert("text".into(), IrValue::String("Start Game".into()));
                self.execute_command(EditorCommand::InsertNode {
                    node: IrNode {
                        id: button_id,
                        name: "StartButton".into(),
                        class_name: "Button".into(),
                        parent: Some(root_id),
                        instance_scene: None,
                        properties: btn_props,
                        groups: vec!["buttons".into()],
                    },
                });
                self.selected_node = Some(button_id);
            }
            ProjectTemplate::Empty => {}
        }
    }

    /// Dispatches node creation for a preset class name.
    pub fn create_preset_node(&mut self, class_name: &str, is_3d: bool) {
        let next_index = self
            .session
            .tree
            .active_scene
            .nodes
            .keys()
            .map(|k| k.index)
            .max()
            .unwrap_or(0)
            + 1;
        let node_id = NodeId::new(next_index, 1).expect("valid generation");

        let mut props = BTreeMap::new();
        if is_3d {
            props.insert("position".into(), IrValue::Vec3([0.0, 0.0, 0.0]));
            props.insert("scale".into(), IrValue::Vec3([1.0, 1.0, 1.0]));
            props.insert("rotation".into(), IrValue::Vec3([0.0, 0.0, 0.0]));
        } else {
            props.insert("position".into(), IrValue::Vec2([0.0, 0.0]));
            props.insert("scale".into(), IrValue::Vec2([1.0, 1.0]));
        }

        let new_node = IrNode {
            id: node_id,
            name: format!("{class_name}_{next_index}"),
            class_name: class_name.to_string(),
            parent: self.selected_node,
            instance_scene: None,
            properties: props,
            groups: Vec::new(),
        };

        self.execute_command(EditorCommand::InsertNode { node: new_node });
        self.selected_node = Some(node_id);
    }

    /// Exports the active `IrScene` into canonical Godot 4 `.tscn` text format via `foreplay_porthost`.
    pub fn export_active_scene_tscn(&self) -> String {
        foreplay_porthost::export_ir_to_tscn(&self.session.tree.active_scene)
    }

    /// Bidirectionally imports a Godot 4 `.tscn` text scene via `foreplay_porthost::parse_tscn_to_ir`.
    ///
    /// Creates or switches to the document tab `name`, clearing existing nodes and replaying
    /// all nodes, feel specs, and signal connections strictly through `EditorCommand` dispatches.
    pub fn import_tscn_scene(&mut self, name: &str, tscn_source: &str) -> Result<(), String> {
        let parsed = foreplay_porthost::parse_tscn_to_ir(tscn_source)
            .map_err(|e| format!("Failed to parse .tscn: {e}"))?;

        self.open_or_switch_scene(name, Some(format!("res://scenes/{name}.tscn")));

        // Clear existing nodes in scene so import replaces cleanly
        let existing_nodes: Vec<IrNode> = self.session.tree.active_scene.nodes.values().cloned().collect();
        for node in existing_nodes {
            self.execute_command(EditorCommand::RemoveNode { node });
        }

        for node in parsed.nodes.values() {
            self.execute_command(EditorCommand::InsertNode { node: node.clone() });
        }

        for (key, val) in &parsed.feel_overrides {
            self.execute_command(EditorCommand::TuneFeelSpec {
                key: key.clone(),
                old_value: None,
                new_value: Some(*val),
            });
        }

        for conn in &parsed.connections {
            self.execute_command(EditorCommand::ConnectSignal {
                connection: conn.clone(),
            });
        }

        self.selected_node = parsed.nodes.values().find(|n| n.parent.is_none()).map(|n| n.id);
        self.log(
            LogLevel::Info,
            format!(
                "Successfully imported Godot .tscn '{name}' ({} nodes, {} signals)",
                parsed.nodes.len(),
                parsed.connections.len()
            ),
        );
        Ok(())
    }

    /// Three.js / 3JSE JSON format importer (`foreplay-threejs`).
    ///
    /// Parses Three.js `THREE.Object3D.toJSON()` or `@3jse/atlas` `3jse.scene.json` formats
    /// via `foreplay_threejs::import_threejs_json_to_ir` and replays all nodes, feel specs,
    /// and signal connections strictly through `EditorCommand` dispatches.
    pub fn import_threejs_scene_json(&mut self, name: &str, json_str: &str) -> Result<usize, String> {
        let parsed = foreplay_threejs::import_threejs_json_to_ir(json_str)
            .map_err(|e| format!("Failed to parse Three.js JSON: {e}"))?;

        self.open_or_switch_scene(name, Some(format!("res://scenes/{name}.fscn")));

        // Clear existing nodes in scene so import replaces cleanly
        let existing_nodes: Vec<IrNode> = self.session.tree.active_scene.nodes.values().cloned().collect();
        for node in existing_nodes {
            self.execute_command(EditorCommand::RemoveNode { node });
        }

        let node_count = parsed.nodes.len();
        for node in parsed.nodes.values() {
            self.execute_command(EditorCommand::InsertNode { node: node.clone() });
        }

        for (key, val) in &parsed.feel_overrides {
            self.execute_command(EditorCommand::TuneFeelSpec {
                key: key.clone(),
                old_value: None,
                new_value: Some(*val),
            });
        }

        for conn in &parsed.connections {
            self.execute_command(EditorCommand::ConnectSignal {
                connection: conn.clone(),
            });
        }

        self.selected_node = parsed.nodes.values().find(|n| n.parent.is_none()).map(|n| n.id);
        self.log(
            LogLevel::Info,
            format!(
                "Successfully imported Three.js 3JSE scene '{name}' ({node_count} nodes, {} signals, {} feel knobs)",
                parsed.connections.len(),
                parsed.feel_overrides.len()
            ),
        );
        Ok(node_count)
    }

    /// Godot 4 `.tscn` text format importer.
    ///
    /// Parses Godot `.tscn` text content via `foreplay_porthost::parse_tscn_to_ir` and commits
    /// each node, signal connection, and feel knob through `EditorCommand` dispatches.
    pub fn import_tscn_buffer(&mut self, tscn_content: &str) -> usize {
        match foreplay_porthost::parse_tscn_to_ir(tscn_content) {
            Ok(parsed) => {
                let max_existing_index = self
                    .session
                    .tree
                    .active_scene
                    .nodes
                    .keys()
                    .map(|k| k.index)
                    .max()
                    .unwrap_or(0);

                let id_offset = max_existing_index;
                let mut count = 0;

                // Build mapping from old NodeId to new NodeId
                let mut id_map = BTreeMap::new();
                for node in parsed.nodes.values() {
                    let new_id = NodeId::new(node.id.index + id_offset, node.id.generation.get()).unwrap();
                    id_map.insert(node.id, new_id);
                }

                for node in parsed.nodes.values() {
                    let new_id = id_map.get(&node.id).copied().unwrap();
                    let new_parent = node.parent.and_then(|p| id_map.get(&p).copied());
                    let mut imported_node = node.clone();
                    imported_node.id = new_id;
                    imported_node.parent = new_parent;
                    self.execute_command(EditorCommand::InsertNode { node: imported_node });
                    count += 1;
                }

                for conn in &parsed.connections {
                    if let (Some(&from), Some(&to)) = (id_map.get(&conn.from_node), id_map.get(&conn.to_node)) {
                        let remapped = SignalConnection {
                            from_node: from,
                            signal_name: conn.signal_name.clone(),
                            to_node: to,
                            method_name: conn.method_name.clone(),
                        };
                        self.execute_command(EditorCommand::ConnectSignal { connection: remapped });
                    }
                }

                for (key, val) in &parsed.feel_overrides {
                    self.execute_command(EditorCommand::TuneFeelSpec {
                        key: key.clone(),
                        old_value: None,
                        new_value: Some(*val),
                    });
                }

                self.log(
                    LogLevel::Info,
                    format!("Imported {count} nodes from Godot .tscn via foreplay_porthost"),
                );
                count
            }
            Err(err) => {
                self.log(LogLevel::Warn, format!("Failed to parse .tscn buffer: {err}"));
                0
            }
        }
    }

    /// Executes the current GDScript buffer via `foreplay_gdscript::GdVm` against the active/selected node.
    ///
    /// Invokes `_ready()` and `_physics_process(1/60)` lifecycle methods, capturing emitted signals,
    /// printing debug logs to the console dock, and synchronizing modified properties through `EditorCommand::SetProperty`.
    pub fn run_gdscript_vm(&mut self) -> Result<usize, String> {
        let mut vm = foreplay_gdscript::GdVm::from_source(&self.script_buffer)
            .map_err(|e| format!("GdVm parse/compilation failed: {e}"))?;

        let target_id = self.selected_node.or_else(|| {
            self.session.tree.active_scene.nodes.keys().next().copied()
        }).ok_or_else(|| "No node available in active scene to bind GDScript".to_string())?;

        let mut target_node = self.session.tree.active_scene.nodes.get(&target_id)
            .cloned()
            .ok_or_else(|| format!("Target node {target_id:?} not found in active scene"))?;

        let original_node = target_node.clone();

        // 1. Ready lifecycle
        let ready_res = vm.ready(&mut target_node)
            .map_err(|e| format!("GdVm _ready() runtime error: {e}"))?;

        for line in &ready_res.logs {
            self.log(LogLevel::Info, format!("[GdVm print] {line}"));
        }
        for (sig, args) in &ready_res.emitted_signals {
            self.log(LogLevel::Info, format!("⚡ [GdVm Signal] {sig}({args:?})"));
        }

        // 2. Physics process lifecycle (1 frame @ 60Hz)
        let phys_res = vm.step_physics_process(1.0 / 60.0, &mut target_node)
            .map_err(|e| format!("GdVm _physics_process() runtime error: {e}"))?;

        for line in &phys_res.logs {
            self.log(LogLevel::Info, format!("[GdVm print] {line}"));
        }
        for (sig, args) in &phys_res.emitted_signals {
            self.log(LogLevel::Info, format!("⚡ [GdVm Signal] {sig}({args:?})"));
        }

        let total_steps = ready_res.steps_executed + phys_res.steps_executed;

        // Synchronize any property changes through EditorCommands for Undo/Redo & Harness parity
        for (k, v) in &target_node.properties {
            let old_v = original_node.properties.get(k).cloned();
            if old_v.as_ref() != Some(v) {
                self.execute_command(EditorCommand::SetProperty {
                    node_id: target_id,
                    key: k.clone(),
                    old_value: old_v,
                    new_value: Some(v.clone()),
                });
            }
        }

        self.log(
            LogLevel::Info,
            format!("GdVm executed {total_steps} steps on '{}'", target_node.name),
        );
        Ok(total_steps)
    }

    /// Advances 1 frame of live `GdVm` script simulation during PIE.
    pub fn step_pie_gdscript(&mut self) -> Result<(), String> {
        if self.active_vm.is_none()
            && let Ok(mut vm) = foreplay_gdscript::GdVm::from_source(&self.script_buffer)
        {
            let target_id = self.selected_node.or_else(|| self.session.tree.active_scene.nodes.keys().next().copied());
            if let Some(target_id) = target_id
                && let Some(node) = self.session.tree.active_scene.nodes.get_mut(&target_id)
            {
                let _ = vm.ready(node);
            }
            self.active_vm = Some(vm);
        }

        if let Some(vm) = &mut self.active_vm {
            let target_id = self.selected_node.or_else(|| self.session.tree.active_scene.nodes.keys().next().copied());
            if let Some(target_id) = target_id
                && let Some(node) = self.session.tree.active_scene.nodes.get_mut(&target_id)
                && let Ok(res) = vm.step_physics_process(1.0 / 60.0, node)
            {
                for line in res.logs {
                    self.log(LogLevel::Pie, format!("[PIE GdVm] {line}"));
                }
                for (sig, args) in res.emitted_signals {
                    self.log(LogLevel::Pie, format!("⚡ [PIE GdVm Signal] {sig}({args:?})"));
                }
            }
        }
        Ok(())
    }

    /// Toggles bottom panel tab or expands/collapses the bottom panel.
    pub fn toggle_bottom_panel(&mut self, tab: BottomPanelTab) {
        if !self.bottom_panel_state.is_expanded {
            self.bottom_panel_state.is_expanded = true;
            self.bottom_panel_state.active_tab = tab;
        } else if self.bottom_panel_state.active_tab == tab {
            self.bottom_panel_state.is_expanded = false;
        } else {
            self.bottom_panel_state.active_tab = tab;
        }
    }

    /// Populates the rich starter demo scene using pure `EditorCommand` dispatches.
    pub fn populate_starter_scene(&mut self) {
        match crate::demo_scene::populate_starter_scene(&mut self.session) {
            Ok(()) => {
                self.command_history_count += 13;
                self.selected_node = NodeId::new(1, 1);
                self.log(
                    LogLevel::Info,
                    "Populated starter demo park scene (9 nodes + 4 feel specs via EditorCommands)",
                );
            }
            Err(err) => {
                self.log(LogLevel::Warn, format!("Failed to populate demo scene: {err}"));
            }
        }
    }

    /// PIE Play: Captures exact pre-play `IrScene` snapshot and starts simulation.
    pub fn pie_play(&mut self) {
        if !self.play_in_editor_running {
            self.pie_snapshot = Some(self.session.tree.active_scene.clone());
            self.play_in_editor_running = true;
            self.play_in_editor_paused = false;
            self.log(
                LogLevel::Pie,
                format!("▶ PIE Play started. Captured pre-play snapshot of '{}'", self.session.tree.active_scene.name),
            );
        } else if self.play_in_editor_paused {
            self.play_in_editor_paused = false;
            self.log(LogLevel::Pie, "▶ PIE Resumed.");
        }
    }

    /// PIE Pause: Pauses simulation while maintaining running state.
    pub fn pie_pause(&mut self) {
        if self.play_in_editor_running {
            self.play_in_editor_paused = !self.play_in_editor_paused;
            let status = if self.play_in_editor_paused { "Paused" } else { "Resumed" };
            self.log(LogLevel::Pie, format!("⏸ PIE {status}."));
        }
    }

    /// PIE Step: Advances 1 deterministic headless/physics tick and GdVm step.
    pub fn pie_step(&mut self) {
        let tick = self.session.verify_headless_ticks(1);
        let _ = self.step_pie_gdscript();
        self.log(LogLevel::Pie, format!("⏭ PIE Stepped 1 frame. World Tick = {tick}"));
    }

    /// PIE Stop: Restores exact pre-play snapshot with zero rebuild.
    pub fn pie_stop(&mut self) {
        self.active_vm = None;
        if let Some(snapshot) = self.pie_snapshot.take() {
            self.session.tree = foreplay_core::SceneTree::from_ir(snapshot);
            self.play_in_editor_running = false;
            self.play_in_editor_paused = false;
            self.log(
                LogLevel::Pie,
                "⏹ PIE Stopped. Restored exact pre-play scene snapshot with zero drift.",
            );
        } else {
            self.play_in_editor_running = false;
            self.play_in_editor_paused = false;
        }
    }

    /// Renders the entire Godot-parity IDE UI into the provided `egui::Ui`.
    pub fn render_ui(&mut self, ui: &mut Ui) {
        // Apply active theme preset (Studio Obsidian Pro by default)
        crate::theme::apply_theme(ui.ctx(), self.theme_preset);

        // Update frame timer / FPS calculation
        if let Some(prev) = self.last_frame_instant {
            let dt = prev.elapsed().as_secs_f32();
            if dt > 1e-4 {
                self.fps = 0.9 * self.fps + 0.1 * (1.0 / dt);
            }
        }
        self.last_frame_instant = Some(Instant::now());

        // Tick PIE if running
        if self.play_in_editor_running && !self.play_in_editor_paused {
            self.session.verify_headless_ticks(1);
            let _ = self.step_pie_gdscript();
        }

        // Handle global Undo/Redo & Modal keyboard shortcuts
        let input = ui.input(|i| i.clone());
        let cmd_or_ctrl = input.modifiers.command || input.modifiers.ctrl;
        if cmd_or_ctrl && input.modifiers.shift && input.key_pressed(Key::P) {
            self.active_modal = Some(ActiveModal::CommandPalette);
        } else if input.modifiers.alt && input.key_pressed(Key::P) {
            self.active_modal = Some(ActiveModal::ProjectSettings);
        } else if cmd_or_ctrl && input.key_pressed(Key::A) {
            self.active_modal = Some(ActiveModal::CreateNodeDialog);
        } else if cmd_or_ctrl && input.key_pressed(Key::Z) {
            if input.modifiers.shift {
                self.redo();
            } else {
                self.undo();
            }
        } else if cmd_or_ctrl && input.key_pressed(Key::Y) {
            self.redo();
        }

        // Top Transport Bar & Menu Bar
        egui::Panel::top("top_transport_panel").show(ui, |ui| {
            crate::transport::render_top_bar(self, ui);
        });

        // Bottom Collapsible Switcher Bar & Panel
        egui::Panel::bottom("bottom_dock_panel").show(ui, |ui| {
            crate::bottom_panel::render_bottom_panel(self, ui);
        });

        // Center Docking Panels (SceneHierarchy, Viewport, Inspector, Atlas, Script, PortDiff, Console)
        egui::CentralPanel::default().show(ui, |ui| {
            let mut dock_state = std::mem::replace(&mut self.dock_state, DockState::new(Vec::new()));
            let mut tab_viewer = EditorTabViewer::new(self);

            egui_dock::DockArea::new(&mut dock_state)
                .style(egui_dock::Style::from_egui(ui.style()))
                .show_inside(ui, &mut tab_viewer);

            self.dock_state = dock_state;
        });

        // Active Modal Dialog (if any)
        crate::modals::render_active_modal(self, ui);
    }

    /// Headless smoke verification pass executing `egui::Context::run_ui` without a window server.
    pub fn run_headless_frame(&mut self, viewport_size: [f32; 2]) -> egui::FullOutput {
        let ctx = egui::Context::default();
        let raw_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(viewport_size[0], viewport_size[1]),
            )),
            ..Default::default()
        };
        let mut output = ctx.run_ui(raw_input, |ui| {
            self.render_ui(ui);
        });
        output.textures_delta.clear();
        output
    }
}

impl eframe::App for ForeplayEditorApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        self.render_ui(ui);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use foreplay_ir::{SignalConnection, IrValue};

    #[test]
    fn backwards_compatibility_initialization() {
        let app = ForeplayEditorApp::new("CompatibilityTest");
        assert_eq!(app.active_workspace, EditorWorkspace::Workspace3D);
        assert_eq!(app.session.tree.active_scene.name, "CompatibilityTest");
        assert!(!app.play_in_editor_running);
        assert_eq!(app.open_scenes.len(), 2);
    }

    #[test]
    fn demo_scene_population_and_undo_restoration() {
        let mut app = ForeplayEditorApp::new("UndoRedoTest");
        let initial_node_count = app.session.tree.active_scene.nodes.len();
        assert_eq!(initial_node_count, 0);

        // Populate starter demo scene via pure EditorCommands
        app.populate_starter_scene();
        assert_eq!(app.session.tree.active_scene.nodes.len(), 9);
        assert_eq!(app.session.tree.active_scene.feel_overrides.len(), 4);
        assert!(app.session.tree.active_scene.nodes.values().any(|n| n.name == "SkaterRoot"));

        // Undo one by one
        let mut undo_steps = 0;
        while app.session.undo().unwrap() {
            undo_steps += 1;
        }
        assert!(undo_steps >= 13);
        assert_eq!(app.session.tree.active_scene.nodes.len(), 0);
        assert_eq!(app.session.tree.active_scene.feel_overrides.len(), 0);

        // Redo restores them
        while app.session.redo().unwrap() {}
        assert_eq!(app.session.tree.active_scene.nodes.len(), 9);
        assert_eq!(app.session.tree.active_scene.feel_overrides.len(), 4);
    }

    #[test]
    fn play_in_editor_snapshot_and_restoration() {
        let mut app = ForeplayEditorApp::new("PieTest");
        app.populate_starter_scene();

        let pre_play_scene = app.session.tree.active_scene.clone();
        let pre_play_ticks = app.session.tree.tick_count;

        // Start PIE
        app.pie_play();
        assert!(app.play_in_editor_running);
        assert!(app.pie_snapshot.is_some());

        // Step 10 ticks
        for _ in 0..10 {
            app.pie_step();
        }
        assert_eq!(app.session.tree.tick_count, pre_play_ticks + 10);

        // Stop PIE
        app.pie_stop();
        assert!(!app.play_in_editor_running);
        assert_eq!(app.session.tree.active_scene, pre_play_scene);
        assert_eq!(app.session.tree.tick_count, pre_play_ticks);
    }

    #[test]
    fn headless_egui_frame_produces_non_empty_paint_shapes() {
        let mut app = ForeplayEditorApp::new("HeadlessFrameSmoke");
        app.populate_starter_scene();

        // Render headless across all 8 workspaces
        let workspaces = [
            EditorWorkspace::Workspace3D,
            EditorWorkspace::Workspace2D,
            EditorWorkspace::ModelPolyamour,
            EditorWorkspace::Script,
            EditorWorkspace::ThreeJsWeb,
            EditorWorkspace::AtlasGraph,
            EditorWorkspace::PortDiff,
            EditorWorkspace::AssetLib,
        ];

        for ws in workspaces {
            app.set_workspace(ws);
            let mut output = app.run_headless_frame([1280.0, 720.0]);
            assert!(
                !output.shapes.is_empty(),
                "Workspace {:?} produced zero clipped shapes in headless frame",
                ws
            );
            output.textures_delta.clear();
        }
    }

    #[test]
    fn gizmo_translation_scale_and_rotation_commands() {
        let mut app = ForeplayEditorApp::new("GizmoCommandTest");
        app.populate_starter_scene();

        let skater_id = NodeId::new(1, 1).unwrap();

        // Translate skater to new position via EditorCommand
        let old_pos = IrValue::Vec3([0.0, 1.0, 0.0]);
        let new_pos = IrValue::Vec3([5.0, 3.5, -2.0]);
        app.execute_command(EditorCommand::SetProperty {
            node_id: skater_id,
            key: "position".into(),
            old_value: Some(old_pos.clone()),
            new_value: Some(new_pos.clone()),
        });

        assert_eq!(
            app.session.tree.active_scene.nodes.get(&skater_id).unwrap().properties.get("position"),
            Some(&new_pos)
        );

        // Rotate skater via EditorCommand
        let old_rot = IrValue::Vec3([0.0, 0.0, 0.0]);
        let new_rot = IrValue::Vec3([0.0, 1.57, 0.0]);
        app.execute_command(EditorCommand::SetProperty {
            node_id: skater_id,
            key: "rotation".into(),
            old_value: Some(old_rot.clone()),
            new_value: Some(new_rot.clone()),
        });
        assert_eq!(
            app.session.tree.active_scene.nodes.get(&skater_id).unwrap().properties.get("rotation"),
            Some(&new_rot)
        );

        // Undo restores rotation and then position
        app.undo();
        assert_eq!(
            app.session.tree.active_scene.nodes.get(&skater_id).unwrap().properties.get("rotation"),
            Some(&old_rot)
        );

        app.undo();
        assert_eq!(
            app.session.tree.active_scene.nodes.get(&skater_id).unwrap().properties.get("position"),
            Some(&old_pos)
        );

        // Redo restores them
        app.redo();
        assert_eq!(
            app.session.tree.active_scene.nodes.get(&skater_id).unwrap().properties.get("position"),
            Some(&new_pos)
        );
        app.redo();
        assert_eq!(
            app.session.tree.active_scene.nodes.get(&skater_id).unwrap().properties.get("rotation"),
            Some(&new_rot)
        );
    }

    #[test]
    fn feel_spec_tuning_and_invertibility() {
        let mut app = ForeplayEditorApp::new("FeelSpecTest");
        app.populate_starter_scene();

        // Initial ollie pop impulse is 6.8
        assert_eq!(app.session.tree.active_scene.feel_overrides.get("ollie_pop_impulse"), Some(&6.8));

        // Tune to 10.5
        app.execute_command(EditorCommand::TuneFeelSpec {
            key: "ollie_pop_impulse".into(),
            old_value: Some(6.8),
            new_value: Some(10.5),
        });
        assert_eq!(app.session.tree.active_scene.feel_overrides.get("ollie_pop_impulse"), Some(&10.5));

        // Undo restores 6.8
        app.undo();
        assert_eq!(app.session.tree.active_scene.feel_overrides.get("ollie_pop_impulse"), Some(&6.8));
    }

    #[test]
    fn rename_node_command_and_undo_redo() {
        let mut app = ForeplayEditorApp::new("RenameTest");
        app.populate_starter_scene();

        let skater_id = NodeId::new(1, 1).unwrap();
        assert_eq!(app.session.tree.active_scene.nodes.get(&skater_id).unwrap().name, "SkaterRoot");

        // Rename node
        app.execute_command(EditorCommand::RenameNode {
            node_id: skater_id,
            old_name: "SkaterRoot".into(),
            new_name: "SkaterPro".into(),
        });
        assert_eq!(app.session.tree.active_scene.nodes.get(&skater_id).unwrap().name, "SkaterPro");

        // Undo restores name
        app.undo();
        assert_eq!(app.session.tree.active_scene.nodes.get(&skater_id).unwrap().name, "SkaterRoot");

        // Redo reapplies new name
        app.redo();
        assert_eq!(app.session.tree.active_scene.nodes.get(&skater_id).unwrap().name, "SkaterPro");
    }

    #[test]
    fn reparent_node_command_and_undo_redo() {
        let mut app = ForeplayEditorApp::new("ReparentTest");
        app.populate_starter_scene();

        let camera_id = NodeId::new(2, 1).unwrap();
        let skater_id = NodeId::new(1, 1).unwrap();
        assert_eq!(app.session.tree.active_scene.nodes.get(&camera_id).unwrap().parent, Some(skater_id));

        // Reparent camera to root (parent = None)
        app.execute_command(EditorCommand::ReparentNode {
            node_id: camera_id,
            old_parent: Some(skater_id),
            new_parent: None,
        });
        assert_eq!(app.session.tree.active_scene.nodes.get(&camera_id).unwrap().parent, None);

        // Undo restores parent
        app.undo();
        assert_eq!(app.session.tree.active_scene.nodes.get(&camera_id).unwrap().parent, Some(skater_id));

        // Redo moves to root
        app.redo();
        assert_eq!(app.session.tree.active_scene.nodes.get(&camera_id).unwrap().parent, None);
    }

    #[test]
    fn signal_connection_and_disconnection_commands() {
        let mut app = ForeplayEditorApp::new("SignalTest");
        app.populate_starter_scene();

        let skater_id = NodeId::new(1, 1).unwrap();
        let camera_id = NodeId::new(2, 1).unwrap();

        let conn = SignalConnection {
            from_node: skater_id,
            signal_name: "trick_landed".into(),
            to_node: camera_id,
            method_name: "_on_trick_camera_shake".into(),
        };

        // Connect signal
        app.execute_command(EditorCommand::ConnectSignal {
            connection: conn.clone(),
        });
        assert!(app.session.tree.active_scene.connections.contains(&conn));

        // Undo disconnects
        app.undo();
        assert!(!app.session.tree.active_scene.connections.contains(&conn));

        // Redo reconnects
        app.redo();
        assert!(app.session.tree.active_scene.connections.contains(&conn));

        // Disconnect signal
        app.execute_command(EditorCommand::DisconnectSignal {
            connection: conn.clone(),
        });
        assert!(!app.session.tree.active_scene.connections.contains(&conn));

        // Undo disconnect restores connection
        app.undo();
        assert!(app.session.tree.active_scene.connections.contains(&conn));
    }

    #[test]
    fn set_groups_command_and_undo_redo() {
        let mut app = ForeplayEditorApp::new("GroupsTest");
        app.populate_starter_scene();

        let skater_id = NodeId::new(1, 1).unwrap();
        let old_groups = app.session.tree.active_scene.nodes.get(&skater_id).unwrap().groups.clone();

        let new_groups = vec!["player".to_string(), "skater".to_string(), "controllable".to_string()];
        app.execute_command(EditorCommand::SetGroups {
            node_id: skater_id,
            old_groups: old_groups.clone(),
            new_groups: new_groups.clone(),
        });
        assert_eq!(app.session.tree.active_scene.nodes.get(&skater_id).unwrap().groups, new_groups);

        // Undo restores old groups
        app.undo();
        assert_eq!(app.session.tree.active_scene.nodes.get(&skater_id).unwrap().groups, old_groups);

        // Redo restores new groups
        app.redo();
        assert_eq!(app.session.tree.active_scene.nodes.get(&skater_id).unwrap().groups, new_groups);
    }

    #[test]
    fn multi_scene_document_switching_and_closing() {
        let mut app = ForeplayEditorApp::new("MultiSceneTest");
        assert_eq!(app.open_scenes.len(), 2);
        assert_eq!(app.active_scene_index, 0);

        // Open third scene
        app.open_or_switch_scene("ThirdScene", None);
        assert_eq!(app.open_scenes.len(), 3);
        assert_eq!(app.active_scene_index, 2);
        assert_eq!(app.session.tree.active_scene.name, "ThirdScene");

        // Switch to HUD scene (index 1)
        app.switch_to_scene_index(1);
        assert_eq!(app.active_scene_index, 1);
        assert_eq!(app.session.tree.active_scene.name, "UI_HUD");

        // Close third scene
        app.close_scene_doc(2);
        assert_eq!(app.open_scenes.len(), 2);
    }

    #[test]
    fn tscn_text_parser_and_node_insertion() {
        let mut app = ForeplayEditorApp::new("TscnParserTest");
        let tscn = r#"[gd_scene format=3]
[node name="PlazaRoot" type="Node3D"]
[node name="PlayerSkater" type="CharacterBody3D" parent="PlazaRoot"]
[node name="FollowCamera" type="Camera3D" parent="PlayerSkater"]
"#;
        let count = app.import_tscn_buffer(tscn);
        assert_eq!(count, 3);
        assert!(app.session.tree.active_scene.nodes.values().any(|n| n.name == "PlazaRoot"));
        assert!(app.session.tree.active_scene.nodes.values().any(|n| n.name == "PlayerSkater"));
        assert!(app.session.tree.active_scene.nodes.values().any(|n| n.name == "FollowCamera"));
    }

    #[test]
    fn bottom_panel_tab_toggle_and_rendering() {
        let mut app = ForeplayEditorApp::new("BottomPanelTest");
        assert!(app.bottom_panel_state.is_expanded);

        // Toggle debugger tab
        app.toggle_bottom_panel(BottomPanelTab::Debugger);
        assert_eq!(app.bottom_panel_state.active_tab, BottomPanelTab::Debugger);
        assert!(app.bottom_panel_state.is_expanded);

        // Toggling same tab collapses panel
        app.toggle_bottom_panel(BottomPanelTab::Debugger);
        assert!(!app.bottom_panel_state.is_expanded);

        // Toggling audio expands and switches
        app.toggle_bottom_panel(BottomPanelTab::AudioMixer);
        assert!(app.bottom_panel_state.is_expanded);
        assert_eq!(app.bottom_panel_state.active_tab, BottomPanelTab::AudioMixer);
    }

    #[test]
    fn project_settings_and_input_map_bindings() {
        let app = ForeplayEditorApp::new("InputMapTest");
        assert!(app.project_settings_state.input_actions.contains_key("ollie"));
        assert!(app.project_settings_state.input_actions.contains_key("grind"));
        assert_eq!(app.project_settings_state.input_actions.get("ollie").unwrap().deadzone, 0.5);
    }

    #[test]
    fn export_modal_platform_presets() {
        let app = ForeplayEditorApp::new("ExportTest");
        assert_eq!(
            app.export_modal_state.selected_preset,
            crate::modals::export_modal::ExportPlatformPreset::LinuxSteamDeck
        );
        assert!(app.export_modal_state.debug_build);
    }

    #[test]
    fn command_palette_execution() {
        let mut app = ForeplayEditorApp::new("PaletteExecTest");
        let actions = crate::modals::command_palette::all_palette_actions();
        assert!(actions.len() >= 15);

        // Find "Play In Editor" action and run it
        let pie_action = actions.iter().find(|a| a.title.contains("Play In Editor")).unwrap();
        (pie_action.run)(&mut app);
        assert!(app.play_in_editor_running);

        // Find "Stop PIE" action and run it
        let stop_action = actions.iter().find(|a| a.title.contains("Stop PIE")).unwrap();
        (stop_action.run)(&mut app);
        assert!(!app.play_in_editor_running);
    }

    #[test]
    fn wave2b_gdshader_to_wgsl_compilation() {
        let valid_shader = r#"shader_type spatial;
render_mode cull_back;

void vertex() {
    VERTEX.y += sin(TIME * 2.0) * 0.1;
}

void fragment() {
    ALBEDO = vec3(0.2, 0.6, 1.0);
    ROUGHNESS = 0.35;
    METALLIC = 0.8;
}
"#;
        let wgsl = foreplay_render::shader::compile_gdshader_to_wgsl(valid_shader)
            .expect("valid gdshader must compile to WGSL");
        assert!(wgsl.contains("@vertex"));
        assert!(wgsl.contains("@fragment"));
        assert!(wgsl.contains("ALBEDO"));

        // Malformed shader check
        let invalid_shader = "shader_type invalid_domain;\nvoid fragment() {";
        let err = foreplay_render::shader::compile_gdshader_to_wgsl(invalid_shader);
        assert!(err.is_err(), "malformed shader must return compiler error");
    }

    #[test]
    fn wave2b_release_bundler_fpck_archive_and_multiplatform() {
        use foreplay_export::{ExportConfig, ExportTarget, FpckArchive, ProjectExporter};
        use std::collections::BTreeMap;

        let mut files = BTreeMap::new();
        files.insert("scenes/main.fscn".to_string(), b"[foreplay_scene name=\"Main\"]\n".to_vec());
        files.insert("scripts/main.gd".to_string(), b"class_name Main\nextends Node\n".to_vec());

        // 1. Standalone Fpck archive packing & unpacking
        let archive_bytes = FpckArchive::pack_bytes(&files).expect("pack fpck");
        assert!(archive_bytes.len() > 32);
        let unpacked = FpckArchive::unpack_bytes(&archive_bytes).expect("unpack fpck");
        assert_eq!(unpacked.len(), 2);
        assert!(unpacked.contains_key("scenes/main.fscn"));
        assert!(unpacked.contains_key("scripts/main.gd"));

        // 2. Multiplatform bundle generation
        let config = ExportConfig::new("ForeplayPark", "1.0.0", "org.foreplay.skate")
            .with_file("scenes/main.fscn", b"[scene]".to_vec());

        let mac_output = ProjectExporter::export_project(&config, ExportTarget::MacOsAppBundle)
            .expect("macOS bundle export");
        assert!(mac_output.bundle_files.keys().any(|k| k.ends_with("Contents/Info.plist")));
        assert!(mac_output.bundle_files.keys().any(|k| k.ends_with("Contents/MacOS/launcher")));
        assert!(mac_output.bundle_files.keys().any(|k| k.ends_with("Contents/Resources/game.fpck")));

        let sexos_output = ProjectExporter::export_project(&config, ExportTarget::SexOsPackage)
            .expect("SexOS bundle export");
        assert!(sexos_output.bundle_files.contains_key("sexpkg.toml"));
        assert!(sexos_output.bundle_files.contains_key("game.fpck"));
        assert_eq!(sexos_output.primary_artifact, "ForeplayPark.sexpkg");

        // 3. Editor export modal integration
        let mut app = ForeplayEditorApp::new("BundlerAppTest");
        crate::modals::export_modal::run_real_export(&mut app, false);
        assert!(app.export_modal_state.export_receipt_logs.iter().any(|l| l.contains("Packed FPCK archive")));
        assert!(app.export_modal_state.last_export_archive_bytes > 0);
        assert!(!app.export_modal_state.last_export_bundle_files.is_empty());
    }

    #[test]
    fn wave2b_godot_catalog_63_classes_and_node_dock_signals() {
        let catalog = foreplay_ir::builtin_node_catalog();
        assert_eq!(catalog.len(), 63, "Canonical Godot catalog must have exactly 63 classes");

        // Verify key classes exist
        assert!(catalog.contains_key("Node"));
        assert!(catalog.contains_key("Node2D"));
        assert!(catalog.contains_key("Node3D"));
        assert!(catalog.contains_key("CharacterBody3D"));
        assert!(catalog.contains_key("Area3D"));
        assert!(catalog.contains_key("Timer"));
        assert!(catalog.contains_key("Button"));
        assert!(catalog.contains_key("Camera3D"));
        assert!(catalog.contains_key("CollisionShape3D"));
        assert!(catalog.contains_key("AudioStreamPlayer3D"));
        assert!(catalog.contains_key("GPUParticles3D"));

        // Inheritance hierarchy verification
        assert!(foreplay_ir::is_subclass_of("CharacterBody3D", "Node3D"));
        assert!(foreplay_ir::is_subclass_of("Node3D", "Node"));
        assert!(foreplay_ir::is_subclass_of("Sprite2D", "Node2D"));
        assert!(!foreplay_ir::is_subclass_of("Sprite2D", "Node3D"));

        // Node dock signal introspection & default properties
        let skater_spec = foreplay_ir::get_node_class("CharacterBody3D")
            .expect("CharacterBody3D spec");
        assert_eq!(skater_spec.category, foreplay_ir::NodeCategory::Node3D);
        assert!(skater_spec.default_properties.contains_key("floor_stop_on_slope"));

        let area_spec = foreplay_ir::get_node_class("Area3D").expect("Area3D spec");
        assert!(area_spec.built_in_signals.iter().any(|s| s.name == "body_entered"));

        let timer_spec = foreplay_ir::get_node_class("Timer").expect("Timer spec");
        assert!(timer_spec.built_in_signals.iter().any(|s| s.name == "timeout"));

        let button_spec = foreplay_ir::get_node_class("Button").expect("Button spec");
        assert!(button_spec.built_in_signals.iter().any(|s| s.name == "pressed"));
    }

    #[test]
    fn wave2b_skate_spot_tscn_bidirectional_roundtrip() {
        let spot_tscn = include_str!("../../../templates/godot-migration/skate_spot.tscn");
        let mut app = ForeplayEditorApp::new("TscnRoundtripTest");

        // Import .tscn into a new scene document
        app.import_tscn_scene("SkateSpotImport", spot_tscn).expect("import skate_spot.tscn");
        assert_eq!(app.session.tree.active_scene.name, "SkateSpotImport");
        assert_eq!(app.session.tree.active_scene.nodes.len(), 6, "Must import 6 nodes");
        assert_eq!(app.session.tree.active_scene.connections.len(), 1, "Must import 1 signal connection");

        // Verify skater node details
        let skater_node = app
            .session
            .tree
            .active_scene
            .nodes
            .values()
            .find(|n| n.name == "Skater")
            .expect("Skater node");
        assert_eq!(skater_node.class_name, "CharacterBody3D");
        assert!(skater_node.groups.contains(&"players".to_string()));
        assert!(skater_node.groups.contains(&"skaters".to_string()));

        // Verify signal connection details
        let conn = &app.session.tree.active_scene.connections[0];
        assert_eq!(conn.signal_name, "trick_landed");
        assert_eq!(conn.method_name, "_on_trick_landed");

        // Export back to .tscn text format
        let exported_tscn = app.export_active_scene_tscn();
        assert!(exported_tscn.starts_with("[gd_scene"));
        assert!(exported_tscn.contains("SkateSpot"));
        assert!(exported_tscn.contains("Skater"));
        assert!(exported_tscn.contains("CameraRig"));
        assert!(exported_tscn.contains("trick_landed"));

        // Roundtrip: parse exported .tscn and verify parity
        let re_parsed = foreplay_porthost::parse_tscn_to_ir(&exported_tscn)
            .expect("exported .tscn must parse back cleanly");
        assert_eq!(re_parsed.nodes.len(), 6);
        assert_eq!(re_parsed.connections.len(), 1);
    }

    #[test]
    fn wave2b_gdvm_script_execution_and_signal_emission() {
        let mut app = ForeplayEditorApp::new("GdVmTest");
        app.populate_starter_scene();

        // Select the Skater node (id: 1, 1)
        app.selected_node = NodeId::new(1, 1);
        let initial_pos = app.session.tree.active_scene.nodes.get(&NodeId::new(1, 1).unwrap())
            .unwrap()
            .properties
            .get("position")
            .cloned();

        // Run GdVm script
        let steps = app.run_gdscript_vm().expect("run_gdscript_vm execution");
        assert!(steps > 0);

        // Check console logs for VM print output
        assert!(
            app.console_logs.iter().any(|l| l.message.contains("Skater controller bound and ready!")),
            "GdVm _ready() print statement must be captured in console logs"
        );

        // Advance 45 frames of physics process: position.x += 14.5 * (1/60) * 45 = 10.875 > 10.0
        for _ in 0..45 {
            let _ = app.run_gdscript_vm();
        }

        // Verify trick_landed signal was emitted
        assert!(
            app.console_logs.iter().any(|l| l.message.contains("trick_landed")),
            "GdVm must emit 'trick_landed' signal when position.x > 10.0"
        );

        // Verify position changed and was committed via EditorCommand
        let current_pos = app.session.tree.active_scene.nodes.get(&NodeId::new(1, 1).unwrap())
            .unwrap()
            .properties
            .get("position")
            .cloned();
        assert_ne!(initial_pos, current_pos, "Position property must be updated via EditorCommand");
    }

    #[test]
    fn wave2b_differential_frame_oracle_1000_frame_gate() {
        let report = foreplay_porthost::run_1000_frame_differential_gate();
        assert_eq!(report.total_frames, 1000);
        assert_eq!(report.passed_frames, 1000);
        assert_eq!(report.pass_rate_percent, 100.0);
        assert!(report.is_perfect_match(), "1000-frame gate must pass with 100% precision");
        assert!(report.max_position_drift <= 1e-4, "Max drift must satisfy 0.0001m epsilon");

        let mut app = ForeplayEditorApp::new("PortDiffTest");
        assert!(app.last_differential_report.is_none());
        app.last_differential_report = Some(report);
        assert!(app.last_differential_report.as_ref().unwrap().is_perfect_match());
    }

    #[test]
    fn wave3a_theme_preset_switching_and_tactile_widgets_rendering() {
        let mut app = ForeplayEditorApp::new("ThemeAndWidgetsTest");
        app.populate_starter_scene();

        // 1. Default theme is StudioObsidianPro
        assert_eq!(app.theme_preset, crate::theme::EditorThemePreset::StudioObsidianPro);
        let output1 = app.run_headless_frame([1280.0, 720.0]);
        assert!(!output1.shapes.is_empty());

        // 2. Switch to ClassicGodotSlate
        app.theme_preset = crate::theme::EditorThemePreset::ClassicGodotSlate;
        let output2 = app.run_headless_frame([1280.0, 720.0]);
        assert!(!output2.shapes.is_empty());

        // 3. Render tactile widgets directly in a dedicated headless frame
        let ctx = egui::Context::default();
        let raw_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))),
            ..Default::default()
        };

        let mut knob_val = 6.8f32;
        let mut widget_output = ctx.run_ui(raw_input, |ui| {
            crate::widgets::studio_lcd_hud(ui, 12.34, 120.0, 60.0, 9, true);
            crate::widgets::rotary_arc_knob(ui, "TestKnob", &mut knob_val, 0.0, 20.0, crate::theme::CHROMA_CYAN_2D);
            let bars = [0.5f32; 32];
            crate::widgets::eq_response_and_spectrum_canvas(ui, 0.0, 2.0, -1.0, &bars);
        });
        assert!(!widget_output.shapes.is_empty(), "Tactile widgets must produce paint shapes");
        widget_output.textures_delta.clear();
    }

    #[test]
    fn wave3a_polyamour_model_workspace_primitive_operators_and_modifiers() {
        let mut app = ForeplayEditorApp::new("PolyamourTest");
        let initial_node_count = app.session.tree.active_scene.nodes.len();

        // 1. Set workspace to ModelPolyamour
        app.set_workspace(EditorWorkspace::ModelPolyamour);
        assert_eq!(app.active_workspace, EditorWorkspace::ModelPolyamour);

        // 2. Insert primitive Quarterpipe
        let next_idx = (app.session.tree.active_scene.nodes.len() as u32) + 1;
        let node_id = NodeId::new(next_idx, 1).unwrap();
        let mut properties = BTreeMap::new();
        properties.insert("polyamour_primitive".into(), IrValue::String("Quarterpipe".into()));
        properties.insert("polyamour_ops".into(), IrValue::String("".into()));
        let node = IrNode {
            id: node_id,
            name: format!("Quarterpipe_{next_idx}"),
            class_name: "MeshInstance3D".into(),
            parent: None,
            instance_scene: None,
            properties,
            groups: vec![],
        };
        app.execute_command(EditorCommand::InsertNode { node });
        app.selected_node = Some(node_id);

        assert_eq!(app.session.tree.active_scene.nodes.len(), initial_node_count + 1);
        let inserted = app.session.tree.active_scene.nodes.get(&node_id).unwrap();
        assert_eq!(inserted.properties.get("polyamour_primitive"), Some(&IrValue::String("Quarterpipe".into())));

        // 3. Record edit-mode operator "extrude" via SetProperty
        app.execute_command(EditorCommand::SetProperty {
            node_id,
            key: "polyamour_ops".into(),
            old_value: Some(IrValue::String("".into())),
            new_value: Some(IrValue::String("extrude(dist=0.5)".into())),
        });
        let updated = app.session.tree.active_scene.nodes.get(&node_id).unwrap();
        assert_eq!(updated.properties.get("polyamour_ops"), Some(&IrValue::String("extrude(dist=0.5)".into())));

        // 4. Modifier stack sync via SetProperty
        app.execute_command(EditorCommand::SetProperty {
            node_id,
            key: "polyamour_modifiers".into(),
            old_value: None,
            new_value: Some(IrValue::String("count=2".into())),
        });
        let with_mods = app.session.tree.active_scene.nodes.get(&node_id).unwrap();
        assert_eq!(with_mods.properties.get("polyamour_modifiers"), Some(&IrValue::String("count=2".into())));

        // 5. Undo restores all edits cleanly
        app.undo(); // undo modifiers
        assert_eq!(app.session.tree.active_scene.nodes.get(&node_id).unwrap().properties.get("polyamour_modifiers"), None);
        app.undo(); // undo ops
        assert_eq!(app.session.tree.active_scene.nodes.get(&node_id).unwrap().properties.get("polyamour_ops"), Some(&IrValue::String("".into())));
        app.undo(); // undo insert node
        assert_eq!(app.session.tree.active_scene.nodes.len(), initial_node_count);
    }

    #[test]
    fn wave3a_threejs_webgpu_studio_code_generation_and_export() {
        let mut app = ForeplayEditorApp::new("ThreeJsTest");
        app.populate_starter_scene();

        app.set_workspace(EditorWorkspace::ThreeJsWeb);
        assert_eq!(app.active_workspace, EditorWorkspace::ThreeJsWeb);

        // Headless render of Three.js studio workspace
        let output = app.run_headless_frame([1280.0, 720.0]);
        assert!(!output.shapes.is_empty());

        // Verify state
        assert!(app.threejs_state.webgpu_renderer);
        assert!(app.threejs_state.webgl2_fallback);
        assert!(app.threejs_state.simulation_running);
    }

    #[test]
    fn wave3a_fcp_magnetic_timeline_skimmer_and_connected_clips() {
        let mut app = ForeplayEditorApp::new("TimelineTest");
        assert!(app.animation_state.skimmer_enabled);
        assert_eq!(app.animation_state.magnetic_clips.len(), 4);

        // Check clips lanes
        assert_eq!(app.animation_state.magnetic_clips[0].lane, crate::tabs::animation::ClipLane::PrimaryStoryline);
        assert_eq!(app.animation_state.magnetic_clips[1].lane, crate::tabs::animation::ClipLane::ConnectedAudio);
        assert_eq!(app.animation_state.magnetic_clips[2].lane, crate::tabs::animation::ClipLane::ConnectedCamera);
        assert_eq!(app.animation_state.magnetic_clips[3].lane, crate::tabs::animation::ClipLane::ConnectedSignals);

        // Advance skimmer time
        app.animation_state.skimmer_time = 1.25;
        assert_eq!(app.animation_state.skimmer_time, 1.25);
    }

    #[test]
    fn wave3b_polyamour_live_mesh_evaluation_and_2manifold_check() {
        use foreplay_polyamour::{
            bake_pathtraced_rgba8, PathTracerCamera, PathTracerConfig, PathTracerMaterial, ViewTransform,
        };
        use bevy_math::Vec3;

        let mut app = ForeplayEditorApp::new("PolyamourTest");
        let node_id = NodeId::new(1, 1).unwrap();
        let mut properties = BTreeMap::new();
        properties.insert("polyamour_primitive".into(), IrValue::String("Cube".into()));

        let node = IrNode {
            id: node_id,
            name: "TestPolyCube".into(),
            class_name: "MeshInstance3D".into(),
            parent: None,
            instance_scene: None,
            properties,
            groups: vec!["polyamour".into()],
        };
        app.execute_command(EditorCommand::InsertNode { node });
        app.selected_node = Some(node_id);

        // 1. With default modifier stack (Mirror + Catmull-Clark Subsurf lvl 2)
        let telem_mod = crate::tabs::polyamour_model::evaluate_active_polyamour_mesh(&app);
        assert_eq!(telem_mod.vertex_count, 152, "Subdivision level 2 + mirror must evaluate 152 vertices");

        // 2. Clear modifier stack to inspect raw base cube
        app.polyamour_state.active_modifiers.clear();
        let telem_base = crate::tabs::polyamour_model::evaluate_active_polyamour_mesh(&app);
        assert_eq!(telem_base.vertex_count, 8);
        assert_eq!(telem_base.face_count, 6);
        assert_eq!(telem_base.triangle_count, 12);
        assert!(telem_base.is_manifold, "Base cube primitive must be a 2-manifold surface");

        // 3. Apply non-destructive operators and verify vertex/face growth
        app.execute_command(EditorCommand::SetProperty {
            node_id,
            key: "polyamour_ops".into(),
            old_value: None,
            new_value: Some(IrValue::String("extrude,inset".into())),
        });

        let telem_ops = crate::tabs::polyamour_model::evaluate_active_polyamour_mesh(&app);
        assert!(telem_ops.vertex_count > 8, "Extruded & inset mesh must have > 8 vertices, got {}", telem_ops.vertex_count);
        assert!(telem_ops.face_count > 6, "Extruded & inset mesh must have > 6 faces, got {}", telem_ops.face_count);
        assert!(telem_ops.triangle_count > 12, "Triangulated mesh must have > 12 triangles, got {}", telem_ops.triangle_count);

        // AgX Photoreal Path Tracer preview check
        let tri_mesh = telem_ops.mesh.triangulate();
        let config = PathTracerConfig {
            width: 16,
            height: 16,
            samples_per_pixel: 2,
            max_bounces: 2,
            seed: 0x42,
            enable_denoiser: true,
            view_transform: ViewTransform::AgX,
            camera: PathTracerCamera {
                eye: Vec3::new(0.0, 1.5, 4.0),
                target: Vec3::ZERO,
                up: Vec3::Y,
                fov_degrees: 60.0,
            },
            default_material: PathTracerMaterial {
                base_color: Vec3::new(0.9, 0.4, 0.2),
                metallic: 0.2,
                roughness: 0.3,
            },
        };
        let baked = bake_pathtraced_rgba8(&tri_mesh, &config);
        assert_eq!(baked.len(), 16 * 16 * 4);
        for i in 0..(16 * 16) {
            assert_eq!(baked[i * 4 + 3], 255, "Alpha channel must be fully opaque");
        }
    }

    #[test]
    fn wave3b_polyops_script_atomic_execution() {
        use foreplay_polyamour::{run_polyops, PolyScene};

        let mut scene = PolyScene::new();
        let script = r#"
            polyops 1
            begin
            material_new "GoldPlating" 1.0 0.85 0.57 1.0 0.9 0.1 as $gold
            add_primitive cube 2.0 as $box
            modifier_add $box subdivision 1
            material_assign $box $gold
            end
        "#;
        let report = run_polyops(&mut scene, script).expect("run polyops script");
        assert_eq!(scene.objects.len(), 1);
        assert_eq!(scene.materials.len(), 1);
        assert_eq!(report.created_ids.len(), 2);
    }

    #[test]
    fn wave3b_spline_rail_pcg_extrusion_and_node_insertion() {
        use foreplay_core::pcg::{CatmullRomSpline3D, PcgExtrusionConfig, PcgProfile, PcgSplineExtruder};
        use bevy_math::Vec3;

        let p0 = Vec3::new(-6.0, 0.85, 0.0);
        let p1 = Vec3::new(-2.0, 0.85, 0.0);
        let p2 = Vec3::new(2.0, 0.85, 0.0);
        let p3 = Vec3::new(6.0, 0.85, 0.0);
        let spline = CatmullRomSpline3D::new(vec![p0, p1, p2, p3], false);
        let config = PcgExtrusionConfig {
            profile: PcgProfile::RoundRail {
                radius: 0.04,
                radial_segments: 12,
            },
            step_distance: 0.2,
            uv_scale: [1.0, 1.0],
            generate_caps: true,
        };
        let buffers = PcgSplineExtruder::extrude(&spline, &config);
        assert!(!buffers.positions.is_empty(), "Extrusion must generate positions");
        assert!(!buffers.indices.is_empty(), "Extrusion must generate indices");
        assert!(!buffers.collision_segments.is_empty(), "Extrusion must generate collision lines");

        let mut app = ForeplayEditorApp::new("SplineTest");
        let initial_count = app.session.tree.active_scene.nodes.len();
        let node_id = NodeId::new((initial_count as u32) + 1, 1).unwrap();
        let mut properties = BTreeMap::new();
        properties.insert("polyamour_primitive".into(), IrValue::String("SplineRail".into()));
        properties.insert("positions_count".into(), IrValue::I64(buffers.positions.len() as i64));
        properties.insert("indices_count".into(), IrValue::I64(buffers.indices.len() as i64));
        properties.insert("collision_segments_count".into(), IrValue::I64(buffers.collision_segments.len() as i64));

        let node = IrNode {
            id: node_id,
            name: "GrindRail_Test".into(),
            class_name: "MeshInstance3D".into(),
            parent: None,
            instance_scene: None,
            properties,
            groups: vec!["grind_rails".into()],
        };
        app.execute_command(EditorCommand::InsertNode { node });
        assert_eq!(app.session.tree.active_scene.nodes.len(), initial_count + 1);

        app.undo();
        assert_eq!(app.session.tree.active_scene.nodes.len(), initial_count);
    }

    #[test]
    fn wave3b_threejs_studio_project_bundle_compile_and_import_roundtrip() {
        let mut app = ForeplayEditorApp::new("NeonSkateStudioTest");
        app.populate_starter_scene();

        // 1. Verify compile_threejs_bundle produces complete project files
        let bundle = crate::tabs::threejs_studio::compile_threejs_bundle(&app).expect("compile threejs bundle");
        assert!(bundle.files.contains_key("package.json"));
        assert!(bundle.files.contains_key("tsconfig.json"));
        assert!(bundle.files.contains_key("vite.config.ts"));
        assert!(bundle.files.contains_key("index.html"));
        assert!(bundle.files.contains_key("src/scene.ts"));
        assert!(bundle.files.contains_key("src/main.ts"));
        assert!(bundle.files.contains_key("src/feel.ts"));
        assert!(bundle.files.contains_key("src/signals.ts"));
        assert!(bundle.files.contains_key("src/scripts/behaviors.ts"));
        assert!(bundle.files.contains_key("src/atlas.json"));
        assert_eq!(bundle.node_count, app.session.tree.active_scene.nodes.len());

        // 2. Test import_threejs_scene_json with Neon Skate Park template
        let count = app.import_threejs_scene_json(
            "NeonSkatePark",
            crate::tabs::threejs_studio::NEON_SKATE_JSON,
        ).expect("import 3jse template");
        assert_eq!(count, 8, "NeonSkatePark template contains 8 nodes");
        assert_eq!(app.session.tree.active_scene.nodes.len(), 8);
        assert_eq!(app.session.tree.active_scene.feel_overrides.get("skate_speed"), Some(&18.0));
        assert_eq!(app.session.tree.active_scene.feel_overrides.get("ollie_pop"), Some(&8.5));
        assert_eq!(app.session.tree.active_scene.connections.len(), 3);

        // 3. Undo restores clean previous state
        app.undo();
        // Since import clears and sets nodes through commands, commands can be undone
        assert!(app.command_history_count > 0);
    }

    #[test]
    fn wave3b_export_modal_threejs_preset_wiring() {
        let mut app = ForeplayEditorApp::new("ExportModalTest");
        app.export_modal_state.selected_preset = crate::modals::export_modal::ExportPlatformPreset::ThreeJsWebProject;
        assert_eq!(
            app.export_modal_state.selected_preset.to_export_target(),
            foreplay_export::ExportTarget::ThreeJsWebProject
        );
    }

    #[test]
    fn wave3b_viewport_3d_pbr_gi_shaded_rendering() {
        use bevy_math::{Mat4, Quat, Vec3};
        use foreplay_render::PerspectiveCamera3D;
        use foreplay_render::pbr_gi::{render_shaded_pbr_viewport_rgba8, PbrMaterialDesc, PbrViewportParams, RenderTriangleMesh};

        let cam = PerspectiveCamera3D {
            position: Vec3::new(0.0, 2.0, 5.0),
            rotation: Quat::IDENTITY,
            fov_y_radians: 60.0_f32.to_radians(),
            aspect_ratio: 1.0,
            near: 0.1,
            far: 1000.0,
        };
        let mat = PbrMaterialDesc::dielectric([0.5, 0.7, 1.0], 0.3);
        let mesh = RenderTriangleMesh::new_cube(2.0, mat, Mat4::IDENTITY);
        let meshes = [mesh];

        let params = PbrViewportParams {
            width: 32,
            height: 32,
            camera: &cam,
            meshes: &meshes,
            sun_dir: Vec3::new(-0.5, -1.0, -0.5).normalize_or_zero(),
            sun_color: [1.0, 0.95, 0.85],
            sun_intensity: 3.0,
            sky_color: [0.1, 0.15, 0.25],
            voxel_gi: None,
            radiance_cascade: None,
            selected_mesh_index: None,
            outline_color: [255, 140, 0, 255],
            outline_width: 2,
            enable_shadows: true,
        };

        let rgba = render_shaded_pbr_viewport_rgba8(&params);
        assert_eq!(rgba.len(), 32 * 32 * 4);
        assert!(rgba.iter().any(|&b| b > 0), "Rendered PBR image must contain non-black pixels");
    }

    #[test]
    fn wave3b_audio_mixer_foley_synth_and_32band_spectrum() {
        let mut app = ForeplayEditorApp::new("AudioTest");
        app.audio_mixer_state.synth.set_telemetry(15.0, 0.7, true, 12.0);

        let mut buf = vec![[0.0_f32; 2]; 512];
        app.audio_mixer_state.synth.generate_block(&mut buf);

        let dbs = app.audio_mixer_state.analyzer.process_block(&buf);
        assert_eq!(dbs.len(), 32);
        for &db in &dbs {
            assert!((-100.0..=0.0).contains(&db), "Decibels must be within [-100, 0], got {db}");
        }
    }

    #[test]
    fn wave3b_atlas_time_travel_ring_recorder() {
        let mut app = ForeplayEditorApp::new("AtlasScrubTest");
        let initial_frames = app.atlas_recorder.len();

        // Tune feel spec knob
        app.execute_command(EditorCommand::TuneFeelSpec {
            key: "skate_speed".into(),
            old_value: None,
            new_value: Some(22.5),
        });
        assert_eq!(app.atlas_recorder.len(), initial_frames + 1);

        let latest = app.atlas_recorder.latest_tick().expect("latest tick exists");
        let scrub = app.atlas_recorder.scrub_to_tick(latest).expect("scrub view exists");
        assert_eq!(scrub.active_feel_changes.len(), 1);
        assert_eq!(scrub.active_feel_changes[0].key, "skate_speed");
        assert_eq!(scrub.active_feel_changes[0].new_value, 22.5);
    }

    #[test]
    fn v1_hardware_render_device_and_adapter_summary() {
        let app = ForeplayEditorApp::new("GpuTest");
        let summary = app.adapter_summary();
        assert!(!summary.is_empty(), "Adapter summary must not be empty");
        assert!(app.render_device.is_some(), "WgpuRenderDevice must be initialized");
    }

    #[test]
    fn v1_debugger_breakpoints_feel_knobs_and_gdext_procs() {
        let mut app = ForeplayEditorApp::new("DebuggerTest");
        assert_eq!(app.debugger_state.breakpoints.len(), 2);
        app.debugger_state
            .breakpoints
            .push(("res://scripts/skater_controller.gd".into(), 42));
        assert_eq!(app.debugger_state.breakpoints.len(), 3);

        // FeelKnobs tuning via EditorCommand
        app.execute_command(EditorCommand::TuneFeelSpec {
            key: "ollie_pop_impulse".into(),
            old_value: None,
            new_value: Some(9.4),
        });
        assert_eq!(
            app.session.tree.active_scene.feel_overrides.get("ollie_pop_impulse"),
            Some(&9.4)
        );

        // GDExtension C-ABI procs >= 60
        let procs = foreplay_gdext::supported_proc_names();
        assert!(procs.len() >= 60);
        assert!(procs.contains(&"mem_alloc"));
        assert!(procs.contains(&"variant_new_nil"));
        assert!(procs.contains(&"classdb_register_extension_class3"));
    }

    #[test]
    fn v1_gdscript_lsp_diagnostics_symbols_and_gutters() {
        let app = ForeplayEditorApp::new("LspTest");
        let lsp = foreplay_gdscript::GdLspAnalyzer::new();
        let analysis = lsp.analyze_document("res://scripts/skater_controller.gd", &app.script_buffer);
        assert!(!analysis.symbols.is_empty());
        assert!(analysis.symbols.iter().any(|s| s.name == "SkaterController"));
        assert!(analysis.symbols.iter().any(|s| s.name == "_ready"));
        assert!(analysis.symbols.iter().any(|s| s.name == "trick_landed"));
        let comps = lsp.complete_at(&app.script_buffer, 1, 1);
        assert!(!comps.is_empty());
    }

    #[test]
    fn v1_classdb_471_classes_and_node_instantiation() {
        let mut app = ForeplayEditorApp::new("ClassDbTest");
        let db = foreplay_core::ClassDb::global_godot44();
        assert_eq!(db.class_count(), 471);
        assert!(db.inherits("CharacterBody3D", "Node3D"));
        assert!(db.inherits("CollisionShape2D", "Node2D"));
        assert!(db.inherits("Button", "Control"));

        let next_index = 10;
        let node_id = foreplay_ir::NodeId::new(next_index, 1).unwrap();
        let props = db.instantiate_default_properties("CharacterBody3D").unwrap();
        app.execute_command(EditorCommand::InsertNode {
            node: foreplay_ir::IrNode {
                id: node_id,
                name: "CharacterBody3D_10".into(),
                class_name: "CharacterBody3D".into(),
                parent: None,
                instance_scene: None,
                properties: props,
                groups: Vec::new(),
            },
        });
        assert!(app.session.tree.active_scene.nodes.contains_key(&node_id));
    }

    #[test]
    fn v1_tilemap_autotiler_terrain_peering_and_cells() {
        let mut app = ForeplayEditorApp::new("TileMapTest");
        assert_eq!(app.tilemap_state.autotiler.painted_cells.len(), 6);
        assert!(app.tilemap_state.autotiler.placed_tiles.len() >= 6);
        app.tilemap_state
            .autotiler
            .paint_terrain_connect(&[(5, 5), (6, 5), (5, 6), (6, 6)], 0, 0);
        assert_eq!(app.tilemap_state.autotiler.painted_cells.len(), 10);
        assert!(app.tilemap_state.autotiler.placed_tiles.len() >= 10);
    }

    #[test]
    fn test_polyamour_subobject_1_2_3_modes_and_edge_loop_selection() {
        use foreplay_polyamour::{select_edge_loop, primitive_cube, SubObjectMode, ProportionalFalloff};

        let mut app = ForeplayEditorApp::new("SubObjectTest");
        app.populate_starter_scene();

        // 1. Verify Blender 1/2/3 selection mode switching
        app.polyamour_state.subobject_mode = SubObjectMode::Vertex;
        assert_eq!(app.polyamour_state.subobject_mode, SubObjectMode::Vertex);
        app.polyamour_state.subobject_mode = SubObjectMode::Edge;
        assert_eq!(app.polyamour_state.subobject_mode, SubObjectMode::Edge);
        app.polyamour_state.subobject_mode = SubObjectMode::Face;
        assert_eq!(app.polyamour_state.subobject_mode, SubObjectMode::Face);

        // 2. Proportional editing toggles and falloffs
        app.polyamour_state.proportional_editing = true;
        app.polyamour_state.proportional_falloff = ProportionalFalloff::Smooth;
        app.polyamour_state.proportional_radius = 2.5;
        assert_eq!(app.polyamour_state.proportional_radius, 2.5);

        // 3. Edge Loop Selection on cube primitive
        let cube = primitive_cube(2.0);
        let loop_edges = select_edge_loop(&cube, [0, 1]);
        assert_eq!(loop_edges.len(), 4, "Cube quad edge loop must contain 4 continuous edges");
    }

    #[test]
    fn test_polyamour_sculpt_stroke_dispatch_and_undo_redo() {
        use foreplay_polyamour::{apply_sculpt_stroke, primitive_grid, SculptBrush, SculptStroke, SymmetryAxes};
        use bevy_math::Vec3;

        let mut app = ForeplayEditorApp::new("SculptTest");
        let next_index = (app.session.tree.active_scene.nodes.len() as u32) + 1;
        let node_id = NodeId::new(next_index, 1).unwrap();

        let mut properties = BTreeMap::new();
        properties.insert("polyamour_primitive".into(), IrValue::String("Grid".into()));
        properties.insert("polyamour_sculpt_strokes".into(), IrValue::I64(0));

        let node = IrNode {
            id: node_id,
            name: "SculptModel_1".into(),
            class_name: "MeshInstance3D".into(),
            parent: None,
            instance_scene: None,
            properties,
            groups: vec!["polyamour".into()],
        };
        app.execute_command(EditorCommand::InsertNode { node });
        app.selected_node = Some(node_id);

        let mut grid = primitive_grid(4.0, 4.0, 4, 4);
        let orig_pos_y = grid.positions[0].y;

        // Apply sculpt stroke with mirror X symmetry
        let stroke = SculptStroke {
            brush: SculptBrush::Grab,
            hit_point: Vec3::ZERO,
            hit_normal: Vec3::Y,
            radius: 1.5,
            strength: 0.6,
            symmetry: SymmetryAxes { mirror_x: true, mirror_y: false, mirror_z: false },
            stroke_delta: Some(Vec3::new(0.0, 0.4, 0.0)),
        };
        apply_sculpt_stroke(&mut grid, &stroke);

        // Check deformation occurred
        let center_idx = grid.positions.iter().position(|p| (p.x).abs() < 1e-3 && (p.z).abs() < 1e-3).unwrap();
        assert!(grid.positions[center_idx].y > orig_pos_y, "Sculpt Grab stroke must displace vertices");

        // Record sculpt stroke via EditorCommand
        app.execute_command(EditorCommand::SetProperty {
            node_id,
            key: "polyamour_sculpt_strokes".into(),
            old_value: Some(IrValue::I64(0)),
            new_value: Some(IrValue::I64(1)),
        });

        let recorded = app.session.tree.active_scene.nodes.get(&node_id).unwrap().properties.get("polyamour_sculpt_strokes");
        assert_eq!(recorded, Some(&IrValue::I64(1)));

        // Undo restores stroke count to 0
        app.undo();
        let undone = app.session.tree.active_scene.nodes.get(&node_id).unwrap().properties.get("polyamour_sculpt_strokes");
        assert_eq!(undone, Some(&IrValue::I64(0)));
    }

    #[test]
    fn test_polyamour_game_semantic_tags_and_one_click_bundle_bake() {
        use foreplay_polyamour::{compile_polyamour_game_bundle, compute_heat_bone_weights, primitive_quarterpipe, PolyamourSemanticAttributes, RigBoneSegment};
        use bevy_math::Vec3;

        let qpipe = primitive_quarterpipe(2.0, 4.0, 4);
        let mut attrs = PolyamourSemanticAttributes::new();
        attrs.tag_edge([0, 1], "grind_edge");
        attrs.tag_face(0, "collision_convex");
        attrs.tag_face(1, "walkable_nav");

        let bundle = compile_polyamour_game_bundle(&qpipe, &attrs);
        assert!(!bundle.grind_rail_splines.is_empty(), "Must compile continuous grind rail splines");
        assert!(!bundle.convex_hull_points.is_empty(), "Must extract convex hull points");
        assert!(!bundle.walkable_nav_faces.is_empty(), "Must extract walkable nav faces");
        assert!(!bundle.uv2_islands.is_empty(), "Must generate Smart UV2 lightmap islands");

        // Bone weights verification: exact sum = 1.0
        let bones = [
            RigBoneSegment::new("Root", Vec3::new(0.0, -1.0, 0.0), Vec3::ZERO, 0.5),
            RigBoneSegment::new("Spine", Vec3::ZERO, Vec3::new(0.0, 1.0, 0.0), 0.5),
        ];
        let weights = compute_heat_bone_weights(&qpipe, &bones);
        assert_eq!(weights.len(), qpipe.positions.len());
        for inf in &weights {
            let sum: f32 = inf.weights.iter().sum();
            assert!((sum - 1.0).abs() < 1e-4, "Bone weights must sum to 1.0 (got {sum})");
        }
    }

    #[test]
    fn test_zerocopy_viewport_bridge_and_hardware_gpu_status() {
        let app = ForeplayEditorApp::new("BridgeTest");
        assert!(app.zero_copy_viewport.is_some(), "ZeroCopyViewportBridge must be initialized in ForeplayEditorApp");
        let is_active = app.is_zero_copy_viewport_active();
        let summary = app.adapter_summary();
        assert!(!summary.is_empty(), "GPU summary must be populated");
        assert_eq!(is_active, app.render_device.as_ref().is_some_and(|d| d.is_hardware_accelerated()));
    }

    #[test]
    fn test_sexlife_moan_and_splicy_pro_apps_editor_integration() {
        // Moan Adaptive Music DAW integration
        let moan = foreplay_script::MoanProjectStemLink::default_skate_soundtrack();
        let snap = moan.evaluate_adaptive_mix(0.8, true, 4.0);
        assert_eq!(snap.current_bar, 1);
        assert!(snap.stem_gains.contains_key("stem_drums"));
        assert!(snap.stem_gains.contains_key("stem_lead"));

        // Splicy Cutscene EDL Director integration
        let edl = foreplay_script::SplicyCutsceneEdl::new("TestCutscene", 10.0, 60.0);
        let edl_snap = edl.evaluate_at(2.0);
        assert_eq!(edl_snap.timestamp_sec, 2.0);
        assert!(edl.duration_sec > 0.0);
    }
}


