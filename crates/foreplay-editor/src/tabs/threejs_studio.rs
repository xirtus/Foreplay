//! Dual-Runtime 3JSE / Three.js WebGPU Studio Dock (`foreplay-editor`).
//!
//! Provides:
//! - Dual-runtime project configuration (`WebGPURenderer` + `WebGL2` fallback, Vite + TypeScript, `@3jse/atlas` manifest, live `FeelSpec` DevHUD)
//! - Multi-file code preview tabs (`src/scene.ts`, `src/main.ts`, `src/feel.ts`, `package.json`, `index.html`)
//! - Action buttons for Exporting a Three.js game, importing `3jse.scene.json`, and running DevServer simulation.

use std::collections::BTreeMap;
use egui::{Color32, FontId, Pos2, RichText, ScrollArea, Stroke, Ui, Vec2};
use foreplay_threejs::{compile_ir_to_threejs_project, ThreeJsExportOptions, ThreeJsProjectBundle, ThreeToneMapping};
use crate::ForeplayEditorApp;
use crate::theme::{
    CHROMA_AMBER_AUDIO, CHROMA_CYAN_2D, CHROMA_EMERALD_THREEJS, CHROMA_MINT_UI,
    STUDIO_OBSIDIAN_BORDER, STUDIO_OBSIDIAN_CANVAS,
    STUDIO_OBSIDIAN_TEXT_MUTED, STUDIO_OBSIDIAN_TEXT_NORMAL,
};

pub const NEON_SKATE_JSON: &str = include_str!("../../../../templates/threejs-3jse/neon_skate_3jse.json");

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
pub enum ThreeJsCodeTab {
    #[default]
    SceneTs,
    MainTs,
    FeelTs,
    BehaviorsTs,
    SignalsTs,
    PackageJson,
    IndexHtml,
    AtlasJson,
}

impl ThreeJsCodeTab {
    pub const ALL: [Self; 8] = [
        Self::SceneTs,
        Self::MainTs,
        Self::FeelTs,
        Self::BehaviorsTs,
        Self::SignalsTs,
        Self::PackageJson,
        Self::IndexHtml,
        Self::AtlasJson,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::SceneTs => "src/scene.ts",
            Self::MainTs => "src/main.ts",
            Self::FeelTs => "src/feel.ts",
            Self::BehaviorsTs => "src/scripts/behaviors.ts",
            Self::SignalsTs => "src/signals.ts",
            Self::PackageJson => "package.json",
            Self::IndexHtml => "index.html",
            Self::AtlasJson => "src/atlas.json",
        }
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ThreeJsStudioState {
    pub active_code_tab: ThreeJsCodeTab,
    pub webgpu_renderer: bool,
    pub webgl2_fallback: bool,
    pub atlas_manifest: bool,
    pub devhud_overlay: bool,
    pub simulation_running: bool,
    pub export_status: Option<String>,
}

impl Default for ThreeJsStudioState {
    fn default() -> Self {
        Self {
            active_code_tab: ThreeJsCodeTab::SceneTs,
            webgpu_renderer: true,
            webgl2_fallback: true,
            atlas_manifest: true,
            devhud_overlay: true,
            simulation_running: true,
            export_status: None,
        }
    }
}

pub fn show(app: &mut ForeplayEditorApp, ui: &mut Ui) {
    // 1. Header & Quick Actions
    ui.horizontal(|ui| {
        ui.label(RichText::new("Three.js WebGPU Studio").strong().color(CHROMA_EMERALD_THREEJS));
        ui.colored_label(STUDIO_OBSIDIAN_TEXT_MUTED, "| Dual-Runtime Rust + Three.js Browser Target");

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("🌐 DevServer Simulation").clicked() {
                app.threejs_state.simulation_running = !app.threejs_state.simulation_running;
            }

            if ui.button("🛹 Load Neon Skate Park (3JSE)").on_hover_text("Imports the 3JSE Neon Skate Park template through EditorCommand bus").clicked() {
                match app.import_threejs_scene_json("NeonSkatePark", NEON_SKATE_JSON) {
                    Ok(count) => {
                        let msg = format!("Loaded Neon Skate Park ({count} nodes) via EditorCommand");
                        app.threejs_state.export_status = Some(msg.clone());
                        app.log(crate::tabs::console::LogLevel::Info, msg);
                    }
                    Err(e) => {
                        app.threejs_state.export_status = Some(format!("Import error: {e}"));
                    }
                }
            }

            if ui.button("📥 Import 3jse.scene.json").clicked() {
                match app.import_threejs_scene_json("ImportedScene", NEON_SKATE_JSON) {
                    Ok(count) => {
                        let msg = format!("Imported {count} nodes from 3jse.scene.json");
                        app.threejs_state.export_status = Some(msg.clone());
                        app.log(crate::tabs::console::LogLevel::Info, msg);
                    }
                    Err(e) => {
                        app.threejs_state.export_status = Some(format!("Import failed: {e}"));
                    }
                }
            }

            if ui.button("📤 Export Three.js Game").clicked() {
                match compile_threejs_bundle(app) {
                    Ok(bundle) => {
                        if let Some(scene_ts) = bundle.files.get("src/scene.ts") {
                            ui.ctx().copy_text(scene_ts.clone());
                        }
                        let msg = format!("Exported Three.js project ({} files, {} nodes)", bundle.files.len(), bundle.node_count);
                        app.threejs_state.export_status = Some(msg.clone());
                        app.log(crate::tabs::console::LogLevel::Info, msg);
                    }
                    Err(e) => {
                        app.threejs_state.export_status = Some(format!("Export error: {e}"));
                    }
                }
            }
        });
    });

    ui.separator();

    // 2. Configuration Bar
    ui.horizontal(|ui| {
        ui.group(|ui| {
            ui.label(RichText::new("Target Pipeline:").strong());
            ui.checkbox(&mut app.threejs_state.webgpu_renderer, "WebGPURenderer");
            ui.checkbox(&mut app.threejs_state.webgl2_fallback, "WebGL2 Fallback");
            ui.separator();
            ui.checkbox(&mut app.threejs_state.atlas_manifest, "@3jse/atlas Manifest");
            ui.checkbox(&mut app.threejs_state.devhud_overlay, "FeelSpec DevHUD");
        });

        if let Some(status) = &app.threejs_state.export_status {
            ui.colored_label(CHROMA_MINT_UI, status);
        }
    });

    ui.add_space(4.0);

    // 3. Multi-File Code Preview Tabs
    ui.horizontal(|ui| {
        for &tab in &ThreeJsCodeTab::ALL {
            let is_active = app.threejs_state.active_code_tab == tab;
            let title = tab.title();
            let label = if is_active {
                RichText::new(title).strong().color(Color32::WHITE)
            } else {
                RichText::new(title).color(STUDIO_OBSIDIAN_TEXT_MUTED)
            };

            if ui.selectable_label(is_active, label).clicked() {
                app.threejs_state.active_code_tab = tab;
            }
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("📋 Copy Code").clicked() {
                let code = get_file_content(app, app.threejs_state.active_code_tab);
                ui.ctx().copy_text(code);
                app.log(crate::tabs::console::LogLevel::Info, "Copied file to clipboard");
            }
        });
    });

    ui.separator();

    // 4. Code Display Area
    let file_content = get_file_content(app, app.threejs_state.active_code_tab);
    ScrollArea::both().show(ui, |ui| {
        let desired_size = Vec2::new(ui.available_width().max(600.0), ui.available_height().max(300.0));
        let (rect, _resp) = ui.allocate_exact_size(desired_size, egui::Sense::hover());
        let painter = ui.painter_at(rect);

        // Editor canvas
        painter.rect(
            rect,
            4.0,
            STUDIO_OBSIDIAN_CANVAS,
            Stroke::new(1.0, STUDIO_OBSIDIAN_BORDER),
            egui::StrokeKind::Inside,
        );

        let font = FontId::monospace(11.0);
        let mut y = rect.min.y + 8.0;
        let line_height = 16.0;

        for (idx, line) in file_content.lines().enumerate() {
            if y + line_height > rect.max.y {
                break;
            }

            // Line numbers
            let line_no_str = format!("{:3} ", idx + 1);
            painter.text(
                Pos2::new(rect.min.x + 8.0, y),
                egui::Align2::LEFT_TOP,
                line_no_str,
                font.clone(),
                Color32::from_rgb(0x45, 0x52, 0x6e),
            );

            // Syntax coloring
            let line_color = if line.starts_with("//") || line.starts_with("/*") {
                STUDIO_OBSIDIAN_TEXT_MUTED
            } else if line.contains("import ") || line.contains("export ") || line.contains("const ") || line.contains("function ") {
                CHROMA_CYAN_2D
            } else if line.contains("new THREE.") || line.contains("WebGPURenderer") {
                CHROMA_EMERALD_THREEJS
            } else if line.contains('"') || line.contains('\'') {
                CHROMA_AMBER_AUDIO
            } else {
                STUDIO_OBSIDIAN_TEXT_NORMAL
            };

            painter.text(
                Pos2::new(rect.min.x + 48.0, y),
                egui::Align2::LEFT_TOP,
                line,
                font.clone(),
                line_color,
            );

            y += line_height;
        }
    });
}

pub fn compile_threejs_bundle(app: &ForeplayEditorApp) -> Result<ThreeJsProjectBundle, String> {
    let mut gd_map = BTreeMap::new();
    let ts_map = BTreeMap::new();
    if let Some(selected_id) = app.selected_node {
        gd_map.insert(selected_id, app.script_buffer.clone());
    }
    let opts = ThreeJsExportOptions {
        project_name: app.session.tree.active_scene.name.clone(),
        use_webgpu_renderer: app.threejs_state.webgpu_renderer,
        include_feel_dev_hud: app.threejs_state.devhud_overlay,
        include_atlas_json: app.threejs_state.atlas_manifest,
        tone_mapping: ThreeToneMapping::AgX,
    };
    compile_ir_to_threejs_project(
        &app.session.tree.active_scene,
        &gd_map,
        &ts_map,
        &opts,
    ).map_err(|e| format!("{e}"))
}

fn get_file_content(app: &ForeplayEditorApp, tab: ThreeJsCodeTab) -> String {
    match compile_threejs_bundle(app) {
        Ok(bundle) => {
            let key = tab.title();
            bundle.files.get(key).cloned().unwrap_or_else(|| format!("// File '{key}' not present in generated bundle"))
        }
        Err(e) => format!("// Compilation error: {e}"),
    }
}
