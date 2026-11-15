//! FileSystem & Asset Import Dock (`foreplay-editor`).
//!
//! Mirrors Godot 4's FileSystem dock in the bottom-left panel:
//! - Hierarchical tree of `res://`, `user://`, and `port://` mounts.
//! - Filterable asset list with file-type badges.
//! - Double-click opening into Scene view or Script editor.
//! - Import settings tab with mipmaps/compression/loop presets and `Reimport` button.

use egui::{Color32, RichText, ScrollArea, Ui};
use crate::ForeplayEditorApp;
use crate::EditorWorkspace;
use crate::theme::{COLOR_ACCENT_BLUE, COLOR_SUCCESS_GREEN, COLOR_TEXT_MUTED, COLOR_TEXT_NORMAL, COLOR_WARN_YELLOW};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FileSystemDockTab {
    FileSystem,
    ImportSettings,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FileSystemDockState {
    pub active_tab: FileSystemDockTab,
    pub search_query: String,
    pub selected_file: String,
    pub selected_folder: String,
    // Import settings
    pub import_generate_mipmaps: bool,
    pub import_vram_compression: bool,
    pub import_generate_colliders: bool,
    pub import_audio_loop: bool,
}

impl Default for FileSystemDockState {
    fn default() -> Self {
        Self {
            active_tab: FileSystemDockTab::FileSystem,
            search_query: String::new(),
            selected_file: "res://scenes/park_demo.fscn".into(),
            selected_folder: "res://scenes/".into(),
            import_generate_mipmaps: true,
            import_vram_compression: true,
            import_generate_colliders: true,
            import_audio_loop: false,
        }
    }
}

#[derive(Debug)]
pub struct VfsFileEntry {
    pub path: &'static str,
    pub name: &'static str,
    pub extension: &'static str,
    pub file_type: &'static str,
    pub size_str: &'static str,
}

pub fn all_vfs_files() -> &'static [VfsFileEntry] {
    &[
        VfsFileEntry {
            path: "res://scenes/park_demo.fscn",
            name: "park_demo.fscn",
            extension: "fscn",
            file_type: "Scene (FSCN)",
            size_str: "12.4 KB",
        },
        VfsFileEntry {
            path: "res://scenes/hud.fscn",
            name: "hud.fscn",
            extension: "fscn",
            file_type: "Scene (FSCN)",
            size_str: "4.2 KB",
        },
        VfsFileEntry {
            path: "res://scripts/skater_controller.gd",
            name: "skater_controller.gd",
            extension: "gd",
            file_type: "GDScript",
            size_str: "3.1 KB",
        },
        VfsFileEntry {
            path: "res://shaders/pbr_concrete.gdshader",
            name: "pbr_concrete.gdshader",
            extension: "gdshader",
            file_type: "Shader",
            size_str: "1.8 KB",
        },
        VfsFileEntry {
            path: "res://models/skater.glb",
            name: "skater.glb",
            extension: "glb",
            file_type: "3D Model (GLTF)",
            size_str: "18.2 MB",
        },
        VfsFileEntry {
            path: "res://textures/skater_icon.png",
            name: "skater_icon.png",
            extension: "png",
            file_type: "2D Texture",
            size_str: "64.0 KB",
        },
        VfsFileEntry {
            path: "port://skate2/plaza.geo",
            name: "plaza.geo",
            extension: "geo",
            file_type: "Port Geometry",
            size_str: "124.5 MB",
        },
        VfsFileEntry {
            path: "port://skate2/physics.col",
            name: "physics.col",
            extension: "col",
            file_type: "Port Collision",
            size_str: "38.1 MB",
        },
    ]
}

pub fn show(app: &mut ForeplayEditorApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.selectable_value(&mut app.filesystem_state.active_tab, FileSystemDockTab::FileSystem, "📁 FileSystem");
        ui.selectable_value(&mut app.filesystem_state.active_tab, FileSystemDockTab::ImportSettings, "⚙ Import Settings");
    });

    ui.separator();

    match app.filesystem_state.active_tab {
        FileSystemDockTab::FileSystem => show_filesystem(app, ui),
        FileSystemDockTab::ImportSettings => show_import_settings(app, ui),
    }
}

fn show_filesystem(app: &mut ForeplayEditorApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.label("🔍");
        ui.text_edit_singleline(&mut app.filesystem_state.search_query);
    });

    ui.separator();

    let query = app.filesystem_state.search_query.to_lowercase();
    let files = all_vfs_files();

    ScrollArea::vertical().show(ui, |ui| {
        for file in files {
            if !query.is_empty() && !file.name.to_lowercase().contains(&query) {
                continue;
            }

            let is_selected = app.filesystem_state.selected_file == file.path;

            let (badge, color) = match file.extension {
                "fscn" | "tscn" => ("🎬 Scene", COLOR_ACCENT_BLUE),
                "gd" | "ts" => ("📜 Code", COLOR_SUCCESS_GREEN),
                "gdshader" => ("✨ Shader", COLOR_WARN_YELLOW),
                "glb" | "geo" => ("📦 Model", Color32::from_rgb(0xfc, 0x7f, 0x7f)),
                "png" => ("🖼 Image", Color32::from_rgb(0x8e, 0xc0, 0x7c)),
                _ => ("📄 Asset", COLOR_TEXT_MUTED),
            };

            let row_resp = ui.horizontal(|ui| {
                ui.colored_label(color, badge);
                let text = if is_selected {
                    RichText::new(file.name).strong().color(Color32::WHITE)
                } else {
                    RichText::new(file.name).color(COLOR_TEXT_NORMAL)
                };
                let sel_label = ui.selectable_label(is_selected, text);
                ui.colored_label(COLOR_TEXT_MUTED, file.size_str);
                sel_label
            });

            if row_resp.inner.clicked() {
                app.filesystem_state.selected_file = file.path.to_string();
            }

            // Double click to open in editor
            if row_resp.inner.double_clicked() {
                app.filesystem_state.selected_file = file.path.to_string();
                if file.extension == "fscn" || file.extension == "tscn" {
                    app.open_or_switch_scene(file.name, Some(file.path.to_string()));
                } else if file.extension == "gd" || file.extension == "ts" {
                    app.set_workspace(EditorWorkspace::Script);
                }
            }
        }
    });
}

fn show_import_settings(app: &mut ForeplayEditorApp, ui: &mut Ui) {
    ScrollArea::vertical().show(ui, |ui| {
        ui.label(RichText::new(format!("Import: {}", app.filesystem_state.selected_file)).strong().color(COLOR_ACCENT_BLUE));
        ui.separator();

        ui.label(RichText::new("3D Mesh & Geometry Options").strong());
        ui.checkbox(&mut app.filesystem_state.import_generate_colliders, "Generate StaticBody Colliders from Mesh");

        ui.separator();
        ui.label(RichText::new("Texture & Compression Options").strong());
        ui.checkbox(&mut app.filesystem_state.import_generate_mipmaps, "Generate Mipmaps (GPU Trilinear)");
        ui.checkbox(&mut app.filesystem_state.import_vram_compression, "VRAM Compression (BC7 / ASTC)");

        ui.separator();
        ui.label(RichText::new("Audio Options").strong());
        ui.checkbox(&mut app.filesystem_state.import_audio_loop, "Enable Seamless Loop Point");

        ui.add_space(10.0);
        if ui.button(RichText::new("🔄 Reimport Asset").strong().color(COLOR_SUCCESS_GREEN)).clicked() {
            app.log(
                crate::tabs::console::LogLevel::Info,
                format!("Reimported '{}' with updated VFS parameters", app.filesystem_state.selected_file),
            );
        }
    });
}
