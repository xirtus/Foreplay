//! Godot 4 Editor Modals and Dialogs (`foreplay-editor`).

pub mod project_manager;
pub mod project_settings;
pub mod export_modal;
pub mod command_palette;
pub mod create_node_dialog;

use egui::Ui;
use crate::ForeplayEditorApp;

/// The currently active modal dialog, if any.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ActiveModal {
    ProjectManager,
    ProjectSettings,
    ExportProject,
    CommandPalette,
    CreateNodeDialog,
}

/// Renders the active modal dialog on top of the editor UI.
pub fn render_active_modal(app: &mut ForeplayEditorApp, ui: &mut Ui) {
    let Some(modal) = app.active_modal else {
        return;
    };

    let title = match modal {
        ActiveModal::ProjectManager => "Project Manager",
        ActiveModal::ProjectSettings => "Project Settings",
        ActiveModal::ExportProject => "Export Project",
        ActiveModal::CommandPalette => "Command Palette",
        ActiveModal::CreateNodeDialog => "Create New Node",
    };

    let mut is_open = true;
    egui::Window::new(title)
        .collapsible(false)
        .resizable(true)
        .default_size([720.0, 480.0])
        .open(&mut is_open)
        .show(ui.ctx(), |ui| match modal {
            ActiveModal::ProjectManager => project_manager::show(app, ui),
            ActiveModal::ProjectSettings => project_settings::show(app, ui),
            ActiveModal::ExportProject => export_modal::show(app, ui),
            ActiveModal::CommandPalette => command_palette::show(app, ui),
            ActiveModal::CreateNodeDialog => create_node_dialog::show(app, ui),
        });

    if !is_open {
        app.active_modal = None;
    }
}
