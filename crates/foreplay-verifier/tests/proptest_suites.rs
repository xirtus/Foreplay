//! Independent Adversarial Verifier — Comprehensive Proptest & Fuzzing Suites.
//!
//! Validates:
//! 1. Stateful proptest Undo/Redo state machine (up to 150 operations across hundreds of cases)
//! 2. Bevy 0.20 `SceneTree` relational & rebuild equivalence (bijectivity, `ChildOf`, `Children`, zero orphans)
//! 3. Deterministic `.fscn` serialization stability
//! 4. `FrameDifferentialVerdict` adversarial IEEE-754 attacks (verifying NaN is NEVER within epsilon)
//! 5. `PhysicsServer3D::hotswap_backend` 1,000-swap thrashing

use foreplay_core::SceneTree;
use foreplay_ir::{IrNode, IrScene, NodeId};
use foreplay_porthost::FrameDifferentialVerdict;
use foreplay_verifier::adversarial::{
    run_all_adversarial_suites, verify_frame_differential_verdict_adversarial,
    verify_physics_hotswap_thrash,
};
use foreplay_verifier::scene_tree_fuzzer::{
    arb_tree_scene, verify_bijective_ecs_entities, verify_fscn_emission_stability,
    verify_hierarchy_relations, verify_rebuild_world_equivalence,
};
use foreplay_verifier::undo_redo_fuzzer::{
    arb_fuzz_operation, make_test_node, run_stateful_fuzz_sequence,
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
    /// Tests arbitrary sequences of up to 150 operations across hundreds of cases,
    /// proving all 4 mathematical invariants against the reference shadow-history model:
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
    /// scene.to_fscn_string() produces byte-for-byte identical output across repeated
    /// invocations and regardless of insertion order into maps
    #[test]
    fn test_fscn_output_determinism_proptest(
        scene in arb_tree_scene()
    ) {
        let stability_result = verify_fscn_emission_stability(&scene);
        prop_assert!(stability_result.is_ok(), "FSCN emission stability failed: {:?}", stability_result.err());
    }

    /// Deliverable 3: Adversarial Float Attacks on FrameDifferentialVerdict
    /// Verifies that NaN is NEVER accepted as within_epsilon == true!
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
                "NaN float parameter MUST NEVER produce within_epsilon == true! pos: {pos_err}, vel: {vel_err}, eps: {epsilon}"
            );
        } else if pos_err > epsilon || vel_err > epsilon {
            prop_assert!(
                !verdict.within_epsilon,
                "Error exceeding epsilon MUST produce within_epsilon == false! pos: {pos_err}, vel: {vel_err}, eps: {epsilon}"
            );
        } else {
            prop_assert!(
                verdict.within_epsilon,
                "Valid errors <= epsilon MUST produce within_epsilon == true! pos: {pos_err}, vel: {vel_err}, eps: {epsilon}"
            );
        }
    }
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

/// Unit test verifying all suites via public run_all_adversarial_suites() API.
#[test]
fn test_run_all_adversarial_suites_receipts() {
    let receipts = run_all_adversarial_suites();
    assert_eq!(receipts.len(), 5);
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
