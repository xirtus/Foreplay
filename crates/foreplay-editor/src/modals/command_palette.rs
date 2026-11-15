//! Godot 4 Command Palette Modal (`foreplay-editor`).
//!
//! Activated via `Ctrl+Shift+P` (or `Cmd+Shift+P`):
//! Provides quick fuzzy search across all editor actions, workspace switches,
//! node creation presets, and dialog openers.

use egui::{Key, RichText, ScrollArea, Ui};
use crate::ForeplayEditorApp;
use crate::EditorWorkspace;
use crate::modals::ActiveModal;
use crate::theme::{COLOR_TEXT_MUTED, COLOR_TEXT_NORMAL, COLOR_WARN_YELLOW};

#[derive(Clone, Debug, Default)]
pub struct CommandPaletteState {
    pub search_query: String,
    pub selected_index: usize,
}

pub struct PaletteAction {
    pub category: &'static str,
    pub title: &'static str,
    pub shortcut: &'static str,
    pub run: Box<dyn Fn(&mut ForeplayEditorApp)>,
}

impl std::fmt::Debug for PaletteAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PaletteAction")
            .field("category", &self.category)
            .field("title", &self.title)
            .field("shortcut", &self.shortcut)
            .finish()
    }
}

pub fn all_palette_actions() -> Vec<PaletteAction> {
    vec![
        // Workspaces
        PaletteAction {
            category: "Workspace",
            title: "Switch to 3D Viewport",
            shortcut: "F1",
            run: Box::new(|app| app.set_workspace(EditorWorkspace::Workspace3D)),
        },
        PaletteAction {
            category: "Workspace",
            title: "Switch to 2D Viewport",
            shortcut: "F2",
            run: Box::new(|app| app.set_workspace(EditorWorkspace::Workspace2D)),
        },
        PaletteAction {
            category: "Workspace",
            title: "Switch to Script & FSCN View",
            shortcut: "F3",
            run: Box::new(|app| app.set_workspace(EditorWorkspace::Script)),
        },
        PaletteAction {
            category: "Workspace",
            title: "Switch to Atlas Semantic Graph",
            shortcut: "F4",
            run: Box::new(|app| app.set_workspace(EditorWorkspace::AtlasGraph)),
        },
        PaletteAction {
            category: "Workspace",
            title: "Switch to Port Diff Oracle",
            shortcut: "F5",
            run: Box::new(|app| app.set_workspace(EditorWorkspace::PortDiff)),
        },
        PaletteAction {
            category: "Workspace",
            title: "Switch to Asset Library",
            shortcut: "F6",
            run: Box::new(|app| app.set_workspace(EditorWorkspace::AssetLib)),
        },

        // Transport
        PaletteAction {
            category: "Transport",
            title: "Play In Editor (PIE)",
            shortcut: "F5",
            run: Box::new(|app| app.pie_play()),
        },
        PaletteAction {
            category: "Transport",
            title: "Pause / Resume PIE",
            shortcut: "F7",
            run: Box::new(|app| app.pie_pause()),
        },
        PaletteAction {
            category: "Transport",
            title: "Step 1 Frame Tick",
            shortcut: "F10",
            run: Box::new(|app| app.pie_step()),
        },
        PaletteAction {
            category: "Transport",
            title: "Stop PIE",
            shortcut: "F8",
            run: Box::new(|app| app.pie_stop()),
        },

        // Node Creation
        PaletteAction {
            category: "Scene",
            title: "Create Node Dialog...",
            shortcut: "Ctrl+A",
            run: Box::new(|app| app.active_modal = Some(ActiveModal::CreateNodeDialog)),
        },
        PaletteAction {
            category: "Scene",
            title: "Add CharacterBody3D (Skater)",
            shortcut: "",
            run: Box::new(|app| app.create_preset_node("CharacterBody3D", true)),
        },
        PaletteAction {
            category: "Scene",
            title: "Add Camera3D",
            shortcut: "",
            run: Box::new(|app| app.create_preset_node("Camera3D", true)),
        },
        PaletteAction {
            category: "Scene",
            title: "Add DirectionalLight3D",
            shortcut: "",
            run: Box::new(|app| app.create_preset_node("DirectionalLight3D", true)),
        },
        PaletteAction {
            category: "Scene",
            title: "Add StaticBody3D (Obstacle / Ramp)",
            shortcut: "",
            run: Box::new(|app| app.create_preset_node("StaticBody3D", true)),
        },
        PaletteAction {
            category: "Scene",
            title: "Add Sprite2D",
            shortcut: "",
            run: Box::new(|app| app.create_preset_node("Sprite2D", false)),
        },
        PaletteAction {
            category: "Scene",
            title: "Add TileMapLayer",
            shortcut: "",
            run: Box::new(|app| app.create_preset_node("TileMapLayer", false)),
        },

        // Edit
        PaletteAction {
            category: "Edit",
            title: "Undo",
            shortcut: "Ctrl+Z",
            run: Box::new(|app| app.undo()),
        },
        PaletteAction {
            category: "Edit",
            title: "Redo",
            shortcut: "Ctrl+Y",
            run: Box::new(|app| app.redo()),
        },

        // Project Modals
        PaletteAction {
            category: "Project",
            title: "Project Settings...",
            shortcut: "Alt+P",
            run: Box::new(|app| app.active_modal = Some(ActiveModal::ProjectSettings)),
        },
        PaletteAction {
            category: "Project",
            title: "Export Project...",
            shortcut: "",
            run: Box::new(|app| app.active_modal = Some(ActiveModal::ExportProject)),
        },
        PaletteAction {
            category: "Project",
            title: "Project Manager...",
            shortcut: "",
            run: Box::new(|app| app.active_modal = Some(ActiveModal::ProjectManager)),
        },

        // Bottom Panels
        PaletteAction {
            category: "View",
            title: "Toggle Debugger & Profiler",
            shortcut: "Ctrl+1",
            run: Box::new(|app| app.toggle_bottom_panel(crate::bottom_panel::BottomPanelTab::Debugger)),
        },
        PaletteAction {
            category: "View",
            title: "Toggle Audio Bus Mixer",
            shortcut: "Ctrl+2",
            run: Box::new(|app| app.toggle_bottom_panel(crate::bottom_panel::BottomPanelTab::AudioMixer)),
        },
        PaletteAction {
            category: "View",
            title: "Toggle Animation Timeline",
            shortcut: "Ctrl+3",
            run: Box::new(|app| app.toggle_bottom_panel(crate::bottom_panel::BottomPanelTab::Animation)),
        },
        PaletteAction {
            category: "View",
            title: "Toggle Shader Editor",
            shortcut: "Ctrl+4",
            run: Box::new(|app| app.toggle_bottom_panel(crate::bottom_panel::BottomPanelTab::ShaderEditor)),
        },
        PaletteAction {
            category: "View",
            title: "Toggle TileMap Painter",
            shortcut: "Ctrl+5",
            run: Box::new(|app| app.toggle_bottom_panel(crate::bottom_panel::BottomPanelTab::TileMap)),
        },
        PaletteAction {
            category: "View",
            title: "Toggle AI Agent Harness",
            shortcut: "Ctrl+6",
            run: Box::new(|app| app.toggle_bottom_panel(crate::bottom_panel::BottomPanelTab::AiHarness)),
        },
    ]
}

pub fn show(app: &mut ForeplayEditorApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.label("🔍");
        let search_resp = ui.text_edit_singleline(&mut app.command_palette_state.search_query);
        if search_resp.changed() {
            app.command_palette_state.selected_index = 0;
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Esc").clicked() {
                app.active_modal = None;
            }
        });
    });

    ui.separator();

    let query = app.command_palette_state.search_query.to_lowercase();
    let actions = all_palette_actions();
    let filtered_actions: Vec<(usize, &PaletteAction)> = actions
        .iter()
        .enumerate()
        .filter(|(_, a)| {
            query.is_empty()
                || a.title.to_lowercase().contains(&query)
                || a.category.to_lowercase().contains(&query)
        })
        .collect();

    // Handle Enter and Up/Down navigation
    let enter_pressed = ui.input(|i| i.key_pressed(Key::Enter));
    let up_pressed = ui.input(|i| i.key_pressed(Key::ArrowUp));
    let down_pressed = ui.input(|i| i.key_pressed(Key::ArrowDown));

    if down_pressed && !filtered_actions.is_empty() {
        app.command_palette_state.selected_index =
            (app.command_palette_state.selected_index + 1) % filtered_actions.len();
    }
    if up_pressed && !filtered_actions.is_empty() {
        if app.command_palette_state.selected_index == 0 {
            app.command_palette_state.selected_index = filtered_actions.len() - 1;
        } else {
            app.command_palette_state.selected_index -= 1;
        }
    }

    let mut action_to_run: Option<usize> = None;

    if enter_pressed && !filtered_actions.is_empty() {
        let selected_idx = app.command_palette_state.selected_index.min(filtered_actions.len() - 1);
        action_to_run = Some(filtered_actions[selected_idx].0);
    }

    ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
        for (display_idx, &(orig_idx, action)) in filtered_actions.iter().enumerate() {
            let is_highlighted = display_idx == app.command_palette_state.selected_index;

            let row_resp = ui.horizontal(|ui| {
                ui.colored_label(COLOR_TEXT_MUTED, format!("[{}]", action.category));
                let title_text = if is_highlighted {
                    RichText::new(action.title).color(COLOR_WARN_YELLOW).strong()
                } else {
                    RichText::new(action.title).color(COLOR_TEXT_NORMAL)
                };
                ui.label(title_text);

                if !action.shortcut.is_empty() {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.colored_label(COLOR_TEXT_MUTED, action.shortcut);
                    });
                }
            });

            if row_resp.response.clicked() {
                action_to_run = Some(orig_idx);
            }
        }
    });

    if let Some(idx) = action_to_run {
        app.active_modal = None;
        (actions[idx].run)(app);
    }
}
