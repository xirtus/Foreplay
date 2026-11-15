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

use egui::{Id, WidgetText};
use egui_dock::{DockState, NodeIndex, TabViewer};
use crate::ForeplayEditorApp;

/// Dock tabs matching Godot 4 + 3JSE Atlas + RustPorts Differential Inspector.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum EditorTab {
    SceneHierarchy,
    Viewport3D,
    Viewport2D,
    Inspector,
    FeelSpec,
    AtlasGraph,
    ScriptView,
    PortDiff,
    Console,
    AssetLib,
}

impl EditorTab {
    pub fn title(self) -> &'static str {
        match self {
            Self::SceneHierarchy => "Scene Hierarchy",
            Self::Viewport3D => "3D Viewport",
            Self::Viewport2D => "2D Viewport",
            Self::Inspector => "Inspector",
            Self::FeelSpec => "FeelSpec Live",
            Self::AtlasGraph => "Atlas Graph",
            Self::ScriptView => "Script & FSCN",
            Self::PortDiff => "Port Diff Oracle",
            Self::Console => "Console & History",
            Self::AssetLib => "Asset Library",
        }
    }
}

/// Builds the default Godot 4-style dock layout:
/// - Left: SceneHierarchy
/// - Center: Viewport3D, Viewport2D, ScriptView, AtlasGraph, PortDiff, AssetLib
/// - Right: Inspector, FeelSpec
/// - Bottom: Console
pub fn build_default_dock_state() -> DockState<EditorTab> {
    // Start with Center tabs in root leaf
    let center_tabs = vec![
        EditorTab::Viewport3D,
        EditorTab::Viewport2D,
        EditorTab::ScriptView,
        EditorTab::AtlasGraph,
        EditorTab::PortDiff,
        EditorTab::AssetLib,
    ];
    let mut state = DockState::new(center_tabs);

    // Split Left: SceneHierarchy (20% width)
    let [left_node, center_node] = state
        .main_surface_mut()
        .split_left(NodeIndex::root(), 0.20, vec![EditorTab::SceneHierarchy]);

    // Split Right off the center: Inspector and FeelSpec (25% width of remaining)
    let [_center_node, _right_node] = state.main_surface_mut().split_right(
        center_node,
        0.72,
        vec![EditorTab::Inspector, EditorTab::FeelSpec],
    );

    // Split Bottom off the center: Console (25% height)
    let [_center_node, _bottom_node] = state
        .main_surface_mut()
        .split_below(center_node, 0.75, vec![EditorTab::Console]);

    _ = left_node;
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
            EditorTab::Viewport3D => viewport_3d::show(self.app, ui),
            EditorTab::Viewport2D => viewport_2d::show(self.app, ui),
            EditorTab::Inspector => inspector::show_inspector(self.app, ui),
            EditorTab::FeelSpec => inspector::show_feel_spec(self.app, ui),
            EditorTab::AtlasGraph => atlas::show(self.app, ui),
            EditorTab::ScriptView => script::show(self.app, ui),
            EditorTab::PortDiff => port_diff::show(self.app, ui),
            EditorTab::Console => console::show(self.app, ui),
            EditorTab::AssetLib => asset_lib::show(self.app, ui),
        }
    }
}
