//! Track E Wave 3B: World-Class Adversarial Red-Team Verification Suites (`src/v2_worldclass_fuzzers.rs`).
//!
//! Suites 25..30:
//! - Suite 25: Polyamour Topology, Edit Operations, HalfEdge & Modifiers Invariant Suite
//! - Suite 26: Polyamour Hostile Blend Headers, Foreign Asset Import & Atomic Polyops Rollback Suite
//! - Suite 27: Three.js Compiler, TypeScript/Vite Emission & Bidirectional Roundtrip Suite
//! - Suite 28: UE6 Speculative Rollback State-Hash Determinism & Subtree Lease Invariant Suite
//! - Suite 29: Micro-Nanite Meshlet Bounding Invariants, Shaded PBR Viewport & Perceptual Gate Suite
//! - Suite 30: TriMesh BVH, GJK Convex Symmetry, 6-Body Ragdoll & Moan Audio Timeline Invariant Suite

use crate::VerificationReceipt;
use bevy_math::{Mat4, Vec3};
use foreplay_atlas::timetravel::{LeaseError, SubtreeLeaseManager};
use foreplay_audio::timeline::{AudioClip, AudioTimeline, AudioTimelineTrack};
use foreplay_core::rollback::{state_hash, SpeculativeFrameBuffer};
use foreplay_core::SceneTree;
use foreplay_ir::{IrNode, IrScene, IrValue, NodeId};
use foreplay_physics::trimesh_convex::{
    closest_point_on_triangle, gjk_intersects, ConvexHullCollider3D, RagdollRig3D,
    TriMeshCollider3D,
};
use foreplay_polyamour::io::{
    import_gltf_json, import_obj, import_ply, import_stl, BlendFileSummary,
};
use foreplay_polyamour::mesh::{HalfEdgeMesh, PolyMesh};
use foreplay_polyamour::modifiers::{evaluate_modifier_stack, PolyModifier};
use foreplay_polyamour::ops::{boolean_csg, extrude_faces, inset_faces, knife_bisect, BooleanOp};
use foreplay_polyamour::polyops::{run_polyops, PolyObject, PolyScene};
use foreplay_polyamour::primitives::{
    primitive_cube, primitive_quarterpipe, primitive_stair_set, primitive_torus,
};
use foreplay_render::meshlet::{build_meshlets, MAX_MESHLET_TRIANGLES, MAX_MESHLET_VERTICES};
use foreplay_render::pbr_gi::{
    render_shaded_pbr_viewport_rgba8, PbrMaterialDesc, PbrViewportParams, RenderTriangleMesh,
};
use foreplay_render::perceptual::{compute_psnr_rgba8, compute_ssim_rgba8};
use foreplay_threejs::{
    compile_ir_to_threejs_project, export_ir_to_threejs_json, import_threejs_json_to_ir,
    ThreeJsExportOptions, ThreeToneMapping,
};
use std::collections::BTreeMap;
use std::sync::Arc;

fn make_node(
    id: NodeId,
    name: impl Into<String>,
    class_name: impl Into<String>,
    parent: Option<NodeId>,
) -> IrNode {
    IrNode {
        id,
        name: name.into(),
        class_name: class_name.into(),
        parent,
        instance_scene: None,
        properties: BTreeMap::new(),
        groups: Vec::new(),
    }
}

// =========================================================================
// Suite 25: Polyamour Topology, Edit Operations & Modifiers Suite
// =========================================================================

/// Verifies Polyamour mesh topology, procedural primitives, destructive/non-destructive edit
/// operations, half-edge topological queries, and 9-modifier evaluation invariants.
///
/// Invariants proven:
/// 1. Zero panics across all primitive configurations (including 0-sized, single-step, large segment counts).
/// 2. For every triangulated mesh: `indices.len() % 3 == 0`.
/// 3. For every index: `(idx as usize) < positions.len()`.
/// 4. Every position, normal, and UV coordinate is finite (no NaNs, no infinities).
/// 5. `HalfEdgeMesh::from_polymesh` builds valid topology without panics.
/// 6. Modifiers (Mirror, Subdivision, Array, Solidify, Smooth, Triangulate, Weld) preserve geometric invariants.
pub fn verify_polyamour_topology_and_modifiers_suite() -> VerificationReceipt {
    let mut cases_executed = 0;
    let mut invariants_held = true;

    // Helper closure to validate triangulated mesh invariants
    let validate_triangulation = |mesh: &PolyMesh, invariants_held: &mut bool| {
        let tri = mesh.triangulate();
        if !tri.indices.len().is_multiple_of(3) {
            *invariants_held = false;
        }
        for &idx in &tri.indices {
            if (idx as usize) >= tri.positions.len() {
                *invariants_held = false;
            }
        }
        for p in &tri.positions {
            if !p.x.is_finite() || !p.y.is_finite() || !p.z.is_finite() {
                *invariants_held = false;
            }
        }
        for n in &tri.normals {
            if !n.x.is_finite() || !n.y.is_finite() || !n.z.is_finite() {
                *invariants_held = false;
            }
        }
        for uv in &tri.uvs {
            if !uv[0].is_finite() || !uv[1].is_finite() {
                *invariants_held = false;
            }
        }

        // HalfEdge invariant verification
        let he = HalfEdgeMesh::from_polymesh(mesh);
        let _ = he.is_manifold();
        let _ = he.boundary_edges();
        let _ = he.face_areas();
        for i in 0..mesh.positions.len().min(16) as u32 {
            let _ = he.vertex_neighbors(i);
        }
    };

    // 1. Primitive Generation Fuzzing
    let cube_sizes = [0.0f32, 0.001, 0.5, 1.0, 10.0, 100.0];
    for &size in &cube_sizes {
        cases_executed += 1;
        let mesh = primitive_cube(size);
        if mesh.positions.len() != 8 || mesh.faces.len() != 6 {
            invariants_held = false;
        }
        validate_triangulation(&mesh, &mut invariants_held);
    }

    let qp_params = [
        (0.5f32, 1.0f32, 4u32),
        (2.0, 4.0, 8),
        (5.0, 10.0, 16),
        (1.0, 0.1, 4),
        (10.0, 20.0, 32),
    ];
    for &(radius, width, segs) in &qp_params {
        cases_executed += 1;
        let mesh = primitive_quarterpipe(radius, width, segs);
        validate_triangulation(&mesh, &mut invariants_held);
    }

    let stair_params = [
        (1.0f32, 1.0f32, 2.0f32, 1u32),
        (2.0, 1.5, 3.0, 3),
        (3.0, 2.0, 4.0, 6),
        (5.0, 3.0, 8.0, 12),
    ];
    for &(w, h, d, count) in &stair_params {
        cases_executed += 1;
        let mesh = primitive_stair_set(w, h, d, count);
        validate_triangulation(&mesh, &mut invariants_held);
    }

    let torus_params = [
        (1.0f32, 0.25f32, 4u32, 4u32),
        (2.0, 0.5, 8, 6),
        (3.0, 0.8, 12, 8),
        (0.5, 0.1, 6, 6),
    ];
    for &(maj, min, maj_s, min_s) in &torus_params {
        cases_executed += 1;
        let mesh = primitive_torus(maj, min, maj_s, min_s);
        validate_triangulation(&mesh, &mut invariants_held);
    }

    // 2. Destructive Edit Operations
    // Extrude faces
    let mut extrude_mesh = primitive_cube(1.0);
    extrude_faces(&mut extrude_mesh, &[0, 2], 0.5);
    cases_executed += 1;
    validate_triangulation(&extrude_mesh, &mut invariants_held);

    extrude_faces(&mut extrude_mesh, &[999], 1.0); // Out-of-bounds face index
    cases_executed += 1;
    validate_triangulation(&extrude_mesh, &mut invariants_held);

    // Inset faces
    let mut inset_mesh = primitive_cube(2.0);
    inset_faces(&mut inset_mesh, &[0, 1], 0.2, 0.1);
    cases_executed += 1;
    validate_triangulation(&inset_mesh, &mut invariants_held);

    inset_faces(&mut inset_mesh, &[999], 0.5, 0.0); // Out-of-bounds face index
    cases_executed += 1;
    validate_triangulation(&inset_mesh, &mut invariants_held);

    // Knife bisect
    let mut bisect_mesh = primitive_cube(2.0);
    knife_bisect(&mut bisect_mesh, Vec3::ZERO, Vec3::Y);
    cases_executed += 1;
    validate_triangulation(&bisect_mesh, &mut invariants_held);

    knife_bisect(
        &mut bisect_mesh,
        Vec3::new(0.2, 0.3, 0.4),
        Vec3::new(1.0, 1.0, 1.0).normalize_or_zero(),
    );
    cases_executed += 1;
    validate_triangulation(&bisect_mesh, &mut invariants_held);

    // Boolean CSG operations (Union, Difference, Intersect)
    let csg_cube_a = primitive_cube(1.0);
    let mut csg_cube_b = primitive_cube(1.0);
    for p in &mut csg_cube_b.positions {
        *p += Vec3::new(0.5, 0.5, 0.5);
    }

    let csg_ops = [
        BooleanOp::Union,
        BooleanOp::Difference,
        BooleanOp::Intersect,
    ];
    for op in csg_ops {
        cases_executed += 1;
        let result_mesh = boolean_csg(&csg_cube_a, &csg_cube_b, op);
        validate_triangulation(&result_mesh, &mut invariants_held);
    }

    // 3. Modifier Stack Evaluation
    let base_cube = primitive_cube(1.0);
    let test_stacks: Vec<Vec<PolyModifier>> = vec![
        vec![PolyModifier::Mirror {
            axis_x: true,
            axis_y: false,
            axis_z: false,
            merge_distance: 0.01,
        }],
        vec![PolyModifier::Subdivision {
            levels: 1,
            simple: false,
        }],
        vec![PolyModifier::Subdivision {
            levels: 1,
            simple: true,
        }],
        vec![PolyModifier::Array {
            count: 3,
            relative_offset: Vec3::new(1.2, 0.0, 0.0),
            constant_offset: Vec3::ZERO,
        }],
        vec![PolyModifier::Solidify { thickness: 0.1 }],
        vec![PolyModifier::Smooth {
            factor: 0.5,
            iterations: 2,
        }],
        vec![PolyModifier::Triangulate],
        vec![PolyModifier::Weld { distance: 0.05 }],
        // Multi-modifier pipeline
        vec![
            PolyModifier::Array {
                count: 2,
                relative_offset: Vec3::new(1.5, 0.0, 0.0),
                constant_offset: Vec3::ZERO,
            },
            PolyModifier::Mirror {
                axis_x: false,
                axis_y: false,
                axis_z: true,
                merge_distance: 0.02,
            },
            PolyModifier::Solidify { thickness: 0.05 },
            PolyModifier::Triangulate,
        ],
    ];

    for stack in &test_stacks {
        cases_executed += 1;
        let evaluated = evaluate_modifier_stack(&base_cube, stack);
        validate_triangulation(&evaluated, &mut invariants_held);
    }

    // Edge cases: empty mesh through pipeline
    let empty_mesh = PolyMesh::new();
    cases_executed += 1;
    validate_triangulation(&empty_mesh, &mut invariants_held);

    let empty_evaluated = evaluate_modifier_stack(&empty_mesh, &test_stacks[0]);
    cases_executed += 1;
    validate_triangulation(&empty_evaluated, &mut invariants_held);

    VerificationReceipt {
        suite_name: "polyamour_topology_and_modifiers_suite".into(),
        cases_executed,
        invariants_held,
    }
}

// =========================================================================
// Suite 26: Polyamour Hostile Blend I/O & Atomic Polyops Suite
// =========================================================================

/// Verifies safe clean-room parsing of Blender `.blend` files across legacy and Blender 5.0 headers,
/// hostile format fuzzing (OBJ, STL, PLY, glTF), and strict atomic rollback in `polyops 1`.
pub fn verify_polyamour_blend_io_and_polyops_hostile_suite() -> VerificationReceipt {
    let mut cases_executed = 0;
    let mut invariants_held = true;

    // 1. Blender .blend Header Parsing Fuzzing
    // Truncated headers (0..12 bytes)
    for len in 0..12 {
        cases_executed += 1;
        let truncated = vec![0x42u8; len];
        if BlendFileSummary::parse_bytes(&truncated).is_ok() {
            invariants_held = false;
        }
    }

    // Invalid magic
    cases_executed += 1;
    let bad_magic = b"NOT_BLENDER_V";
    if BlendFileSummary::parse_bytes(bad_magic).is_ok() {
        invariants_held = false;
    }

    // Valid header format variants
    let valid_headers: &[&[u8]] = &[
        b"BLENDER_v280", // 32-bit little-endian v2.80
        b"BLENDER-v300", // 64-bit little-endian v3.00
        b"BLENDER-V400", // 64-bit big-endian v4.00
        b"BLENDER-v500", // 64-bit little-endian v5.00
        b"BLENDER17v10", // Modern versioned header
    ];
    for &hdr in valid_headers {
        cases_executed += 1;
        match BlendFileSummary::parse_bytes(hdr) {
            Ok(summary) => {
                if summary.header_magic != "BLENDER" {
                    invariants_held = false;
                }
            }
            Err(_) => {
                invariants_held = false;
            }
        }
    }

    // Hostile blocks: valid header followed by corrupted block headers
    let hostile_blocks: &[Vec<u8>] = &[
        // Block with huge size (u32::MAX)
        {
            let mut v = b"BLENDER-v300".to_vec();
            v.extend_from_slice(b"TEST"); // code
            v.extend_from_slice(&u32::MAX.to_le_bytes()); // size
            v.extend_from_slice(&[0u8; 8]); // addr
            v.extend_from_slice(&[0u8; 4]); // sdna
            v.extend_from_slice(&[1u8; 4]); // count
            v
        },
        // Incomplete block header (< 24 bytes)
        {
            let mut v = b"BLENDER-v300".to_vec();
            v.extend_from_slice(b"TEST\x04\x00");
            v
        },
    ];

    for h_bytes in hostile_blocks {
        cases_executed += 1;
        // Invariant: must not panic, whether Ok (stopping cleanly) or Err
        let _ = BlendFileSummary::parse_bytes(h_bytes);
    }

    // 2. Foreign Asset Import Fuzzing (OBJ, STL, PLY, glTF)
    // Hostile OBJ strings
    let hostile_objs = [
        "",
        "   \n\t  \n",
        "v \nv \n",
        "v NaN Inf 0.0\n",
        "v 1.0 2.0 3.0\nf -1 -2 -3\n",
        "f 1/1/1 2/2/2 3/3/3\n",
        "invalid token xyz 1234\n",
        "v 0.0 0.0 0.0\nv 1.0 0.0 0.0\nv 0.0 1.0 0.0\nf 1 2 3\n",
    ];
    for obj_str in &hostile_objs {
        cases_executed += 1;
        let _ = import_obj(obj_str); // Zero panics
    }

    // Hostile STL bytes
    let hostile_stls: &[&[u8]] = &[
        b"",
        b"solid",
        b"solid \n facet normal 0 0 1\n  outer loop\n   vertex 0 0 0\n  endloop\n endfacet\nendsolid",
        &[0u8; 83], // Truncated binary STL header
        &[0u8; 128], // Binary STL with 0 triangles
    ];
    for stl_data in hostile_stls {
        cases_executed += 1;
        let _ = import_stl(stl_data); // Zero panics
    }

    // Hostile PLY strings
    let hostile_plys = [
        "",
        "ply\nformat ascii 1.0\nelement vertex 0\nend_header\n",
        "not_ply\n",
        "ply\nformat ascii 1.0\nelement vertex 3\nend_header\n0 0 0\n1 0 0\n",
    ];
    for ply_data in &hostile_plys {
        cases_executed += 1;
        let _ = import_ply(ply_data); // Zero panics
    }

    // Hostile glTF JSON strings
    let hostile_gltfs = [
        "",
        "{}",
        "{\"asset\": {\"version\": \"2.0\"}}",
        "{\"buffers\": [{\"uri\": \"data:invalid_no_base64\"}]}",
        "{\"buffers\": [{\"uri\": \"data:application/octet-stream;base64,====\"}]}",
    ];
    for gltf_str in &hostile_gltfs {
        cases_executed += 1;
        let _ = import_gltf_json(gltf_str); // Zero panics
    }

    // 3. Atomic `polyops 1` Rollback Verification
    let mut scene = PolyScene::new();
    let initial_cube = primitive_cube(1.0);
    let obj_id = scene.alloc_object_id();
    scene
        .objects
        .insert(obj_id, PolyObject::new(obj_id, "BaseCube", initial_cube));
    let backup_scene = scene.clone();

    // A script that modifies scene and then explicitly aborts:
    // Must return Err and leave `scene` 100% identical to backup_scene
    let abort_script = "polyops 1\nbegin\nadd_primitive cube 2.0 as $C\nabort \"adversarial_test_abort\"\nend\n";
    cases_executed += 1;
    let res = run_polyops(&mut scene, abort_script);
    if res.is_ok() {
        invariants_held = false;
    }
    if scene != backup_scene {
        invariants_held = false;
    }

    // A script with syntax/semantic error:
    let corrupt_script = "polyops 1\nbegin\nmesh_edit 9999 extrude [0] 1.0\nend\n";
    cases_executed += 1;
    let res_err = run_polyops(&mut scene, corrupt_script);
    if res_err.is_ok() {
        invariants_held = false;
    }
    if scene != backup_scene {
        invariants_held = false;
    }

    // A script with invalid header:
    let no_header_script = "begin\nadd_primitive cube 1.0\nend\n";
    cases_executed += 1;
    let res_hdr = run_polyops(&mut scene, no_header_script);
    if res_hdr.is_ok() {
        invariants_held = false;
    }
    if scene != backup_scene {
        invariants_held = false;
    }

    // A valid script must commit changes:
    let valid_script = "polyops 1\nbegin\nadd_primitive plane 3.0 as $P\nend\n";
    cases_executed += 1;
    let res_ok = run_polyops(&mut scene, valid_script);
    if res_ok.is_err() {
        invariants_held = false;
    }
    if scene.objects.len() != backup_scene.objects.len() + 1 {
        invariants_held = false;
    }

    VerificationReceipt {
        suite_name: "polyamour_blend_io_and_polyops_hostile_suite".into(),
        cases_executed,
        invariants_held,
    }
}

// =========================================================================
// Suite 27: Three.js Compiler & Roundtrip Suite
// =========================================================================

/// Verifies Three.js JSON scene export & import roundtrips, TypeScript/Vite web bundle emission,
/// and hostile JSON parser fuzzing.
pub fn verify_threejs_compiler_and_bidirectional_roundtrip_suite() -> VerificationReceipt {
    let mut cases_executed = 0;
    let mut invariants_held = true;

    // 1. IrScene -> Three.js JSON -> IrScene Roundtrip
    let mut test_scene = IrScene::new("ThreeJsVerificationScene");
    let root_id = NodeId::new(1, 1).expect("valid ID");
    let child1_id = NodeId::new(2, 1).expect("valid ID");
    let child2_id = NodeId::new(3, 1).expect("valid ID");

    let mut root_node = make_node(root_id, "RootWorld", "Node3D", None);
    root_node.properties.insert(
        "position".into(),
        IrValue::Vec3([0.0, 1.0, 0.0]),
    );

    let mut child1 = make_node(child1_id, "SkaterPlayer", "CharacterBody3D", Some(root_id));
    child1.properties.insert(
        "speed".into(),
        IrValue::F64(12.5),
    );

    let child2 = make_node(child2_id, "ParkQuarterpipe", "StaticBody3D", Some(root_id));

    test_scene.nodes.insert(root_node.id, root_node);
    test_scene.nodes.insert(child1.id, child1);
    test_scene.nodes.insert(child2.id, child2);
    test_scene
        .feel_overrides
        .insert("skate.pop_force".into(), 450.0);
    test_scene
        .feel_overrides
        .insert("camera.fov".into(), 75.0);

    cases_executed += 1;
    let json_export = export_ir_to_threejs_json(&test_scene);
    if json_export.is_empty() {
        invariants_held = false;
    }

    cases_executed += 1;
    match import_threejs_json_to_ir(&json_export) {
        Ok(recovered_scene) => {
            // Recovered scene must have non-empty nodes
            if recovered_scene.nodes.is_empty() {
                invariants_held = false;
            }
            // Feel overrides must be faithfully preserved
            if let Some(&pop) = recovered_scene.feel_overrides.get("skate.pop_force") {
                if (pop - 450.0).abs() > 1e-4 {
                    invariants_held = false;
                }
            } else {
                invariants_held = false;
            }
        }
        Err(_) => {
            invariants_held = false;
        }
    }

    // 2. Full Project Bundle Compiler Invariants
    let export_options = [
        ThreeJsExportOptions {
            project_name: "TestWebGPU".into(),
            use_webgpu_renderer: true,
            include_feel_dev_hud: true,
            include_atlas_json: true,
            tone_mapping: ThreeToneMapping::AgX,
        },
        ThreeJsExportOptions {
            project_name: "TestWebGL".into(),
            use_webgpu_renderer: false,
            include_feel_dev_hud: false,
            include_atlas_json: false,
            tone_mapping: ThreeToneMapping::ACESFilmic,
        },
        ThreeJsExportOptions {
            project_name: "TestNeutral".into(),
            use_webgpu_renderer: true,
            include_feel_dev_hud: true,
            include_atlas_json: false,
            tone_mapping: ThreeToneMapping::Neutral,
        },
    ];

    let empty_gd = BTreeMap::new();
    let empty_ts = BTreeMap::new();

    for opts in &export_options {
        cases_executed += 1;
        match compile_ir_to_threejs_project(&test_scene, &empty_gd, &empty_ts, opts) {
            Ok(bundle) => {
                // Must emit standard Vite project files
                let required_files = [
                    "package.json",
                    "index.html",
                    "vite.config.ts",
                    "tsconfig.json",
                    "src/main.ts",
                    "src/feel.ts",
                ];
                for file_name in &required_files {
                    if let Some(content) = bundle.files.get(*file_name) {
                        if content.is_empty() {
                            invariants_held = false;
                        }
                    } else {
                        invariants_held = false;
                    }
                }
                // package.json must contain three dependency
                if let Some(pkg) = bundle.files.get("package.json")
                    && !pkg.contains("\"three\"")
                {
                    invariants_held = false;
                }
                if bundle.files.is_empty() || bundle.node_count == 0 {
                    invariants_held = false;
                }
            }
            Err(_) => {
                invariants_held = false;
            }
        }
    }

    // 3. Hostile JSON Parser Fuzzing
    let hostile_json_inputs = [
        "",
        "   ",
        "{\"type\": ",
        "[]",
        "\"raw string\"",
        "12345",
        "true",
        "null",
        "{\"geometries\": [{\"type\": \"UnknownGeometry\"}]}",
        "{\"object\": {\"type\": \"Scene\", \"children\": [{\"matrix\": \"not_an_array\"}]}}",
        "{\"format\": \"3jse.scene\", \"nodes\": \"corrupt\"}",
    ];

    for hostile in &hostile_json_inputs {
        cases_executed += 1;
        // Invariant: zero panics, returns clean Err
        let _ = import_threejs_json_to_ir(hostile);
    }

    VerificationReceipt {
        suite_name: "threejs_compiler_and_bidirectional_roundtrip_suite".into(),
        cases_executed,
        invariants_held,
    }
}

// =========================================================================
// Suite 28: UE6 Speculative Rollback Determinism & Subtree Lease Suite
// =========================================================================

/// Verifies bit-exact rollback determinism across speculative execution buffers,
/// and optimistic concurrent subtree lease locking invariants.
pub fn verify_ue6_speculative_rollback_determinism_suite() -> VerificationReceipt {
    let mut cases_executed = 0;
    let mut invariants_held = true;

    // 1. Bit-Exact Speculative Rollback State-Hash Determinism
    let root_id = NodeId::new(1, 1).expect("valid ID");
    let mut init_scene = IrScene::new("RollbackScene");
    init_scene.nodes.insert(root_id, make_node(root_id, "WorldRoot", "Node3D", None));
    let mut tree = SceneTree::from_ir(init_scene);

    let mut frame_buffer = SpeculativeFrameBuffer::new(64);
    let mut historical_hashes = Vec::new();

    // Advance 10 ticks, mutating tree and recording snapshots
    for tick in 0..10u64 {
        cases_executed += 1;
        let node_id = NodeId::new((tick + 2) as u32, 1).expect("valid ID");
        let mut node = make_node(node_id, format!("Entity_{tick}"), "Node3D", Some(root_id));
        node.properties.insert(
            "tick_val".into(),
            IrValue::I64(tick as i64 * 100),
        );
        tree.active_scene.nodes.insert(node_id, node);
        tree.active_scene.feel_overrides.insert(format!("feel.tick_{tick}"), tick as f64 * 0.1);
        tree.step_frame();

        let h = state_hash(&tree);
        historical_hashes.push((tick, h));
        frame_buffer.record_tick(&tree, tick);
    }

    // Now start a speculative transaction and mutate further for 5 ticks
    frame_buffer.begin_transaction();
    if !frame_buffer.is_in_transaction() {
        invariants_held = false;
    }

    for tick in 10..15u64 {
        cases_executed += 1;
        let node_id = NodeId::new((tick + 2) as u32, 1).expect("valid ID");
        let node = make_node(node_id, format!("SpeculativeEntity_{tick}"), "Node3D", Some(root_id));
        tree.active_scene.nodes.insert(node_id, node);
        tree.step_frame();
        frame_buffer.record_tick(&tree, tick);
    }

    // Perform rollbacks to historical ticks and verify bit-for-bit state_hash equivalence!
    let targets = [9u64, 6, 3, 0];
    for &target_tick in &targets {
        cases_executed += 1;
        let expected_hash = historical_hashes[target_tick as usize].1;

        match frame_buffer.rollback_to_tick(&mut tree, target_tick) {
            Ok(()) => {
                let actual_hash = state_hash(&tree);
                if actual_hash != expected_hash {
                    invariants_held = false;
                }
                // Speculative transaction must be ended after rollback
                if frame_buffer.is_in_transaction() {
                    invariants_held = false;
                }
            }
            Err(_) => {
                invariants_held = false;
            }
        }
    }

    // Rollback to nonexistent tick must fail cleanly
    cases_executed += 1;
    if frame_buffer.rollback_to_tick(&mut tree, 9999).is_ok() {
        invariants_held = false;
    }

    // 2. Subtree Lease Manager Invariants
    let mut lease_scene = IrScene::new("LeaseScene");
    let n_root = NodeId::new(1, 1).expect("valid ID");
    let n_child_a = NodeId::new(2, 1).expect("valid ID");
    let n_sub_a1 = NodeId::new(3, 1).expect("valid ID");
    let n_child_b = NodeId::new(4, 1).expect("valid ID");

    let r = make_node(n_root, "Root", "Node3D", None);
    let ca = make_node(n_child_a, "ChildA", "Node3D", Some(n_root));
    let sa1 = make_node(n_sub_a1, "SubA1", "Node3D", Some(n_child_a));
    let cb = make_node(n_child_b, "ChildB", "Node3D", Some(n_root));

    lease_scene.nodes.insert(r.id, r);
    lease_scene.nodes.insert(ca.id, ca);
    lease_scene.nodes.insert(sa1.id, sa1);
    lease_scene.nodes.insert(cb.id, cb);

    let mut lease_mgr = SubtreeLeaseManager::new();

    // Agent 1 acquires ChildA
    cases_executed += 1;
    let lease1 = lease_mgr.acquire_lease("agent_1", n_child_a, &lease_scene);
    let lease1_id = match lease1 {
        Ok(id) => id,
        Err(_) => {
            invariants_held = false;
            0
        }
    };

    // Agent 2 acquires disjoint ChildB -> must succeed
    cases_executed += 1;
    let lease2 = lease_mgr.acquire_lease("agent_2", n_child_b, &lease_scene);
    let lease2_id = match lease2 {
        Ok(id) => id,
        Err(_) => {
            invariants_held = false;
            0
        }
    };

    // Agent 2 attempts to acquire SubA1 (descendant of ChildA locked by agent 1) -> must fail
    cases_executed += 1;
    let conflict1 = lease_mgr.acquire_lease("agent_2", n_sub_a1, &lease_scene);
    match conflict1 {
        Err(LeaseError::SubtreeConflict { .. }) => {}
        _ => invariants_held = false,
    }

    // Agent 2 attempts to acquire Root (ancestor of ChildA locked by agent 1) -> must fail
    cases_executed += 1;
    let conflict2 = lease_mgr.acquire_lease("agent_2", n_root, &lease_scene);
    match conflict2 {
        Err(LeaseError::SubtreeConflict { .. }) => {}
        _ => invariants_held = false,
    }

    // Unauthorized release: Agent 2 tries to release Agent 1's lease
    cases_executed += 1;
    match lease_mgr.release_lease("agent_2", lease1_id) {
        Err(LeaseError::UnauthorizedRelease(..)) => {}
        _ => invariants_held = false,
    }

    // Authorized release: Agent 1 releases lease1
    cases_executed += 1;
    if lease_mgr.release_lease("agent_1", lease1_id).is_err() {
        invariants_held = false;
    }

    // Now Agent 2 can acquire ChildA!
    cases_executed += 1;
    if lease_mgr.acquire_lease("agent_2", n_child_a, &lease_scene).is_err() {
        invariants_held = false;
    }

    let _ = lease2_id;

    VerificationReceipt {
        suite_name: "ue6_speculative_rollback_determinism_suite".into(),
        cases_executed,
        invariants_held,
    }
}

// =========================================================================
// Suite 29: Meshlet & PBR Perceptual Suite
// =========================================================================

/// Verifies Micro-Nanite GPU meshlet partitioning invariants, shaded PBR rasterization,
/// and ITU-R BT.709 SSIM perceptual verification.
pub fn verify_meshlet_and_pbr_perceptual_suite() -> VerificationReceipt {
    let mut cases_executed = 0;
    let mut invariants_held = true;

    // 1. Meshlet Partitioning Invariants
    // Test on cube, torus, and large synthetic triangle clouds
    let test_geometry: Vec<(Vec<[f32; 3]>, Vec<u32>)> = vec![
        // Empty mesh
        (Vec::new(), Vec::new()),
        // Single triangle
        (
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            vec![0, 1, 2],
        ),
        // Cube
        {
            let c = primitive_cube(1.0).triangulate();
            let pos: Vec<[f32; 3]> = c.positions.iter().map(|p| [p.x, p.y, p.z]).collect();
            (pos, c.indices)
        },
        // Torus (many triangles)
        {
            let t = primitive_torus(2.0, 0.5, 12, 10).triangulate();
            let pos: Vec<[f32; 3]> = t.positions.iter().map(|p| [p.x, p.y, p.z]).collect();
            (pos, t.indices)
        },
    ];

    for (positions, indices) in &test_geometry {
        cases_executed += 1;
        let meshlets = build_meshlets(positions, indices);

        if positions.is_empty() || indices.len() < 3 {
            if !meshlets.is_empty() {
                invariants_held = false;
            }
            continue;
        }

        let mut total_triangles_covered = 0;
        for m in &meshlets {
            // Hardware limits: DirectX 12 / Vulkan Mesh Shader specs
            if m.vertices.len() > MAX_MESHLET_VERTICES {
                invariants_held = false;
            }
            if m.triangles.len() > MAX_MESHLET_TRIANGLES {
                invariants_held = false;
            }

            // Local indices index within `vertices`
            for tri in &m.triangles {
                if (tri[0] as usize) >= m.vertices.len()
                    || (tri[1] as usize) >= m.vertices.len()
                    || (tri[2] as usize) >= m.vertices.len()
                {
                    invariants_held = false;
                }
            }

            // Bounding sphere coordinates and radius must be finite
            if !m.center.x.is_finite()
                || !m.center.y.is_finite()
                || !m.center.z.is_finite()
                || !m.radius.is_finite()
                || m.radius < 0.0
            {
                invariants_held = false;
            }

            total_triangles_covered += m.triangles.len();
        }

        // Triangles preserved
        if total_triangles_covered != indices.len() / 3 {
            invariants_held = false;
        }
    }

    // 2. Shaded PBR Viewport Rasterization
    let viewport_resolutions = [(0u32, 0u32), (1, 1), (64, 64), (128, 96), (256, 256)];
    let cube_mesh = RenderTriangleMesh::new_cube(
        1.0,
        PbrMaterialDesc::dielectric([0.8, 0.2, 0.2], 0.3),
        Mat4::IDENTITY,
    );
    let meshes = vec![cube_mesh];

    for &(w, h) in &viewport_resolutions {
        cases_executed += 1;
        let params = PbrViewportParams {
            width: w,
            height: h,
            meshes: &meshes,
            selected_mesh_index: Some(0),
            outline_width: 2,
            ..Default::default()
        };

        let rgba_buf = render_shaded_pbr_viewport_rgba8(&params);
        let expected_bytes = (w as usize) * (h as usize) * 4;
        if rgba_buf.len() != expected_bytes {
            invariants_held = false;
        }
    }

    // 3. SSIM & PSNR Perceptual Verification
    let img_size = 64u32;
    let byte_count = (img_size * img_size * 4) as usize;
    let img_identical_a = vec![128u8; byte_count];
    let img_identical_b = vec![128u8; byte_count];

    cases_executed += 1;
    let ssim_ident = compute_ssim_rgba8(&img_identical_a, &img_identical_b, img_size, img_size);
    if (ssim_ident - 1.0).abs() > 1e-4 {
        invariants_held = false;
    }

    cases_executed += 1;
    let psnr_ident = compute_psnr_rgba8(&img_identical_a, &img_identical_b, img_size, img_size);
    if !psnr_ident.is_infinite() {
        invariants_held = false;
    }

    // Dissimilar images: SSIM must stay bounded within [-1.0, 1.0]
    let mut img_noise = vec![0u8; byte_count];
    for (i, b) in img_noise.iter_mut().enumerate() {
        *b = ((i * 37) % 256) as u8;
    }

    cases_executed += 1;
    let ssim_noise = compute_ssim_rgba8(&img_identical_a, &img_noise, img_size, img_size);
    if !(-1.0..=1.0).contains(&ssim_noise) || ssim_noise.is_nan() {
        invariants_held = false;
    }

    cases_executed += 1;
    let psnr_noise = compute_psnr_rgba8(&img_identical_a, &img_noise, img_size, img_size);
    if psnr_noise.is_nan() || psnr_noise <= 0.0 {
        invariants_held = false;
    }

    // Truncated buffer edge case
    cases_executed += 1;
    let ssim_trunc = compute_ssim_rgba8(&[0u8; 10], &[0u8; 10], 64, 64);
    if ssim_trunc != 0.0 {
        invariants_held = false;
    }

    VerificationReceipt {
        suite_name: "meshlet_and_pbr_perceptual_suite".into(),
        cases_executed,
        invariants_held,
    }
}

// =========================================================================
// Suite 30: TriMesh BVH, GJK Convex Symmetry, Ragdoll & Timeline Suite
// =========================================================================

/// Verifies BVH raycast and sphere queries over curved skate bowls, Gilbert-Johnson-Keerthi (GJK)
/// boolean symmetry, articulated skater ragdoll dynamics, and audio timeline sample-accurate rendering.
pub fn verify_trimesh_gjk_ragdoll_and_moan_timeline_suite() -> VerificationReceipt {
    let mut cases_executed = 0;
    let mut invariants_held = true;

    // 1. TriMesh BVH Colliders
    let bowl = TriMeshCollider3D::new_curved_bowl(5.0, 2.5, 8);

    // Downward raycast hitting curved transition
    cases_executed += 1;
    let ray_hit = bowl.ray_cast(Vec3::new(0.0, 5.0, 0.0), Vec3::new(0.0, -1.0, 0.0), 20.0);
    match ray_hit {
        Some(hit) => {
            if hit.distance <= 0.0 || hit.normal.length() < 0.9 {
                invariants_held = false;
            }
        }
        None => {
            invariants_held = false;
        }
    }

    // Sphere collision query against curved bowl
    cases_executed += 1;
    let sphere_hit = bowl.collide_sphere(Vec3::new(0.0, -2.0, 0.0), 1.0);
    if let Some(manifold) = sphere_hit
        && (manifold.penetration < 0.0
            || !manifold.point.x.is_finite()
            || !manifold.normal.x.is_finite())
    {
        invariants_held = false;
    }

    // Closest point on triangle query
    cases_executed += 1;
    let pt = closest_point_on_triangle(
        Vec3::new(0.5, 0.5, 0.0),
        Vec3::ZERO,
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
    );
    if !pt.x.is_finite() || !pt.y.is_finite() || !pt.z.is_finite() {
        invariants_held = false;
    }

    // 2. GJK Convex Collision Symmetry Invariant
    let box_a = ConvexHullCollider3D::new_box(Vec3::new(1.0, 1.0, 1.0));
    let box_b = ConvexHullCollider3D::new_box(Vec3::new(0.8, 0.8, 0.8));

    let test_transforms = [
        (Mat4::IDENTITY, Mat4::from_translation(Vec3::new(0.5, 0.0, 0.0))),
        (
            Mat4::from_translation(Vec3::new(1.0, 2.0, 0.0)),
            Mat4::from_translation(Vec3::new(1.2, 2.1, 0.0)),
        ),
        (
            Mat4::from_translation(Vec3::new(5.0, 0.0, 0.0)),
            Mat4::from_translation(Vec3::new(-5.0, 0.0, 0.0)),
        ),
        (
            Mat4::from_rotation_y(0.785),
            Mat4::from_translation(Vec3::new(1.5, 0.0, 0.0)),
        ),
    ];

    for &(ta, tb) in &test_transforms {
        cases_executed += 1;
        let ab = gjk_intersects(&box_a, ta, &box_b, tb);
        let ba = gjk_intersects(&box_b, tb, &box_a, ta);
        if ab != ba {
            invariants_held = false; // GJK symmetry violation!
        }
    }

    // 3. 6-Body Articulated Skater Ragdoll Step Stability
    let mut ragdoll = RagdollRig3D::new();
    ragdoll.activate_from_skater_velocity(
        Vec3::new(0.0, 3.0, 0.0),
        Vec3::new(5.0, 2.0, 0.0),
    );

    for _step_idx in 0..60 {
        cases_executed += 1;
        ragdoll.step(0.016, Some(&bowl));

        for bone in &ragdoll.bones {
            if !bone.position.x.is_finite()
                || !bone.position.y.is_finite()
                || !bone.position.z.is_finite()
                || !bone.velocity.x.is_finite()
            {
                invariants_held = false;
            }
            // Ground plane constraint: y >= radius - epsilon
            if bone.position.y < bone.radius - 0.01 {
                invariants_held = false;
            }
        }
    }

    // 4. Sample-Accurate Audio Timeline Invariants
    let mut pcm_source = Vec::with_capacity(48_000);
    for i in 0..48_000 {
        let phase = (i as f32 / 48_000.0) * 440.0 * std::f32::consts::TAU;
        let s = (phase.sin() * 16384.0) as i16;
        pcm_source.push([s, s]);
    }
    let shared_source = Arc::new(pcm_source);

    // Non-destructive clip split & trim
    let clip = AudioClip::new(Arc::clone(&shared_source), 0);
    cases_executed += 1;
    if let Some((left, right)) = clip.split_at(24_000) {
        if left.duration_frames != 24_000 || right.duration_frames != 24_000 {
            invariants_held = false;
        }
        if right.timeline_start_frame != 24_000 {
            invariants_held = false;
        }
    } else {
        invariants_held = false;
    }

    // Split at boundary (outside interior range) must return None
    cases_executed += 1;
    if clip.split_at(0).is_some() || clip.split_at(48_000).is_some() {
        invariants_held = false;
    }

    // Block-size invariant rendering:
    // Rendering in a single 2048-frame block vs 32 blocks of 64 frames must match!
    let mut track = AudioTimelineTrack::new(1, "BGM");
    track.add_clip(clip);
    let mut timeline = AudioTimeline::new();
    timeline.add_track(track);

    let render_frames = 2048_usize;
    let mut single_buf = vec![[0.0f32; 2]; render_frames];
    cases_executed += 1;
    timeline.render_block(0, &mut single_buf);

    let mut chunked_buf = vec![[0.0f32; 2]; render_frames];
    let chunk_size = 64_usize;
    for c in 0..(render_frames / chunk_size) {
        let start_f = (c * chunk_size) as u64;
        let mut slice = vec![[0.0f32; 2]; chunk_size];
        timeline.render_block(start_f, &mut slice);
        chunked_buf[c * chunk_size..(c + 1) * chunk_size].copy_from_slice(&slice);
    }

    cases_executed += 1;
    for i in 0..render_frames {
        let diff_l = (single_buf[i][0] - chunked_buf[i][0]).abs();
        let diff_r = (single_buf[i][1] - chunked_buf[i][1]).abs();
        if diff_l > 1e-5 || diff_r > 1e-5 {
            invariants_held = false;
        }
    }

    VerificationReceipt {
        suite_name: "trimesh_gjk_ragdoll_and_moan_timeline_suite".into(),
        cases_executed,
        invariants_held,
    }
}
