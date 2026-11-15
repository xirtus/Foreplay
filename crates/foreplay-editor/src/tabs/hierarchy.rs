//! Scene Hierarchy Dock (`foreplay-editor`).
//!
//! Renders a tree view of `active_scene.nodes` with parent-child indentation,
//! Godot 4 category color badges (Coral 3D, Sky-blue 2D, Emerald UI, Violet Anim, Amber Audio, White Core),
//! search filtering, node creation dialog launcher, inline renaming, reparenting, and deletion.

use std::collections::BTreeMap;
use egui::{Color32, RichText, ScrollArea, Ui};
use foreplay_ir::{EditorCommand, IrNode, NodeId};
use crate::ForeplayEditorApp;
use crate::modals::ActiveModal;
use crate::theme::{COLOR_ACCENT_BLUE, COLOR_ERROR_RED, COLOR_SUCCESS_GREEN, COLOR_TEXT_MUTED};

pub fn show(app: &mut ForeplayEditorApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Scene Nodes").strong().color(COLOR_ACCENT_BLUE));
        let count = app.session.tree.active_scene.nodes.len();
        ui.colored_label(COLOR_TEXT_MUTED, format!("({count})"));

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(RichText::new("➕ Add Child...").strong().color(COLOR_SUCCESS_GREEN)).clicked() {
                app.active_modal = Some(ActiveModal::CreateNodeDialog);
            }
        });
    });

    ui.separator();

    // Search filter
    ui.horizontal(|ui| {
        ui.label("🔍");
        ui.text_edit_singleline(&mut app.search_query);
        if !app.search_query.is_empty() && ui.small_button("✖").clicked() {
            app.search_query.clear();
        }
    });

    ui.separator();

    // Selected node actions (Rename, Reparent, Delete)
    if let Some(selected_id) = app.selected_node
        && let Some(target_node) = app.session.tree.active_scene.nodes.get(&selected_id).cloned()
    {
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("Selected: '{}'", target_node.name)).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(RichText::new("🗑 Delete").color(COLOR_ERROR_RED)).clicked() {
                        let cmd = EditorCommand::RemoveNode { node: target_node.clone() };
                        app.execute_command(cmd);
                        app.selected_node = None;
                    }
                });
            });

            // Inline Rename
            ui.horizontal(|ui| {
                ui.label("Rename:");
                let mut temp_name = target_node.name.clone();
                let resp = ui.text_edit_singleline(&mut temp_name);
                if (resp.lost_focus() || ui.button("Apply").clicked()) && temp_name != target_node.name && !temp_name.trim().is_empty() {
                    let cmd = EditorCommand::RenameNode {
                        node_id: selected_id,
                        old_name: target_node.name.clone(),
                        new_name: temp_name.trim().to_string(),
                    };
                    app.execute_command(cmd);
                }
            });

            // Reparent
            ui.horizontal(|ui| {
                ui.label("Parent:");
                if target_node.parent.is_some() {
                    if ui.button("⬆ Move to Scene Root").clicked() {
                        let cmd = EditorCommand::ReparentNode {
                            node_id: selected_id,
                            old_parent: target_node.parent,
                            new_parent: None,
                        };
                        app.execute_command(cmd);
                    }
                } else {
                    ui.colored_label(COLOR_TEXT_MUTED, "(Root Node)");
                }

                // Combo box to pick a new parent among other nodes
                let other_nodes: Vec<(NodeId, String)> = app
                    .session
                    .tree
                    .active_scene
                    .nodes
                    .iter()
                    .filter(|(id, _)| **id != selected_id && target_node.parent != Some(**id))
                    .map(|(id, n)| (*id, n.name.clone()))
                    .collect();

                if !other_nodes.is_empty() {
                    egui::ComboBox::from_id_salt(format!("reparent_combo_{selected_id:?}"))
                        .selected_text("Reparent to...")
                        .show_ui(ui, |ui| {
                            for (candidate_id, candidate_name) in other_nodes {
                                if ui.button(format!("⤷ {candidate_name}")).clicked() {
                                    let cmd = EditorCommand::ReparentNode {
                                        node_id: selected_id,
                                        old_parent: target_node.parent,
                                        new_parent: Some(candidate_id),
                                    };
                                    app.execute_command(cmd);
                                }
                            }
                        });
                }
            });
        });
        ui.separator();
    }

    // Quick node creation presets collapsible
    ui.collapsing("⚡ Quick Add Presets", |ui| {
        ui.horizontal_wrapped(|ui| {
            preset_button(app, ui, "CharacterBody3D", true);
            preset_button(app, ui, "StaticBody3D", true);
            preset_button(app, ui, "Camera3D", true);
            preset_button(app, ui, "DirectionalLight3D", true);
            preset_button(app, ui, "Node3D", true);
            preset_button(app, ui, "Node2D", false);
            preset_button(app, ui, "Sprite2D", false);
            preset_button(app, ui, "TileMapLayer", false);
            preset_button(app, ui, "Control", false);
            preset_button(app, ui, "AudioStreamPlayer3D", true);
        });
    });

    ui.separator();

    // Node Tree View
    ScrollArea::vertical().show(ui, |ui| {
        let nodes = app.session.tree.active_scene.nodes.clone();
        let search = app.search_query.to_lowercase();

        // Find root nodes (parent is None or parent not in map)
        let root_nodes: Vec<NodeId> = nodes
            .iter()
            .filter(|(_, n)| {
                n.parent.is_none() || !nodes.contains_key(&n.parent.unwrap())
            })
            .map(|(id, _)| *id)
            .collect();

        for root_id in root_nodes {
            render_node_recursive(app, ui, root_id, &nodes, 0, &search);
        }
    });
}

fn preset_button(app: &mut ForeplayEditorApp, ui: &mut Ui, class_name: &str, is_3d: bool) {
    if ui.button(format!("+ {class_name}")).clicked() {
        app.create_preset_node(class_name, is_3d);
    }
}

fn class_category_badge(class_name: &str) -> (&'static str, Color32) {
    match class_name {
        "CharacterBody3D" | "Camera3D" | "StaticBody3D" | "DirectionalLight3D" | "Node3D"
        | "CollisionShape3D" | "MeshInstance3D" | "Path3D" | "NavigationRegion3D" => {
            ("[3D]", Color32::from_rgb(0xfc, 0x7f, 0x7f)) // Coral 3D
        }
        "Node2D" | "Sprite2D" | "TileMapLayer" | "CharacterBody2D" | "StaticBody2D"
        | "CollisionShape2D" | "Camera2D" => {
            ("[2D]", Color32::from_rgb(0x8e, 0xc0, 0x7c)) // Sky-blue 2D
        }
        "Control" | "HUDCanvasRoot" | "Label" | "Button" | "Panel" | "MarginContainer"
        | "VBoxContainer" | "HBoxContainer" | "ScrollContainer" => {
            ("[UI]", Color32::from_rgb(0x4e, 0xc9, 0xb0)) // Emerald UI
        }
        "AnimationPlayer" | "AnimationTree" => {
            ("[Anim]", Color32::from_rgb(0xb3, 0x88, 0xff)) // Violet Anim
        }
        "AudioStreamPlayer" | "AudioStreamPlayer2D" | "AudioStreamPlayer3D" => {
            ("[Audio]", Color32::from_rgb(0xff, 0xb7, 0x4d)) // Amber Audio
        }
        _ => ("[Node]", Color32::from_rgb(0xd8, 0xde, 0xe9)), // Slate-white Node
    }
}

fn render_node_recursive(
    app: &mut ForeplayEditorApp,
    ui: &mut Ui,
    node_id: NodeId,
    nodes: &BTreeMap<NodeId, IrNode>,
    depth: usize,
    search: &str,
) {
    let Some(node) = nodes.get(&node_id) else {
        return;
    };

    let matches = search.is_empty()
        || node.name.to_lowercase().contains(search)
        || node.class_name.to_lowercase().contains(search);

    let is_selected = app.selected_node == Some(node_id);

    if matches {
        let indent = depth as f32 * 14.0;
        let response = ui.horizontal(|ui| {
            if indent > 0.0 {
                ui.add_space(indent);
            }

            // Godot 4 colored category badge
            let (badge, badge_color) = class_category_badge(&node.class_name);
            ui.colored_label(badge_color, badge);

            // Name
            let name_text = if is_selected {
                RichText::new(&node.name).color(Color32::WHITE).strong()
            } else {
                RichText::new(&node.name).color(Color32::from_rgb(0xdf, 0xe2, 0xe7))
            };
            let label_resp = ui.selectable_label(is_selected, name_text);

            // Instance scene badge if present
            if let Some(inst) = &node.instance_scene {
                ui.colored_label(COLOR_TEXT_MUTED, format!("📦 {inst}"));
            }

            // Groups count indicator
            if !node.groups.is_empty() {
                ui.colored_label(COLOR_TEXT_MUTED, format!("🏷 {}", node.groups.len()));
            }

            label_resp
        });

        if response.inner.clicked() {
            app.selected_node = Some(node_id);
        }
    }

    // Recurse children
    let children: Vec<NodeId> = nodes
        .iter()
        .filter(|(_, n)| n.parent == Some(node_id))
        .map(|(id, _)| *id)
        .collect();

    for child_id in children {
        render_node_recursive(app, ui, child_id, nodes, depth + 1, search);
    }
}
