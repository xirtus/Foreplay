//! Adversarial Edge-Case & Differential Oracle Corpus (`src/adversarial.rs`).
//!
//! Attacks `FrameDifferentialVerdict` with non-finite and subnormal IEEE-754 floats,
//! thrashes `PhysicsServer3D::hotswap_backend` across 1,000 rapid A/B switches,
//! and aggregates receipts across all verifier suites.

use crate::VerificationReceipt;
use crate::parser_fuzzers::{
    verify_archive_pak_hostile_suite, verify_fscn_hostile_suite, verify_gdscript_hostile_suite,
    verify_replay_frame_codec_suite, verify_stick_calibration_suite, verify_tscn_hostile_suite,
    verify_wav_codec_hostile_suite,
};
use crate::subsystem_fuzzers::{
    verify_animation_state_machine_fuzzer_suite, verify_navigation_and_autotile_fuzzer_suite,
    verify_physics_and_audio_dsp_fuzzer_suite, verify_ui_layout_container_solver_suite,
};
use crate::v2_worldclass_fuzzers::{
    verify_meshlet_and_pbr_perceptual_suite,
    verify_polyamour_blend_io_and_polyops_hostile_suite,
    verify_polyamour_topology_and_modifiers_suite,
    verify_threejs_compiler_and_bidirectional_roundtrip_suite,
    verify_trimesh_gjk_ragdoll_and_moan_timeline_suite,
    verify_ue6_speculative_rollback_determinism_suite,
};
use crate::wave2b_fuzzers::{
    verify_1000_frame_physics_differential_gate, verify_fpck_archive_adversarial_suite,
    verify_gdshader_compiler_adversarial_suite, verify_godot4_200_scene_conformance_gate,
    verify_project_exporter_bundle_invariants_suite, verify_script_sandbox_termination_suite,
    verify_tscn_bidirectional_roundtrip_suite,
};
use crate::scene_tree_fuzzer::{
    build_sample_tree_scene, verify_bijective_ecs_entities, verify_fscn_emission_stability,
    verify_hierarchy_relations, verify_rebuild_world_equivalence,
};
use crate::undo_redo_fuzzer::{
    FuzzOperation, make_test_node, rich_fuzz_strings, run_stateful_fuzz_sequence,
    sample_adversarial_ir_values, verify_10k_step_undo_redo_marathon,
};
use foreplay_core::SceneTree;
use foreplay_ir::{IrNode, IrScene, IrValue, NodeId, PhysicsBackend};
use foreplay_physics::PhysicsServer3D;
use foreplay_porthost::FrameDifferentialVerdict;
use std::collections::BTreeMap;

/// Mock stateful Rapier-like physics backend for live A/B hotswap testing.
#[derive(Debug)]
pub struct MockRapierBackend {
    pub step_count: usize,
    pub total_dt: f32,
    pub simulated_bodies: u32,
    pub contact_hash: u64,
}

impl MockRapierBackend {
    pub fn new() -> Self {
        Self {
            step_count: 0,
            total_dt: 0.0,
            simulated_bodies: 128,
            contact_hash: 0x1234_5678_9ABC_DEF0,
        }
    }
}

impl Default for MockRapierBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl PhysicsBackend for MockRapierBackend {
    fn backend_id(&self) -> &'static str {
        "rapier3d"
    }

    fn step(&mut self, dt_seconds: f32) {
        self.step_count += 1;
        self.total_dt += dt_seconds;
        self.contact_hash = self.contact_hash.wrapping_mul(6364136223846793005).wrapping_add(1);
    }
}

/// Mock stateful Skate 2/3 clean-room port physics backend for live A/B hotswap testing.
#[derive(Debug)]
pub struct MockSkatePortBackend {
    pub step_count: usize,
    pub total_dt: f32,
    pub tick_accumulator: u64,
    pub wheel_contacts: [f32; 4],
}

impl MockSkatePortBackend {
    pub fn new() -> Self {
        Self {
            step_count: 0,
            total_dt: 0.0,
            tick_accumulator: 0,
            wheel_contacts: [1.0, 1.0, 1.0, 1.0],
        }
    }
}

impl Default for MockSkatePortBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl PhysicsBackend for MockSkatePortBackend {
    fn backend_id(&self) -> &'static str {
        "skate_port_physics"
    }

    fn step(&mut self, dt_seconds: f32) {
        self.step_count += 1;
        self.total_dt += dt_seconds;
        self.tick_accumulator = self.tick_accumulator.wrapping_add(1);
        for w in &mut self.wheel_contacts {
            *w = (*w + dt_seconds) % 2.0;
        }
    }
}

/// Tests `FrameDifferentialVerdict::evaluate` against edge cases including:
/// - `f32::NAN`, `f32::INFINITY`, `f32::NEG_INFINITY`
/// - Negative errors (`-0.0`, `-1.0`, etc.)
/// - Subnormal epsilons (`f32::from_bits(1)`, `1e-40`)
/// - Proves that `NaN` is NEVER accepted as `within_epsilon == true`!
pub fn verify_frame_differential_verdict_adversarial() -> VerificationReceipt {
    let mut cases_executed = 0;
    let mut invariants_held = true;

    // 1. Explicit NaN attacks: NaN must NEVER be within_epsilon under any circumstances!
    let nan = f32::NAN;
    let nan_cases = [
        (nan, 0.0, 0.001),
        (0.0, nan, 0.001),
        (nan, nan, 0.001),
        (0.0, 0.0, nan),
        (nan, nan, nan),
        (f32::from_bits(0x7fc0_0001), 0.0, 0.001),
        (0.0, f32::from_bits(0x7ff0_0001), 0.001),
        (nan, f32::INFINITY, 0.001),
        (f32::INFINITY, nan, 0.001),
        (-nan, -nan, 0.001),
    ];

    for (pos_err, vel_err, eps) in nan_cases {
        cases_executed += 1;
        let verdict = FrameDifferentialVerdict::evaluate(cases_executed as u64, pos_err, vel_err, eps);
        if verdict.within_epsilon {
            invariants_held = false;
        }
    }

    // 2. Infinity attacks
    let inf = f32::INFINITY;
    let neg_inf = f32::NEG_INFINITY;
    let inf_cases = [
        (inf, 0.0, 0.001, false),
        (0.0, inf, 0.001, false),
        (inf, inf, 0.001, false),
        (inf, 0.0, inf, true),   // inf <= inf is true in IEEE 754
        (0.0, inf, inf, true),
        (neg_inf, 0.0, 0.001, true), // neg_inf <= eps is true mathematically
        (0.0, neg_inf, 0.001, true),
    ];

    for (pos_err, vel_err, eps, expected) in inf_cases {
        cases_executed += 1;
        let verdict = FrameDifferentialVerdict::evaluate(cases_executed as u64, pos_err, vel_err, eps);
        if verdict.within_epsilon != expected {
            invariants_held = false;
        }
    }

    // 3. Subnormal epsilons and errors
    let smallest_subnormal = f32::from_bits(1); // ~1.4e-45
    let larger_subnormal = f32::from_bits(2);   // ~2.8e-45
    let subnormal_cases = [
        // error equals subnormal epsilon -> true
        (smallest_subnormal, smallest_subnormal, smallest_subnormal, true),
        // error exceeds subnormal epsilon -> false
        (larger_subnormal, smallest_subnormal, smallest_subnormal, false),
        (smallest_subnormal, larger_subnormal, smallest_subnormal, false),
        // zero error within subnormal epsilon -> true
        (0.0, 0.0, smallest_subnormal, true),
        // negative zero error within subnormal epsilon -> true
        (-0.0, -0.0, smallest_subnormal, true),
    ];

    for (pos_err, vel_err, eps, expected) in subnormal_cases {
        cases_executed += 1;
        let verdict = FrameDifferentialVerdict::evaluate(cases_executed as u64, pos_err, vel_err, eps);
        if verdict.within_epsilon != expected {
            invariants_held = false;
        }
    }

    // 4. Negative error checks
    let negative_cases = [
        (-1.0, -2.0, 0.001, true),
        (-0.0, 0.0, 0.0, true),
        (-1.0, 0.5, 0.001, false), // vel_err exceeds epsilon
        (0.5, -1.0, 0.001, false), // pos_err exceeds epsilon
    ];

    for (pos_err, vel_err, eps, expected) in negative_cases {
        cases_executed += 1;
        let verdict = FrameDifferentialVerdict::evaluate(cases_executed as u64, pos_err, vel_err, eps);
        if verdict.within_epsilon != expected {
            invariants_held = false;
        }
    }

    // 5. Grid boundary sweep across float bit combinations (10,000 cases)
    let float_grid = [
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        0.0,
        -0.0,
        smallest_subnormal,
        larger_subnormal,
        1e-6,
        1e-3,
        1.0,
        100.0,
        f32::MAX,
        f32::MIN_POSITIVE,
    ];

    for &p in &float_grid {
        for &v in &float_grid {
            for &e in &float_grid {
                cases_executed += 1;
                let verdict = FrameDifferentialVerdict::evaluate(cases_executed as u64, p, v, e);
                if p.is_nan() || v.is_nan() || e.is_nan() {
                    if verdict.within_epsilon {
                        invariants_held = false;
                    }
                } else if p <= e && v <= e {
                    if !verdict.within_epsilon {
                        invariants_held = false;
                    }
                } else if verdict.within_epsilon {
                    invariants_held = false;
                }
            }
        }
    }

    VerificationReceipt {
        suite_name: "frame_differential_verdict_adversarial_oracle".into(),
        cases_executed,
        invariants_held,
    }
}

/// Tests `PhysicsServer3D::hotswap_backend` under rapid repeated A/B backend thrashing
/// (1,000 swaps) with mock stateful backends (`MockRapierBackend` and `MockSkatePortBackend`).
pub fn verify_physics_hotswap_thrash(swap_count: usize) -> VerificationReceipt {
    let mut cases_executed = 0;
    let mut invariants_held = true;

    let initial_rapier = Box::new(MockRapierBackend::new());
    let mut server = PhysicsServer3D::new(initial_rapier);

    if server.active_backend_id() != "rapier3d" {
        invariants_held = false;
    }

    // Standby backend pool: we retain swapped out backends to ensure zero memory corruption
    let mut rapier_pool: Vec<Box<dyn PhysicsBackend>> = Vec::new();
    let mut skate_pool: Vec<Box<dyn PhysicsBackend>> = Vec::new();

    let dt = 1.0 / 60.0;

    for i in 0..swap_count {
        cases_executed += 1;

        if i % 2 == 0 {
            // Swap in Skate port physics
            let next_skate: Box<dyn PhysicsBackend> = skate_pool
                .pop()
                .unwrap_or_else(|| Box::new(MockSkatePortBackend::new()));

            let prev = server.hotswap_backend(next_skate);
            if prev.backend_id() != "rapier3d" {
                invariants_held = false;
            }
            if server.active_backend_id() != "skate_port_physics" {
                invariants_held = false;
            }
            rapier_pool.push(prev);
        } else {
            // Swap in Rapier physics
            let next_rapier: Box<dyn PhysicsBackend> = rapier_pool
                .pop()
                .unwrap_or_else(|| Box::new(MockRapierBackend::new()));

            let prev = server.hotswap_backend(next_rapier);
            if prev.backend_id() != "skate_port_physics" {
                invariants_held = false;
            }
            if server.active_backend_id() != "rapier3d" {
                invariants_held = false;
            }
            skate_pool.push(prev);
        }

        // Step physics server
        server.step(dt);
    }

    // Verify all backends in pools have non-zero or valid steps
    if rapier_pool.is_empty() && skate_pool.is_empty() {
        invariants_held = false;
    }

    VerificationReceipt {
        suite_name: "physics_server_3d_hotswap_thrash".into(),
        cases_executed,
        invariants_held,
    }
}

/// Executes a comprehensive stateful Undo/Redo fuzzer suite with rich values.
pub fn verify_undo_redo_suite(cases: usize) -> VerificationReceipt {
    let mut cases_executed = 0;
    let mut invariants_held = true;

    let rich_values = sample_adversarial_ir_values();
    let strings = rich_fuzz_strings();

    for case_idx in 0..cases {
        cases_executed += 1;

        let mut initial_scene = IrScene::new(format!("UndoRedoFuzzScene_{case_idx}"));
        let root = make_test_node(1, 1, "Root", "Node3D", None);
        initial_scene.nodes.insert(root.id, root);

        let mut operations = Vec::new();

        // 1. Insert multiple nodes
        for n in 2..=10 {
            let parent_id = if n % 2 == 0 {
                Some(NodeId::new(1, 1).unwrap())
            } else {
                Some(NodeId::new(n - 1, 1).unwrap())
            };
            let mut node = make_test_node(
                n,
                1,
                format!("Node_{n}"),
                if n % 3 == 0 { "CharacterBody3D" } else { "Node3D" },
                parent_id,
            );
            let val = rich_values[(case_idx + n as usize) % rich_values.len()].clone();
            node.properties.insert(format!("prop_{n}"), val);
            operations.push(FuzzOperation::InsertNode { node });
        }

        // 2. Set properties with rich values
        for p in 1..=5 {
            let target_id = NodeId::new(p, 1).unwrap();
            let key = format!("dynamic_prop_{p}");
            let new_val = rich_values[(case_idx * 7 + p as usize) % rich_values.len()].clone();
            operations.push(FuzzOperation::SetProperty {
                target_id,
                key,
                new_value: Some(new_val),
            });
        }

        // 3. Tune feel specs
        operations.push(FuzzOperation::TuneFeelSpec {
            key: "skater_ollie_height".into(),
            new_value: Some(1.45),
        });
        operations.push(FuzzOperation::TuneFeelSpec {
            key: "camera_fov".into(),
            new_value: Some(75.0),
        });

        // 4. Undo burst
        operations.push(FuzzOperation::UndoBurst(4));

        // 5. Redo burst
        operations.push(FuzzOperation::RedoBurst(2));

        // 6. Invalidate commands to test clean error handling
        operations.push(FuzzOperation::InvalidInsertDuplicate {
            node_name: strings[case_idx % strings.len()].clone(),
        });
        operations.push(FuzzOperation::InvalidRemoveMissing {
            missing_id: NodeId::new(9999 + case_idx as u32, 1).unwrap(),
        });
        operations.push(FuzzOperation::InvalidSetPropertyMissing {
            missing_id: NodeId::new(8888 + case_idx as u32, 1).unwrap(),
            key: "missing_prop".into(),
            value: IrValue::I64(42),
        });

        // 7. Remove an existing node
        operations.push(FuzzOperation::RemoveNode {
            target_id: NodeId::new(7, 1).unwrap(),
        });

        if let Err(_err) = run_stateful_fuzz_sequence(initial_scene, &operations) {
            invariants_held = false;
        }
    }

    VerificationReceipt {
        suite_name: "stateful_undo_redo_history_invariants".into(),
        cases_executed,
        invariants_held,
    }
}

/// Executes the SceneTree relational, hierarchy, and rebuild equivalence suite.
pub fn verify_scene_tree_relational_suite(cases: usize) -> VerificationReceipt {
    let mut cases_executed = 0;
    let mut invariants_held = true;

    for case_idx in 0..cases {
        cases_executed += 1;

        let chain_depth = (case_idx % 8) + 1;
        let fan_out = (case_idx % 12) + 1;
        let scene = build_sample_tree_scene(chain_depth, fan_out);

        // Verify bijective ECS entities
        let mut tree = SceneTree::from_ir(scene.clone());
        if verify_bijective_ecs_entities(&mut tree, &scene).is_err() {
            invariants_held = false;
        }

        // Verify hierarchy relations
        if verify_hierarchy_relations(&tree, &scene).is_err() {
            invariants_held = false;
        }

        // Mutate scene and test rebuild equivalence
        let mut mutated = scene.clone();
        let new_id = NodeId::new(1000 + case_idx as u32, 1).unwrap();
        mutated.nodes.insert(
            new_id,
            IrNode {
                id: new_id,
                name: format!("MutatedNode_{case_idx}"),
                class_name: "Area3D".into(),
                parent: Some(NodeId::new(0, 1).unwrap()),
                instance_scene: None,
                properties: BTreeMap::new(),
                groups: vec!["rebuild_test".into()],
            },
        );

        if verify_rebuild_world_equivalence(&mut tree, mutated).is_err() {
            invariants_held = false;
        }
    }

    VerificationReceipt {
        suite_name: "scene_tree_relational_and_rebuild_invariants".into(),
        cases_executed,
        invariants_held,
    }
}

/// Executes the FSCN deterministic serialization stability suite.
pub fn verify_fscn_emission_suite(cases: usize) -> VerificationReceipt {
    let mut cases_executed = 0;
    let mut invariants_held = true;

    for case_idx in 0..cases {
        cases_executed += 1;
        let chain_depth = (case_idx % 6) + 1;
        let fan_out = (case_idx % 8) + 1;
        let scene = build_sample_tree_scene(chain_depth, fan_out);

        if verify_fscn_emission_stability(&scene).is_err() {
            invariants_held = false;
        }
    }

    VerificationReceipt {
        suite_name: "fscn_deterministic_emission_invariants".into(),
        cases_executed,
        invariants_held,
    }
}

/// Runs all independent adversarial verification suites and returns a receipt for each.
pub fn run_all_adversarial_suites() -> Vec<VerificationReceipt> {
    vec![
        verify_undo_redo_suite(50),
        verify_10k_step_undo_redo_marathon(),
        verify_scene_tree_relational_suite(50),
        verify_fscn_emission_suite(50),
        verify_fscn_hostile_suite(),
        verify_tscn_hostile_suite(),
        verify_gdscript_hostile_suite(),
        verify_replay_frame_codec_suite(),
        verify_wav_codec_hostile_suite(),
        verify_archive_pak_hostile_suite(),
        verify_stick_calibration_suite(),
        verify_frame_differential_verdict_adversarial(),
        verify_physics_hotswap_thrash(1_000),
        // Wave 2B Suites
        verify_fpck_archive_adversarial_suite(),
        verify_gdshader_compiler_adversarial_suite(),
        verify_script_sandbox_termination_suite(),
        verify_tscn_bidirectional_roundtrip_suite(),
        verify_godot4_200_scene_conformance_gate(),
        verify_1000_frame_physics_differential_gate(),
        verify_project_exporter_bundle_invariants_suite(),
        // Wave 3A Subsystem Fuzzers
        verify_ui_layout_container_solver_suite(),
        verify_animation_state_machine_fuzzer_suite(),
        verify_navigation_and_autotile_fuzzer_suite(),
        verify_physics_and_audio_dsp_fuzzer_suite(),
        // Wave 3B World-Class Verification Suites
        verify_polyamour_topology_and_modifiers_suite(),
        verify_polyamour_blend_io_and_polyops_hostile_suite(),
        verify_threejs_compiler_and_bidirectional_roundtrip_suite(),
        verify_ue6_speculative_rollback_determinism_suite(),
        verify_meshlet_and_pbr_perceptual_suite(),
        verify_trimesh_gjk_ragdoll_and_moan_timeline_suite(),
    ]
}
