//! Live `poly://` (`PLNK`) Mid-Gameplay Mesh & `RapierSpace3D` Collider Hot-Streaming,
//! Runtime Parametric `PolyMesh` Evaluation & Shape Keys (`foreplay-player::polyamour_runtime`).

use bevy_math::Vec3;
use foreplay_ir::NodeId;
use foreplay_physics::{BodyType, RapierSpace3D, RigidBodyConfig3D, ShapeConfig3D};
use foreplay_polyamour::{
    compile_polyamour_game_bundle, CompiledPolyamourGameBundle, LiveLinkError, PolyMesh,
    PolyModifier, PolyamourSemanticAttributes, ShapeKeySet,
};
use std::num::NonZeroU32;

/// Packs a `NodeId` into a 64-bit integer identifier for network live-link messages.
#[must_use]
pub fn pack_node_id(node_id: NodeId) -> u64 {
    (u64::from(node_id.generation.get()) << 32) | u64::from(node_id.index)
}

/// Unpacks a 64-bit network identifier into a `NodeId`.
#[must_use]
pub fn unpack_node_id(packed: u64) -> NodeId {
    let gen_id = ((packed >> 32) as u32).max(1);
    let index = (packed & 0xFFFF_FFFF) as u32;
    NodeId {
        generation: NonZeroU32::new(gen_id).unwrap_or(NonZeroU32::MIN),
        index,
    }
}

/// Errors occurring during runtime Polyamour live-link or parametric evaluation.
#[derive(Debug, thiserror::Error)]
pub enum PolyamourRuntimeError {
    #[error("LiveLink transport error: {0}")]
    LiveLink(#[from] LiveLinkError),

    #[error("Node not found: {0:?}")]
    NodeNotFound(NodeId),

    #[error("Parametric execution error: {0}")]
    Parametric(String),
}

/// Live mid-gameplay state for a Polyamour mesh node.
#[derive(Clone, Debug, PartialEq)]
pub struct LivePolyamourNodeState {
    pub node_id: NodeId,
    pub mesh: PolyMesh,
    pub base_mesh: PolyMesh,
    pub attributes: PolyamourSemanticAttributes,
    pub shape_keys: Option<ShapeKeySet>,
    pub compiled_bundle: CompiledPolyamourGameBundle,
    pub revision: u64,
}

impl LivePolyamourNodeState {
    /// Create a new live state initialized with an initial mesh and attributes.
    #[must_use]
    pub fn new(node_id: NodeId, mesh: PolyMesh) -> Self {
        let base_mesh = mesh.clone();
        let attributes = PolyamourSemanticAttributes::new();
        let compiled_bundle = compile_polyamour_game_bundle(&mesh, &attributes);
        let shape_keys = Some(ShapeKeySet::from_polymesh(&base_mesh));

        Self {
            node_id,
            mesh,
            base_mesh,
            attributes,
            shape_keys,
            compiled_bundle,
            revision: 0,
        }
    }
}

/// Telemetry statistics returned after a parametric modification.
#[derive(Clone, Debug, PartialEq)]
pub struct LivePolyamourStats {
    pub node_id: NodeId,
    pub vertex_count: usize,
    pub face_count: usize,
    pub triangle_count: usize,
    pub grind_spline_count: usize,
    pub has_trimesh_collider: bool,
    pub has_convex_collider: bool,
    pub revision: u64,
}

/// Parses parametric modifier commands from a text script (e.g. `subdivide`, `displace`, `array`, `smooth`).
#[must_use]
pub fn parse_parametric_modifiers(ops_script: &str) -> Vec<PolyModifier> {
    let mut modifiers = Vec::new();

    for raw_line in ops_script.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        match parts[0].to_ascii_lowercase().as_str() {
            "subdivide" | "subdivision" => {
                let levels = parts.get(1).and_then(|s| s.parse::<u8>().ok()).unwrap_or(1);
                let simple = parts
                    .get(2)
                    .map(|&s| s == "true" || s == "1" || s == "simple")
                    .unwrap_or(false);
                modifiers.push(PolyModifier::Subdivision { levels, simple });
            }
            "displace" => {
                let strength = parts.get(1).and_then(|s| s.parse::<f32>().ok()).unwrap_or(0.1);
                let frequency = parts.get(2).and_then(|s| s.parse::<f32>().ok()).unwrap_or(1.0);
                modifiers.push(PolyModifier::Displace { strength, frequency });
            }
            "array" => {
                let count = parts.get(1).and_then(|s| s.parse::<u32>().ok()).unwrap_or(2);
                let dx = parts.get(2).and_then(|s| s.parse::<f32>().ok()).unwrap_or(1.0);
                let dy = parts.get(3).and_then(|s| s.parse::<f32>().ok()).unwrap_or(0.0);
                let dz = parts.get(4).and_then(|s| s.parse::<f32>().ok()).unwrap_or(0.0);
                modifiers.push(PolyModifier::Array {
                    count,
                    relative_offset: Vec3::new(dx, dy, dz),
                    constant_offset: Vec3::ZERO,
                });
            }
            "bevel" => {
                let width = parts.get(1).and_then(|s| s.parse::<f32>().ok()).unwrap_or(0.1);
                modifiers.push(PolyModifier::Bevel { width });
            }
            "solidify" => {
                let thickness = parts.get(1).and_then(|s| s.parse::<f32>().ok()).unwrap_or(0.1);
                modifiers.push(PolyModifier::Solidify { thickness });
            }
            "smooth" => {
                let factor = parts.get(1).and_then(|s| s.parse::<f32>().ok()).unwrap_or(0.5);
                let iterations = parts.get(2).and_then(|s| s.parse::<u32>().ok()).unwrap_or(1);
                modifiers.push(PolyModifier::Smooth { factor, iterations });
            }
            "triangulate" => {
                modifiers.push(PolyModifier::Triangulate);
            }
            "weld" => {
                let distance = parts.get(1).and_then(|s| s.parse::<f32>().ok()).unwrap_or(0.001);
                modifiers.push(PolyModifier::Weld { distance });
            }
            "mirror" => {
                let axis_x = parts.get(1).map(|&s| s.contains('x') || s.contains('X') || s == "1").unwrap_or(true);
                let axis_y = parts.get(2).map(|&s| s.contains('y') || s.contains('Y') || s == "1").unwrap_or(false);
                let axis_z = parts.get(3).map(|&s| s.contains('z') || s.contains('Z') || s == "1").unwrap_or(false);
                modifiers.push(PolyModifier::Mirror {
                    axis_x,
                    axis_y,
                    axis_z,
                    merge_distance: 0.001,
                });
            }
            _ => {}
        }
    }

    modifiers
}

/// Hot-updates the `RapierSpace3D` collider for `node_id` in-place using geometry from `CompiledPolyamourGameBundle`.
///
/// Crucially, this preserves the parent rigid body's position, linear velocity, angular velocity,
/// and sleep state, ensuring zero momentum loss during mid-gameplay live editing.
pub fn hot_rebuild_node_collider(
    physics_space: &mut RapierSpace3D,
    node_id: NodeId,
    state: &LivePolyamourNodeState,
) {
    // 1. Determine collider shape configuration from compiled game bundle
    let shape_config = if !state.compiled_bundle.trimesh_collider.0.is_empty()
        && !state.compiled_bundle.trimesh_collider.1.is_empty()
    {
        ShapeConfig3D::TriMesh {
            vertices: state.compiled_bundle.trimesh_collider.0.clone(),
            indices: state.compiled_bundle.trimesh_collider.1.clone(),
        }
    } else if !state.compiled_bundle.convex_hull_points.is_empty() {
        ShapeConfig3D::ConvexHull {
            points: state.compiled_bundle.convex_hull_points.clone(),
        }
    } else {
        // Fallback: triangulate full PolyMesh
        let tri = state.mesh.triangulate();
        let indices: Vec<[u32; 3]> = tri
            .indices
            .chunks_exact(3)
            .map(|chunk| [chunk[0], chunk[1], chunk[2]])
            .collect();
        ShapeConfig3D::TriMesh {
            vertices: tri.positions,
            indices,
        }
    };

    // 2. Ensure rigid body exists in physics space (defaulting to Fixed if not already present)
    if !physics_space.node_to_body.contains_key(&node_id) {
        let cfg = RigidBodyConfig3D {
            body_type: BodyType::Fixed,
            position: Vec3::ZERO,
            mass: 0.0,
            ..Default::default()
        };
        physics_space.add_rigid_body(node_id, &cfg);
    }

    // 3. Remove old collider while leaving rigid body and its dynamic velocities untouched
    if let Some(old_handle) = physics_space.node_to_collider.remove(&node_id) {
        physics_space.colliders.remove(
            old_handle,
            &mut physics_space.island_manager,
            &mut physics_space.bodies,
            &mut physics_space.soft_bodies,
            true,
        );
        physics_space.collider_to_node.remove(&old_handle);
    }

    // 4. Attach new collider with updated geometry
    physics_space.add_collider(node_id, shape_config, 0.5, 0.5, 1, 1);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{create_ir_node, PlayerRuntime};
    use foreplay_ir::{IrScene, IrValue};
    use foreplay_polyamour::PolyLinkMessage;
    use std::collections::BTreeMap;

    #[test]
    fn test_pack_unpack_node_id_roundtrip() {
        let cases = [
            (1, 0),
            (1, 42),
            (2, 999),
            (100, 1024),
            (u32::MAX, u32::MAX),
        ];

        for (gen_val, idx) in cases {
            let node_id = NodeId {
                generation: NonZeroU32::new(gen_val).unwrap(),
                index: idx,
            };
            let packed = pack_node_id(node_id);
            let unpacked = unpack_node_id(packed);
            assert_eq!(node_id, unpacked);
            assert_eq!(unpacked.generation.get(), gen_val);
            assert_eq!(unpacked.index, idx);
        }
    }

    #[test]
    fn test_parse_parametric_modifiers_commands() {
        let script = r#"
            # Comment line
            // Another comment
            subdivide 2 simple
            displace 0.25 2.0
            array 3 1.0 2.0 3.0
            bevel 0.05
            solidify 0.15
            smooth 0.8 4
            triangulate
            weld 0.005
            mirror 1 0 1
        "#;
        let mods = parse_parametric_modifiers(script);
        assert_eq!(mods.len(), 9);
        assert!(matches!(mods[0], PolyModifier::Subdivision { levels: 2, simple: true }));
        assert!(matches!(mods[1], PolyModifier::Displace { strength, frequency } if (strength - 0.25).abs() < 1e-4 && (frequency - 2.0).abs() < 1e-4));
        assert!(matches!(mods[2], PolyModifier::Array { count: 3, .. }));
        assert!(matches!(mods[3], PolyModifier::Bevel { width } if (width - 0.05).abs() < 1e-4));
        assert!(matches!(mods[4], PolyModifier::Solidify { thickness } if (thickness - 0.15).abs() < 1e-4));
        assert!(matches!(mods[5], PolyModifier::Smooth { factor, iterations: 4 } if (factor - 0.8).abs() < 1e-4));
        assert!(matches!(mods[6], PolyModifier::Triangulate));
        assert!(matches!(mods[7], PolyModifier::Weld { distance } if (distance - 0.005).abs() < 1e-4));
        assert!(matches!(mods[8], PolyModifier::Mirror { axis_x: true, axis_y: false, axis_z: true, .. }));
    }

    #[test]
    fn test_live_polyamour_node_state_initialization() {
        let node_id = NodeId {
            generation: NonZeroU32::new(1).unwrap(),
            index: 10,
        };
        let mut mesh = PolyMesh::new();
        mesh.add_vertex(Vec3::new(0.0, 0.0, 0.0));
        mesh.add_vertex(Vec3::new(1.0, 0.0, 0.0));
        mesh.add_vertex(Vec3::new(1.0, 1.0, 0.0));
        mesh.add_vertex(Vec3::new(0.0, 1.0, 0.0));
        mesh.add_face(vec![0, 1, 2, 3], vec![[0.0, 0.0]; 4], false, 0);

        let state = LivePolyamourNodeState::new(node_id, mesh);
        assert_eq!(state.node_id, node_id);
        assert_eq!(state.revision, 0);
        assert_eq!(state.mesh.positions.len(), 4);
        assert_eq!(state.base_mesh.positions.len(), 4);
        assert!(state.shape_keys.is_some());
        assert_eq!(state.shape_keys.as_ref().unwrap().basis.len(), 4);
    }

    #[test]
    fn test_runtime_bind_polyamour_live_link_and_poll() {
        let mut scene = IrScene::new("LinkTest");
        let node_id = NodeId {
            generation: NonZeroU32::new(1).unwrap(),
            index: 1,
        };
        let node = create_ir_node(node_id, "PolyObject", "StaticBody3D");
        scene.nodes.insert(node_id, node);

        let mut runtime = PlayerRuntime::new(scene);
        let addr = runtime.bind_polyamour_live_link("127.0.0.1:0").expect("binds");
        let mut client = foreplay_polyamour::PolyamourLiveLinkClient::connect_timeout(
            addr,
            std::time::Duration::from_secs(2),
        )
        .expect("client connects");

        let mut mesh = PolyMesh::new();
        mesh.add_vertex(Vec3::ZERO);
        mesh.add_vertex(Vec3::X);
        mesh.add_vertex(Vec3::Y);
        mesh.add_face(vec![0, 1, 2], vec![[0.0, 0.0]; 3], false, 0);

        let msg = PolyLinkMessage::OpenMeshInPolyamour {
            node_id: pack_node_id(node_id),
            node_name: "PolyObject".into(),
            mesh: foreplay_polyamour::PolyMeshSnapshot::from_mesh(&mesh),
            sub_selection: foreplay_polyamour::SubObjectSelection::default(),
        };

        client.send_message(&msg).expect("send open mesh");
        let polled = runtime.poll_polyamour_live_link_timeout(std::time::Duration::from_millis(500));
        assert!(polled > 0);

        let start_read = std::time::Instant::now();
        let mut msgs = Vec::new();
        while start_read.elapsed() < std::time::Duration::from_millis(500) {
            if let Ok(m) = client.try_recv_messages()
                && !m.is_empty()
            {
                msgs = m;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert!(msgs.iter().any(|m| matches!(m, PolyLinkMessage::Ack { revision: 0 })));

        assert!(runtime.live_polyamour_meshes().contains_key(&node_id));
        assert_eq!(runtime.live_polyamour_meshes()[&node_id].mesh.positions.len(), 3);
    }

    #[test]
    fn test_mesh_delta_sync_hot_rebuilds_trimesh_collider() {
        let mut scene = IrScene::new("HotRebuildTest");
        let ground_id = NodeId {
            generation: NonZeroU32::new(1).unwrap(),
            index: 2,
        };
        let ground = create_ir_node(ground_id, "Ground", "StaticBody3D");
        scene.nodes.insert(ground_id, ground);

        let mut runtime = PlayerRuntime::new(scene);
        let addr = runtime.bind_polyamour_live_link("127.0.0.1:0").unwrap();
        let mut client = foreplay_polyamour::PolyamourLiveLinkClient::connect_timeout(
            addr,
            std::time::Duration::from_secs(2),
        )
        .unwrap();

        let delta = PolyLinkMessage::MeshDeltaSync {
            node_id: pack_node_id(ground_id),
            revision: 5,
            updated_vertices: vec![
                (0, [0.0, 0.0, 0.0]),
                (1, [2.0, 0.0, 0.0]),
                (2, [2.0, 0.0, 2.0]),
            ],
            replaced_faces: Some(vec![vec![0, 1, 2]]),
            edge_tags: vec![],
            face_tags: vec![(0, "collision_trimesh".into())],
        };

        client.send_message(&delta).unwrap();
        runtime.poll_polyamour_live_link_timeout(std::time::Duration::from_millis(500));

        let handle = runtime.physics_space.node_to_collider[&ground_id];
        let collider = &runtime.physics_space.colliders[handle];
        assert!(collider.shape().as_trimesh().is_some());
    }

    #[test]
    fn test_mesh_delta_sync_preserves_dynamic_body_momentum() {
        let mut scene = IrScene::new("MomentumPreserve");
        let actor_id = NodeId {
            generation: NonZeroU32::new(1).unwrap(),
            index: 1,
        };
        let mut actor = create_ir_node(actor_id, "Actor", "RigidBody3D");
        actor.properties.insert("position".into(), IrValue::Vec3([0.0, 10.0, 0.0]));
        scene.nodes.insert(actor_id, actor);

        let static_id = NodeId {
            generation: NonZeroU32::new(1).unwrap(),
            index: 2,
        };
        let static_node = create_ir_node(static_id, "StaticMesh", "StaticBody3D");
        scene.nodes.insert(static_id, static_node);

        let mut runtime = PlayerRuntime::new(scene);
        let target_vel = Vec3::new(15.0, -2.5, 8.0);
        let target_ang = Vec3::new(0.5, 1.0, -0.5);
        runtime.physics_space.set_body_velocity(actor_id, target_vel);
        runtime.physics_space.set_body_angular_velocity(actor_id, target_ang);

        let addr = runtime.bind_polyamour_live_link("127.0.0.1:0").unwrap();
        let mut client = foreplay_polyamour::PolyamourLiveLinkClient::connect_timeout(
            addr,
            std::time::Duration::from_secs(2),
        )
        .unwrap();

        // Stream multiple rapid mesh deltas to static node
        for rev in 1..=3 {
            let delta = PolyLinkMessage::MeshDeltaSync {
                node_id: pack_node_id(static_id),
                revision: rev,
                updated_vertices: vec![
                    (0, [0.0, rev as f32, 0.0]),
                    (1, [1.0, rev as f32, 0.0]),
                    (2, [0.0, rev as f32, 1.0]),
                ],
                replaced_faces: Some(vec![vec![0, 1, 2]]),
                edge_tags: vec![],
                face_tags: vec![(0, "collision_trimesh".into())],
            };
            client.send_message(&delta).unwrap();
            runtime.poll_polyamour_live_link_timeout(std::time::Duration::from_millis(100));
        }

        // Verify dynamic body momentum was 100% unchanged
        let current_vel = runtime.physics_space.get_body_velocity(actor_id).unwrap();
        let current_ang = runtime.physics_space.get_body_angular_velocity(actor_id).unwrap();
        assert_eq!(current_vel, target_vel);
        assert_eq!(current_ang, target_ang);
    }

    #[test]
    fn test_update_parametric_polyamour_node_modifiers() {
        let mut scene = IrScene::new("ParametricModScene");
        let node_id = NodeId {
            generation: NonZeroU32::new(1).unwrap(),
            index: 5,
        };
        let node = create_ir_node(node_id, "PolyMeshNode", "StaticBody3D");
        scene.nodes.insert(node_id, node);

        let mut runtime = PlayerRuntime::new(scene);

        let mut mesh = PolyMesh::new();
        mesh.add_vertex(Vec3::new(0.0, 0.0, 0.0));
        mesh.add_vertex(Vec3::new(1.0, 0.0, 0.0));
        mesh.add_vertex(Vec3::new(1.0, 1.0, 0.0));
        mesh.add_vertex(Vec3::new(0.0, 1.0, 0.0));
        mesh.add_face(vec![0, 1, 2, 3], vec![[0.0, 0.0]; 4], false, 0);

        let state = LivePolyamourNodeState::new(node_id, mesh);
        runtime.live_polyamour_meshes.insert(node_id, state);

        let script = "subdivide 1 simple\narray 2 2.0 0.0 0.0";
        let stats = runtime
            .update_parametric_polyamour_node(node_id, Some(script), &BTreeMap::new())
            .expect("parametric update succeeds");

        assert!(stats.vertex_count > 4);
        assert!(stats.face_count > 1);
        assert_eq!(stats.revision, 1);
    }

    #[test]
    fn test_update_parametric_polyamour_node_morph_weights() {
        let mut scene = IrScene::new("MorphScene");
        let node_id = NodeId {
            generation: NonZeroU32::new(1).unwrap(),
            index: 6,
        };
        let node = create_ir_node(node_id, "MorphedNode", "StaticBody3D");
        scene.nodes.insert(node_id, node);

        let mut runtime = PlayerRuntime::new(scene);

        let mut mesh = PolyMesh::new();
        mesh.add_vertex(Vec3::new(0.0, 0.0, 0.0));
        mesh.add_vertex(Vec3::new(1.0, 0.0, 0.0));
        mesh.add_vertex(Vec3::new(0.0, 1.0, 0.0));
        mesh.add_face(vec![0, 1, 2], vec![[0.0, 0.0]; 3], false, 0);

        let mut state = LivePolyamourNodeState::new(node_id, mesh);
        if let Some(ref mut sk) = state.shape_keys {
            sk.sculpt_shape_key_delta("Key1", &[Vec3::new(0.0, 5.0, 0.0), Vec3::ZERO, Vec3::ZERO]);
        }
        runtime.live_polyamour_meshes.insert(node_id, state);

        let mut weights = BTreeMap::new();
        weights.insert("Key1".into(), 1.0);

        let stats = runtime
            .update_parametric_polyamour_node(node_id, None, &weights)
            .expect("morph update succeeds");

        assert_eq!(stats.node_id, node_id);
        let updated_state = &runtime.live_polyamour_meshes[&node_id];
        assert_eq!(updated_state.mesh.positions[0].y, 5.0);
    }

    #[test]
    fn test_grind_rail_spline_extraction_on_live_sync() {
        let mut scene = IrScene::new("GrindScene");
        let node_id = NodeId {
            generation: NonZeroU32::new(1).unwrap(),
            index: 8,
        };
        let node = create_ir_node(node_id, "GrindRail", "StaticBody3D");
        scene.nodes.insert(node_id, node);

        let mut runtime = PlayerRuntime::new(scene);
        let addr = runtime.bind_polyamour_live_link("127.0.0.1:0").unwrap();
        let mut client = foreplay_polyamour::PolyamourLiveLinkClient::connect_timeout(
            addr,
            std::time::Duration::from_secs(2),
        )
        .unwrap();

        let delta = PolyLinkMessage::MeshDeltaSync {
            node_id: pack_node_id(node_id),
            revision: 99,
            updated_vertices: vec![
                (0, [0.0, 1.0, 0.0]),
                (1, [10.0, 1.0, 0.0]),
                (2, [10.0, 0.0, 0.0]),
                (3, [0.0, 0.0, 0.0]),
            ],
            replaced_faces: Some(vec![vec![0, 1, 2, 3]]),
            edge_tags: vec![([0, 1], "grind_edge".into())],
            face_tags: vec![],
        };

        client.send_message(&delta).unwrap();
        runtime.poll_polyamour_live_link_timeout(std::time::Duration::from_millis(500));

        let state = &runtime.live_polyamour_meshes[&node_id];
        assert_eq!(state.compiled_bundle.grind_rail_splines.len(), 1);
        let spline = &state.compiled_bundle.grind_rail_splines[0];
        assert_eq!(spline.len(), 2);
        assert_eq!(spline[0], Vec3::new(0.0, 1.0, 0.0));
        assert_eq!(spline[1], Vec3::new(10.0, 1.0, 0.0));
    }
}
