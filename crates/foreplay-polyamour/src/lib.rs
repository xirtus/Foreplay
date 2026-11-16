//! `foreplay-polyamour` — Native Blender / SexOS Polyamour 3D modeling, HalfEdgeMesh,
//! modifier stack, `.blend`/glTF/OBJ/STL/PLY I/O, `polyops 1` scripting, and SAH BVH AgX path tracer.

pub mod io;
pub mod live_link;
pub mod mesh;
pub mod modifiers;
pub mod ops;
pub mod pathtracer;
pub mod polyops;
pub mod primitives;
pub mod sculpt_rig;
pub mod selection_raycast;
pub mod semantic_tags;

pub use io::*;
pub use live_link::*;
pub use mesh::*;
pub use modifiers::*;
pub use ops::*;
pub use pathtracer::*;
pub use polyops::*;
pub use primitives::*;
pub use sculpt_rig::*;
pub use selection_raycast::*;
pub use semantic_tags::*;

/// Version of the `polyops` text command protocol.
pub const POLYOPS_VERSION: u32 = 1;

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_math::Vec3;

    #[test]
    fn primitives_generate_valid_polymesh_and_triangulate() {
        // Test all 8 canonical primitives + 2 level design primitives
        let cube = primitive_cube(2.0);
        assert_eq!(cube.positions.len(), 8);
        assert_eq!(cube.faces.len(), 6);
        let cube_tri = cube.triangulate();
        assert_eq!(cube_tri.indices.len(), 36); // 6 faces * 2 tris * 3 indices

        let plane = primitive_plane(4.0);
        assert_eq!(plane.positions.len(), 4);
        assert_eq!(plane.faces.len(), 1);

        let grid = primitive_grid(10.0, 10.0, 4, 4);
        assert_eq!(grid.faces.len(), 16);

        let uv_sphere = primitive_uv_sphere(1.0, 8, 4);
        assert!(!uv_sphere.faces.is_empty());

        let ico_sphere = primitive_ico_sphere(1.0, 1);
        assert_eq!(ico_sphere.faces.len(), 80); // 20 * 4

        let cylinder = primitive_cylinder(1.0, 2.0, 8);
        assert_eq!(cylinder.faces.len(), 10); // 2 caps + 8 sides

        let cone = primitive_cone(1.0, 2.0, 8);
        assert_eq!(cone.faces.len(), 9); // 1 cap + 8 sides

        let torus = primitive_torus(2.0, 0.5, 8, 6);
        assert_eq!(torus.faces.len(), 48); // 8 * 6

        let qpipe = primitive_quarterpipe(2.0, 4.0, 6);
        assert!(!qpipe.faces.is_empty());
        let qpipe_tri = qpipe.triangulate();
        assert!(!qpipe_tri.indices.is_empty());

        let stairs = primitive_stair_set(3.0, 2.0, 3.0, 4);
        assert!(!stairs.faces.is_empty());
        let stairs_tri = stairs.triangulate();
        assert!(!stairs_tri.indices.is_empty());
    }

    #[test]
    fn half_edge_mesh_topology_and_edge_loop_traversal() {
        let cube = primitive_cube(1.0);
        let he = HalfEdgeMesh::from_polymesh(&cube);

        assert!(he.is_manifold(), "cube must be a 2-manifold surface");
        assert!(he.boundary_edges().is_empty(), "closed cube has no boundary edges");

        // Vertex 0 neighbors in cube (connected to 3 other vertices)
        let neighbors = he.vertex_neighbors(0);
        assert_eq!(neighbors.len(), 3);

        // Quad edge loop
        let loop_edges = he.edge_loop([4, 5]);
        assert_eq!(loop_edges.len(), 4, "cube quad edge loop must contain 4 edges");

        // Face areas
        let areas = he.face_areas();
        assert_eq!(areas.len(), 6);
        for &a in areas {
            assert!((a - 1.0).abs() < 1e-4);
        }
    }

    #[test]
    fn edit_operations_extrude_inset_bisect_and_weld() {
        let mut cube = primitive_cube(2.0);

        // 1. Extrude top face (+Y)
        extrude_faces(&mut cube, &[2], 1.5);
        assert_eq!(cube.faces.len(), 10); // 6 original + 4 side walls

        // 2. Inset front face
        inset_faces(&mut cube, &[0], 0.2, 0.1);
        assert_eq!(cube.faces.len(), 14); // + 4 inset perimeter quads

        // 3. Knife bisect along plane X = 0
        knife_bisect(&mut cube, Vec3::ZERO, Vec3::X);
        assert!(cube.faces.len() > 14);

        // 4. Weld duplicate vertices
        let orig_vert_count = cube.positions.len();
        cube.weld_vertices(0.001);
        assert!(cube.positions.len() <= orig_vert_count);
    }

    #[test]
    fn boolean_csg_union_difference_intersect() {
        let a = primitive_cube(2.0);
        let mut b = primitive_cube(2.0);
        for p in &mut b.positions {
            *p += Vec3::new(1.0, 0.0, 0.0);
        }

        let union_mesh = boolean_csg(&a, &b, BooleanOp::Union);
        assert!(!union_mesh.faces.is_empty());

        let diff_mesh = boolean_csg(&a, &b, BooleanOp::Difference);
        assert!(!diff_mesh.faces.is_empty());

        let isect_mesh = boolean_csg(&a, &b, BooleanOp::Intersect);
        assert!(!isect_mesh.faces.is_empty());
    }

    #[test]
    fn modifier_stack_catmull_clark_mirror_array_solidify() {
        let base = primitive_cube(2.0);

        let stack = vec![
            PolyModifier::Subdivision {
                levels: 1,
                simple: false, // Catmull-Clark
            },
            PolyModifier::Mirror {
                axis_x: true,
                axis_y: false,
                axis_z: false,
                merge_distance: 0.01,
            },
            PolyModifier::Array {
                count: 3,
                relative_offset: Vec3::new(1.2, 0.0, 0.0),
                constant_offset: Vec3::ZERO,
            },
            PolyModifier::Solidify { thickness: 0.1 },
            PolyModifier::Triangulate,
        ];

        let result = evaluate_modifier_stack(&base, &stack);
        assert!(!result.positions.is_empty());
        assert!(!result.faces.is_empty());
        // After Triangulate, every face must be a triangle
        for face in &result.faces {
            assert_eq!(face.verts.len(), 3);
        }
    }

    #[test]
    fn io_roundtrip_obj_stl_ply_gltf() {
        let original = primitive_cube(1.0);

        // 1. OBJ
        let obj_str = export_obj(&original);
        assert!(obj_str.contains("v -0.5"));
        let obj_imported = import_obj(&obj_str).expect("import obj");
        assert_eq!(obj_imported.positions.len(), 8);
        assert_eq!(obj_imported.faces.len(), 6);

        // 2. STL ASCII & Binary
        let stl_ascii = export_stl_ascii(&original, "CubeSolid");
        assert!(stl_ascii.contains("solid CubeSolid"));
        let stl_imported = import_stl(stl_ascii.as_bytes()).expect("import stl ascii");
        assert_eq!(stl_imported.positions.len(), 8); // welded

        // 3. PLY
        let ply_str = export_ply(&original);
        assert!(ply_str.contains("element vertex 8"));
        let ply_imported = import_ply(&ply_str).expect("import ply");
        assert_eq!(ply_imported.positions.len(), 8);
        assert_eq!(ply_imported.faces.len(), 6);

        // 4. glTF JSON
        let gltf_json = export_gltf_json(&original);
        assert!(gltf_json.contains("data:application/octet-stream;base64,"));
        let gltf_imported = import_gltf_json(&gltf_json).expect("import gltf json");
        assert_eq!(gltf_imported.positions.len(), 8);
    }

    #[test]
    fn blend_file_cleanroom_header_and_dna_parser() {
        // Construct mock 64-bit little-endian Blender 4.0 header with a ME (Mesh) block and ENDB
        let mut mock_blend = Vec::new();
        // Magic + 64-bit pointer '-' + little-endian 'v' + version '400'
        mock_blend.extend_from_slice(b"BLENDER-v400");

        // Block 1: ME (Mesh block), size = 32 bytes, addr = 0x1000, sdna = 5, count = 1
        mock_blend.extend_from_slice(b"ME\0\0");
        mock_blend.extend_from_slice(&32u32.to_le_bytes());
        mock_blend.extend_from_slice(&0x1000u64.to_le_bytes());
        mock_blend.extend_from_slice(&5u32.to_le_bytes());
        mock_blend.extend_from_slice(&1u32.to_le_bytes());
        mock_blend.extend_from_slice(&[0u8; 32]); // payload

        // Block 2: DNA1, size = 16 bytes
        mock_blend.extend_from_slice(b"DNA1");
        mock_blend.extend_from_slice(&16u32.to_le_bytes());
        mock_blend.extend_from_slice(&0x2000u64.to_le_bytes());
        mock_blend.extend_from_slice(&0u32.to_le_bytes());
        mock_blend.extend_from_slice(&1u32.to_le_bytes());
        mock_blend.extend_from_slice(&[0u8; 16]); // payload

        // Block 3: ENDB
        mock_blend.extend_from_slice(b"ENDB");
        mock_blend.extend_from_slice(&0u32.to_le_bytes());
        mock_blend.extend_from_slice(&0u64.to_le_bytes());
        mock_blend.extend_from_slice(&0u32.to_le_bytes());
        mock_blend.extend_from_slice(&0u32.to_le_bytes());

        let summary = BlendFileSummary::parse_bytes(&mock_blend).expect("parse mock blend");
        assert_eq!(summary.header_magic, "BLENDER");
        assert_eq!(summary.version, "400");
        assert_eq!(summary.pointer_size, 8);
        assert!(summary.is_little_endian);
        assert_eq!(summary.mesh_block_count, 1);
        assert!(summary.has_dna);
        assert_eq!(summary.block_count, 3);

        // Hostile truncated input must return error gracefully without panic
        let truncated = &mock_blend[0..10];
        assert!(matches!(
            BlendFileSummary::parse_bytes(truncated),
            Err(BlendParseError::HeaderTooShort(_))
        ));
    }

    #[test]
    fn polyops_1_atomic_script_and_rollback_on_abort() {
        let mut scene = PolyScene::new();

        let script = r#"
            polyops 1
            begin
            material_new "WoodPlank" 0.6 0.4 0.2 1.0 0.1 0.7 as $wood
            add_primitive cube 2.0 as $box
            set $box position 0.0 1.0 0.0
            modifier_add $box subdivision 1
            material_assign $box $wood
            end
        "#;

        let report = run_polyops(&mut scene, script).expect("run polyops");
        assert_eq!(scene.objects.len(), 1);
        assert_eq!(scene.materials.len(), 1);
        assert!(scene.handles.contains_key("$box"));
        assert!(scene.handles.contains_key("$wood"));
        assert_eq!(report.created_ids.len(), 2);

        // Roundtrip serialization
        let emitted = scene_to_polyops(&scene);
        assert!(emitted.starts_with("polyops 1"));
        assert!(emitted.contains("material_new"));
        assert!(emitted.contains("add_mesh"));

        // Test atomic rollback on abort
        let abort_script = r#"
            polyops 1
            begin
            add_primitive cylinder 1.0 2.0 16 as $bad_cyl
            abort "simulated network cancellation"
            end
        "#;

        let res = run_polyops(&mut scene, abort_script);
        assert!(matches!(res, Err(PolyopsError::Aborted(_))));
        // State must be completely untouched by aborted script
        assert_eq!(scene.objects.len(), 1);
        assert!(!scene.handles.contains_key("$bad_cyl"));
    }

    #[test]
    fn pathtracer_bakes_deterministic_rgba8_with_agx_and_sah_bvh() {
        let cube = primitive_cube(2.0);
        let tri_mesh = cube.triangulate();

        let config = PathTracerConfig {
            width: 16,
            height: 16,
            samples_per_pixel: 2,
            max_bounces: 2,
            seed: 0x42,
            enable_denoiser: true,
            view_transform: ViewTransform::AgX,
            camera: PathTracerCamera {
                eye: Vec3::new(0.0, 1.5, 4.0),
                target: Vec3::ZERO,
                up: Vec3::Y,
                fov_degrees: 60.0,
            },
            default_material: PathTracerMaterial {
                base_color: Vec3::new(0.9, 0.4, 0.2),
                metallic: 0.2,
                roughness: 0.3,
            },
        };

        // Render pass 1
        let pixels1 = bake_pathtraced_rgba8(&tri_mesh, &config);
        assert_eq!(pixels1.len(), 16 * 16 * 4);

        // Render pass 2 (must be 100% bit-exact identical)
        let pixels2 = bake_pathtraced_rgba8(&tri_mesh, &config);
        assert_eq!(pixels1, pixels2, "Deterministic path tracer must produce bit-exact output");

        // Verify alpha channel is fully opaque
        for i in 0..(16 * 16) {
            assert_eq!(pixels1[i * 4 + 3], 255);
        }
    }

    #[test]
    fn live_link_wire_codec_all_messages_roundtrip() {
        let messages = vec![
            PolyLinkMessage::Hello {
                app_name: "ForeplayEngine".into(),
                protocol_version: 1,
            },
            PolyLinkMessage::OpenMeshInPolyamour {
                node_id: 101,
                node_name: "HeroModel".into(),
                mesh: PolyMeshSnapshot::from_mesh(&primitive_cube(1.0)),
                sub_selection: SubObjectSelection::new(),
            },
            PolyLinkMessage::MeshDeltaSync {
                node_id: 101,
                revision: 42,
                updated_vertices: vec![(0, [1.0, 2.0, 3.0]), (1, [4.0, 5.0, 6.0])],
                replaced_faces: Some(vec![vec![0, 1, 2], vec![0, 2, 3]]),
                edge_tags: vec![([0, 1], "grind_edge".into())],
                face_tags: vec![(0, "collision_convex".into())],
            },
            PolyLinkMessage::SelectionSync {
                node_id: 101,
                selection: {
                    let mut s = SubObjectSelection::new();
                    s.select_vertex(0);
                    s.select_edge(1, 2);
                    s.select_face(3);
                    s
                },
            },
            PolyLinkMessage::UndoMarker {
                label: "Extrude Face".into(),
                revision: 43,
            },
            PolyLinkMessage::Ack { revision: 43 },
        ];

        for original in messages {
            let frame = encode_frame(&original).expect("encode frame");
            assert!(frame.len() >= HEADER_SIZE);
            assert_eq!(&frame[0..4], POLY_LINK_MAGIC);

            let (decoded, consumed) = decode_frame(&frame)
                .expect("decode frame")
                .expect("complete frame");
            assert_eq!(consumed, frame.len());
            assert_eq!(decoded, original);
        }
    }

    #[test]
    fn live_link_wire_codec_corrupt_magic_and_checksum() {
        let msg = PolyLinkMessage::Hello {
            app_name: "TestApp".into(),
            protocol_version: 1,
        };
        let mut frame = encode_frame(&msg).expect("encode frame");

        // 1. Corrupt magic
        frame[0] = b'X';
        assert!(matches!(
            decode_frame(&frame),
            Err(LiveLinkError::InvalidMagic(_))
        ));

        // Restore magic and corrupt checksum/payload
        frame[0] = b'P';
        let last = frame.len() - 1;
        frame[last] ^= 0xFF;
        assert!(matches!(
            decode_frame(&frame),
            Err(LiveLinkError::ChecksumMismatch { .. })
        ));
    }

    #[test]
    fn live_link_wire_codec_max_frame_cap_protection() {
        let mut hostile_header = Vec::new();
        hostile_header.extend_from_slice(POLY_LINK_MAGIC);
        hostile_header.extend_from_slice(&1u32.to_le_bytes());
        // Claim 128 MiB payload (> 64 MiB limit)
        let hostile_len = (128 * 1024 * 1024u32).to_le_bytes();
        hostile_header.extend_from_slice(&hostile_len);
        hostile_header.extend_from_slice(&0u64.to_le_bytes());

        assert!(matches!(
            decode_frame(&hostile_header),
            Err(LiveLinkError::FrameTooLarge(_))
        ));
    }

    #[test]
    fn live_link_wire_codec_chunked_streaming_decode() {
        let msg = PolyLinkMessage::Ack { revision: 999 };
        let frame = encode_frame(&msg).expect("encode");

        let mut buffer = Vec::new();
        // Feed in 3-byte chunks
        for chunk in frame.chunks(3) {
            buffer.extend_from_slice(chunk);
            if buffer.len() < frame.len() {
                assert!(decode_frame(&buffer).unwrap().is_none());
            }
        }

        let res = decode_frame(&buffer).unwrap();
        assert!(res.is_some());
        let (decoded, consumed) = res.unwrap();
        assert_eq!(decoded, msg);
        assert_eq!(consumed, frame.len());
    }

    #[test]
    fn live_link_loopback_tcp_handshake_and_broadcast() {
        use std::time::Duration;

        let mut server = PolyamourLiveLinkServer::bind("127.0.0.1:0").expect("server bind");
        let addr = server.local_addr().expect("local addr");

        let mut client = PolyamourLiveLinkClient::connect_timeout(addr, Duration::from_millis(500))
            .expect("client connect");

        // Client sends Hello
        let hello = PolyLinkMessage::Hello {
            app_name: "PolyamourEditor".into(),
            protocol_version: 1,
        };
        client.send_message(&hello).expect("client send");

        std::thread::sleep(Duration::from_millis(50));
        let server_msgs = server.poll_messages();
        assert_eq!(server_msgs.len(), 1);
        assert_eq!(server_msgs[0], hello);

        // Server broadcasts Ack
        let ack = PolyLinkMessage::Ack { revision: 100 };
        let sent = server.broadcast(&ack).expect("broadcast");
        assert_eq!(sent, 1);

        std::thread::sleep(Duration::from_millis(50));
        let client_msgs = client.try_recv_messages().expect("client recv");
        assert_eq!(client_msgs.len(), 1);
        assert_eq!(client_msgs[0], ack);
    }

    #[test]
    fn live_link_apply_mesh_delta_inplace_streaming() {
        let mut cube = primitive_cube(2.0);
        let orig_pos_0 = cube.positions[0];

        let delta = PolyLinkMessage::MeshDeltaSync {
            node_id: 1,
            revision: 5,
            updated_vertices: vec![(0, [15.0, 20.0, 25.0])],
            replaced_faces: None,
            edge_tags: vec![],
            face_tags: vec![],
        };

        assert!(apply_mesh_delta(&mut cube, &delta));
        assert_ne!(cube.positions[0], orig_pos_0);
        assert_eq!(cube.positions[0], Vec3::new(15.0, 20.0, 25.0));
    }

    #[test]
    fn live_link_polymesh_snapshot_roundtrip() {
        let cube = primitive_cube(2.0);
        let snapshot = PolyMeshSnapshot::from_mesh(&cube);
        let restored = snapshot.to_mesh();

        assert_eq!(restored.positions.len(), cube.positions.len());
        assert_eq!(restored.faces.len(), cube.faces.len());
        for (p_orig, p_rest) in cube.positions.iter().zip(restored.positions.iter()) {
            assert!((*p_orig - *p_rest).length() < 1e-5);
        }
    }

    #[test]
    fn subobject_selection_operations() {
        let mut sel = SubObjectSelection::new();
        assert!(sel.is_empty());

        sel.select_vertex(2);
        sel.select_edge(5, 3); // canonicalized to [3, 5]
        sel.select_face(0);

        assert!(!sel.is_empty());
        assert_eq!(sel.total_selected_count(), 3);
        assert!(sel.edges.contains(&[3, 5]));

        let cube = primitive_cube(2.0);
        let all_verts = sel.all_selected_vertex_indices(&cube);
        assert!(all_verts.contains(&2));
        assert!(all_verts.contains(&3));
        assert!(all_verts.contains(&5));

        sel.deselect_edge(3, 5);
        assert_eq!(sel.edges.len(), 0);

        sel.clear();
        assert!(sel.is_empty());
    }

    #[test]
    fn raycast_face_picking_moller_trumbore() {
        let cube = primitive_cube(2.0); // centered at origin, X,Y,Z in [-1, 1]
        // Ray aimed from (0, 0, 5) towards NEG_Z (0, 0, -1)
        let ray = Ray3D::new(Vec3::new(0.0, 0.0, 5.0), Vec3::NEG_Z);

        let hit = pick_face_ray(&cube, ray).expect("face hit");
        assert!((hit.distance - 4.0).abs() < 1e-3);
        assert!((hit.hit_point.z - 1.0).abs() < 1e-3);
        assert!((hit.face_normal - Vec3::Z).length() < 1e-3);
    }

    #[test]
    fn raycast_edge_picking_closest_segment() {
        let cube = primitive_cube(2.0);
        // Front top edge is at Y=1, Z=1, extending X from -1 to 1
        // Aim ray slightly above edge: (0, 1.05, 5) towards NEG_Z
        let ray = Ray3D::new(Vec3::new(0.0, 1.05, 5.0), Vec3::NEG_Z);
        let hit = pick_edge_ray(&cube, ray, 0.2).expect("edge hit");

        assert!(hit.distance > 0.0);
        assert!((hit.edge_point.y - 1.0).abs() < 1e-3);
        assert!((hit.edge_point.z - 1.0).abs() < 1e-3);
    }

    #[test]
    fn raycast_vertex_picking_sphere_tolerance() {
        let cube = primitive_cube(2.0);
        // Corner at (1, 1, 1)
        let ray = Ray3D::new(Vec3::new(1.05, 1.05, 5.0), Vec3::NEG_Z);
        let hit = pick_vertex_ray(&cube, ray, 0.2).expect("vertex hit");

        assert!(hit.distance > 0.0);
        assert!((hit.vertex_pos - Vec3::ONE).length() < 1e-3);
    }

    #[test]
    fn raycast_picking_miss_returns_none() {
        let cube = primitive_cube(2.0);
        // Ray pointing completely away from the cube
        let ray = Ray3D::new(Vec3::new(50.0, 50.0, 50.0), Vec3::Y);

        assert!(pick_face_ray(&cube, ray).is_none());
        assert!(pick_edge_ray(&cube, ray, 0.5).is_none());
        assert!(pick_vertex_ray(&cube, ray, 0.5).is_none());
    }

    #[test]
    fn topological_edge_loop_selection() {
        let cube = primitive_cube(2.0);
        let edges = select_edge_loop(&cube, [4, 5]);
        assert_eq!(edges.len(), 4);
    }

    #[test]
    fn topological_face_loop_selection() {
        let cube = primitive_cube(2.0);
        let face_loop = select_face_loop(&cube, 0, [0, 1]);
        assert!(!face_loop.is_empty());
    }

    #[test]
    fn box_select_ndc_vertex_edge_face() {
        let cube = primitive_cube(2.0);
        // Orthographic identity projection matrix
        let identity = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];

        // Box selecting positive quadrant: X in [0.0, 2.0], Y in [0.0, 2.0]
        let sel_v = box_select_ndc(&cube, identity, [0.0, 0.0], [2.0, 2.0], SubObjectMode::Vertex);
        assert!(!sel_v.vertices.is_empty());

        let sel_e = box_select_ndc(&cube, identity, [-2.0, -2.0], [2.0, 2.0], SubObjectMode::Edge);
        assert!(!sel_e.edges.is_empty());

        let sel_f = box_select_ndc(&cube, identity, [-2.0, -2.0], [2.0, 2.0], SubObjectMode::Face);
        assert_eq!(sel_f.faces.len(), 6);
    }

    #[test]
    fn proportional_editing_falloff_curves() {
        let mut cube = primitive_cube(2.0);
        let mut sel = SubObjectSelection::new();
        sel.select_vertex(0);

        let initial_p1 = cube.positions[1];
        transform_selection_proportional(
            &mut cube,
            &sel,
            Vec3::new(0.0, 2.0, 0.0),
            5.0,
            ProportionalFalloff::Smooth,
        );

        // Vertex 0 moved the full translation
        // Neighboring vertex 1 moved a proportional amount
        assert_ne!(cube.positions[1], initial_p1);

        // Test falloff evaluations
        assert!((ProportionalFalloff::Linear.evaluate(0.0) - 1.0).abs() < 1e-4);
        assert!((ProportionalFalloff::Linear.evaluate(1.0) - 0.0).abs() < 1e-4);
        assert!((ProportionalFalloff::Smooth.evaluate(0.0) - 1.0).abs() < 1e-4);
        assert!((ProportionalFalloff::Smooth.evaluate(1.0) - 0.0).abs() < 1e-4);
        assert!((ProportionalFalloff::Sphere.evaluate(0.0) - 1.0).abs() < 1e-4);
        assert!((ProportionalFalloff::Root.evaluate(0.0) - 1.0).abs() < 1e-4);
        assert!((ProportionalFalloff::Sharp.evaluate(0.0) - 1.0).abs() < 1e-4);
    }

    #[test]
    fn polyops_v2_transcript_emit_and_parse_roundtrip() {
        let steps = vec![
            PolyOpStep::SelectVertex { indices: vec![0, 1, 2] },
            PolyOpStep::SelectEdge { edges: vec![[0, 1], [2, 3]] },
            PolyOpStep::SelectFace { indices: vec![1, 2] },
            PolyOpStep::SelectLoop { edge: [0, 1] },
            PolyOpStep::ExtrudeFaces { face_indices: vec![2], distance: 1.5 },
            PolyOpStep::InsetFaces { face_indices: vec![0], thickness: 0.2, depth: 0.1 },
            PolyOpStep::BevelEdges { edges: vec![[0, 1]], offset: 0.05, segments: 2 },
            PolyOpStep::LoopCut { axis: 0, factor: 0.5 },
            PolyOpStep::TranslateProportional {
                translation: [0.0, 1.0, 0.0],
                radius: 2.0,
                falloff: ProportionalFalloff::Smooth,
            },
            PolyOpStep::TagEdge { edge: [0, 1], tag: "grind_edge".into() },
            PolyOpStep::TagFace { face_index: 2, tag: "collision_convex".into() },
            PolyOpStep::ClearSelection,
        ];

        let transcript = emit_polyops_v2(&steps);
        assert!(transcript.starts_with("polyops 2\n"));

        let parsed = parse_polyops_v2(&transcript).expect("parse polyops v2");
        assert_eq!(parsed, steps);
    }

    #[test]
    fn polyops_v2_replay_parameterized_modeling() {
        let mut cube = primitive_cube(2.0);
        let mut attrs = PolyamourSemanticAttributes::new();

        let steps = vec![
            PolyOpStep::ExtrudeFaces { face_indices: vec![2], distance: 1.0 },
            PolyOpStep::TagEdge { edge: [0, 1], tag: "grind_edge".into() },
            PolyOpStep::TagFace { face_index: 0, tag: "collision_convex".into() },
        ];

        replay_polyops_v2(&mut cube, &mut attrs, &steps).expect("replay steps");
        assert_eq!(cube.faces.len(), 10); // 6 + 4 extruded side walls
        assert!(attrs.has_edge_tag([0, 1], "grind_edge"));
        assert!(attrs.has_face_tag(0, "collision_convex"));
    }

    #[test]
    fn semantic_attributes_tagging_and_filtering() {
        let mut attrs = PolyamourSemanticAttributes::new();

        attrs.tag_edge([1, 0], "grind_edge");
        attrs.tag_edge([1, 2], "grind_edge");
        attrs.tag_edge([2, 3], "uv_seam");
        attrs.tag_face(0, "collision_convex");
        attrs.tag_face(1, "collision_trimesh");

        assert!(attrs.has_edge_tag([0, 1], "grind_edge"));
        assert!(attrs.has_edge_tag([2, 3], "uv_seam"));
        assert!(!attrs.has_edge_tag([0, 1], "uv_seam"));

        assert_eq!(attrs.get_edges_with_tag("grind_edge").len(), 2);
        assert_eq!(attrs.get_faces_with_tag("collision_convex"), vec![0]);

        attrs.untag_edge([0, 1], "grind_edge");
        assert!(!attrs.has_edge_tag([0, 1], "grind_edge"));
    }

    #[test]
    fn semantic_tags_grind_rail_splines_chaining() {
        let mut mesh = PolyMesh::new();
        let v0 = mesh.add_vertex(Vec3::new(0.0, 1.0, 0.0));
        let v1 = mesh.add_vertex(Vec3::new(1.0, 1.0, 0.0));
        let v2 = mesh.add_vertex(Vec3::new(2.0, 1.0, 0.0));
        mesh.loose_edges.push([v0, v1]);
        mesh.loose_edges.push([v1, v2]);

        let mut attrs = PolyamourSemanticAttributes::new();
        attrs.tag_edge([v0, v1], "grind_edge");
        attrs.tag_edge([v1, v2], "grind_edge");

        let bundle = compile_polyamour_game_bundle(&mesh, &attrs);
        assert_eq!(bundle.grind_rail_splines.len(), 1);
        let spline = &bundle.grind_rail_splines[0];
        assert_eq!(spline.len(), 3);
        assert_eq!(spline[0], Vec3::new(0.0, 1.0, 0.0));
        assert_eq!(spline[2], Vec3::new(2.0, 1.0, 0.0));
    }

    #[test]
    fn semantic_tags_closed_loop_grind_rail() {
        let mut mesh = PolyMesh::new();
        let v0 = mesh.add_vertex(Vec3::new(0.0, 0.0, 0.0));
        let v1 = mesh.add_vertex(Vec3::new(2.0, 0.0, 0.0));
        let v2 = mesh.add_vertex(Vec3::new(2.0, 0.0, 2.0));
        let v3 = mesh.add_vertex(Vec3::new(0.0, 0.0, 2.0));

        let mut attrs = PolyamourSemanticAttributes::new();
        attrs.tag_edge([v0, v1], "grind_edge");
        attrs.tag_edge([v1, v2], "grind_edge");
        attrs.tag_edge([v2, v3], "grind_edge");
        attrs.tag_edge([v3, v0], "grind_edge");

        let bundle = compile_polyamour_game_bundle(&mesh, &attrs);
        assert_eq!(bundle.grind_rail_splines.len(), 1);
        let spline = &bundle.grind_rail_splines[0];
        // Closed loop has same start and end
        assert_eq!(spline.first(), spline.last());
    }

    #[test]
    fn semantic_tags_physics_colliders_convex_and_trimesh() {
        let cube = primitive_cube(2.0);
        let mut attrs = PolyamourSemanticAttributes::new();
        attrs.tag_face(0, "collision_convex");
        attrs.tag_face(1, "collision_trimesh");

        let bundle = compile_polyamour_game_bundle(&cube, &attrs);
        assert_eq!(bundle.convex_hull_points.len(), 4);
        assert_eq!(bundle.trimesh_collider.1.len(), 2); // Quad face 1 triangulated into 2 triangles
    }

    #[test]
    fn semantic_tags_smart_uv2_atlas_packing_no_overlap() {
        let cube = primitive_cube(2.0);
        let attrs = PolyamourSemanticAttributes::new();
        let bundle = compile_polyamour_game_bundle(&cube, &attrs);

        assert_eq!(bundle.uv2_islands.len(), 6);
        // Verify every UV island is completely within [0, 1]^2
        for island in &bundle.uv2_islands {
            assert!(island.min_uv[0] >= 0.0 && island.max_uv[0] <= 1.0);
            assert!(island.min_uv[1] >= 0.0 && island.max_uv[1] <= 1.0);
        }

        // Verify no overlapping islands
        for i in 0..bundle.uv2_islands.len() {
            for j in (i + 1)..bundle.uv2_islands.len() {
                let a = &bundle.uv2_islands[i];
                let b = &bundle.uv2_islands[j];
                let overlap_x = a.min_uv[0] < b.max_uv[0] && a.max_uv[0] > b.min_uv[0];
                let overlap_y = a.min_uv[1] < b.max_uv[1] && a.max_uv[1] > b.min_uv[1];
                assert!(!(overlap_x && overlap_y), "Island {i} and {j} must not overlap in UV2 atlas");
            }
        }
    }

    #[test]
    fn sculpt_brushes_grab_inflate_flatten_crease() {
        let mut grid = primitive_grid(4.0, 4.0, 4, 4);

        // 1. Grab brush
        let grab = SculptStroke {
            brush: SculptBrush::Grab,
            hit_point: Vec3::ZERO,
            hit_normal: Vec3::Y,
            radius: 2.0,
            strength: 0.8,
            symmetry: SymmetryAxes::none(),
            stroke_delta: Some(Vec3::new(0.0, 1.5, 0.0)),
        };
        apply_sculpt_stroke(&mut grid, &grab);
        let center_v = grid.positions.iter().find(|p| p.x.abs() < 0.1 && p.z.abs() < 0.1).unwrap();
        assert!(center_v.y > 0.5);

        // 2. Inflate brush
        let inflate = SculptStroke {
            brush: SculptBrush::Inflate,
            hit_point: Vec3::ZERO,
            hit_normal: Vec3::Y,
            radius: 1.5,
            strength: 0.5,
            symmetry: SymmetryAxes::none(),
            stroke_delta: None,
        };
        apply_sculpt_stroke(&mut grid, &inflate);

        // 3. Flatten brush
        let flatten = SculptStroke {
            brush: SculptBrush::Flatten,
            hit_point: Vec3::new(0.0, 0.5, 0.0),
            hit_normal: Vec3::Y,
            radius: 2.0,
            strength: 1.0,
            symmetry: SymmetryAxes::none(),
            stroke_delta: None,
        };
        apply_sculpt_stroke(&mut grid, &flatten);

        // 4. Crease brush
        let crease = SculptStroke {
            brush: SculptBrush::Crease,
            hit_point: Vec3::ZERO,
            hit_normal: Vec3::Y,
            radius: 1.5,
            strength: 0.7,
            symmetry: SymmetryAxes::none(),
            stroke_delta: None,
        };
        apply_sculpt_stroke(&mut grid, &crease);
    }

    #[test]
    fn sculpt_symmetry_mirror_x() {
        let mut mesh = PolyMesh::new();
        let v_left = mesh.add_vertex(Vec3::new(-1.0, 0.0, 0.0));
        let v_right = mesh.add_vertex(Vec3::new(1.0, 0.0, 0.0));

        let stroke = SculptStroke {
            brush: SculptBrush::Inflate,
            hit_point: Vec3::new(1.0, 0.0, 0.0),
            hit_normal: Vec3::X,
            radius: 0.5,
            strength: 1.0,
            symmetry: SymmetryAxes::x_only(),
            stroke_delta: None,
        };

        apply_sculpt_stroke(&mut mesh, &stroke);
        // Right vertex moved +X
        assert!(mesh.positions[v_right as usize].x > 1.0);
        // Left vertex symmetrically moved -X
        assert!(mesh.positions[v_left as usize].x < -1.0);
    }

    #[test]
    fn sculpt_rig_auto_heat_bone_weights_smooth_normalization() {
        let cylinder = primitive_cylinder(1.0, 4.0, 8);
        let bones = vec![
            RigBoneSegment::new("Spine", Vec3::new(0.0, -2.0, 0.0), Vec3::new(0.0, 0.0, 0.0), 1.5),
            RigBoneSegment::new("Head", Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 2.0, 0.0), 1.5),
        ];

        let weights = compute_heat_bone_weights(&cylinder, &bones);
        assert_eq!(weights.len(), cylinder.positions.len());

        for (v_idx, influence) in weights.iter().enumerate() {
            let sum: f32 = influence.weights.iter().sum();
            assert!(
                (sum - 1.0).abs() < 1e-4,
                "Vertex {v_idx} bone weight sum {sum} must be 1.0"
            );
            for (b_i, &w) in influence.weights.iter().enumerate() {
                assert!(!w.is_nan(), "Vertex {v_idx} bone {b_i} weight must not be NaN");
                assert!((0.0..=1.0).contains(&w));
            }
        }
    }

    #[test]
    fn sculpt_rig_degenerate_bone_heat_weights_safety() {
        let cube = primitive_cube(2.0);
        // Degenerate zero-length bone: head == tail
        let bones = vec![
            RigBoneSegment::new("PointBone", Vec3::ZERO, Vec3::ZERO, 0.0),
        ];

        let weights = compute_heat_bone_weights(&cube, &bones);
        assert_eq!(weights.len(), cube.positions.len());
        for influence in weights {
            let sum: f32 = influence.weights.iter().sum();
            assert!((sum - 1.0).abs() < 1e-4);
            assert!(!influence.weights[0].is_nan());
        }
    }

    #[test]
    fn shape_key_set_basis_morph_blending() {
        use std::collections::BTreeMap;

        let basis = vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 1.0, 1.0)];
        let mut shape_keys = ShapeKeySet::new(basis);

        let target_smile = vec![Vec3::new(0.0, 1.0, 0.0), Vec3::new(1.0, 2.0, 1.0)];
        assert!(shape_keys.add_shape_key("Smile", target_smile));

        // Evaluate at 0.0 weight
        let mut weights = BTreeMap::new();
        weights.insert("Smile".to_string(), 0.0);
        let pos_zero = shape_keys.evaluate_blended_positions(&weights);
        assert_eq!(pos_zero, shape_keys.basis);

        // Evaluate at 0.5 weight
        weights.insert("Smile".to_string(), 0.5);
        let pos_half = shape_keys.evaluate_blended_positions(&weights);
        assert_eq!(pos_half[0], Vec3::new(0.0, 0.5, 0.0));
        assert_eq!(pos_half[1], Vec3::new(1.0, 1.5, 1.0));

        // Evaluate at 1.0 weight
        weights.insert("Smile".to_string(), 1.0);
        let pos_full = shape_keys.evaluate_blended_positions(&weights);
        assert_eq!(pos_full[0], Vec3::new(0.0, 1.0, 0.0));
        assert_eq!(pos_full[1], Vec3::new(1.0, 2.0, 1.0));
    }
}
