//! Bidirectional 3JSE and Three.js scene importer & exporter.
//!
//! Converts between `THREE.Object3D.toJSON()` / `3jse.scene.json` formats and `foreplay_ir::IrScene`.

use crate::error::ThreeJsError;
use bevy_math::{EulerRot, Mat4, Quat, Vec3};
use foreplay_ir::{IrNode, IrScene, IrValue, NodeId, SignalConnection};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// Imports a Three.js scene JSON string (`THREE.Object3D.toJSON()` or `3jse.scene.json`) into `IrScene`.
pub fn import_threejs_json_to_ir(json_str: &str) -> Result<IrScene, ThreeJsError> {
    let root: Value = serde_json::from_str(json_str)
        .map_err(|e| ThreeJsError::InvalidScene(format!("Invalid JSON document: {e}")))?;

    // 1. Check for 3jse.scene format
    if root.get("format").and_then(|v| v.as_str()) == Some("3jse.scene")
        || (root.get("nodes").is_some() && root.get("feelSpec").is_some())
    {
        return import_3jse_scene_format(&root);
    }

    // 2. Standard THREE.Object3D.toJSON() format
    import_threejs_object_format(&root)
}

/// Exports an `IrScene` into a valid, lossless `THREE.Object3D.toJSON()` document string.
pub fn export_ir_to_threejs_json(scene: &IrScene) -> String {
    let mut geometries = Vec::new();
    let mut materials = Vec::new();

    // Group nodes by parent
    let mut children_map: BTreeMap<Option<NodeId>, Vec<NodeId>> = BTreeMap::new();
    for (&node_id, node) in &scene.nodes {
        let parent = node.parent.filter(|p| scene.nodes.contains_key(p));
        children_map.entry(parent).or_default().push(node_id);
    }

    // Sort children for determinism
    for list in children_map.values_mut() {
        list.sort();
    }

    // Convert root children
    let mut root_children = Vec::new();
    if let Some(roots) = children_map.get(&None) {
        for &root_id in roots {
            let child_obj = export_node_recursive(
                root_id,
                scene,
                &children_map,
                &mut geometries,
                &mut materials,
            );
            root_children.push(child_obj);
        }
    }

    let scene_uuid = deterministic_uuid(&format!("Scene_{}", scene.name));

    let json_doc = json!({
        "metadata": {
            "version": 4.5,
            "type": "Object",
            "generator": "Foreplay ThreeJs Exporter"
        },
        "geometries": geometries,
        "materials": materials,
        "object": {
            "uuid": scene_uuid,
            "type": "Scene",
            "name": scene.name,
            "matrix": [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0],
            "userData": {
                "foreplay": {
                    "scene_name": scene.name,
                    "feel_overrides": scene.feel_overrides,
                    "connections": scene.connections
                }
            },
            "children": root_children
        }
    });

    serde_json::to_string_pretty(&json_doc).unwrap_or_else(|_| "{}".into())
}

fn export_node_recursive(
    node_id: NodeId,
    scene: &IrScene,
    children_map: &BTreeMap<Option<NodeId>, Vec<NodeId>>,
    geometries: &mut Vec<Value>,
    materials: &mut Vec<Value>,
) -> Value {
    let node = &scene.nodes[&node_id];
    let node_uuid = deterministic_uuid(&format!("Node_{}_{}", node_id.index, node_id.generation));

    let (three_type, geom_uuid, mat_uuid) = match node.class_name.as_str() {
        "Camera3D" => ("PerspectiveCamera", None, None),
        "DirectionalLight3D" => ("DirectionalLight", None, None),
        "OmniLight3D" => ("PointLight", None, None),
        "SpotLight3D" => ("SpotLight", None, None),
        "MeshInstance3D" | "CSGBox3D" => {
            let g_uuid = deterministic_uuid(&format!("Geom_{}_{}", node_id.index, node_id.generation));
            let m_uuid = deterministic_uuid(&format!("Mat_{}_{}", node_id.index, node_id.generation));

            // Box dimensions from properties
            let (w, h, d) = extract_size_props(&node.properties);
            geometries.push(json!({
                "uuid": g_uuid,
                "type": "BoxGeometry",
                "width": w,
                "height": h,
                "depth": d
            }));

            let color_int = extract_color_prop(&node.properties);
            let roughness = extract_f64_prop(&node.properties, "roughness", 0.5);
            let metalness = extract_f64_prop(&node.properties, "metalness", 0.1);

            materials.push(json!({
                "uuid": m_uuid,
                "type": "MeshStandardMaterial",
                "color": color_int,
                "roughness": roughness,
                "metalness": metalness
            }));

            ("Mesh", Some(g_uuid), Some(m_uuid))
        }
        _ => ("Group", None, None),
    };

    let matrix_cols = compute_matrix_from_properties(&node.properties);

    // Recursively convert children
    let mut children_objs = Vec::new();
    if let Some(children) = children_map.get(&Some(node_id)) {
        for &child_id in children {
            let child_val = export_node_recursive(
                child_id,
                scene,
                children_map,
                geometries,
                materials,
            );
            children_objs.push(child_val);
        }
    }

    let mut obj = json!({
        "uuid": node_uuid,
        "type": three_type,
        "name": node.name,
        "matrix": matrix_cols,
        "userData": {
            "foreplayId": { "index": node.id.index, "generation": node.id.generation.get() },
            "class_name": node.class_name,
            "groups": node.groups,
            "properties": node.properties
        },
        "children": children_objs
    });

    if let Some(g) = geom_uuid {
        obj["geometry"] = Value::String(g);
    }
    if let Some(m) = mat_uuid {
        obj["material"] = Value::String(m);
    }

    obj
}

fn import_3jse_scene_format(root: &Value) -> Result<IrScene, ThreeJsError> {
    let name = root
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("3JseScene")
        .to_string();

    let mut scene = IrScene::new(name);

    // Feel spec overrides
    if let Some(feel_obj) = root.get("feelSpec").or_else(|| root.get("feel_overrides")).and_then(|v| v.as_object()) {
        for (k, v) in feel_obj {
            if let Some(num) = v.as_f64() {
                scene.feel_overrides.insert(k.clone(), num);
            }
        }
    }

    // Connections
    if let Some(conn_arr) = root.get("signals").or_else(|| root.get("connections")).and_then(|v| v.as_array()) {
        for c_val in conn_arr {
            if let Ok(conn) = serde_json::from_value::<SignalConnection>(c_val.clone()) {
                scene.connections.push(conn);
            }
        }
    }

    // Nodes
    if let Some(nodes_val) = root.get("nodes") {
        if let Some(nodes_arr) = nodes_val.as_array() {
            for n_val in nodes_arr {
                let node = parse_node_from_json(n_val)?;
                scene.nodes.insert(node.id, node);
            }
        } else if let Some(nodes_obj) = nodes_val.as_object() {
            for (_key, n_val) in nodes_obj {
                let node = parse_node_from_json(n_val)?;
                scene.nodes.insert(node.id, node);
            }
        }
    }

    Ok(scene)
}

fn import_threejs_object_format(root: &Value) -> Result<IrScene, ThreeJsError> {
    let object_val = root
        .get("object")
        .ok_or_else(|| ThreeJsError::InvalidScene("Missing 'object' root in Three.js JSON".into()))?;

    // Build geometry lookup: uuid -> Value
    let mut geom_map = BTreeMap::new();
    if let Some(geoms) = root.get("geometries").and_then(|v| v.as_array()) {
        for g in geoms {
            if let Some(uuid) = g.get("uuid").and_then(|v| v.as_str()) {
                geom_map.insert(uuid.to_string(), g.clone());
            }
        }
    }

    // Build material lookup: uuid -> Value
    let mut mat_map = BTreeMap::new();
    if let Some(mats) = root.get("materials").and_then(|v| v.as_array()) {
        for m in mats {
            if let Some(uuid) = m.get("uuid").and_then(|v| v.as_str()) {
                mat_map.insert(uuid.to_string(), m.clone());
            }
        }
    }

    // Check for foreplay scene metadata in root or object.userData
    let scene_name = object_val
        .get("userData")
        .and_then(|u| u.get("foreplay"))
        .and_then(|f| f.get("scene_name"))
        .and_then(|v| v.as_str())
        .or_else(|| object_val.get("name").and_then(|v| v.as_str()))
        .unwrap_or("ThreeJsImportedScene")
        .to_string();

    let mut scene = IrScene::new(scene_name);

    // Extract feel overrides from foreplay metadata
    if let Some(overrides) = object_val
        .get("userData")
        .and_then(|u| u.get("foreplay"))
        .and_then(|f| f.get("feel_overrides"))
        .and_then(|v| v.as_object())
    {
        for (k, v) in overrides {
            if let Some(num) = v.as_f64() {
                scene.feel_overrides.insert(k.clone(), num);
            }
        }
    }

    // Extract signal connections from foreplay metadata
    if let Some(conns) = object_val
        .get("userData")
        .and_then(|u| u.get("foreplay"))
        .and_then(|f| f.get("connections"))
        .and_then(|v| v.as_array())
    {
        for c in conns {
            if let Ok(conn) = serde_json::from_value::<SignalConnection>(c.clone()) {
                scene.connections.push(conn);
            }
        }
    }

    let mut next_id: u32 = 1;

    // Traverse children of root object
    if let Some(children) = object_val.get("children").and_then(|v| v.as_array()) {
        for child_val in children {
            import_object_recursive(
                child_val,
                None,
                &geom_map,
                &mat_map,
                &mut scene,
                &mut next_id,
            )?;
        }
    }

    Ok(scene)
}

fn import_object_recursive(
    obj_val: &Value,
    parent_id: Option<NodeId>,
    geom_map: &BTreeMap<String, Value>,
    mat_map: &BTreeMap<String, Value>,
    scene: &mut IrScene,
    next_id: &mut u32,
) -> Result<NodeId, ThreeJsError> {
    let node_id = if let Some(id_val) = obj_val
        .get("userData")
        .and_then(|u| u.get("foreplayId").or_else(|| u.get("id")))
    {
        parse_node_id_val(id_val).unwrap_or_else(|| {
            let id = NodeId::new(*next_id, 1).unwrap();
            *next_id += 1;
            id
        })
    } else {
        let id = NodeId::new(*next_id, 1).unwrap();
        *next_id += 1;
        id
    };

    let name = obj_val
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("Node")
        .to_string();

    let raw_type = obj_val
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("Group");

    let class_name = if let Some(c) = obj_val
        .get("userData")
        .and_then(|u| u.get("class_name").or_else(|| u.get("foreplay_class")))
        .and_then(|v| v.as_str())
    {
        c.to_string()
    } else {
        map_three_type_to_godot_class(raw_type)
    };

    let groups: Vec<String> = obj_val
        .get("userData")
        .and_then(|u| u.get("groups"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    let mut properties = BTreeMap::new();

    // Copy any custom properties from userData.properties
    if let Some(props_obj) = obj_val
        .get("userData")
        .and_then(|u| u.get("properties"))
        .and_then(|v| v.as_object())
    {
        for (k, v) in props_obj {
            if let Ok(ir_v) = serde_json::from_value::<IrValue>(v.clone()) {
                properties.insert(k.clone(), ir_v);
            }
        }
    }

    // Extract transform: decompose 4x4 column-major matrix or read position/rotation/scale
    if let Some(matrix_arr) = obj_val.get("matrix").and_then(|v| v.as_array()) {
        if matrix_arr.len() == 16 {
            let mut cols = [0.0f32; 16];
            for (idx, elem) in matrix_arr.iter().enumerate() {
                cols[idx] = elem.as_f64().unwrap_or(0.0) as f32;
            }
            let (pos, rot, scale) = decompose_cols_matrix(&cols)?;
            properties.insert("position".into(), IrValue::Vec3([pos.x, pos.y, pos.z]));
            properties.insert("quaternion".into(), IrValue::Quat([rot.x, rot.y, rot.z, rot.w]));
            properties.insert("scale".into(), IrValue::Vec3([scale.x, scale.y, scale.z]));
        }
    } else {
        if let Some(pos_val) = obj_val.get("position")
            && let Some(arr) = pos_val.as_array()
            && arr.len() == 3
        {
            properties.insert(
                "position".into(),
                IrValue::Vec3([
                    arr[0].as_f64().unwrap_or(0.0) as f32,
                    arr[1].as_f64().unwrap_or(0.0) as f32,
                    arr[2].as_f64().unwrap_or(0.0) as f32,
                ]),
            );
        }
        if let Some(scale_val) = obj_val.get("scale")
            && let Some(arr) = scale_val.as_array()
            && arr.len() == 3
        {
            properties.insert(
                "scale".into(),
                IrValue::Vec3([
                    arr[0].as_f64().unwrap_or(1.0) as f32,
                    arr[1].as_f64().unwrap_or(1.0) as f32,
                    arr[2].as_f64().unwrap_or(1.0) as f32,
                ]),
            );
        }
    }

    // Geometry lookup
    if let Some(geom_uuid) = obj_val.get("geometry").and_then(|v| v.as_str())
        && let Some(geom) = geom_map.get(geom_uuid)
    {
        let gtype = geom.get("type").and_then(|v| v.as_str()).unwrap_or("");
        match gtype {
            "BoxGeometry" => {
                let w = geom.get("width").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
                let h = geom.get("height").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
                let d = geom.get("depth").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
                properties.insert("size".into(), IrValue::Vec3([w, h, d]));
            }
            "SphereGeometry" => {
                let r = geom.get("radius").and_then(|v| v.as_f64()).unwrap_or(1.0);
                properties.insert("radius".into(), IrValue::F64(r));
            }
            "CylinderGeometry" => {
                let h = geom.get("height").and_then(|v| v.as_f64()).unwrap_or(2.0);
                let r = geom.get("radiusBottom").and_then(|v| v.as_f64()).unwrap_or(1.0);
                properties.insert("height".into(), IrValue::F64(h));
                properties.insert("radius".into(), IrValue::F64(r));
            }
            _ => {}
        }
    }

    // Material lookup
    if let Some(mat_uuid) = obj_val.get("material").and_then(|v| v.as_str())
        && let Some(mat) = mat_map.get(mat_uuid)
    {
        if let Some(c_int) = mat.get("color").and_then(|v| v.as_u64()) {
            let r = ((c_int >> 16) & 0xff) as f32 / 255.0;
            let g = ((c_int >> 8) & 0xff) as f32 / 255.0;
            let b = (c_int & 0xff) as f32 / 255.0;
            properties.insert("color".into(), IrValue::Color([r, g, b, 1.0]));
        }
        if let Some(rough) = mat.get("roughness").and_then(|v| v.as_f64()) {
            properties.insert("roughness".into(), IrValue::F64(rough));
        }
        if let Some(metal) = mat.get("metalness").and_then(|v| v.as_f64()) {
            properties.insert("metalness".into(), IrValue::F64(metal));
        }
    }

    let ir_node = IrNode {
        id: node_id,
        name,
        class_name,
        parent: parent_id,
        instance_scene: None,
        properties,
        groups,
    };

    scene.nodes.insert(node_id, ir_node);

    // Recursively parse children
    if let Some(children) = obj_val.get("children").and_then(|v| v.as_array()) {
        for child_val in children {
            import_object_recursive(
                child_val,
                Some(node_id),
                geom_map,
                mat_map,
                scene,
                next_id,
            )?;
        }
    }

    Ok(node_id)
}

fn parse_node_from_json(val: &Value) -> Result<IrNode, ThreeJsError> {
    let id_val = val
        .get("id")
        .ok_or_else(|| ThreeJsError::InvalidScene("Missing 'id' in node".into()))?;
    let id = parse_node_id_val(id_val)
        .ok_or_else(|| ThreeJsError::InvalidScene("Invalid 'id' format in node".into()))?;

    let name = val
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("Node")
        .to_string();

    let class_name = val
        .get("class_name")
        .or_else(|| val.get("type"))
        .and_then(|v| v.as_str())
        .unwrap_or("Node3D")
        .to_string();

    let parent = val.get("parent").and_then(parse_node_id_val);

    let instance_scene = val
        .get("instance_scene")
        .and_then(|v| v.as_str())
        .map(str::to_string);

    let groups: Vec<String> = val
        .get("groups")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    let mut properties = BTreeMap::new();
    if let Some(props_obj) = val.get("properties").and_then(|v| v.as_object()) {
        for (k, v) in props_obj {
            if let Ok(ir_v) = serde_json::from_value::<IrValue>(v.clone()) {
                properties.insert(k.clone(), ir_v);
            }
        }
    }

    Ok(IrNode {
        id,
        name,
        class_name,
        parent,
        instance_scene,
        properties,
        groups,
    })
}

fn parse_node_id_val(v: &Value) -> Option<NodeId> {
    if let Some(obj) = v.as_object() {
        let idx = obj.get("index").and_then(|v| v.as_u64())? as u32;
        let generation = obj.get("generation").and_then(|v| v.as_u64()).unwrap_or(1) as u32;
        NodeId::new(idx, generation)
    } else if let Some(num) = v.as_u64() {
        NodeId::new(num as u32, 1)
    } else {
        None
    }
}

fn map_three_type_to_godot_class(three_type: &str) -> String {
    match three_type {
        "PerspectiveCamera" | "Camera" => "Camera3D".into(),
        "DirectionalLight" => "DirectionalLight3D".into(),
        "PointLight" => "OmniLight3D".into(),
        "SpotLight" => "SpotLight3D".into(),
        "Mesh" => "MeshInstance3D".into(),
        _ => "Node3D".into(),
    }
}

pub fn decompose_cols_matrix(
    cols: &[f32; 16],
) -> Result<(Vec3, Quat, Vec3), ThreeJsError> {
    let mat = Mat4::from_cols_array(cols);
    let (scale, rotation, translation) = mat.to_scale_rotation_translation();

    // Sanity check for non-finite values
    if !scale.is_finite() || !rotation.is_finite() || !translation.is_finite() {
        // Fallback to direct translation column
        let pos = Vec3::new(cols[12], cols[13], cols[14]);
        return Ok((pos, Quat::IDENTITY, Vec3::ONE));
    }

    Ok((translation, rotation, scale))
}

fn compute_matrix_from_properties(props: &BTreeMap<String, IrValue>) -> Vec<f32> {
    let pos = match props.get("position") {
        Some(IrValue::Vec3(v)) => Vec3::new(v[0], v[1], v[2]),
        _ => Vec3::ZERO,
    };

    let quat = if let Some(IrValue::Quat(q)) = props.get("quaternion") {
        Quat::from_xyzw(q[0], q[1], q[2], q[3])
    } else if let Some(IrValue::Vec3(r)) = props.get("rotation") {
        Quat::from_euler(EulerRot::XYZ, r[0], r[1], r[2])
    } else {
        Quat::IDENTITY
    };

    let scale = match props.get("scale") {
        Some(IrValue::Vec3(v)) => Vec3::new(v[0], v[1], v[2]),
        _ => Vec3::ONE,
    };

    let mat = Mat4::from_scale_rotation_translation(scale, quat, pos);
    mat.to_cols_array().to_vec()
}

fn extract_size_props(props: &BTreeMap<String, IrValue>) -> (f32, f32, f32) {
    if let Some(IrValue::Vec3(s)) = props.get("size") {
        (s[0], s[1], s[2])
    } else {
        (1.0, 1.0, 1.0)
    }
}

fn extract_color_prop(props: &BTreeMap<String, IrValue>) -> u32 {
    if let Some(IrValue::Color(c)) = props.get("color").or_else(|| props.get("material_color")) {
        let r = (c[0].clamp(0.0, 1.0) * 255.0) as u32;
        let g = (c[1].clamp(0.0, 1.0) * 255.0) as u32;
        let b = (c[2].clamp(0.0, 1.0) * 255.0) as u32;
        (r << 16) | (g << 8) | b
    } else {
        0xffffff
    }
}

fn extract_f64_prop(props: &BTreeMap<String, IrValue>, key: &str, default: f64) -> f64 {
    match props.get(key) {
        Some(IrValue::F64(v)) => *v,
        Some(IrValue::I64(v)) => *v as f64,
        _ => default,
    }
}

fn deterministic_uuid(seed: &str) -> String {
    let mut h1: u64 = 0xcbf29ce484222325;
    let mut h2: u64 = 0x100000001b3;
    for &b in seed.as_bytes() {
        h1 = (h1 ^ (b as u64)).wrapping_mul(0x100000001b3);
        h2 = (h2.wrapping_add(b as u64)).wrapping_mul(0xcbf29ce484222325);
    }
    format!(
        "{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}",
        (h1 >> 32) as u32,
        (h1 >> 16) as u16,
        (h1 & 0x0fff) as u16,
        ((h2 >> 48) as u16 & 0x3fff) | 0x8000,
        h2 & 0xffffffffffff
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matrix_decomposition_and_composition_roundtrip() {
        let translation = Vec3::new(10.5, -3.2, 5.0);
        let rotation = Quat::from_euler(EulerRot::XYZ, 0.2, 0.4, -0.1);
        let scale = Vec3::new(2.0, 2.0, 2.0);

        let mat = Mat4::from_scale_rotation_translation(scale, rotation, translation);
        let cols = mat.to_cols_array();

        let (out_t, out_r, out_s) = decompose_cols_matrix(&cols).unwrap();

        assert!((out_t.x - translation.x).abs() < 1e-4);
        assert!((out_t.y - translation.y).abs() < 1e-4);
        assert!((out_t.z - translation.z).abs() < 1e-4);

        assert!((out_s.x - scale.x).abs() < 1e-4);
        assert!((out_s.y - scale.y).abs() < 1e-4);
        assert!((out_s.z - scale.z).abs() < 1e-4);

        assert!((out_r.dot(rotation)).abs() > 0.999);
    }

    #[test]
    fn bidirectional_threejs_roundtrip() {
        let mut scene = IrScene::new("SkateParkThree");
        scene.feel_overrides.insert("skate_speed".into(), 18.0);

        let root_id = NodeId::new(1, 1).unwrap();
        let child_id = NodeId::new(2, 1).unwrap();

        let mut root_props = BTreeMap::new();
        root_props.insert("position".into(), IrValue::Vec3([0.0, 1.0, 0.0]));
        root_props.insert("size".into(), IrValue::Vec3([2.0, 1.0, 3.0]));
        root_props.insert("color".into(), IrValue::Color([0.8, 0.2, 0.1, 1.0]));

        scene.nodes.insert(
            root_id,
            IrNode {
                id: root_id,
                name: "Ground".into(),
                class_name: "MeshInstance3D".into(),
                parent: None,
                instance_scene: None,
                properties: root_props,
                groups: vec!["environment".into()],
            },
        );

        let mut child_props = BTreeMap::new();
        child_props.insert("position".into(), IrValue::Vec3([0.0, 2.0, 5.0]));
        scene.nodes.insert(
            child_id,
            IrNode {
                id: child_id,
                name: "MainCamera".into(),
                class_name: "Camera3D".into(),
                parent: Some(root_id),
                instance_scene: None,
                properties: child_props,
                groups: Vec::new(),
            },
        );

        scene.connections.push(SignalConnection {
            from_node: root_id,
            signal_name: "interacted".into(),
            to_node: child_id,
            method_name: "focus".into(),
        });

        // Export to Three.js JSON
        let json_str = export_ir_to_threejs_json(&scene);
        assert!(json_str.contains("MeshStandardMaterial"));
        assert!(json_str.contains("BoxGeometry"));
        assert!(json_str.contains("Ground"));
        assert!(json_str.contains("MainCamera"));

        // Import back from Three.js JSON
        let imported = import_threejs_json_to_ir(&json_str).expect("Import must succeed");

        assert_eq!(imported.name, "SkateParkThree");
        assert_eq!(imported.nodes.len(), 2);
        assert_eq!(imported.feel_overrides.get("skate_speed"), Some(&18.0));
        assert_eq!(imported.connections.len(), 1);

        let imp_ground = &imported.nodes[&root_id];
        assert_eq!(imp_ground.name, "Ground");
        assert_eq!(imp_ground.class_name, "MeshInstance3D");
        assert_eq!(imp_ground.groups, vec!["environment"]);

        let imp_cam = &imported.nodes[&child_id];
        assert_eq!(imp_cam.name, "MainCamera");
        assert_eq!(imp_cam.class_name, "Camera3D");
        assert_eq!(imp_cam.parent, Some(root_id));
    }

    #[test]
    fn imports_template_neon_skate_3jse_json() {
        let template_json = include_str!("../../../templates/threejs-3jse/neon_skate_3jse.json");
        let scene = import_threejs_json_to_ir(template_json).expect("Template import succeeds");

        assert_eq!(scene.name, "NeonSkatePark");
        assert_eq!(scene.nodes.len(), 8);
        assert_eq!(scene.connections.len(), 3);
        assert_eq!(scene.feel_overrides["skate_speed"], 18.0);
        assert_eq!(scene.feel_overrides["ollie_pop"], 8.5);

        let skater = scene.nodes.values().find(|n| n.name == "NeonSkater").unwrap();
        assert_eq!(skater.class_name, "CharacterBody3D");
        assert!(skater.groups.contains(&"skater".to_string()));

        let cam = scene.nodes.values().find(|n| n.name == "FollowCamera").unwrap();
        assert_eq!(cam.parent, Some(skater.id));

        // Test roundtrip of the template scene
        let exported_json = export_ir_to_threejs_json(&scene);
        let reimported = import_threejs_json_to_ir(&exported_json).unwrap();
        assert_eq!(reimported.nodes.len(), 8);
        assert_eq!(reimported.connections.len(), 3);
        assert_eq!(reimported.feel_overrides.len(), 5);
    }
}
