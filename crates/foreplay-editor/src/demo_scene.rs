//! Starter demo scene generator (`foreplay-editor`).
//!
//! Populates a rich skater / platformer park scene entirely through
//! `EditorCommand`s so that the Undo stack and AI Harness remain 100% synchronized.

use std::collections::BTreeMap;
use foreplay_harness::HarnessSession;
use foreplay_ir::{EditorCommand, IrError, IrNode, IrValue, NodeId, SignalConnection};

/// Populates the starter demo scene using pure `EditorCommand` dispatches.
pub fn populate_starter_scene(session: &mut HarnessSession) -> Result<(), IrError> {
    // 1. SkaterRoot (CharacterBody3D)
    let skater_id = NodeId::new(1, 1).expect("non-zero generation");
    let mut skater_props = BTreeMap::new();
    skater_props.insert("position".into(), IrValue::Vec3([0.0, 1.0, 0.0]));
    skater_props.insert("scale".into(), IrValue::Vec3([1.0, 1.0, 1.0]));
    skater_props.insert("speed".into(), IrValue::F64(8.5));
    skater_props.insert("max_speed".into(), IrValue::F64(14.5));
    skater_props.insert("is_grounded".into(), IrValue::Bool(true));

    session.execute(EditorCommand::InsertNode {
        node: IrNode {
            id: skater_id,
            name: "SkaterRoot".into(),
            class_name: "CharacterBody3D".into(),
            parent: None,
            instance_scene: None,
            properties: skater_props,
            groups: vec!["players".into(), "skaters".into()],
        },
    })?;

    // 2. SkateCamera (Camera3D) - child of SkaterRoot
    let cam_id = NodeId::new(2, 1).expect("non-zero generation");
    let mut cam_props = BTreeMap::new();
    cam_props.insert("position".into(), IrValue::Vec3([0.0, 2.5, 4.5]));
    cam_props.insert("scale".into(), IrValue::Vec3([1.0, 1.0, 1.0]));
    cam_props.insert("fov".into(), IrValue::F64(75.0));
    cam_props.insert("current".into(), IrValue::Bool(true));

    session.execute(EditorCommand::InsertNode {
        node: IrNode {
            id: cam_id,
            name: "SkateCamera".into(),
            class_name: "Camera3D".into(),
            parent: Some(skater_id),
            instance_scene: None,
            properties: cam_props,
            groups: vec!["cameras".into()],
        },
    })?;

    // 3. SkaterMesh (Node3D) - child of SkaterRoot
    let mesh_id = NodeId::new(3, 1).expect("non-zero generation");
    let mut mesh_props = BTreeMap::new();
    mesh_props.insert("position".into(), IrValue::Vec3([0.0, 0.0, 0.0]));
    mesh_props.insert("scale".into(), IrValue::Vec3([0.8, 1.8, 0.8]));

    session.execute(EditorCommand::InsertNode {
        node: IrNode {
            id: mesh_id,
            name: "SkaterMesh".into(),
            class_name: "Node3D".into(),
            parent: Some(skater_id),
            instance_scene: Some("res://models/skater.glb".into()),
            properties: mesh_props,
            groups: vec![],
        },
    })?;

    // 4. GroundPlane (StaticBody3D)
    let ground_id = NodeId::new(4, 1).expect("non-zero generation");
    let mut ground_props = BTreeMap::new();
    ground_props.insert("position".into(), IrValue::Vec3([0.0, -0.25, 0.0]));
    ground_props.insert("scale".into(), IrValue::Vec3([25.0, 0.5, 25.0]));
    ground_props.insert("friction".into(), IrValue::F64(0.85));

    session.execute(EditorCommand::InsertNode {
        node: IrNode {
            id: ground_id,
            name: "GroundPlane".into(),
            class_name: "StaticBody3D".into(),
            parent: None,
            instance_scene: None,
            properties: ground_props,
            groups: vec!["geometry".into()],
        },
    })?;

    // 5. QuarterPipe (StaticBody3D)
    let qp_id = NodeId::new(5, 1).expect("non-zero generation");
    let mut qp_props = BTreeMap::new();
    qp_props.insert("position".into(), IrValue::Vec3([7.0, 2.0, 0.0]));
    qp_props.insert("scale".into(), IrValue::Vec3([4.0, 4.0, 8.0]));
    qp_props.insert("coping_grindable".into(), IrValue::Bool(true));

    session.execute(EditorCommand::InsertNode {
        node: IrNode {
            id: qp_id,
            name: "QuarterPipe".into(),
            class_name: "StaticBody3D".into(),
            parent: None,
            instance_scene: None,
            properties: qp_props,
            groups: vec!["ramps".into(), "grindable".into()],
        },
    })?;

    // 6. FunBox (StaticBody3D)
    let fb_id = NodeId::new(6, 1).expect("non-zero generation");
    let mut fb_props = BTreeMap::new();
    fb_props.insert("position".into(), IrValue::Vec3([-6.0, 0.75, 2.0]));
    fb_props.insert("scale".into(), IrValue::Vec3([5.0, 1.5, 4.0]));

    session.execute(EditorCommand::InsertNode {
        node: IrNode {
            id: fb_id,
            name: "FunBox".into(),
            class_name: "StaticBody3D".into(),
            parent: None,
            instance_scene: None,
            properties: fb_props,
            groups: vec!["geometry".into()],
        },
    })?;

    // 7. SunLight (DirectionalLight3D)
    let sun_id = NodeId::new(7, 1).expect("non-zero generation");
    let mut sun_props = BTreeMap::new();
    sun_props.insert("position".into(), IrValue::Vec3([10.0, 20.0, 10.0]));
    sun_props.insert("scale".into(), IrValue::Vec3([1.0, 1.0, 1.0]));
    sun_props.insert("intensity".into(), IrValue::F64(1200.0));
    sun_props.insert("color".into(), IrValue::Color([1.0, 0.95, 0.85, 1.0]));

    session.execute(EditorCommand::InsertNode {
        node: IrNode {
            id: sun_id,
            name: "SunLight".into(),
            class_name: "DirectionalLight3D".into(),
            parent: None,
            instance_scene: None,
            properties: sun_props,
            groups: vec!["lights".into()],
        },
    })?;

    // 8. HUDCanvas (Node2D)
    let hud_id = NodeId::new(8, 1).expect("non-zero generation");
    let mut hud_props = BTreeMap::new();
    hud_props.insert("position".into(), IrValue::Vec2([50.0, 50.0]));
    hud_props.insert("scale".into(), IrValue::Vec2([1.0, 1.0]));
    hud_props.insert("score".into(), IrValue::I64(4500));

    session.execute(EditorCommand::InsertNode {
        node: IrNode {
            id: hud_id,
            name: "HUDCanvas".into(),
            class_name: "Node2D".into(),
            parent: None,
            instance_scene: None,
            properties: hud_props,
            groups: vec!["ui".into()],
        },
    })?;

    // 9. PlayerIcon (Sprite2D) - child of HUDCanvas
    let icon_id = NodeId::new(9, 1).expect("non-zero generation");
    let mut icon_props = BTreeMap::new();
    icon_props.insert("position".into(), IrValue::Vec2([32.0, 32.0]));
    icon_props.insert("scale".into(), IrValue::Vec2([1.0, 1.0]));
    icon_props.insert(
        "texture".into(),
        IrValue::ResourcePath("res://textures/skater_icon.png".into()),
    );

    session.execute(EditorCommand::InsertNode {
        node: IrNode {
            id: icon_id,
            name: "PlayerIcon".into(),
            class_name: "Sprite2D".into(),
            parent: Some(hud_id),
            instance_scene: None,
            properties: icon_props,
            groups: vec!["ui".into()],
        },
    })?;

    // FeelSpec tuning overrides
    session.execute(EditorCommand::TuneFeelSpec {
        key: "ollie_pop_impulse".into(),
        old_value: None,
        new_value: Some(6.8),
    })?;
    session.execute(EditorCommand::TuneFeelSpec {
        key: "grind_snap_radius".into(),
        old_value: None,
        new_value: Some(0.35),
    })?;
    session.execute(EditorCommand::TuneFeelSpec {
        key: "gravity_scale".into(),
        old_value: None,
        new_value: Some(1.2),
    })?;
    session.execute(EditorCommand::TuneFeelSpec {
        key: "camera_fov".into(),
        old_value: None,
        new_value: Some(75.0),
    })?;

    // Semantic signal connection: SkaterRoot.trick_landed -> HUDCanvas.on_trick_landed
    session.tree.active_scene.connections.push(SignalConnection {
        from_node: skater_id,
        signal_name: "trick_landed".into(),
        to_node: hud_id,
        method_name: "on_trick_landed".into(),
    });

    Ok(())
}
