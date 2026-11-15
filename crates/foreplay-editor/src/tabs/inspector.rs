//! Schema-Driven Inspector & FeelSpec Dock (`foreplay-editor`).
//!
//! Provides property drawers for every `IrValue` variant, property addition/deletion,
//! and live FeelSpec slider tuning bound to `EditorCommand` dispatches.

use egui::{DragValue, RichText, ScrollArea, Ui};
use foreplay_ir::{EditorCommand, IrValue, NodeId};
use crate::ForeplayEditorApp;
use crate::theme::{
    COLOR_ACCENT_BLUE, COLOR_AXIS_X, COLOR_AXIS_Y, COLOR_AXIS_Z, COLOR_ERROR_RED,
    COLOR_SUCCESS_GREEN, COLOR_TEXT_MUTED, COLOR_TEXT_NORMAL, COLOR_WARN_YELLOW,
};

pub fn show_inspector(app: &mut ForeplayEditorApp, ui: &mut Ui) {
    ScrollArea::vertical().show(ui, |ui| {
        let Some(selected_id) = app.selected_node else {
            ui.vertical_centered(|ui| {
                ui.add_space(30.0);
                ui.colored_label(COLOR_TEXT_MUTED, "No node selected.");
                ui.label("Select a node from Scene Hierarchy or Viewport to inspect.");
            });
            return;
        };

        let Some(node) = app.session.tree.active_scene.nodes.get(&selected_id).cloned() else {
            ui.colored_label(COLOR_ERROR_RED, "Selected node not found in scene.");
            return;
        };

        // Header: Node ID & Class Name
        ui.horizontal(|ui| {
            ui.colored_label(COLOR_TEXT_MUTED, format!("NodeId({}:{})", node.id.index, node.id.generation));
            ui.colored_label(COLOR_ACCENT_BLUE, format!("[{}]", node.class_name));
        });

        // Editable Node Name
        ui.horizontal(|ui| {
            ui.label(RichText::new("Name:").strong());
            let mut name_buf = node.name.clone();
            if ui.text_edit_singleline(&mut name_buf).lost_focus() && name_buf != node.name && !name_buf.is_empty() {
                // To update node name in IrScene, we re-insert the node with updated name
                let mut updated_node = node.clone();
                updated_node.name = name_buf;
                let remove_cmd = EditorCommand::RemoveNode { node: node.clone() };
                let insert_cmd = EditorCommand::InsertNode { node: updated_node };
                app.execute_command(remove_cmd);
                app.execute_command(insert_cmd);
            }
        });

        if let Some(inst) = &node.instance_scene {
            ui.colored_label(COLOR_SUCCESS_GREEN, format!("Instance: {inst}"));
        }

        if !node.groups.is_empty() {
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(COLOR_TEXT_MUTED, "Groups:");
                for g in &node.groups {
                    ui.colored_label(COLOR_WARN_YELLOW, format!("#{g}"));
                }
            });
        }

        ui.separator();
        ui.label(RichText::new("Exported Properties").strong());

        let mut commands_to_run = Vec::new();

        // Render each property drawer
        for (key, val) in &node.properties {
            ui.horizontal(|ui| {
                ui.label(RichText::new(key).color(COLOR_TEXT_NORMAL));

                let (new_val, changed) = render_ir_value_drawer(ui, key, val);
                if changed {
                    commands_to_run.push(EditorCommand::SetProperty {
                        node_id: selected_id,
                        key: key.clone(),
                        old_value: Some(val.clone()),
                        new_value: Some(new_val),
                    });
                }

                if ui.small_button("🗑").on_hover_text("Remove property").clicked() {
                    commands_to_run.push(EditorCommand::SetProperty {
                        node_id: selected_id,
                        key: key.clone(),
                        old_value: Some(val.clone()),
                        new_value: None,
                    });
                }
            });
        }

        for cmd in commands_to_run {
            app.execute_command(cmd);
        }

        ui.separator();

        // + Add Property Helper
        render_add_property_helper(app, ui, selected_id);
    });
}

fn render_ir_value_drawer(ui: &mut Ui, _key: &str, value: &IrValue) -> (IrValue, bool) {
    match value {
        IrValue::Nil => {
            ui.label("nil");
            (IrValue::Nil, false)
        }
        IrValue::Bool(b) => {
            let mut val = *b;
            let changed = ui.checkbox(&mut val, "").changed();
            (IrValue::Bool(val), changed)
        }
        IrValue::I64(n) => {
            let mut val = *n;
            let resp = ui.add(DragValue::new(&mut val).speed(1));
            (IrValue::I64(val), resp.changed())
        }
        IrValue::F64(f) => {
            let mut val = *f;
            let resp = ui.add(DragValue::new(&mut val).speed(0.1));
            (IrValue::F64(val), resp.changed())
        }
        IrValue::String(s) => {
            let mut val = s.clone();
            let resp = ui.text_edit_singleline(&mut val);
            let changed = resp.lost_focus() && val != *s;
            (IrValue::String(val), changed)
        }
        IrValue::Vec2(arr) => {
            let mut x = arr[0];
            let mut y = arr[1];
            ui.colored_label(COLOR_AXIS_X, "X");
            let rx = ui.add(DragValue::new(&mut x).speed(0.1));
            ui.colored_label(COLOR_AXIS_Y, "Y");
            let ry = ui.add(DragValue::new(&mut y).speed(0.1));
            let changed = rx.changed() || ry.changed();
            (IrValue::Vec2([x, y]), changed)
        }
        IrValue::Vec3(arr) => {
            let mut x = arr[0];
            let mut y = arr[1];
            let mut z = arr[2];
            ui.colored_label(COLOR_AXIS_X, "X");
            let rx = ui.add(DragValue::new(&mut x).speed(0.1));
            ui.colored_label(COLOR_AXIS_Y, "Y");
            let ry = ui.add(DragValue::new(&mut y).speed(0.1));
            ui.colored_label(COLOR_AXIS_Z, "Z");
            let rz = ui.add(DragValue::new(&mut z).speed(0.1));
            let changed = rx.changed() || ry.changed() || rz.changed();
            (IrValue::Vec3([x, y, z]), changed)
        }
        IrValue::Quat(arr) => {
            let mut q = *arr;
            let r0 = ui.add(DragValue::new(&mut q[0]).speed(0.01).prefix("X:"));
            let r1 = ui.add(DragValue::new(&mut q[1]).speed(0.01).prefix("Y:"));
            let r2 = ui.add(DragValue::new(&mut q[2]).speed(0.01).prefix("Z:"));
            let r3 = ui.add(DragValue::new(&mut q[3]).speed(0.01).prefix("W:"));
            let changed = r0.changed() || r1.changed() || r2.changed() || r3.changed();
            (IrValue::Quat(q), changed)
        }
        IrValue::Color(c) => {
            let mut col = *c;
            let resp = ui.color_edit_button_rgba_unmultiplied(&mut col);
            (IrValue::Color(col), resp.changed())
        }
        IrValue::NodePath(p) => {
            let mut val = p.clone();
            let resp = ui.text_edit_singleline(&mut val);
            let changed = resp.lost_focus() && val != *p;
            (IrValue::NodePath(val), changed)
        }
        IrValue::ResourcePath(p) => {
            let mut val = p.clone();
            ui.label("📁");
            let resp = ui.text_edit_singleline(&mut val);
            let changed = resp.lost_focus() && val != *p;
            (IrValue::ResourcePath(val), changed)
        }
        IrValue::Array(items) => {
            ui.label(format!("[{} items]", items.len()));
            (IrValue::Array(items.clone()), false)
        }
        IrValue::Map(map) => {
            ui.label(format!("Map({} entries)", map.len()));
            (IrValue::Map(map.clone()), false)
        }
    }
}

fn render_add_property_helper(app: &mut ForeplayEditorApp, ui: &mut Ui, node_id: NodeId) {
    ui.collapsing("➕ Add New Property", |ui| {
        ui.horizontal(|ui| {
            ui.label("Key:");
            ui.text_edit_singleline(&mut app.new_prop_key);
        });

        ui.horizontal(|ui| {
            ui.label("Type:");
            egui::ComboBox::from_id_salt("new_prop_type_combo")
                .selected_text(format!("{:?}", app.new_prop_type))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut app.new_prop_type, PropTypeChoice::F64, "F64");
                    ui.selectable_value(&mut app.new_prop_type, PropTypeChoice::Vec3, "Vec3");
                    ui.selectable_value(&mut app.new_prop_type, PropTypeChoice::String, "String");
                    ui.selectable_value(&mut app.new_prop_type, PropTypeChoice::Bool, "Bool");
                    ui.selectable_value(&mut app.new_prop_type, PropTypeChoice::I64, "I64");
                    ui.selectable_value(&mut app.new_prop_type, PropTypeChoice::Vec2, "Vec2");
                    ui.selectable_value(&mut app.new_prop_type, PropTypeChoice::Color, "Color");
                });
        });

        if ui.button("+ Insert Property").clicked() && !app.new_prop_key.is_empty() {
            let default_val = match app.new_prop_type {
                PropTypeChoice::F64 => IrValue::F64(1.0),
                PropTypeChoice::Vec3 => IrValue::Vec3([0.0, 0.0, 0.0]),
                PropTypeChoice::String => IrValue::String("new_val".into()),
                PropTypeChoice::Bool => IrValue::Bool(false),
                PropTypeChoice::I64 => IrValue::I64(0),
                PropTypeChoice::Vec2 => IrValue::Vec2([0.0, 0.0]),
                PropTypeChoice::Color => IrValue::Color([1.0, 1.0, 1.0, 1.0]),
            };

            let cmd = EditorCommand::SetProperty {
                node_id,
                key: app.new_prop_key.clone(),
                old_value: None,
                new_value: Some(default_val),
            };
            app.execute_command(cmd);
            app.new_prop_key.clear();
        }
    });
}

/// Dedicated FeelSpec live tuning dock panel.
pub fn show_feel_spec(app: &mut ForeplayEditorApp, ui: &mut Ui) {
    ScrollArea::vertical().show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("3JSE FeelSpec Live Tuning").strong());
            ui.colored_label(COLOR_ACCENT_BLUE, "(Interactive Gameplay Knobs)");
        });
        ui.label(RichText::new("Every slider drag immediately updates physics and scene IR via EditorCommand::TuneFeelSpec.").small().color(COLOR_TEXT_MUTED));
        ui.separator();

        let feel_overrides = app.session.tree.active_scene.feel_overrides.clone();

        // 1. Ollie Pop Impulse (Jump Height)
        feel_slider_row(
            app,
            ui,
            "ollie_pop_impulse",
            "Ollie Pop Impulse",
            "Jump vertical impulse force",
            "m/s",
            0.0,
            20.0,
            6.8,
            &feel_overrides,
        );

        // 2. Grind Snap Radius
        feel_slider_row(
            app,
            ui,
            "grind_snap_radius",
            "Grind Snap Radius",
            "Magnetic rail detection & lock distance",
            "m",
            0.05,
            2.0,
            0.35,
            &feel_overrides,
        );

        // 3. Gravity Scale
        feel_slider_row(
            app,
            ui,
            "gravity_scale",
            "Gravity Scale",
            "World gravity multiplier for skate physics",
            "x",
            0.1,
            5.0,
            1.2,
            &feel_overrides,
        );

        // 4. Camera FOV
        feel_slider_row(
            app,
            ui,
            "camera_fov",
            "Camera FOV",
            "Follow skate camera perspective field of view",
            "deg",
            30.0,
            120.0,
            75.0,
            &feel_overrides,
        );

        // Render any additional custom feel overrides
        let standard_keys = ["ollie_pop_impulse", "grind_snap_radius", "gravity_scale", "camera_fov"];
        for (key, &val) in &feel_overrides {
            if !standard_keys.contains(&key.as_str()) {
                feel_slider_row(
                    app,
                    ui,
                    key,
                    key,
                    "Custom feel parameter",
                    "unit",
                    0.0,
                    100.0,
                    val,
                    &feel_overrides,
                );
            }
        }

        ui.separator();

        // Helper to add custom feel knob
        ui.collapsing("➕ Add Custom FeelSpec Knob", |ui| {
            ui.horizontal(|ui| {
                ui.label("Knob Key:");
                ui.text_edit_singleline(&mut app.new_feel_key);
            });
            ui.horizontal(|ui| {
                ui.label("Initial Value:");
                ui.add(DragValue::new(&mut app.new_feel_value).speed(0.1));
            });
            if ui.button("+ Register Feel Knob").clicked() && !app.new_feel_key.is_empty() {
                let cmd = EditorCommand::TuneFeelSpec {
                    key: app.new_feel_key.clone(),
                    old_value: None,
                    new_value: Some(app.new_feel_value),
                };
                app.execute_command(cmd);
                app.new_feel_key.clear();
            }
        });
    });
}

#[expect(clippy::too_many_arguments)]
fn feel_slider_row(
    app: &mut ForeplayEditorApp,
    ui: &mut Ui,
    key: &str,
    label: &str,
    intent: &str,
    unit: &str,
    min: f64,
    max: f64,
    default_val: f64,
    current_overrides: &std::collections::BTreeMap<String, f64>,
) {
    let current_val = *current_overrides.get(key).unwrap_or(&default_val);
    let mut slider_val = current_val;

    ui.group(|ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(label).strong());
            ui.colored_label(COLOR_TEXT_MUTED, format!("({intent})"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.colored_label(COLOR_SUCCESS_GREEN, format!("{slider_val:.2} {unit}"));
            });
        });

        let slider = egui::Slider::new(&mut slider_val, min..=max)
            .suffix(format!(" {unit}"))
            .show_value(true);

        if ui.add(slider).changed() && (slider_val - current_val).abs() > 1e-5 {
            let cmd = EditorCommand::TuneFeelSpec {
                key: key.to_string(),
                old_value: Some(current_val),
                new_value: Some(slider_val),
            };
            app.execute_command(cmd);
        }
    });
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PropTypeChoice {
    F64,
    Vec3,
    String,
    Bool,
    I64,
    Vec2,
    Color,
}
