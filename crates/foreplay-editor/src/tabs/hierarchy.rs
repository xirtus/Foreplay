//! Scene Hierarchy Dock (`foreplay-editor`).
//!
//! Renders a tree view of `active_scene.nodes` with parent-child indentation,
//! PackedScene instance badges, search filtering, node creation presets, and deletion.

use std::collections::BTreeMap;
use egui::{Color32, RichText, ScrollArea, Ui};
use foreplay_ir::{EditorCommand, IrNode, IrValue, NodeId};
use crate::ForeplayEditorApp;
use crate::theme::{COLOR_ERROR_RED, COLOR_SUCCESS_GREEN, COLOR_TEXT_MUTED};

pub fn show(app: &mut ForeplayEditorApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Scene Nodes").strong());
        let count = app.session.tree.active_scene.nodes.len();
        ui.colored_label(COLOR_TEXT_MUTED, format!("({count})"));
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

    // Preset action buttons
    ui.collapsing("➕ Add Node Presets", |ui| {
        ui.horizontal_wrapped(|ui| {
            preset_button(app, ui, "CharacterBody3D", true);
            preset_button(app, ui, "StaticBody3D", true);
            preset_button(app, ui, "Camera3D", true);
            preset_button(app, ui, "DirectionalLight3D", true);
            preset_button(app, ui, "Node3D", true);
            preset_button(app, ui, "Node2D", false);
            preset_button(app, ui, "Sprite2D", false);
        });
    });

    // Delete button if a node is selected
    if let Some(selected_id) = app.selected_node {
        ui.horizontal(|ui| {
            if let Some(target_node) = app.session.tree.active_scene.nodes.get(&selected_id).cloned() {
                let del_btn = egui::Button::new(RichText::new(format!("🗑 Delete '{}'", target_node.name)).color(COLOR_ERROR_RED));
                if ui.add(del_btn).clicked() {
                    let cmd = EditorCommand::RemoveNode { node: target_node };
                    app.execute_command(cmd);
                    app.selected_node = None;
                }
            }
        });
    }

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
        let next_index = app
            .session
            .tree
            .active_scene
            .nodes
            .keys()
            .map(|k| k.index)
            .max()
            .unwrap_or(0)
            + 1;
        let node_id = NodeId::new(next_index, 1).expect("valid generation");

        let mut props = BTreeMap::new();
        if is_3d {
            props.insert("position".into(), IrValue::Vec3([0.0, 0.0, 0.0]));
            props.insert("scale".into(), IrValue::Vec3([1.0, 1.0, 1.0]));
        } else {
            props.insert("position".into(), IrValue::Vec2([0.0, 0.0]));
            props.insert("scale".into(), IrValue::Vec2([1.0, 1.0]));
        }

        let new_node = IrNode {
            id: node_id,
            name: format!("{class_name}_{next_index}"),
            class_name: class_name.to_string(),
            parent: app.selected_node,
            instance_scene: None,
            properties: props,
            groups: Vec::new(),
        };

        app.execute_command(EditorCommand::InsertNode { node: new_node });
        app.selected_node = Some(node_id);
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

            // Class badge
            let badge = match node.class_name.as_str() {
                "CharacterBody3D" => "[Skater/Char]",
                "Camera3D" => "[Camera]",
                "StaticBody3D" => "[StaticMesh]",
                "DirectionalLight3D" => "[Light]",
                "Node3D" => "[Node3D]",
                "Node2D" => "[Canvas2D]",
                "Sprite2D" => "[Sprite2D]",
                _ => "[Node]",
            };
            ui.colored_label(COLOR_SUCCESS_GREEN, badge);

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
