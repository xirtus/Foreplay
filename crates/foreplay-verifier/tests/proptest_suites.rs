//! Independent Adversarial Verifier — Comprehensive Proptest & Fuzzing Suites (Wave 2).
//!
//! Validates:
//! 1. 10,000-Step Stateful Undo/Redo Marathon across all 10 `EditorCommand` variants
//! 2. Cycle detection attacks (self-parenting and multi-hop ancestor cycles)
//! 3. Atomic `Batch` rollback verification
//! 4. Bevy 0.20 `SceneTree` relational & rebuild equivalence (bijectivity, `ChildOf`, `Children`, zero orphans)
//! 5. Deterministic `.fscn` serialization stability & lossless round-trip
//! 6. Hostile parser & wire codec fuzzing (FSCN, TSCN, GDScript, ReplayNet, WAV, Pak, StickCalibration)
//! 7. `FrameDifferentialVerdict` adversarial IEEE-754 float attacks
//! 8. `PhysicsServer3D::hotswap_backend` 1,000-swap thrashing

use foreplay_core::SceneTree;
use foreplay_harness::HarnessSession;
use foreplay_ir::{EditorCommand, IrError, IrNode, IrScene, NodeId};
use foreplay_platform::StickCalibration;
use foreplay_porthost::FrameDifferentialVerdict;
use foreplay_verifier::adversarial::{
    run_all_adversarial_suites, verify_frame_differential_verdict_adversarial,
    verify_physics_hotswap_thrash,
};
use foreplay_verifier::parser_fuzzers::{
    verify_archive_pak_hostile_suite, verify_fscn_hostile_suite, verify_gdscript_hostile_suite,
    verify_replay_frame_codec_suite, verify_stick_calibration_suite, verify_tscn_hostile_suite,
    verify_wav_codec_hostile_suite,
};
use foreplay_export::{FpckArchive, FpckError};
use foreplay_core::ui_layout::{
    AnchorPreset, ContainerKind, ControlLayoutNode, ControlRect, solve_control_hierarchy,
};
use foreplay_core::animation::{
    AnimationClip, AnimationNodeStateMachine, AnimationPlayer, AnimationTrack, InterpolationMode,
    Keyframe, StateTransition,
};
use foreplay_core::navigation::AStarGrid2D;
use foreplay_physics::{CharacterBody3D, Collider3D, PhysicsWorldState};
use foreplay_audio::{AudioEffect, AudioEffectLimiter};
use foreplay_verifier::subsystem_fuzzers::{
    verify_animation_state_machine_fuzzer_suite, verify_navigation_and_autotile_fuzzer_suite,
    verify_physics_and_audio_dsp_fuzzer_suite, verify_ui_layout_container_solver_suite,
};
use foreplay_verifier::v2_worldclass_fuzzers::{
    verify_meshlet_and_pbr_perceptual_suite,
    verify_polyamour_blend_io_and_polyops_hostile_suite,
    verify_polyamour_topology_and_modifiers_suite,
    verify_threejs_compiler_and_bidirectional_roundtrip_suite,
    verify_trimesh_gjk_ragdoll_and_moan_timeline_suite,
    verify_ue6_speculative_rollback_determinism_suite,
};
use foreplay_verifier::v1_godot_parity_fuzzers::{
    verify_audio_ringbuffer_and_input_deadzone_suite,
    verify_bbcode_and_terrain_autotiler_suite,
    verify_classdb_hierarchy_invariants_suite,
    verify_fpckexec_binary_trailer_adversarial_suite,
    verify_gdextension_c_abi_boundary_suite,
    verify_gdextension_manifest_fuzzing_suite,
    verify_gdscript_coroutine_resumption_suite,
    verify_gdscript_match_and_typed_collections_suite,
    verify_gdlsp_analyzer_bounds_suite,
    verify_hostile_glb_fuzzing_suite,
    verify_rapier_port_hotswap_and_controller_proptest_suite,
    verify_tres_manifest_fuzzing_suite,
};
use foreplay_verifier::v1_1_polyamour_nextgen_fuzzers::{
    verify_gpu_bvh_pathtracer_and_radiance_cascades_suite,
    verify_player_live_polyamour_and_threejs_parametric_suite,
    verify_plnk_livelink_wire_protocol_fuzz_suite,
    verify_polyops_v2_parser_and_replay_fuzz_suite,
    verify_rust_hot_reload_schema_migration_suite,
    verify_sculpt_symmetry_and_heat_bone_weights_suite,
    verify_semantic_grind_splines_and_uv2_atlas_suite,
    verify_sexlife_moan_and_splicy_edl_fuzz_suite,
    verify_subobject_raycast_and_proportional_edit_suite,
    verify_wasm_custom_section_adversarial_suite,
};
use foreplay_verifier::wave2b_fuzzers::{
    verify_1000_frame_physics_differential_gate, verify_fpck_archive_adversarial_suite,
    verify_gdshader_compiler_adversarial_suite, verify_godot4_200_scene_conformance_gate,
    verify_project_exporter_bundle_invariants_suite, verify_script_sandbox_termination_suite,
    verify_tscn_bidirectional_roundtrip_suite,
};
use foreplay_verifier::scene_tree_fuzzer::{
    arb_tree_scene, verify_bijective_ecs_entities, verify_fscn_emission_stability,
    verify_hierarchy_relations, verify_rebuild_world_equivalence,
};
use foreplay_verifier::undo_redo_fuzzer::{
    arb_fuzz_operation, make_test_node, run_stateful_fuzz_sequence, scenes_bit_identical,
    verify_10k_step_undo_redo_marathon,
};
use proptest::prelude::*;
use std::collections::BTreeMap;

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 200,
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    /// Deliverable 1: Stateful proptest Undo/Redo State-Machine Fuzzer
    /// Tests arbitrary sequences of up to 150 operations across all 10 `EditorCommand` variants:
    /// a) undo() N times followed by redo() N times produces exact bit-identical IrScene
    /// b) Unwinding entire Undo stack always restores initial IrScene
    /// c) Executing new command after undo() deterministically clears Redo stack
    /// d) Invalid commands fail cleanly with Err(IrError) and never mutate state or pollute stacks
    #[test]
    fn test_stateful_undo_redo_state_machine_proptest(
        operations in prop::collection::vec(arb_fuzz_operation(), 1..=150)
    ) {
        let mut initial_scene = IrScene::new("ProptestHarnessScene");
        let root = make_test_node(0, 1, "RootNode", "Node3D", None);
        initial_scene.nodes.insert(root.id, root);

        let result = run_stateful_fuzz_sequence(initial_scene, &operations);
        prop_assert!(result.is_ok(), "Fuzz sequence failed: {:?}", result.err());
    }

    /// Deliverable 2: Bevy 0.20 SceneTree Relational & Rebuild Equivalence Fuzzer
    /// Generates arbitrary valid DAG/tree IrScene hierarchies (forests, deep chains,
    /// high fan-out trees, signal connections) and verifies:
    /// a) SceneTree::from_ir spawns the exact bijective set of bevy_ecs entities
    /// b) Every IrNode with parent: Some(p) produces ChildOf(parent_ent) and Children on parent contains child_ent
    /// c) rebuild_world_from_ir() after arbitrary mutations produces an isomorphic ECS hierarchy with zero orphaned entities
    #[test]
    fn test_scene_tree_relational_and_rebuild_proptest(
        scene in arb_tree_scene(),
        mutation_node_name in "[A-Z][a-zA-Z0-9_]{1,12}"
    ) {
        let mut tree = SceneTree::from_ir(scene.clone());

        // a) Bijective set of bevy_ecs entities
        let bijectivity_result = verify_bijective_ecs_entities(&mut tree, &scene);
        prop_assert!(bijectivity_result.is_ok(), "Bijectivity check failed: {:?}", bijectivity_result.err());

        // b) ChildOf and Children hierarchy relations
        let hierarchy_result = verify_hierarchy_relations(&tree, &scene);
        prop_assert!(hierarchy_result.is_ok(), "Hierarchy check failed: {:?}", hierarchy_result.err());

        // c) Rebuild world from IR after arbitrary mutation (zero orphaned entities)
        let mut mutated = scene.clone();
        let new_id = NodeId::new(999, 1).expect("valid ID");
        mutated.nodes.insert(
            new_id,
            IrNode {
                id: new_id,
                name: mutation_node_name,
                class_name: "Area3D".into(),
                parent: Some(NodeId::new(0, 1).expect("root")),
                instance_scene: None,
                properties: BTreeMap::new(),
                groups: vec!["rebuild_mutation".into()],
            },
        );

        let rebuild_result = verify_rebuild_world_equivalence(&mut tree, mutated);
        prop_assert!(rebuild_result.is_ok(), "Rebuild equivalence failed: {:?}", rebuild_result.err());
    }

    /// Deliverable 2d: Deterministic .fscn output stability
    #[test]
    fn test_fscn_output_determinism_proptest(
        scene in arb_tree_scene()
    ) {
        let stability_result = verify_fscn_emission_stability(&scene);
        prop_assert!(stability_result.is_ok(), "FSCN emission stability failed: {:?}", stability_result.err());
    }

    /// Deliverable 3: Adversarial Float Attacks on FrameDifferentialVerdict
    #[test]
    fn test_frame_differential_verdict_adversarial_floats_proptest(
        pos_err in prop_oneof![
            any::<f32>(),
            Just(f32::NAN),
            Just(f32::INFINITY),
            Just(f32::NEG_INFINITY),
            Just(0.0),
            Just(-0.0),
            Just(f32::from_bits(1)),
        ],
        vel_err in prop_oneof![
            any::<f32>(),
            Just(f32::NAN),
            Just(f32::INFINITY),
            Just(f32::NEG_INFINITY),
            Just(0.0),
            Just(-0.0),
            Just(f32::from_bits(1)),
        ],
        epsilon in prop_oneof![
            any::<f32>(),
            Just(f32::NAN),
            Just(f32::INFINITY),
            Just(f32::NEG_INFINITY),
            Just(0.001),
            Just(f32::from_bits(1)),
        ],
        frame in any::<u64>()
    ) {
        let verdict = FrameDifferentialVerdict::evaluate(frame, pos_err, vel_err, epsilon);
        prop_assert_eq!(verdict.frame_index, frame);

        if pos_err.is_nan() || vel_err.is_nan() || epsilon.is_nan() {
            prop_assert!(
                !verdict.within_epsilon,
                "NaN float parameter MUST NEVER produce within_epsilon == true!"
            );
        } else if pos_err > epsilon || vel_err > epsilon {
            prop_assert!(
                !verdict.within_epsilon,
                "Error exceeding epsilon MUST produce within_epsilon == false!"
            );
        } else {
            prop_assert!(
                verdict.within_epsilon,
                "Valid errors <= epsilon MUST produce within_epsilon == true!"
            );
        }
    }

    /// Deliverable 2g: StickCalibration radial transfer function proptest
    /// Proves output magnitude is ALWAYS in 0.0..=1.0 and NEVER NaN for ANY inputs.
    #[test]
    fn test_stick_calibration_radial_proptest(
        x in prop_oneof![
            any::<f32>(),
            Just(f32::NAN),
            Just(f32::INFINITY),
            Just(f32::NEG_INFINITY),
            Just(0.0),
            Just(-0.0),
            Just(f32::from_bits(1)),
        ],
        y in prop_oneof![
            any::<f32>(),
            Just(f32::NAN),
            Just(f32::INFINITY),
            Just(f32::NEG_INFINITY),
            Just(0.0),
            Just(-0.0),
            Just(f32::from_bits(1)),
        ]
    ) {
        let calib = StickCalibration::default();
        let [out_x, out_y] = calib.apply_radial([x, y]);

        prop_assert!(!out_x.is_nan(), "out_x was NaN for raw: [{x}, {y}]");
        prop_assert!(!out_y.is_nan(), "out_y was NaN for raw: [{x}, {y}]");
        prop_assert!(out_x.is_finite(), "out_x was non-finite for raw: [{x}, {y}]");
        prop_assert!(out_y.is_finite(), "out_y was non-finite for raw: [{x}, {y}]");

        let mag = (out_x * out_x + out_y * out_y).sqrt();
        prop_assert!(mag <= 1.000001, "Magnitude {mag} exceeded 1.0 for raw: [{x}, {y}]");
        prop_assert!(mag >= 0.0, "Magnitude {mag} was negative for raw: [{x}, {y}]");
    }

    #[test]
    fn test_fpck_archive_proptest_roundtrip_and_bitflip(
        ref files in prop::collection::btree_map("[a-z0-9_]{1,8}\\.[a-z]{1,4}", prop::collection::vec(any::<u8>(), 0..64), 1..6)
    ) {
        let packed = FpckArchive::pack_bytes(files).expect("pack must succeed on valid filenames");
        let unpacked = FpckArchive::unpack_bytes(&packed).expect("unpack must succeed on valid archive");
        prop_assert_eq!(&unpacked, files);

        // Single-bit flip mutation anywhere in payload bytes MUST fail FpckArchive::unpack with ChecksumMismatch!
        let toc_offset = u64::from_le_bytes(packed[12..20].try_into().unwrap()) as usize;
        if toc_offset > 28 {
            let payload_len = toc_offset - 28;
            let flip_offset = 28 + payload_len / 2;
            let mut corrupted = packed.clone();
            corrupted[flip_offset] ^= 0x01;
            let res = FpckArchive::unpack_bytes(&corrupted);
            prop_assert!(matches!(res, Err(FpckError::ChecksumMismatch { .. })), "Payload bit-flip must fail with ChecksumMismatch, got: {res:?}");
        }
    }

    #[test]
    fn test_gdshader_compiler_fuzz_proptest(ref raw_source in "\\PC{0,256}") {
        let _ = foreplay_render::compile_gdshader_to_wgsl(raw_source);
    }

    /// Wave 3A Deliverable 1: UI Layout Solver proptest
    #[test]
    fn test_ui_layout_control_rects_finite_proptest(
        preset_idx in 0usize..16,
        vp_w in 0.0f32..7680.0,
        vp_h in 0.0f32..4320.0,
        child_w in 0.0f32..500.0,
        child_h in 0.0f32..500.0,
    ) {
        let presets = [
            AnchorPreset::TopLeft, AnchorPreset::TopRight, AnchorPreset::BottomLeft, AnchorPreset::BottomRight,
            AnchorPreset::CenterLeft, AnchorPreset::CenterTop, AnchorPreset::CenterRight, AnchorPreset::CenterBottom,
            AnchorPreset::Center, AnchorPreset::LeftWide, AnchorPreset::TopWide, AnchorPreset::RightWide,
            AnchorPreset::BottomWide, AnchorPreset::VCenterWide, AnchorPreset::HCenterWide, AnchorPreset::FullRect,
        ];
        let preset = presets[preset_idx % 16];
        let root_id = NodeId::new(1, 1).unwrap();
        let child_id = NodeId::new(2, 1).unwrap();

        let mut root = ControlLayoutNode::new(root_id, ContainerKind::MarginContainer { margins: [5.0, 5.0, 5.0, 5.0] })
            .with_preset(preset);
        let child = ControlLayoutNode::new(child_id, ContainerKind::Control)
            .with_min_size(child_w, child_h);
        root.add_child(child);

        let vp = ControlRect::new(0.0, 0.0, vp_w, vp_h);
        let solved = solve_control_hierarchy(vp, &root);

        for (_, r) in solved {
            prop_assert!(r.x.is_finite(), "x not finite: {r:?}");
            prop_assert!(r.y.is_finite(), "y not finite: {r:?}");
            prop_assert!(r.width.is_finite(), "width not finite: {r:?}");
            prop_assert!(r.height.is_finite(), "height not finite: {r:?}");
            prop_assert!(r.width >= 0.0, "width negative: {r:?}");
            prop_assert!(r.height >= 0.0, "height negative: {r:?}");
        }
    }

    /// Wave 3A Deliverable 2: Animation interpolation and State machine travel proptest
    #[test]
    fn test_animation_interpolation_and_bfs_proptest(
        dt in -1000.0f32..1000.0,
        t in 0.0f32..10.0,
        mode_idx in 0usize..3,
    ) {
        let modes = [InterpolationMode::Nearest, InterpolationMode::Linear, InterpolationMode::Cubic];
        let mode = modes[mode_idx % 3];

        let mut clip = AnimationClip::new("prop_clip", 5.0);
        clip.add_track(AnimationTrack::Position3DTrack {
            node_path: "Target".into(),
            keyframes: vec![
                Keyframe::new(0.0, [0.0, 0.0, 0.0]),
                Keyframe::new(2.5, [10.0, 5.0, 2.0]),
                Keyframe::new(5.0, [0.0, 10.0, 0.0]),
            ],
            interpolation: mode,
        });

        let mut player = AnimationPlayer::new();
        player.add_clip(clip);
        player.play("prop_clip");
        player.playback_position = t;
        let _ = player.advance(dt);

        // Cyclic state machine travel
        let mut sm = AnimationNodeStateMachine::new();
        sm.add_state("A", "c1");
        sm.add_state("B", "c2");
        sm.add_state("C", "c3");
        sm.add_transition(StateTransition::new("A", "B", 0.1));
        sm.add_transition(StateTransition::new("B", "C", 0.1));
        sm.add_transition(StateTransition::new("C", "A", 0.1));

        let path = sm.travel("C");
        prop_assert!(path.is_ok(), "travel must succeed on cyclic graph: {path:?}");
    }

    /// Wave 3A Deliverable 3: AStarGrid2D pathfinding proptest
    #[test]
    fn test_astar_grid_2d_pathfinding_proptest(
        sx in 0i32..16,
        sy in 0i32..16,
        tx in 0i32..16,
        ty in 0i32..16,
    ) {
        let mut grid = AStarGrid2D::new([0, 0], [15, 15], [32.0, 32.0]);
        // Put a solid obstacle at (8, 8)
        grid.set_point_solid([8, 8], true);

        if let Some(path) = grid.find_path([sx, sy], [tx, ty]) {
            prop_assert_eq!(path.first(), Some(&[sx, sy]));
            prop_assert_eq!(path.last(), Some(&[tx, ty]));
            for p in &path {
                prop_assert!(grid.is_in_bounds(*p));
                prop_assert!(!grid.is_point_solid(*p));
            }
        }
    }

    /// Wave 3A Deliverable 4: CharacterBody and DSP Limiter proptest
    #[test]
    fn test_character_body_and_dsp_finite_proptest(
        vx in -100.0f32..100.0,
        vy in -100.0f32..100.0,
        vz in -100.0f32..100.0,
        amp_l in -50.0f32..50.0,
        amp_r in -50.0f32..50.0,
    ) {
        let world = PhysicsWorldState::new();
        let id = NodeId::new(99, 1).unwrap();
        let mut body = CharacterBody3D::new(id, bevy_math::Vec3::new(0.0, 1.0, 0.0), Collider3D::Sphere { radius: 0.5 });
        body.velocity = bevy_math::Vec3::new(vx, vy, vz);
        let ret_vel = body.move_and_slide(0.016, &world);

        prop_assert!(body.position.is_finite());
        prop_assert!(body.velocity.is_finite());
        prop_assert!(ret_vel.is_finite());

        let mut limiter = AudioEffectLimiter::new(0.0);
        let (out_l, out_r) = limiter.process_stereo(amp_l, amp_r);
        prop_assert!(out_l.is_finite());
        prop_assert!(out_r.is_finite());
    }

    /// Wave 3B Proptest: Polyamour primitive procedural triangulation invariants
    #[test]
    fn test_polyamour_cube_triangulation_proptest(size in 0.01f32..100.0) {
        let cube = foreplay_polyamour::primitives::primitive_cube(size);
        let tri = cube.triangulate();
        prop_assert_eq!(tri.indices.len() % 3, 0);
        for &idx in &tri.indices {
            prop_assert!((idx as usize) < tri.positions.len());
        }
        for p in &tri.positions {
            prop_assert!(p.x.is_finite() && p.y.is_finite() && p.z.is_finite());
        }
    }

    /// Wave 3B Proptest: Three.js JSON feel spec roundtrip invariance
    #[test]
    fn test_threejs_scene_feel_proptest(pop_force in 1.0f64..1000.0) {
        let mut scene = IrScene::new("PropScene");
        scene.feel_overrides.insert("skate.pop_force".into(), pop_force);
        let json_str = foreplay_threejs::export_ir_to_threejs_json(&scene);
        let recovered = foreplay_threejs::import_threejs_json_to_ir(&json_str).unwrap();
        let val = recovered.feel_overrides.get("skate.pop_force").copied().unwrap();
        prop_assert!((val - pop_force).abs() < 1e-4);
    }

    /// Wave 3B Proptest: Micro-Nanite meshlet partition bounding invariants
    #[test]
    fn test_meshlet_bounding_proptest(dim in 0.1f32..50.0) {
        let cube = foreplay_polyamour::primitives::primitive_cube(dim).triangulate();
        let positions: Vec<[f32; 3]> = cube.positions.iter().map(|p| [p.x, p.y, p.z]).collect();
        let meshlets = foreplay_render::meshlet::build_meshlets(&positions, &cube.indices);
        for m in &meshlets {
            prop_assert!(m.vertices.len() <= foreplay_render::meshlet::MAX_MESHLET_VERTICES);
            prop_assert!(m.triangles.len() <= foreplay_render::meshlet::MAX_MESHLET_TRIANGLES);
            prop_assert!(m.radius >= 0.0 && m.radius.is_finite());
            prop_assert!(m.center.x.is_finite() && m.center.y.is_finite() && m.center.z.is_finite());
        }
    }
}

/// Wave 2 Exit Gate: 10,000-Step Stateful Undo/Redo Marathon
#[test]
fn test_10k_step_undo_redo_marathon() {
    let receipt = verify_10k_step_undo_redo_marathon();
    assert_eq!(receipt.cases_executed, 10_000);
    assert!(
        receipt.invariants_held,
        "10,000-step Undo/Redo marathon failed invariants!"
    );
}

/// Attack: ReparentNode cycle detection (self-parenting and multi-hop ancestor cycles)
#[test]
fn test_reparent_hierarchy_cycle_rejection() {
    let mut scene = IrScene::new("CycleTestScene");
    let id_a = NodeId::new(1, 1).unwrap();
    let id_b = NodeId::new(2, 1).unwrap();
    let id_c = NodeId::new(3, 1).unwrap();

    let node_a = make_test_node(1, 1, "NodeA", "Node3D", None);
    let node_b = make_test_node(2, 1, "NodeB", "Node3D", Some(id_a));
    let node_c = make_test_node(3, 1, "NodeC", "Node3D", Some(id_b));

    scene.nodes.insert(id_a, node_a);
    scene.nodes.insert(id_b, node_b);
    scene.nodes.insert(id_c, node_c);

    let mut harness = HarnessSession::new(scene.clone());

    // 1. Attack: Self-parenting A -> Some(A)
    let self_parent_res = harness.execute(EditorCommand::ReparentNode {
        node_id: id_a,
        old_parent: None,
        new_parent: Some(id_a),
    });
    assert!(matches!(self_parent_res, Err(IrError::InvalidHierarchyCycle(id)) if id == id_a));
    assert!(scenes_bit_identical(&harness.tree.active_scene, &scene));

    // 2. Attack: Multi-hop ancestor cycle A -> Some(C) (where C is a descendant of A via A -> B -> C)
    let multihop_res = harness.execute(EditorCommand::ReparentNode {
        node_id: id_a,
        old_parent: None,
        new_parent: Some(id_c),
    });
    assert!(matches!(multihop_res, Err(IrError::InvalidHierarchyCycle(id)) if id == id_a));
    assert!(scenes_bit_identical(&harness.tree.active_scene, &scene));
}

/// Attack: Batch atomic rollback (subcommand failure rolls back entire batch cleanly)
#[test]
fn test_batch_atomic_rollback() {
    let mut scene = IrScene::new("BatchRollbackScene");
    let root = make_test_node(0, 1, "Root", "Node3D", None);
    scene.nodes.insert(root.id, root);

    let mut harness = HarnessSession::new(scene.clone());
    let scene_before = harness.tree.active_scene.clone();

    let sub1 = EditorCommand::TuneFeelSpec {
        key: "ollie_impulse".into(),
        old_value: None,
        new_value: Some(10.5),
    };
    let sub2 = EditorCommand::RenameNode {
        node_id: NodeId::new(0, 1).unwrap(),
        old_name: "Root".into(),
        new_name: "RenamedRoot".into(),
    };
    // Subcommand 3 fails: NodeAlreadyExists duplicate insert
    let sub3_failing = EditorCommand::InsertNode {
        node: make_test_node(0, 1, "DuplicateRoot", "Node3D", None),
    };

    let batch = EditorCommand::Batch {
        label: "AtomicRollbackTest".into(),
        commands: vec![sub1, sub2, sub3_failing],
    };

    let res = harness.execute(batch);
    assert!(matches!(res, Err(IrError::NodeAlreadyExists(_))));

    // Verify scene is 100% bit-identical to before (no partial state committed!)
    assert!(scenes_bit_identical(&harness.tree.active_scene, &scene_before));

    // Verify Undo stack was not polluted
    assert!(!harness.undo().unwrap());
}

/// Unit test verifying all hostile parser fuzzing suites
#[test]
fn test_hostile_parser_fuzzing() {
    let fscn_rcpt = verify_fscn_hostile_suite();
    assert!(fscn_rcpt.invariants_held);
    assert!(fscn_rcpt.cases_executed > 0);

    let tscn_rcpt = verify_tscn_hostile_suite();
    assert!(tscn_rcpt.invariants_held);
    assert!(tscn_rcpt.cases_executed > 0);

    let gd_rcpt = verify_gdscript_hostile_suite();
    assert!(gd_rcpt.invariants_held);
    assert!(gd_rcpt.cases_executed > 0);

    let replay_rcpt = verify_replay_frame_codec_suite();
    assert!(replay_rcpt.invariants_held);
    assert!(replay_rcpt.cases_executed > 0);

    let wav_rcpt = verify_wav_codec_hostile_suite();
    assert!(wav_rcpt.invariants_held);
    assert!(wav_rcpt.cases_executed > 0);

    let pak_rcpt = verify_archive_pak_hostile_suite();
    assert!(pak_rcpt.invariants_held);
    assert!(pak_rcpt.cases_executed > 0);

    let stick_rcpt = verify_stick_calibration_suite();
    assert!(stick_rcpt.invariants_held);
    assert!(stick_rcpt.cases_executed > 0);
}

/// Unit test verifying 1,000 rapid A/B backend hotswaps on PhysicsServer3D.
#[test]
fn test_physics_server_3d_hotswap_1000_swaps() {
    let receipt = verify_physics_hotswap_thrash(1_000);
    assert_eq!(receipt.cases_executed, 1_000);
    assert!(
        receipt.invariants_held,
        "PhysicsServer3D 1,000-swap thrash failed invariants!"
    );
}

/// Unit test verifying table-driven and boundary sweep on FrameDifferentialVerdict.
#[test]
fn test_frame_differential_verdict_adversarial_oracle() {
    let receipt = verify_frame_differential_verdict_adversarial();
    assert!(receipt.cases_executed >= 2_000);
    assert!(
        receipt.invariants_held,
        "FrameDifferentialVerdict failed adversarial float oracle!"
    );
}

/// Unit test verifying all 52 adversarial suites via public run_all_adversarial_suites() API.
#[test]
fn test_run_all_adversarial_suites_receipts() {
    let receipts = run_all_adversarial_suites();
    assert_eq!(receipts.len(), 52);
    for r in &receipts {
        assert!(
            r.invariants_held,
            "Suite '{}' failed invariant checks!",
            r.suite_name
        );
        assert!(
            r.cases_executed > 0,
            "Suite '{}' executed 0 cases!",
            r.suite_name
        );
    }
}

/// Unit test verifying Wave 2B .fpck archive corruption, bit-flip, and path-traversal fuzzer.
#[test]
fn test_fpck_archive_adversarial_suite() {
    let r = verify_fpck_archive_adversarial_suite();
    assert!(r.invariants_held, "fpck archive suite failed invariants!");
    assert!(r.cases_executed > 0);
}

/// Unit test verifying Wave 2B .gdshader -> WGSL compiler hostile fuzzer.
#[test]
fn test_gdshader_compiler_adversarial_suite() {
    let r = verify_gdshader_compiler_adversarial_suite();
    assert!(r.invariants_held, "gdshader compiler suite failed invariants!");
    assert!(r.cases_executed > 0);
}

/// Unit test verifying Wave 2B GdVm and QuickJsComponentRunner sandbox termination fuzzer.
#[test]
fn test_script_sandbox_termination_suite() {
    let r = verify_script_sandbox_termination_suite();
    assert!(r.invariants_held, "script sandbox termination suite failed invariants!");
    assert!(r.cases_executed > 0);
}

/// Unit test verifying Wave 2B bidirectional Godot .tscn round-trip property suite.
#[test]
fn test_tscn_bidirectional_roundtrip_suite() {
    let r = verify_tscn_bidirectional_roundtrip_suite();
    assert!(r.invariants_held, "tscn bidirectional roundtrip suite failed invariants!");
    assert!(r.cases_executed > 0);
}

/// Unit test verifying Godot 4 200-scene conformance gate.
#[test]
fn test_godot4_200_scene_conformance_gate() {
    let r = verify_godot4_200_scene_conformance_gate();
    assert!(r.invariants_held, "200-scene conformance gate failed invariants!");
    assert_eq!(r.cases_executed, 200);
}

/// Unit test verifying PLAN.md Phase 5 1,000-frame physics differential gate.
#[test]
fn test_1000_frame_physics_differential_gate() {
    let r = verify_1000_frame_physics_differential_gate();
    assert!(r.invariants_held, "1,000-frame physics differential gate failed invariants!");
    assert_eq!(r.cases_executed, 1000);
}

/// Unit test verifying 5-platform ProjectExporter release bundle invariants.
#[test]
fn test_project_exporter_bundle_invariants_suite() {
    let r = verify_project_exporter_bundle_invariants_suite();
    assert!(r.invariants_held, "project exporter bundle suite failed invariants!");
    assert!(r.cases_executed > 0);
}

/// Unit test verifying Wave 3A Godot Control UI Layout & Container Solver fuzzer.
#[test]
fn test_ui_layout_container_solver_suite() {
    let r = verify_ui_layout_container_solver_suite();
    assert!(r.invariants_held, "ui layout container solver suite failed invariants!");
    assert!(r.cases_executed > 0);
}

/// Unit test verifying Wave 3A AnimationPlayer & AnimationNodeStateMachine fuzzer.
#[test]
fn test_animation_state_machine_fuzzer_suite() {
    let r = verify_animation_state_machine_fuzzer_suite();
    assert!(r.invariants_held, "animation state machine suite failed invariants!");
    assert!(r.cases_executed > 0);
}

/// Unit test verifying Wave 3A 2D/3D Navigation & TileMapLayer Autotile fuzzer.
#[test]
fn test_navigation_and_autotile_fuzzer_suite() {
    let r = verify_navigation_and_autotile_fuzzer_suite();
    assert!(r.invariants_held, "navigation and autotile suite failed invariants!");
    assert!(r.cases_executed > 0);
}

/// Unit test verifying Wave 3A CharacterBody move_and_slide & 5-Effect Audio DSP fuzzer.
#[test]
fn test_physics_and_audio_dsp_fuzzer_suite() {
    let r = verify_physics_and_audio_dsp_fuzzer_suite();
    assert!(r.invariants_held, "physics and audio dsp suite failed invariants!");
    assert!(r.cases_executed > 0);
}

/// Unit test verifying Wave 3B Polyamour topology, edit operations, and modifiers fuzzer.
#[test]
fn test_polyamour_topology_and_modifiers_suite() {
    let r = verify_polyamour_topology_and_modifiers_suite();
    assert!(r.invariants_held, "polyamour topology suite failed invariants!");
    assert!(r.cases_executed > 0);
}

/// Unit test verifying Wave 3B Polyamour hostile blend headers, foreign asset import & atomic polyops fuzzer.
#[test]
fn test_polyamour_blend_io_and_polyops_hostile_suite() {
    let r = verify_polyamour_blend_io_and_polyops_hostile_suite();
    assert!(r.invariants_held, "polyamour blend io suite failed invariants!");
    assert!(r.cases_executed > 0);
}

/// Unit test verifying Wave 3B Three.js compiler & bidirectional roundtrip suite.
#[test]
fn test_threejs_compiler_and_bidirectional_roundtrip_suite() {
    let r = verify_threejs_compiler_and_bidirectional_roundtrip_suite();
    assert!(r.invariants_held, "threejs compiler suite failed invariants!");
    assert!(r.cases_executed > 0);
}

/// Unit test verifying Wave 3B UE6 speculative rollback determinism & subtree lease suite.
#[test]
fn test_ue6_speculative_rollback_determinism_suite() {
    let r = verify_ue6_speculative_rollback_determinism_suite();
    assert!(r.invariants_held, "ue6 speculative rollback suite failed invariants!");
    assert!(r.cases_executed > 0);
}

/// Unit test verifying Wave 3B Micro-Nanite meshlet & shaded PBR perceptual suite.
#[test]
fn test_meshlet_and_pbr_perceptual_suite() {
    let r = verify_meshlet_and_pbr_perceptual_suite();
    assert!(r.invariants_held, "meshlet and pbr perceptual suite failed invariants!");
    assert!(r.cases_executed > 0);
}

/// Unit test verifying Wave 3B TriMesh BVH, GJK convex symmetry, 6-body ragdoll & moan timeline suite.
#[test]
fn test_trimesh_gjk_ragdoll_and_moan_timeline_suite() {
    let r = verify_trimesh_gjk_ragdoll_and_moan_timeline_suite();
    assert!(r.invariants_held, "trimesh gjk ragdoll timeline suite failed invariants!");
    assert!(r.cases_executed > 0);
}

// ----------------------------------------------------------------------------
// Phase 16 (Track M4-D): Independent Red-Team v1.0.0 Godot-Parity Suites
// ----------------------------------------------------------------------------

#[test]
fn test_phase16_hostile_binary_glb_fuzzing_suite() {
    let r = verify_hostile_glb_fuzzing_suite();
    assert!(r.invariants_held, "hostile glb fuzzing suite failed invariants!");
    assert!(r.cases_executed >= 50);
}

#[test]
fn test_phase16_tres_manifest_fuzzing_suite() {
    let r = verify_tres_manifest_fuzzing_suite();
    assert!(r.invariants_held, "tres manifest fuzzing suite failed invariants!");
    assert!(r.cases_executed >= 20);
}

#[test]
fn test_phase16_gdextension_manifest_fuzzing_suite() {
    let r = verify_gdextension_manifest_fuzzing_suite();
    assert!(r.invariants_held, "gdextension manifest fuzzing suite failed invariants!");
    assert!(r.cases_executed >= 10);
}

#[test]
fn test_phase16_gdscript_coroutine_resumption_suite() {
    let r = verify_gdscript_coroutine_resumption_suite();
    assert!(r.invariants_held, "gdscript coroutine suite failed invariants!");
    assert!(r.cases_executed >= 10);
}

#[test]
fn test_phase16_gdscript_match_and_typed_collections_suite() {
    let r = verify_gdscript_match_and_typed_collections_suite();
    assert!(r.invariants_held, "gdscript match & typed collections suite failed invariants!");
    assert!(r.cases_executed >= 10);
}

#[test]
fn test_phase16_gdlsp_analyzer_bounds_suite() {
    let r = verify_gdlsp_analyzer_bounds_suite();
    assert!(r.invariants_held, "gdlsp analyzer bounds suite failed invariants!");
    assert!(r.cases_executed >= 30);
}

#[test]
fn test_phase16_gdextension_c_abi_boundary_suite() {
    let r = verify_gdextension_c_abi_boundary_suite();
    assert!(r.invariants_held, "gdextension c abi boundary suite failed invariants!");
    assert!(r.cases_executed >= 80);
}

#[test]
fn test_phase16_classdb_hierarchy_invariants_suite() {
    let r = verify_classdb_hierarchy_invariants_suite();
    assert!(r.invariants_held, "classdb hierarchy invariants suite failed invariants!");
    assert!(r.cases_executed >= 5);
}

#[test]
fn test_phase16_rapier_port_hotswap_and_controller_suite() {
    let r = verify_rapier_port_hotswap_and_controller_proptest_suite();
    assert!(r.invariants_held, "rapier port hotswap and controller suite failed invariants!");
    assert!(r.cases_executed >= 50);
}

#[test]
fn test_phase16_fpckexec_binary_trailer_adversarial_suite() {
    let r = verify_fpckexec_binary_trailer_adversarial_suite();
    assert!(r.invariants_held, "fpckexec binary trailer suite failed invariants!");
    assert!(r.cases_executed >= 50);
}

#[test]
fn test_phase16_bbcode_and_terrain_autotiler_suite() {
    let r = verify_bbcode_and_terrain_autotiler_suite();
    assert!(r.invariants_held, "bbcode and terrain autotiler suite failed invariants!");
    assert!(r.cases_executed >= 15);
}

#[test]
fn test_phase16_audio_ringbuffer_and_input_deadzone_suite() {
    let r = verify_audio_ringbuffer_and_input_deadzone_suite();
    assert!(r.invariants_held, "audio ringbuffer and input deadzone suite failed invariants!");
    assert!(r.cases_executed >= 100);
}

// -----------------------------------------------------------------------------
// Phase 21: Independent Red-Team v1.1.0 Polyamour & Next-Gen Adversarial Suites
// -----------------------------------------------------------------------------

#[test]
fn test_phase21_plnk_livelink_wire_protocol_fuzz_suite() {
    let r = verify_plnk_livelink_wire_protocol_fuzz_suite();
    assert!(r.invariants_held, "PLNK live-link wire protocol suite failed invariants!");
    assert!(r.cases_executed >= 50);
}

#[test]
fn test_phase21_polyops_v2_parser_and_replay_fuzz_suite() {
    let r = verify_polyops_v2_parser_and_replay_fuzz_suite();
    assert!(r.invariants_held, "polyops v2 parser and replay suite failed invariants!");
    assert!(r.cases_executed >= 20);
}

#[test]
fn test_phase21_subobject_raycast_and_proportional_edit_suite() {
    let r = verify_subobject_raycast_and_proportional_edit_suite();
    assert!(r.invariants_held, "subobject raycast & proportional edit suite failed invariants!");
    assert!(r.cases_executed >= 30);
}

#[test]
fn test_phase21_sculpt_symmetry_and_heat_bone_weights_suite() {
    let r = verify_sculpt_symmetry_and_heat_bone_weights_suite();
    assert!(r.invariants_held, "sculpt symmetry & heat bone weights suite failed invariants!");
    assert!(r.cases_executed >= 30);
}

#[test]
fn test_phase21_semantic_grind_splines_and_uv2_atlas_suite() {
    let r = verify_semantic_grind_splines_and_uv2_atlas_suite();
    assert!(r.invariants_held, "semantic grind splines & uv2 atlas suite failed invariants!");
    assert!(r.cases_executed >= 20);
}

#[test]
fn test_phase21_wasm_custom_section_adversarial_suite() {
    let r = verify_wasm_custom_section_adversarial_suite();
    assert!(r.invariants_held, "WASM custom section adversarial suite failed invariants!");
    assert!(r.cases_executed >= 10);
}

#[test]
fn test_phase21_rust_hot_reload_schema_migration_suite() {
    let r = verify_rust_hot_reload_schema_migration_suite();
    assert!(r.invariants_held, "rust hot reload schema migration suite failed invariants!");
    assert!(r.cases_executed >= 20);
}

#[test]
fn test_phase21_sexlife_moan_and_splicy_edl_fuzz_suite() {
    let r = verify_sexlife_moan_and_splicy_edl_fuzz_suite();
    assert!(r.invariants_held, "sexlife moan & splicy edl fuzz suite failed invariants!");
    assert!(r.cases_executed >= 50);
}

#[test]
fn test_phase21_gpu_bvh_pathtracer_and_radiance_cascades_suite() {
    let r = verify_gpu_bvh_pathtracer_and_radiance_cascades_suite();
    assert!(r.invariants_held, "GPU BVH pathtracer & radiance cascades suite failed invariants!");
    assert!(r.cases_executed >= 10);
}

#[test]
fn test_phase21_player_live_polyamour_and_threejs_parametric_suite() {
    let r = verify_player_live_polyamour_and_threejs_parametric_suite();
    assert!(r.invariants_held, "player live polyamour & threejs parametric suite failed invariants!");
    assert!(r.cases_executed >= 10);
}



