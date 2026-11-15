//! Asset Library & BYO-ROM Port Explorer Dock (`foreplay-editor`).
//!
//! Provides package browsing and `res://` / `port://` virtual filesystem inspection.

use egui::{RichText, ScrollArea, Ui};
use crate::ForeplayEditorApp;
use crate::theme::{COLOR_ACCENT_BLUE, COLOR_SUCCESS_GREEN, COLOR_TEXT_MUTED};

pub fn show(app: &mut ForeplayEditorApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Asset Library & BYO-ROM Archive VFS").strong());
        ui.colored_label(COLOR_TEXT_MUTED, "| Virtual File System Mounts (res:// & port://)");
    });

    ui.separator();

    ScrollArea::vertical().show(ui, |ui| {
        ui.label(RichText::new("📦 Available Asset Packages & Port ROMs").strong());
        ui.add_space(4.0);

        let packages = [
            (
                "Skate 2 San Vanelona Plaza Environment",
                "port://skate2/plaza.geo",
                "Faithful clean-room geometry & collision from retail XEX/ISO archive.",
                "124.5 MB",
            ),
            (
                "Urban Concrete & Coping PBR Materials",
                "res://materials/concrete/",
                "High-resolution roughness/normal maps for realistic skate grinds.",
                "42.0 MB",
            ),
            (
                "Skater Skeletal Rig & Ragdoll BlendTree",
                "res://models/skater.glb",
                "Full inverse kinematics rig with Havok/Rapier physics collision proxy.",
                "18.2 MB",
            ),
            (
                "Godot 4 Standard Kenney 3D Platformer Kit",
                "res://packages/platformer/",
                "Modular ramps, rails, funboxes, and quarter-pipes ready to instance.",
                "8.5 MB",
            ),
        ];

        for (title, path, desc, size) in packages {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(title).color(COLOR_ACCENT_BLUE).strong());
                    ui.colored_label(COLOR_TEXT_MUTED, format!("({size})"));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Import / Mount").clicked() {
                            app.log(crate::tabs::console::LogLevel::Info, format!("Mounted archive '{path}' into VFS."));
                        }
                    });
                });
                ui.colored_label(COLOR_SUCCESS_GREEN, path);
                ui.label(desc);
            });
            ui.add_space(4.0);
        }

        ui.separator();
        ui.label(RichText::new("📂 Active VFS Mounts & Cached Artifacts").strong());

        egui::Grid::new("vfs_mounts_grid")
            .striped(true)
            .min_col_width(120.0)
            .show(ui, |ui| {
                ui.label(RichText::new("Scheme & Path").strong());
                ui.label(RichText::new("Domain").strong());
                ui.label(RichText::new("Status").strong());
                ui.end_row();

                let mounts = [
                    ("res://models/skater.glb", "Graphics / Mesh", "Mounted"),
                    ("res://textures/skater_icon.png", "2D Texture", "Mounted"),
                    ("port://skate2/plaza.geo", "PortHost Level Geometry", "Mounted (Read-Only)"),
                    ("port://skate2/physics.col", "PortHost Collision Hull", "Mounted (Read-Only)"),
                    ("port://skate2/audio/sfx.bank", "FMOD / Sound Bank", "Ready"),
                ];

                for (path, domain, status) in mounts {
                    ui.label(path);
                    ui.label(domain);
                    ui.colored_label(COLOR_SUCCESS_GREEN, status);
                    ui.end_row();
                }
            });
    });
}
