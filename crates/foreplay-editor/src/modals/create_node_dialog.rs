//! Create New Node Dialog Modal (`foreplay-editor`).
//!
//! Mirrors Godot 4's Create New Node dialog:
//! - Backed by the complete 471-class canonical `foreplay_core::ClassDb::global_godot44()`.
//! - Instant search filtering across class names and inheritance hierarchy.
//! - Categorized inheritance taxonomy (Node3D, Node2D, Control, Animation, Audio, Resource, Node).
//! - Class inheritance hierarchy, default properties and signal previews.
//! - Instantiates the node copying canonical `default_properties` via `EditorCommand::InsertNode`.

use egui::{Color32, RichText, ScrollArea, Ui};
use foreplay_core::ClassDb;
use foreplay_ir::{EditorCommand, IrNode, IrValue, NodeId};
use crate::ForeplayEditorApp;
use crate::theme::{
    COLOR_ACCENT_BLUE, COLOR_SUCCESS_GREEN, COLOR_TEXT_MUTED, COLOR_TEXT_NORMAL, COLOR_WARN_YELLOW,
};

#[derive(Clone, Debug)]
pub struct CreateNodeDialogState {
    pub search_query: String,
    pub selected_class: String,
}

impl Default for CreateNodeDialogState {
    fn default() -> Self {
        Self {
            search_query: String::new(),
            selected_class: "Node3D".to_string(),
        }
    }
}

pub fn show(app: &mut ForeplayEditorApp, ui: &mut Ui) {
    let db = ClassDb::global_godot44();

    ui.horizontal(|ui| {
        ui.label(RichText::new("Create New Node").strong().size(16.0).color(COLOR_ACCENT_BLUE));
        ui.colored_label(
            COLOR_TEXT_MUTED,
            format!("({} Godot 4.4 Classes in ClassDB)", db.class_count()),
        );

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("✖ Close").clicked() {
                app.active_modal = None;
            }
        });
    });

    ui.separator();

    ui.horizontal(|ui| {
        ui.label("Search:");
        ui.text_edit_singleline(&mut app.create_node_dialog_state.search_query);
    });

    ui.separator();

    let query = app.create_node_dialog_state.search_query.to_lowercase();
    let mut all_classes = vec!["Object".to_string()];
    all_classes.extend(db.subclasses_of("Object"));
    all_classes.sort();

    let filtered: Vec<&String> = all_classes
        .iter()
        .filter(|name| {
            if query.is_empty() {
                return true;
            }
            if name.to_lowercase().contains(&query) {
                return true;
            }
            if let Some(info) = db.get(name)
                && let Some(ref p) = info.parent
                && p.to_lowercase().contains(&query)
            {
                return true;
            }
            false
        })
        .collect();

    ui.horizontal(|ui| {
        // Left Column: Node Selection List (471 Godot 4.4 classes)
        ui.allocate_ui(egui::vec2(280.0, 360.0), |ui| {
            ScrollArea::vertical().id_salt("classdb_node_list_scroll").show(ui, |ui| {
                for class_name in filtered {
                    let is_selected = app.create_node_dialog_state.selected_class == *class_name;
                    let (badge, badge_color) = get_class_badge(db, class_name);

                    let resp = ui.horizontal(|ui| {
                        ui.colored_label(badge_color, badge);

                        let text = if is_selected {
                            RichText::new(class_name).strong().color(Color32::WHITE)
                        } else {
                            RichText::new(class_name).color(COLOR_TEXT_NORMAL)
                        };
                        ui.selectable_label(is_selected, text)
                    });

                    if resp.response.clicked() {
                        app.create_node_dialog_state.selected_class = class_name.clone();
                    }
                }
            });
        });

        ui.separator();

        // Right Column: Node Details & Create Action
        ui.allocate_ui(egui::vec2(ui.available_width(), 360.0), |ui| {
            let selected_name = &app.create_node_dialog_state.selected_class;
            let selected_info = db
                .get(selected_name)
                .or_else(|| db.get("Node3D"))
                .unwrap();

            let (badge, badge_color) = get_class_badge(db, &selected_info.name);

            ui.horizontal(|ui| {
                ui.colored_label(badge_color, badge);
                ui.label(RichText::new(&selected_info.name).strong().size(16.0).color(badge_color));
            });

            let parent_text = selected_info.parent.as_deref().unwrap_or("Object (Root)");
            ui.colored_label(COLOR_TEXT_MUTED, format!("Inherits: {parent_text}"));

            if selected_info.is_instantiable {
                ui.colored_label(COLOR_SUCCESS_GREEN, "✔ Instantiable Node");
            } else {
                ui.colored_label(COLOR_WARN_YELLOW, "⚠ Abstract Base Class (Non-instantiable)");
            }

            ui.separator();

            // Default Properties Preview
            ui.label(RichText::new(format!("Default Properties ({}):", selected_info.default_properties.len())).strong());
            ScrollArea::vertical()
                .id_salt("classdb_props_scroll")
                .max_height(90.0)
                .show(ui, |ui| {
                    if selected_info.default_properties.is_empty() {
                        ui.colored_label(COLOR_TEXT_MUTED, "(Inherits parent default properties)");
                    } else {
                        for (key, val) in &selected_info.default_properties {
                            ui.horizontal(|ui| {
                                ui.colored_label(COLOR_ACCENT_BLUE, key);
                                ui.label(format!("= {:?}", val));
                            });
                        }
                    }
                });

            ui.separator();

            // Built-in Signals Preview
            ui.label(RichText::new(format!("Signals ({}):", selected_info.signals.len())).strong());
            ScrollArea::vertical()
                .id_salt("classdb_signals_scroll")
                .max_height(80.0)
                .show(ui, |ui| {
                    if selected_info.signals.is_empty() {
                        ui.colored_label(COLOR_TEXT_MUTED, "(No class-specific signals)");
                    } else {
                        for sig in &selected_info.signals {
                            ui.horizontal(|ui| {
                                ui.colored_label(COLOR_WARN_YELLOW, "⚡");
                                ui.label(sig);
                            });
                        }
                    }
                });

            ui.add_space(10.0);
            if selected_info.is_instantiable {
                if ui
                    .button(
                        RichText::new(format!("➕ Create '{}'", selected_info.name))
                            .strong()
                            .color(COLOR_SUCCESS_GREEN),
                    )
                    .clicked()
                {
                    instantiate_node_from_classdb(app, db, &selected_info.name);
                    app.active_modal = None;
                }
            } else {
                ui.colored_label(COLOR_TEXT_MUTED, "Abstract classes cannot be directly instantiated.");
            }
        });
    });
}

fn get_class_badge(db: &ClassDb, name: &str) -> (&'static str, Color32) {
    if db.inherits(name, "Node3D") {
        ("[3D]", Color32::from_rgb(255, 110, 110))
    } else if db.inherits(name, "Node2D") {
        ("[2D]", Color32::from_rgb(110, 220, 110))
    } else if db.inherits(name, "Control") {
        ("[UI]", Color32::from_rgb(110, 180, 255))
    } else if db.inherits(name, "AnimationMixer") || name.starts_with("Animation") {
        ("[Anim]", Color32::from_rgb(255, 180, 80))
    } else if db.inherits(name, "AudioStream") || name.starts_with("Audio") {
        ("[Audio]", Color32::from_rgb(220, 120, 220))
    } else if db.inherits(name, "Resource") {
        ("[Res]", Color32::from_rgb(240, 220, 100))
    } else {
        ("[Node]", Color32::from_rgb(180, 180, 180))
    }
}

fn instantiate_node_from_classdb(app: &mut ForeplayEditorApp, db: &ClassDb, class_name: &str) {
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

    // Copy canonical default properties from ClassDB inheritance chain
    let mut props = db.instantiate_default_properties(class_name).unwrap_or_default();

    // Ensure standard transforms are present if not in default_properties
    if db.inherits(class_name, "Node3D") {
        props.entry("position".into()).or_insert_with(|| IrValue::Vec3([0.0, 0.0, 0.0]));
        props.entry("scale".into()).or_insert_with(|| IrValue::Vec3([1.0, 1.0, 1.0]));
        props.entry("rotation".into()).or_insert_with(|| IrValue::Vec3([0.0, 0.0, 0.0]));
    } else if db.inherits(class_name, "Node2D") {
        props.entry("position".into()).or_insert_with(|| IrValue::Vec2([0.0, 0.0]));
        props.entry("scale".into()).or_insert_with(|| IrValue::Vec2([1.0, 1.0]));
    } else if db.inherits(class_name, "Control") {
        props.entry("position".into()).or_insert_with(|| IrValue::Vec2([100.0, 100.0]));
        props.entry("scale".into()).or_insert_with(|| IrValue::Vec2([200.0, 50.0]));
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
