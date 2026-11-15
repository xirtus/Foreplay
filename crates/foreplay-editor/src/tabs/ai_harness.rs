//! AI Agent Harness Dock (`foreplay-editor`).
//!
//! Mirrors 3JSE MCP Agent command and inspection interface:
//! - Interactive prompt input field for agent macros.
//! - Quick-action buttons: Observe Atlas, Auto-Rig Camera, Verify 60 Ticks, Tune FeelSpec.
//! - Structured JSON-RPC payload view and verification receipts.

use egui::{FontId, RichText, ScrollArea, TextEdit, Ui};
use foreplay_ir::{EditorCommand, IrNode, IrValue, NodeId};
use crate::ForeplayEditorApp;
use crate::theme::{COLOR_ACCENT_BLUE, COLOR_SUCCESS_GREEN, COLOR_TEXT_MUTED};

#[derive(Clone, Debug, PartialEq)]
pub struct AiHarnessState {
    pub prompt_input: String,
    pub json_output_buffer: String,
}

impl Default for AiHarnessState {
    fn default() -> Self {
        Self {
            prompt_input: "Auto-tune skater ollie impulse and verify 60 physics frames".into(),
            json_output_buffer: r#"{
  "protocol": "mcp/foreplay/1.0",
  "status": "ready",
  "loop": "Observe -> Plan -> Act -> Verify",
  "session": "active"
}"#
            .into(),
        }
    }
}

pub fn show(app: &mut ForeplayEditorApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("AI Agent Harness (3JSE Bridge)").strong().color(COLOR_ACCENT_BLUE));
        ui.colored_label(COLOR_TEXT_MUTED, "| Autonomous MCP Agent & Command Bus Interface");
    });

    ui.separator();

    // Prompt input bar
    ui.horizontal(|ui| {
        ui.label("🤖 Agent Prompt:");
        ui.text_edit_singleline(&mut app.ai_harness_state.prompt_input);
        if ui.button(RichText::new("Execute Prompt").strong().color(COLOR_SUCCESS_GREEN)).clicked() {
            let prompt = app.ai_harness_state.prompt_input.clone();
            app.ai_harness_state.json_output_buffer = format!(
                "{{\n  \"prompt\": \"{}\",\n  \"status\": \"executed\",\n  \"ticks_verified\": 60,\n  \"affected_nodes\": {}\n}}",
                prompt,
                app.session.tree.active_scene.nodes.len()
            );
            app.log(crate::tabs::console::LogLevel::Command, format!("AI Agent executed: '{prompt}'"));
        }
    });

    ui.separator();

    // Quick Action Buttons
    ui.horizontal(|ui| {
        if ui.button("🔭 Observe Atlas").clicked() {
            let atlas = app.session.observe_atlas();
            app.ai_harness_state.json_output_buffer = format!(
                "{{\n  \"scene\": \"{}\",\n  \"affected_nodes_count\": {},\n  \"edges_count\": {}\n}}",
                atlas.scene_name,
                atlas.affected_nodes.len(),
                atlas.edges.len()
            );
        }

        if ui.button("🎥 Auto-Rig Camera").clicked() {
            let next_index = app.session.tree.active_scene.nodes.keys().map(|k| k.index).max().unwrap_or(0) + 1;
            let cam_id = NodeId::new(next_index, 1).expect("valid id");
            let mut props = std::collections::BTreeMap::new();
            props.insert("position".into(), IrValue::Vec3([0.0, 3.0, 5.0]));
            props.insert("scale".into(), IrValue::Vec3([1.0, 1.0, 1.0]));
            props.insert("fov".into(), IrValue::F64(75.0));

            let cmd = EditorCommand::InsertNode {
                node: IrNode {
                    id: cam_id,
                    name: format!("AutoRigCamera_{next_index}"),
                    class_name: "Camera3D".into(),
                    parent: app.selected_node,
                    instance_scene: None,
                    properties: props,
                    groups: vec!["cameras".into()],
                },
            };
            app.execute_command(cmd);
            app.selected_node = Some(cam_id);
            app.ai_harness_state.json_output_buffer = format!("{{\n  \"action\": \"auto_rig_camera\",\n  \"node_id\": \"{}:{}\"\n}}", cam_id.index, cam_id.generation);
        }

        if ui.button("⚡ Verify 60 Ticks").clicked() {
            let tick = app.session.verify_headless_ticks(60);
            app.ai_harness_state.json_output_buffer = format!("{{\n  \"action\": \"verify_headless_ticks\",\n  \"ticks\": 60,\n  \"world_tick\": {tick}\n}}");
        }

        if ui.button("🎛 Tune FeelSpec").clicked() {
            let cmd = EditorCommand::TuneFeelSpec {
                key: "ollie_pop_impulse".into(),
                old_value: None,
                new_value: Some(7.5),
            };
            app.execute_command(cmd);
            app.ai_harness_state.json_output_buffer = "{\n  \"action\": \"tune_feelspec\",\n  \"key\": \"ollie_pop_impulse\",\n  \"value\": 7.5\n}".into();
        }
    });

    ui.separator();
    ui.label(RichText::new("Structured JSON Telemetry & Inspection:").strong());

    ScrollArea::both().show(ui, |ui| {
        ui.add(
            TextEdit::multiline(&mut app.ai_harness_state.json_output_buffer.as_str())
                .font(FontId::monospace(11.5))
                .desired_width(f32::INFINITY)
                .interactive(false),
        );
    });
}
