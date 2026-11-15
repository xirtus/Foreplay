//! Godot 4 Node Dock (Signals & Groups) (`foreplay-editor`).
//!
//! Mirrors Godot 4's Node tab located beside the Inspector:
//! - Signals sub-tab: Displays real built-in Godot signals looked up from
//!   `foreplay_ir::catalog::get_node_class`, active signal connections, and signal connection picker
//!   dispatching `EditorCommand::ConnectSignal` and `EditorCommand::DisconnectSignal`.
//! - Groups sub-tab: Manages node groups dispatching `EditorCommand::SetGroups`.

use egui::{RichText, ScrollArea, Ui};
use foreplay_ir::catalog::get_node_class;
use foreplay_ir::{EditorCommand, NodeId, SignalConnection};
use crate::ForeplayEditorApp;
use crate::theme::{
    COLOR_ACCENT_BLUE, COLOR_ERROR_RED, COLOR_SUCCESS_GREEN, COLOR_TEXT_MUTED, COLOR_WARN_YELLOW,
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NodeDockSubTab {
    Signals,
    Groups,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NodeDockState {
    pub active_tab: NodeDockSubTab,
    pub selected_signal: String,
    pub target_node_id: Option<NodeId>,
    pub receiver_method: String,
    pub new_group_name: String,
}

impl Default for NodeDockState {
    fn default() -> Self {
        Self {
            active_tab: NodeDockSubTab::Signals,
            selected_signal: "trick_landed".into(),
            target_node_id: None,
            receiver_method: "_on_trick_landed".into(),
            new_group_name: String::new(),
        }
    }
}

pub fn show(app: &mut ForeplayEditorApp, ui: &mut Ui) {
    let Some(selected_id) = app.selected_node else {
        ui.vertical_centered(|ui| {
            ui.add_space(30.0);
            ui.colored_label(COLOR_TEXT_MUTED, "No node selected.");
            ui.label("Select a node to inspect its Signals and Groups.");
        });
        return;
    };

    let Some(node) = app.session.tree.active_scene.nodes.get(&selected_id).cloned() else {
        ui.colored_label(COLOR_ERROR_RED, "Selected node not found in scene.");
        return;
    };

    ui.horizontal(|ui| {
        ui.colored_label(COLOR_ACCENT_BLUE, format!("Node: '{}' [{}]", node.name, node.class_name));
    });

    ui.separator();

    ui.horizontal(|ui| {
        ui.selectable_value(&mut app.node_dock_state.active_tab, NodeDockSubTab::Signals, "🔌 Signals");
        ui.selectable_value(&mut app.node_dock_state.active_tab, NodeDockSubTab::Groups, "🏷 Groups");
    });

    ui.separator();

    match app.node_dock_state.active_tab {
        NodeDockSubTab::Signals => show_signals_tab(app, ui, selected_id, &node),
        NodeDockSubTab::Groups => show_groups_tab(app, ui, selected_id, &node),
    }
}

fn show_signals_tab(app: &mut ForeplayEditorApp, ui: &mut Ui, selected_id: NodeId, node: &foreplay_ir::IrNode) {
    ScrollArea::vertical().show(ui, |ui| {
        ui.label(RichText::new("Canonical Class Signals (Godot 4.4 ClassDB)").strong());

        // Look up class spec from foreplay-ir catalog
        let class_spec = get_node_class(&node.class_name);
        let signals = class_spec
            .map(|s| {
                s.built_in_signals
                    .iter()
                    .map(|sig| {
                        let sig_sig = if sig.parameters.is_empty() {
                            format!("{}()", sig.name)
                        } else {
                            let params = sig
                                .parameters
                                .iter()
                                .map(|(p, t)| format!("{p}: {t}"))
                                .collect::<Vec<_>>()
                                .join(", ");
                            format!("{}({params})", sig.name)
                        };
                        (sig.name.clone(), sig_sig, sig.doc.clone())
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(|| {
                vec![
                    ("ready".into(), "ready()".into(), "Emitted when node enters tree.".into()),
                    ("tree_entered".into(), "tree_entered()".into(), "Emitted upon tree entry.".into()),
                    ("tree_exited".into(), "tree_exited()".into(), "Emitted upon tree exit.".into()),
                ]
            });

        for (sig_name, sig_display, sig_doc) in signals {
            let is_sel = app.node_dock_state.selected_signal == sig_name;
            ui.horizontal(|ui| {
                ui.colored_label(COLOR_WARN_YELLOW, "⚡");
                let label_resp = ui.selectable_label(is_sel, &sig_display);
                if label_resp.clicked() {
                    app.node_dock_state.selected_signal = sig_name.clone();
                    app.node_dock_state.receiver_method = format!("_on_{}_{}", node.name.to_lowercase(), sig_name);
                }
                label_resp.on_hover_text(sig_doc);
            });
        }

        ui.separator();
        ui.label(RichText::new("Active Signal Connections").strong());

        let connections = app.session.tree.active_scene.connections.clone();
        let mut to_disconnect = None;

        for conn in &connections {
            if conn.from_node == selected_id || conn.to_node == selected_id {
                let to_node_name = app
                    .session
                    .tree
                    .active_scene
                    .nodes
                    .get(&conn.to_node)
                    .map(|n| n.name.as_str())
                    .unwrap_or("Unknown");

                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        ui.colored_label(COLOR_SUCCESS_GREEN, "🔌 Connected");
                        ui.label(format!("signal '{}' -> {}.{}()", conn.signal_name, to_node_name, conn.method_name));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Disconnect").clicked() {
                                to_disconnect = Some(conn.clone());
                            }
                        });
                    });
                });
            }
        }

        if let Some(conn) = to_disconnect {
            app.execute_command(EditorCommand::DisconnectSignal { connection: conn });
        }

        ui.separator();
        ui.label(RichText::new("Connect Signal to Receiver Method").strong());

        ui.horizontal(|ui| {
            ui.label("Signal:");
            ui.colored_label(COLOR_WARN_YELLOW, &app.node_dock_state.selected_signal);
        });

        ui.horizontal(|ui| {
            ui.label("Target Node:");
            let target_name = app
                .node_dock_state
                .target_node_id
                .and_then(|id| app.session.tree.active_scene.nodes.get(&id))
                .map(|n| n.name.as_str())
                .unwrap_or("None selected");

            egui::ComboBox::from_id_salt("signal_target_node_combo")
                .selected_text(target_name)
                .show_ui(ui, |ui| {
                    for (&other_id, other_node) in &app.session.tree.active_scene.nodes {
                        if other_id != selected_id {
                            let is_chosen = app.node_dock_state.target_node_id == Some(other_id);
                            if ui.selectable_label(is_chosen, &other_node.name).clicked() {
                                app.node_dock_state.target_node_id = Some(other_id);
                            }
                        }
                    }
                });
        });

        ui.horizontal(|ui| {
            ui.label("Method:");
            ui.text_edit_singleline(&mut app.node_dock_state.receiver_method);
        });

        if let Some(target_id) = app.node_dock_state.target_node_id
            && ui.button(RichText::new("🔗 Connect Signal").strong().color(COLOR_SUCCESS_GREEN)).clicked()
        {
            let conn = SignalConnection {
                from_node: selected_id,
                signal_name: app.node_dock_state.selected_signal.clone(),
                to_node: target_id,
                method_name: app.node_dock_state.receiver_method.clone(),
            };
            app.execute_command(EditorCommand::ConnectSignal { connection: conn });
        }
    });
}

fn show_groups_tab(app: &mut ForeplayEditorApp, ui: &mut Ui, selected_id: NodeId, node: &foreplay_ir::IrNode) {
    ScrollArea::vertical().show(ui, |ui| {
        ui.label(RichText::new("Node Groups").strong());
        ui.colored_label(COLOR_TEXT_MUTED, "Groups tag nodes for collision queries, broadcast calls, and feel tuning.");

        ui.add_space(6.0);

        // Add new group
        ui.horizontal(|ui| {
            ui.label("Add Group:");
            ui.text_edit_singleline(&mut app.node_dock_state.new_group_name);
            if ui.button("➕ Add").clicked() && !app.node_dock_state.new_group_name.trim().is_empty() {
                let mut groups = node.groups.clone();
                let new_g = app.node_dock_state.new_group_name.trim().to_string();
                if !groups.contains(&new_g) {
                    groups.push(new_g);
                    let cmd = EditorCommand::SetGroups {
                        node_id: selected_id,
                        old_groups: node.groups.clone(),
                        new_groups: groups,
                    };
                    app.execute_command(cmd);
                    app.node_dock_state.new_group_name.clear();
                }
            }
        });

        ui.separator();
        ui.label(RichText::new("Active Groups on Node").strong());

        if node.groups.is_empty() {
            ui.colored_label(COLOR_TEXT_MUTED, "This node does not belong to any groups.");
        } else {
            let mut to_remove = None;
            for (idx, group) in node.groups.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.colored_label(COLOR_ACCENT_BLUE, format!("🏷 {group}"));
                    if ui.small_button("✖").clicked() {
                        to_remove = Some(idx);
                    }
                });
            }

            if let Some(remove_idx) = to_remove {
                let mut groups = node.groups.clone();
                groups.remove(remove_idx);
                let cmd = EditorCommand::SetGroups {
                    node_id: selected_id,
                    old_groups: node.groups.clone(),
                    new_groups: groups,
                };
                app.execute_command(cmd);
            }
        }

        ui.separator();
        ui.label(RichText::new("Common Group Presets").strong());
        let presets = ["player", "skaters", "obstacles", "rails", "ramps", "ui", "triggers"];
        let mut modified = false;
        let mut groups = node.groups.clone();

        ui.horizontal_wrapped(|ui| {
            for p in presets {
                if ui.small_button(format!("+ {p}")).clicked() && !groups.contains(&p.to_string()) {
                    groups.push(p.to_string());
                    modified = true;
                }
            }
        });

        if modified {
            let cmd = EditorCommand::SetGroups {
                node_id: selected_id,
                old_groups: node.groups.clone(),
                new_groups: groups,
            };
            app.execute_command(cmd);
        }
    });
}
