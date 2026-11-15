//! Create New Node Dialog Modal (`foreplay-editor`).
//!
//! Mirrors Godot 4's Create New Node dialog:
//! - Backed by the canonical 63-class `foreplay_ir::catalog::builtin_node_catalog()`.
//! - Instant search filtering across class names and documentation strings.
//! - Categorized inheritance taxonomy (Node3D, Node2D, ControlUi, Plain, Animation, Audio).
//! - Class description and signal preview.
//! - Instantiates the node copying canonical `default_properties` via `EditorCommand::InsertNode`.

use egui::{Color32, RichText, ScrollArea, Ui};
use foreplay_ir::catalog::{builtin_node_catalog, NodeCategory as IrNodeCategory, NodeClassSpec};
use foreplay_ir::{EditorCommand, IrNode, IrValue, NodeId};
use crate::ForeplayEditorApp;
use crate::theme::{
    COLOR_ACCENT_BLUE, COLOR_SUCCESS_GREEN, COLOR_TEXT_MUTED, COLOR_TEXT_NORMAL,
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
    ui.horizontal(|ui| {
        ui.label(RichText::new("Create New Node").strong().size(16.0).color(COLOR_ACCENT_BLUE));
        let catalog = builtin_node_catalog();
        ui.colored_label(COLOR_TEXT_MUTED, format!("({} Canonical Classes)", catalog.len()));

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
    let catalog = builtin_node_catalog();

    let filtered: Vec<&NodeClassSpec> = catalog
        .values()
        .filter(|spec| {
            query.is_empty()
                || spec.name.to_lowercase().contains(&query)
                || spec.doc.to_lowercase().contains(&query)
        })
        .collect();

    ui.horizontal(|ui| {
        // Left Column: Node Selection List (63 Godot 4.4 classes)
        ui.allocate_ui(egui::vec2(280.0, 340.0), |ui| {
            ScrollArea::vertical().show(ui, |ui| {
                for spec in filtered {
                    let is_selected = app.create_node_dialog_state.selected_class == spec.name;
                    let badge_color = category_to_color32(spec.category);

                    let resp = ui.horizontal(|ui| {
                        let badge = match spec.category {
                            IrNodeCategory::Node3D => "[3D]",
                            IrNodeCategory::Node2D => "[2D]",
                            IrNodeCategory::ControlUi => "[UI]",
                            IrNodeCategory::Animation => "[Anim]",
                            IrNodeCategory::Audio => "[Audio]",
                            IrNodeCategory::Plain => "[Node]",
                        };
                        ui.colored_label(badge_color, badge);

                        let text = if is_selected {
                            RichText::new(&spec.name).strong().color(Color32::WHITE)
                        } else {
                            RichText::new(&spec.name).color(COLOR_TEXT_NORMAL)
                        };
                        ui.selectable_label(is_selected, text)
                    });

                    if resp.response.clicked() {
                        app.create_node_dialog_state.selected_class = spec.name.clone();
                    }
                }
            });
        });

        ui.separator();

        // Right Column: Node Details & Create Action
        ui.allocate_ui(egui::vec2(ui.available_width(), 340.0), |ui| {
            let selected_name = &app.create_node_dialog_state.selected_class;
            let selected_spec = catalog
                .get(selected_name)
                .or_else(|| catalog.get("Node3D"))
                .unwrap();

            let badge_color = category_to_color32(selected_spec.category);

            ui.label(RichText::new(&selected_spec.name).strong().size(15.0).color(badge_color));
            let parent_text = selected_spec.parent_class.as_deref().unwrap_or("Object (Engine Root)");
            ui.colored_label(COLOR_TEXT_MUTED, format!("Inherits: {parent_text}"));
            ui.separator();

            ui.label(&selected_spec.doc);

            // Built-in Signals Preview
            if !selected_spec.built_in_signals.is_empty() {
                ui.add_space(6.0);
                ui.label(RichText::new("Signals:").strong());
                for sig in &selected_spec.built_in_signals {
                    ui.horizontal(|ui| {
                        ui.colored_label(COLOR_ACCENT_BLUE, "⚡");
                        ui.label(format!("{}({})", sig.name, sig.parameters.iter().map(|(p, t)| format!("{p}: {t}")).collect::<Vec<_>>().join(", ")));
                    });
                }
            }

            ui.add_space(16.0);
            if ui.button(RichText::new(format!("➕ Create '{}'", selected_spec.name)).strong().color(COLOR_SUCCESS_GREEN)).clicked() {
                instantiate_node_from_catalog(app, selected_spec);
                app.active_modal = None;
            }
        });
    });
}

fn category_to_color32(category: IrNodeCategory) -> Color32 {
    let [r, g, b, a] = category.badge_color();
    Color32::from_rgba_unmultiplied(
        (r * 255.0) as u8,
        (g * 255.0) as u8,
        (b * 255.0) as u8,
        (a * 255.0) as u8,
    )
}

fn instantiate_node_from_catalog(app: &mut ForeplayEditorApp, spec: &NodeClassSpec) {
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

    // Copy canonical default properties from foreplay-ir catalog
    let mut props = spec.default_properties.clone();

    // Ensure standard transforms are present if not in default_properties
    match spec.category {
        IrNodeCategory::Node3D => {
            props.entry("position".into()).or_insert_with(|| IrValue::Vec3([0.0, 0.0, 0.0]));
            props.entry("scale".into()).or_insert_with(|| IrValue::Vec3([1.0, 1.0, 1.0]));
            props.entry("rotation".into()).or_insert_with(|| IrValue::Vec3([0.0, 0.0, 0.0]));
        }
        IrNodeCategory::Node2D => {
            props.entry("position".into()).or_insert_with(|| IrValue::Vec2([0.0, 0.0]));
            props.entry("scale".into()).or_insert_with(|| IrValue::Vec2([1.0, 1.0]));
        }
        IrNodeCategory::ControlUi => {
            props.entry("position".into()).or_insert_with(|| IrValue::Vec2([100.0, 100.0]));
            props.entry("scale".into()).or_insert_with(|| IrValue::Vec2([200.0, 50.0]));
        }
        _ => {}
    }

    let new_node = IrNode {
        id: node_id,
        name: format!("{}_{next_index}", spec.name),
        class_name: spec.name.clone(),
        parent: app.selected_node,
        instance_scene: None,
        properties: props,
        groups: Vec::new(),
    };

    app.execute_command(EditorCommand::InsertNode { node: new_node });
    app.selected_node = Some(node_id);
}
