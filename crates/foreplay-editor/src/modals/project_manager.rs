//! Project Manager Modal (`foreplay-editor`).
//!
//! Mirrors Godot 4's Project Manager startup screen & project creator:
//! - Recent project cards with renderer tagging (`Forward+ (wgpu)`, `Mobile`, `Compatibility / SexOS Scanout`)
//! - Template selectors (`3D Skate Park`, `2D Platformer`, `UI Menu`, `Empty`)
//! - Import Godot `.tscn` scene loader.

use egui::{RichText, ScrollArea, Ui};
use crate::ForeplayEditorApp;
use crate::theme::{COLOR_ACCENT_BLUE, COLOR_SUCCESS_GREEN, COLOR_TEXT_MUTED};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ProjectRenderer {
    ForwardPlusWgpu,
    Mobile,
    CompatibilitySexOs,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ProjectTemplate {
    SkatePark3D,
    Platformer2D,
    UiMenu,
    Empty,
}

#[derive(Debug)]
pub struct ProjectManagerState {
    pub active_tab: ProjectManagerTab,
    pub new_project_name: String,
    pub new_project_path: String,
    pub selected_renderer: ProjectRenderer,
    pub selected_template: ProjectTemplate,
    pub tscn_import_buffer: String,
    pub status_message: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ProjectManagerTab {
    Projects,
    NewProject,
    ImportTscn,
}

impl Default for ProjectManagerState {
    fn default() -> Self {
        Self {
            active_tab: ProjectManagerTab::Projects,
            new_project_name: "ForeplaySkatePlaza".into(),
            new_project_path: "res://projects/ForeplaySkatePlaza/".into(),
            selected_renderer: ProjectRenderer::ForwardPlusWgpu,
            selected_template: ProjectTemplate::SkatePark3D,
            tscn_import_buffer: r#"[gd_scene format=3]
[node name="SkaterPlazaRoot" type="Node3D"]
[node name="Skater" type="CharacterBody3D" parent="."]
[node name="Camera3D" type="Camera3D" parent="Skater"]
"#.into(),
            status_message: None,
        }
    }
}

pub fn show(app: &mut ForeplayEditorApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Foreplay Project Manager").strong().size(16.0).color(COLOR_ACCENT_BLUE));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("✖ Close").clicked() {
                app.active_modal = None;
            }
        });
    });

    ui.separator();

    ui.horizontal(|ui| {
        ui.selectable_value(&mut app.project_manager_state.active_tab, ProjectManagerTab::Projects, "📁 Local Projects");
        ui.selectable_value(&mut app.project_manager_state.active_tab, ProjectManagerTab::NewProject, "➕ New Project");
        ui.selectable_value(&mut app.project_manager_state.active_tab, ProjectManagerTab::ImportTscn, "📥 Import Godot .tscn");
    });

    ui.separator();

    match app.project_manager_state.active_tab {
        ProjectManagerTab::Projects => show_recent_projects(app, ui),
        ProjectManagerTab::NewProject => show_new_project(app, ui),
        ProjectManagerTab::ImportTscn => show_import_tscn(app, ui),
    }

    if let Some(ref msg) = app.project_manager_state.status_message {
        ui.separator();
        ui.colored_label(COLOR_SUCCESS_GREEN, msg);
    }
}

fn show_recent_projects(app: &mut ForeplayEditorApp, ui: &mut Ui) {
    ScrollArea::vertical().show(ui, |ui| {
        let projects = [
            ("ForeplayParkDemo", "res://scenes/park_demo.fscn", "Forward+ (wgpu)", "2 mins ago"),
            ("SanVanelonaMegaRamp", "port://skate2/megaramp.geo", "Compatibility / SexOS Scanout", "1 hour ago"),
            ("KenneyPlatformer2D", "res://scenes/platformer_level1.fscn", "Forward+ (wgpu)", "Yesterday"),
            ("SexOS_SilkUI_HUD", "res://scenes/hud.fscn", "Mobile", "3 days ago"),
        ];

        for (name, path, renderer, modified) in projects {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(name).strong().size(14.0).color(COLOR_ACCENT_BLUE));
                    ui.colored_label(COLOR_TEXT_MUTED, format!("({renderer})"));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Open / Edit").clicked() {
                            app.open_or_switch_scene(name, Some(path.to_string()));
                            app.active_modal = None;
                        }
                    });
                });
                ui.horizontal(|ui| {
                    ui.colored_label(COLOR_SUCCESS_GREEN, path);
                    ui.colored_label(COLOR_TEXT_MUTED, format!("• Modified {modified}"));
                });
            });
            ui.add_space(4.0);
        }
    });
}

fn show_new_project(app: &mut ForeplayEditorApp, ui: &mut Ui) {
    ui.label(RichText::new("Create New Project").strong());
    ui.add_space(6.0);

    ui.horizontal(|ui| {
        ui.label("Project Name:");
        ui.text_edit_singleline(&mut app.project_manager_state.new_project_name);
    });

    ui.horizontal(|ui| {
        ui.label("Project Path:");
        ui.text_edit_singleline(&mut app.project_manager_state.new_project_path);
    });

    ui.separator();

    ui.label(RichText::new("Renderer:").strong());
    ui.horizontal(|ui| {
        ui.radio_value(&mut app.project_manager_state.selected_renderer, ProjectRenderer::ForwardPlusWgpu, "Forward+ (wgpu high-end)");
        ui.radio_value(&mut app.project_manager_state.selected_renderer, ProjectRenderer::Mobile, "Mobile (tiled / low bandwidth)");
        ui.radio_value(&mut app.project_manager_state.selected_renderer, ProjectRenderer::CompatibilitySexOs, "Compatibility / SexOS Scanout");
    });

    ui.separator();

    ui.label(RichText::new("Starter Template:").strong());
    ui.horizontal(|ui| {
        ui.radio_value(&mut app.project_manager_state.selected_template, ProjectTemplate::SkatePark3D, "🛹 3D Skate Park");
        ui.radio_value(&mut app.project_manager_state.selected_template, ProjectTemplate::Platformer2D, "🎮 2D Platformer");
        ui.radio_value(&mut app.project_manager_state.selected_template, ProjectTemplate::UiMenu, "🖥 UI Menu");
        ui.radio_value(&mut app.project_manager_state.selected_template, ProjectTemplate::Empty, "📄 Empty Scene");
    });

    ui.add_space(10.0);

    if ui.button(RichText::new("🚀 Create & Edit Project").strong()).clicked() {
        let name = app.project_manager_state.new_project_name.clone();
        app.create_new_project_scene(&name, app.project_manager_state.selected_template);
        app.project_manager_state.status_message = Some(format!("Created and opened project '{name}'"));
        app.active_modal = None;
    }
}

fn show_import_tscn(app: &mut ForeplayEditorApp, ui: &mut Ui) {
    ui.label(RichText::new("Godot 4 .tscn Scene Importer").strong());
    ui.label(RichText::new("Paste or inspect Godot 4 .tscn text to convert into canonical Foreplay IR:").small().color(COLOR_TEXT_MUTED));
    ui.add_space(4.0);

    ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
        ui.text_edit_multiline(&mut app.project_manager_state.tscn_import_buffer);
    });

    ui.add_space(6.0);
    if ui.button("📥 Parse & Import into Active Scene").clicked() {
        let count = app.import_tscn_buffer(&app.project_manager_state.tscn_import_buffer.clone());
        app.project_manager_state.status_message = Some(format!("Successfully imported {count} nodes from .tscn"));
        app.active_modal = None;
    }
}
