//! Foreplay Editor (`foreplay-editor`) — Godot-parity dockable workspaces built over `HarnessSession`.

pub mod camera;
pub mod demo_scene;
pub mod tabs;
pub mod theme;
pub mod transport;

use std::time::Instant;
use egui::{Key, Ui};
use egui_dock::DockState;
use foreplay_harness::HarnessSession;
use foreplay_ir::{EditorCommand, IrScene, NodeId};
use crate::camera::{Camera2D, Camera3D, GizmoMode};
use crate::tabs::console::{ConsoleLogEntry, LogLevel};
use crate::tabs::inspector::PropTypeChoice;
use crate::tabs::{build_default_dock_state, EditorTab, EditorTabViewer};

/// Top-bar workspace tabs matching Godot 4 + 3JSE Atlas + RustPorts Differential Inspector.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum EditorWorkspace {
    Workspace2D,
    Workspace3D,
    Script,
    AtlasGraph,
    PortDiff,
    AssetLib,
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
}

impl std::fmt::Debug for ForeplayEditorApp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ForeplayEditorApp")
            .field("active_workspace", &self.active_workspace)
            .field("scene_name", &self.session.tree.active_scene.name)
            .field("play_in_editor_running", &self.play_in_editor_running)
            .field("selected_node", &self.selected_node)
            .finish()
    }
}

impl ForeplayEditorApp {
    /// Constructs a new editor app instance for the given project name.
    pub fn new(project_name: impl Into<String>) -> Self {
        let name_str = project_name.into();
        let scene = IrScene::new(name_str.clone());
        let session = HarnessSession::new(scene);

        let script_sample = r#"extends CharacterBody3D
# Foreplay Game Engine — Skate Character Controller
# Mirrors Godot 4 CharacterBody3D API

@export var max_speed: float = 14.5
@export var ollie_pop: float = 6.8
@export var grind_magnetism: float = 0.35

signal trick_landed(trick_name: String, score: int)
signal rail_grind_started(rail_node: NodePath)

func _physics_process(delta: float) -> void:
    # 1. Apply FeelSpec tuned gravity
    if not is_on_floor():
        velocity.y -= ProjectSettings.get_setting("physics/3d/default_gravity") * delta

    # 2. Pop ollie with instantaneous impulse
    if is_on_floor() and Input.is_action_just_pressed("ollie"):
        velocity.y = ollie_pop
        emit_signal("trick_landed", "Kickflip 360", 450)

    move_and_slide()
"#
        .to_string();

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
        };

        app.log(
            LogLevel::Info,
            format!("Initialized Foreplay Editor for project '{name_str}'"),
        );
        app
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
        if let Err(err) = self.session.execute(cmd) {
            self.log(LogLevel::Warn, format!("Command failed: {err}"));
        } else {
            self.command_history_count += 1;
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
            EditorWorkspace::Script => EditorTab::ScriptView,
            EditorWorkspace::AtlasGraph => EditorTab::AtlasGraph,
            EditorWorkspace::PortDiff => EditorTab::PortDiff,
            EditorWorkspace::AssetLib => EditorTab::AssetLib,
        };

        if let Some(tab_path) = self.dock_state.find_tab(&target_tab) {
            self.dock_state.set_active_tab(tab_path).ok();
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

    /// PIE Step: Advances 1 deterministic headless/physics tick.
    pub fn pie_step(&mut self) {
        let tick = self.session.verify_headless_ticks(1);
        self.log(LogLevel::Pie, format!("⏭ PIE Stepped 1 frame. World Tick = {tick}"));
    }

    /// PIE Stop: Restores exact pre-play snapshot with zero rebuild.
    pub fn pie_stop(&mut self) {
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
        // Apply Godot 4 styling theme
        crate::theme::apply_godot_theme(ui.ctx());

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
        }

        // Handle global Undo/Redo keyboard shortcuts
        let input = ui.input(|i| i.clone());
        let cmd_or_ctrl = input.modifiers.command || input.modifiers.ctrl;
        if cmd_or_ctrl && input.key_pressed(Key::Z) {
            if input.modifiers.shift {
                self.redo();
            } else {
                self.undo();
            }
        } else if cmd_or_ctrl && input.key_pressed(Key::Y) {
            self.redo();
        }

        // Top Transport Bar
        egui::Panel::top("top_transport_panel").show(ui, |ui| {
            crate::transport::render_top_bar(self, ui);
        });

        // Center Docking Panels (SceneHierarchy, Viewport, Inspector, Atlas, Script, PortDiff, Console)
        egui::CentralPanel::default().show(ui, |ui| {
            // Temporarily take ownership of dock_state to satisfy borrow checker
            let mut dock_state = std::mem::replace(&mut self.dock_state, DockState::new(Vec::new()));
            let mut tab_viewer = EditorTabViewer::new(self);

            egui_dock::DockArea::new(&mut dock_state)
                .style(egui_dock::Style::from_egui(ui.style()))
                .show_inside(ui, &mut tab_viewer);

            self.dock_state = dock_state;
        });
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
        ctx.run_ui(raw_input, |ui| {
            self.render_ui(ui);
        })
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
    use foreplay_ir::IrValue;

    #[test]
    fn backwards_compatibility_initialization() {
        let app = ForeplayEditorApp::new("CompatibilityTest");
        assert_eq!(app.active_workspace, EditorWorkspace::Workspace3D);
        assert_eq!(app.session.tree.active_scene.name, "CompatibilityTest");
        assert!(!app.play_in_editor_running);
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

        // Render headless across all 6 workspaces
        let workspaces = [
            EditorWorkspace::Workspace3D,
            EditorWorkspace::Workspace2D,
            EditorWorkspace::Script,
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
    fn gizmo_translation_and_scale_commands() {
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

        // Undo restores old position
        app.undo();
        assert_eq!(
            app.session.tree.active_scene.nodes.get(&skater_id).unwrap().properties.get("position"),
            Some(&old_pos)
        );

        // Redo restores new position
        app.redo();
        assert_eq!(
            app.session.tree.active_scene.nodes.get(&skater_id).unwrap().properties.get("position"),
            Some(&new_pos)
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
}
