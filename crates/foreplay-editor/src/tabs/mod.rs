//! Dockable tab definitions and viewer implementation (`foreplay-editor`).

pub mod hierarchy;
pub mod viewport_3d;
pub mod viewport_2d;
pub mod inspector;
pub mod atlas;
pub mod script;
pub mod port_diff;
pub mod console;
pub mod asset_lib;
pub mod node_dock;
pub mod history_dock;
pub mod filesystem;
pub mod animation;
pub mod audio_mixer;
pub mod shader_editor;
pub mod tilemap;
pub mod debugger;
pub mod ai_harness;
pub mod polyamour_model;
pub mod threejs_studio;

use egui::{Id, WidgetText};
use egui_dock::{DockState, NodeIndex, TabViewer};
use crate::ForeplayEditorApp;

/// Dock tabs matching Godot 4 + 3JSE Atlas + RustPorts Differential Inspector + Polyamour + Three.js Studio.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum EditorTab {
    SceneHierarchy,
    FileSystem,
    Viewport3D,
    Viewport2D,
    Inspector,
    NodeSignalsGroups,
    History,
    FeelSpec,
    AtlasGraph,
    ScriptView,
    PortDiff,
    Console,
    AssetLib,
    Debugger,
    AudioMixer,
    Animation,
    ShaderEditor,
    TileMap,
    AiHarness,
    PolyamourModel,
    ThreeJsStudio,
}

impl EditorTab {
    pub fn title(self) -> &'static str {
        match self {
            Self::SceneHierarchy => "Scene Hierarchy",
            Self::FileSystem => "FileSystem",
            Self::Viewport3D => "3D Viewport",
            Self::Viewport2D => "2D Viewport",
            Self::Inspector => "Inspector",
            Self::NodeSignalsGroups => "Node (Signals/Groups)",
            Self::History => "History",
            Self::FeelSpec => "FeelSpec Live",
            Self::AtlasGraph => "Atlas Graph",
            Self::ScriptView => "Script & FSCN",
            Self::PortDiff => "Port Diff Oracle",
            Self::Console => "Console / Output",
            Self::AssetLib => "Asset Library",
            Self::Debugger => "Debugger & Profiler",
            Self::AudioMixer => "Audio Mixer",
            Self::Animation => "Animation",
            Self::ShaderEditor => "Shader Editor",
            Self::TileMap => "TileMap",
            Self::AiHarness => "AI Harness",
            Self::PolyamourModel => "Model (Polyamour 3D)",
            Self::ThreeJsStudio => "Three.js Studio (3JSE)",
        }
    }
}

/// Builds the default Godot 4 / Apple Pro-style dock layout:
/// - Left-Top: SceneHierarchy
/// - Left-Bottom: FileSystem (split below SceneHierarchy)
/// - Center: Viewport3D, Viewport2D, PolyamourModel, ScriptView, ThreeJsStudio, AtlasGraph, PortDiff, AssetLib
/// - Right: Inspector, NodeSignalsGroups, History, FeelSpec
/// - Bottom: Console, Debugger, AudioMixer, Animation, ShaderEditor, TileMap, AiHarness
pub fn build_default_dock_state() -> DockState<EditorTab> {
    // Start with Center tabs in root leaf
    let center_tabs = vec![
        EditorTab::Viewport3D,
        EditorTab::Viewport2D,
        EditorTab::PolyamourModel,
        EditorTab::ScriptView,
        EditorTab::ThreeJsStudio,
        EditorTab::AtlasGraph,
        EditorTab::PortDiff,
        EditorTab::AssetLib,
    ];
    let mut state = DockState::new(center_tabs);

    // Split Left: SceneHierarchy (20% width)
    let [left_node, center_node] = state
        .main_surface_mut()
        .split_left(NodeIndex::root(), 0.20, vec![EditorTab::SceneHierarchy]);

    // Split Left-Bottom: FileSystem under SceneHierarchy (55% height of left dock)
    let [_hierarchy_node, _fs_node] = state
        .main_surface_mut()
        .split_below(left_node, 0.55, vec![EditorTab::FileSystem]);

    // Split Right off the center: Inspector, NodeSignalsGroups, History, FeelSpec (26% width of remaining)
    let [_center_node, _right_node] = state.main_surface_mut().split_right(
        center_node,
        0.72,
        vec![
            EditorTab::Inspector,
            EditorTab::NodeSignalsGroups,
            EditorTab::History,
            EditorTab::FeelSpec,
        ],
    );

    // Split Bottom off the center: Console, Debugger, AudioMixer, Animation, ShaderEditor, TileMap, AiHarness (26% height)
    let [_center_node, _bottom_node] = state.main_surface_mut().split_below(
        center_node,
        0.74,
        vec![
            EditorTab::Console,
            EditorTab::Debugger,
            EditorTab::AudioMixer,
            EditorTab::Animation,
            EditorTab::ShaderEditor,
            EditorTab::TileMap,
            EditorTab::AiHarness,
        ],
    );

    state
}

/// TabViewer bridging egui_dock events to `ForeplayEditorApp`.
#[derive(Debug)]
pub struct EditorTabViewer<'a> {
    pub app: &'a mut ForeplayEditorApp,
}

impl<'a> EditorTabViewer<'a> {
    pub fn new(app: &'a mut ForeplayEditorApp) -> Self {
        Self { app }
    }
}

impl TabViewer for EditorTabViewer<'_> {
    type Tab = EditorTab;

    fn id(&mut self, tab: &mut Self::Tab) -> Id {
        Id::new(format!("tab_{tab:?}"))
    }

    fn title(&mut self, tab: &mut Self::Tab) -> WidgetText {
        tab.title().into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        match tab {
            EditorTab::SceneHierarchy => hierarchy::show(self.app, ui),
            EditorTab::FileSystem => filesystem::show(self.app, ui),
            EditorTab::Viewport3D => viewport_3d::show(self.app, ui),
            EditorTab::Viewport2D => viewport_2d::show(self.app, ui),
            EditorTab::Inspector => inspector::show_inspector(self.app, ui),
            EditorTab::NodeSignalsGroups => node_dock::show(self.app, ui),
            EditorTab::History => history_dock::show(self.app, ui),
            EditorTab::FeelSpec => inspector::show_feel_spec(self.app, ui),
            EditorTab::AtlasGraph => atlas::show(self.app, ui),
            EditorTab::ScriptView => script::show(self.app, ui),
            EditorTab::PortDiff => port_diff::show(self.app, ui),
            EditorTab::Console => console::show(self.app, ui),
            EditorTab::AssetLib => asset_lib::show(self.app, ui),
            EditorTab::Debugger => debugger::show(self.app, ui),
            EditorTab::AudioMixer => audio_mixer::show(self.app, ui),
            EditorTab::Animation => animation::show(self.app, ui),
            EditorTab::ShaderEditor => shader_editor::show(self.app, ui),
            EditorTab::TileMap => tilemap::show(self.app, ui),
            EditorTab::AiHarness => ai_harness::show(self.app, ui),
            EditorTab::PolyamourModel => polyamour_model::show(self.app, ui),
            EditorTab::ThreeJsStudio => threejs_studio::show(self.app, ui),
        }
    }
}
