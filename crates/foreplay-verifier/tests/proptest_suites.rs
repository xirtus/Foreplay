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

/// Unit test verifying all 20 adversarial suites via public run_all_adversarial_suites() API.
#[test]
fn test_run_all_adversarial_suites_receipts() {
    let receipts = run_all_adversarial_suites();
    assert_eq!(receipts.len(), 20);
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

