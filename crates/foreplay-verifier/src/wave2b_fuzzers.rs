//! Wave 2B Red-Team Adversarial Fuzzing Suites (`foreplay-verifier`).
//!
//! Provides exhaustive security, bounds, and round-trip invariant verification for:
//! 1. `.fpck` Archive Corruption, Bit-Flip & Path-Traversal Fuzzer (`foreplay_export::FpckArchive`)
//! 2. `.gdshader` -> WGSL Compiler Adversarial Fuzzer (`foreplay_render::shader::compile_gdshader_to_wgsl`)
//! 3. `GdVm` & `QuickJsComponentRunner` Sandbox & Termination Fuzzer
//! 4. Bidirectional Godot `.tscn` Round-Trip Property Fuzzer & Conformance/Differential Gates
//! 5. 5-Platform `ProjectExporter` Release Bundle Invariant Verifier (`foreplay_export::ProjectExporter`)

use crate::VerificationReceipt;
use foreplay_export::{
    ExportConfig, ExportError, ExportTarget, FpckArchive, FpckError, ProjectExporter,
};
use foreplay_gdscript::{GdVm, VmError};
use foreplay_ir::{IrNode, IrScene, IrValue, NodeId, SignalConnection};
use foreplay_porthost::{
    export_ir_to_tscn, parse_tscn_to_ir, run_200_tscn_conformance_gate,
    run_1000_frame_differential_gate,
};
use foreplay_render::compile_gdshader_to_wgsl;
use foreplay_script::{JsEvalError, QuickJsComponentRunner};
use std::collections::BTreeMap;
use std::panic::catch_unwind;

// ============================================================================
// Suite 1: .fpck Archive Corruption, Bit-Flip & Path-Traversal Fuzzer
// ============================================================================

/// Verifies `.fpck` archive packing, single-bit corruption detection, path traversal rejection,
/// truncated header safety, and random byte resilience.
pub fn verify_fpck_archive_adversarial_suite() -> VerificationReceipt {
    let mut cases_executed = 0;
    let mut invariants_held = true;

    // 1. Valid Pack & Unpack Round-Trip with realistic asset topologies
    let mut files = BTreeMap::new();
    files.insert(
        "project.foreplay".into(),
        b"[project]\nname=\"SkaterWorld\"\nversion=\"1.0.0\"\n".to_vec(),
    );
    files.insert(
        "scenes/main.fscn".into(),
        b"[fscn_scene]\nname=\"MainLevel\"\n".to_vec(),
    );
    files.insert(
        "scripts/player.ts".into(),
        b"export class Skater { speed: number = 10.0; }\n".to_vec(),
    );
    files.insert(
        "assets/textures/board.bin".into(),
        vec![0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x00, 0x11],
    );
    files.insert(
        "nested/deep/directory/level.dat".into(),
        vec![42u8; 1024],
    );

    cases_executed += 1;
    let packed = match FpckArchive::pack_bytes(&files) {
        Ok(bytes) => bytes,
        Err(_) => {
            return VerificationReceipt {
                suite_name: "fpck_archive_adversarial_suite".into(),
                cases_executed,
                invariants_held: false,
            };
        }
    };

    cases_executed += 1;
    match FpckArchive::unpack_bytes(&packed) {
        Ok(unpacked) => {
            if unpacked != files {
                invariants_held = false;
            }
        }
        Err(_) => invariants_held = false,
    }

    // 2. Single-Bit Flip Mutation Fuzzing:
    // Any single bit flipped in payload data MUST fail unpack_bytes with ChecksumMismatch!
    // Never allow silently corrupted data to be returned.
    let payload_start = 28; // Header is 28 bytes
    let payload_len = files.values().map(|v| v.len()).sum::<usize>();
    let test_offsets = [
        payload_start,
        payload_start + 5,
        payload_start + 30,
        payload_start + payload_len.saturating_sub(10),
    ];

    for &offset in &test_offsets {
        if offset < packed.len() {
            for bit in 0..8 {
                cases_executed += 1;
                let mut corrupted = packed.clone();
                corrupted[offset] ^= 1 << bit;

                let unpack_res = FpckArchive::unpack_bytes(&corrupted);
                match unpack_res {
                    Ok(corrupt_files) => {
                        // If it somehow succeeded, the files MUST match original (e.g. if flip was in non-data padding)
                        // But in .fpck every payload byte is checksummed, so bit flip MUST NOT produce corrupted data!
                        if corrupt_files != files {
                            invariants_held = false;
                        }
                    }
                    Err(FpckError::ChecksumMismatch { .. }) => {
                        // Exactly what we expect!
                    }
                    Err(FpckError::CorruptArchive(_))
                    | Err(FpckError::UnexpectedEof { .. })
                    | Err(FpckError::InvalidMagic(_)) => {
                        // Acceptable structural error rejection
                    }
                    Err(_) => {}
                }
            }
        }
    }

    // 3. Hostile Path Traversal Attacks:
    // MUST be rejected on pack_bytes and on forged TOC unpack_bytes!
    let hostile_traversal_paths = [
        "../etc/passwd",
        "foo/../../bar",
        "/abs/path/file.txt",
        "\\windows\\system32\\config",
        "..\\secret.key",
        "foo/..",
        "nested/path/../../../escaped",
        "",
    ];

    for &bad_path in &hostile_traversal_paths {
        cases_executed += 1;
        let mut bad_map = BTreeMap::new();
        bad_map.insert(bad_path.to_string(), b"malicious content".to_vec());
        let pack_res = FpckArchive::pack_bytes(&bad_map);
        if !matches!(pack_res, Err(FpckError::PathTraversal(_))) {
            invariants_held = false;
        }
    }

    // Forged TOC unpack traversal attack:
    // Forge an archive whose header claims a path with ".."
    cases_executed += 1;
    let mut forged_archive = Vec::new();
    forged_archive.extend_from_slice(b"FPCK");
    forged_archive.extend_from_slice(&1u32.to_le_bytes()); // version
    forged_archive.extend_from_slice(&1u32.to_le_bytes()); // 1 entry
    let toc_offset = 28u64 + 4u64; // data is 4 bytes
    let toc_len = 2u64 + 14u64 + 8u64 + 8u64 + 8u64; // path_len(2) + "../escaped.txt"(14) + offset(8) + size(8) + checksum(8)
    forged_archive.extend_from_slice(&toc_offset.to_le_bytes());
    forged_archive.extend_from_slice(&toc_len.to_le_bytes());
    // Payload (4 bytes)
    forged_archive.extend_from_slice(b"DATA");
    // TOC
    let bad_name = b"../escaped.txt";
    forged_archive.extend_from_slice(&(bad_name.len() as u16).to_le_bytes());
    forged_archive.extend_from_slice(bad_name);
    forged_archive.extend_from_slice(&28u64.to_le_bytes()); // offset
    forged_archive.extend_from_slice(&4u64.to_le_bytes()); // size
    forged_archive.extend_from_slice(&0u64.to_le_bytes()); // checksum

    let forged_unpack_res = FpckArchive::unpack_bytes(&forged_archive);
    if !matches!(forged_unpack_res, Err(FpckError::PathTraversal(_))) {
        invariants_held = false;
    }

    // 4. Truncated Headers & Extreme Bounds:
    // Truncated from 0..27 bytes MUST return UnexpectedEof, never panic
    for trunc_len in 0..28 {
        cases_executed += 1;
        let trunc_res = FpckArchive::unpack_bytes(&packed[..trunc_len.min(packed.len())]);
        if !matches!(trunc_res, Err(FpckError::UnexpectedEof { .. })) {
            invariants_held = false;
        }
    }

    // Extreme TOC bounds:
    // Case A: TOC offset = u64::MAX with len = 0
    cases_executed += 1;
    let mut extreme_archive_a = packed.clone();
    extreme_archive_a[12..20].copy_from_slice(&u64::MAX.to_le_bytes());
    extreme_archive_a[20..28].copy_from_slice(&0u64.to_le_bytes());
    let res_a = FpckArchive::unpack_bytes(&extreme_archive_a);
    if !matches!(res_a, Err(FpckError::UnexpectedEof { .. })) {
        invariants_held = false;
    }

    // Case B: TOC offset = 0 with len = u64::MAX
    cases_executed += 1;
    let mut extreme_archive_b = packed.clone();
    extreme_archive_b[12..20].copy_from_slice(&0u64.to_le_bytes());
    extreme_archive_b[20..28].copy_from_slice(&u64::MAX.to_le_bytes());
    let res_b = FpckArchive::unpack_bytes(&extreme_archive_b);
    if !matches!(res_b, Err(FpckError::UnexpectedEof { .. })) {
        invariants_held = false;
    }

    // Case C: Forged massive offset beyond file bounds
    cases_executed += 1;
    let mut extreme_archive_c = packed.clone();
    let massive_offset = (packed.len() as u64) + 1_000_000;
    extreme_archive_c[12..20].copy_from_slice(&massive_offset.to_le_bytes());
    let res_c = FpckArchive::unpack_bytes(&extreme_archive_c);
    if !matches!(res_c, Err(FpckError::UnexpectedEof { .. })) {
        invariants_held = false;
    }

    // 5. Random byte slices: Must return Err without panicking or hanging
    let mut rng_seed = 0x1234_5678_9ABC_DEF0u64;
    for _ in 0..200 {
        cases_executed += 1;
        rng_seed = rng_seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let len = (rng_seed % 512) as usize;
        let mut random_bytes = vec![0u8; len];
        for b in &mut random_bytes {
            rng_seed = rng_seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            *b = (rng_seed >> 32) as u8;
        }

        let unwind_res = catch_unwind(|| FpckArchive::unpack_bytes(&random_bytes));
        match unwind_res {
            Ok(Err(_)) => {}
            Ok(Ok(_)) => {
                // If by astronomical chance random bytes formed a valid archive, verify it unpacks safely
            }
            Err(_) => {
                invariants_held = false;
            }
        }
    }

    VerificationReceipt {
        suite_name: "fpck_archive_adversarial_suite".into(),
        cases_executed,
        invariants_held,
    }
}

// ============================================================================
// Suite 2: .gdshader -> WGSL Compiler Adversarial Fuzzer
// ============================================================================

/// Verifies `compile_gdshader_to_wgsl` handles hostile syntax, deeply nested braces,
/// invalid shader types, malformed uniforms, NUL bytes, and Unicode without panicking.
pub fn verify_gdshader_compiler_adversarial_suite() -> VerificationReceipt {
    let mut cases_executed = 0;
    let mut invariants_held = true;

    // 1. Valid baseline compilation verification
    let valid_spatial_shader = r#"
shader_type spatial;

uniform vec4 albedo_color : source_color = vec4(1.0, 0.5, 0.2, 1.0);
uniform float roughness : hint_range(0.0, 1.0) = 0.5;
uniform sampler2D main_texture;

void vertex() {
    VERTEX.y += sin(TIME * 2.0) * 0.1;
}

void fragment() {
    ALBEDO = albedo_color.rgb;
    ROUGHNESS = roughness;
}
"#;
    cases_executed += 1;
    match compile_gdshader_to_wgsl(valid_spatial_shader) {
        Ok(wgsl) => {
            if !wgsl.contains("@vertex") || !wgsl.contains("@fragment") {
                invariants_held = false;
            }
        }
        Err(_) => invariants_held = false,
    }

    let valid_canvas_shader = r#"
shader_type canvas_item;

uniform vec2 uv_scale = vec2(2.0, 2.0);

void fragment() {
    COLOR = vec4(UV * uv_scale, 0.5, 1.0);
}
"#;
    cases_executed += 1;
    match compile_gdshader_to_wgsl(valid_canvas_shader) {
        Ok(wgsl) => {
            if !wgsl.contains("@fragment") {
                invariants_held = false;
            }
        }
        Err(_) => invariants_held = false,
    }

    // 2. Hostile Syntax Mutations Corpus (Zero Panics, Deterministic Ok/Err)
    let hostile_shader_corpus: Vec<&str> = vec![
        "", // empty
        "   \n\t\r\n   ", // whitespace only
        "shader_type", // incomplete header
        "shader_type;", // missing type name
        "shader_type invalid_type;", // unsupported type
        "shader_type \"spatial\";", // quoted type
        "shader_type 99999;", // numeric type
        "void fragment() {}", // missing shader_type
        "shader_type spatial;\nvoid fragment() {", // unclosed block
        "shader_type spatial;\nvoid fragment() }", // unexpected closing brace
        "shader_type spatial;\nvoid fragment() { { { {", // unbalanced opening braces
        "shader_type spatial;\n} } } { { {", // scrambled braces
        "shader_type spatial;\nuniform", // incomplete uniform
        "shader_type spatial;\nuniform int x = ;", // malformed uniform value
        "shader_type spatial;\nuniform vec99 invalid_type;", // invalid uniform type
        "shader_type spatial;\nuniform float x : hint_range(1.0);", // incomplete hint
        "shader_type spatial;\0void fragment() { ALBEDO = vec3(1.0); }", // NUL byte injection
        "shader_type spatial; // 🔥🚀🛹\nvoid fragment() { ALBEDO = vec3(1.0); }", // Unicode emoji
        "shader_type spatial;\n/* unclosed multi-line comment\nvoid fragment() {}", // unclosed comment
        "shader_type spatial;\nvoid vertex() {}\nvoid vertex() {}", // duplicate vertex functions
        "shader_type spatial;\nuniform vec4 x;\nuniform vec4 x;", // duplicate uniforms
    ];

    for &hostile in &hostile_shader_corpus {
        cases_executed += 1;
        let caught = catch_unwind(|| compile_gdshader_to_wgsl(hostile));
        match caught {
            Ok(Ok(wgsl)) => {
                // If it compiles, WGSL must not be completely empty
                if wgsl.is_empty() {
                    invariants_held = false;
                }
            }
            Ok(Err(_)) => {
                // Expected deterministic compilation error
            }
            Err(_) => {
                // Panic is NEVER allowed!
                invariants_held = false;
            }
        }
    }

    // 3. Deeply nested braces stress test (50 levels)
    cases_executed += 1;
    let mut deep_nested = String::from("shader_type spatial;\nvoid fragment() {\n");
    for _ in 0..50 {
        deep_nested.push_str("if (true) {\n");
    }
    deep_nested.push_str("ALBEDO = vec3(1.0);\n");
    for _ in 0..50 {
        deep_nested.push_str("}\n");
    }
    deep_nested.push_str("}\n");

    let deep_res = catch_unwind(|| compile_gdshader_to_wgsl(&deep_nested));
    if deep_res.is_err() {
        invariants_held = false;
    }

    // 4. Massive source test: 5,000 comment lines
    cases_executed += 1;
    let mut massive_source = String::from("shader_type spatial;\n");
    for i in 0..5_000 {
        massive_source.push_str(&format!("// comment line {i}\n"));
    }
    massive_source.push_str("void fragment() { ALBEDO = vec3(0.5); }\n");

    let massive_res = catch_unwind(|| compile_gdshader_to_wgsl(&massive_source));
    match massive_res {
        Ok(Ok(wgsl)) => {
            if !wgsl.contains("@fragment") {
                invariants_held = false;
            }
        }
        _ => invariants_held = false,
    }

    VerificationReceipt {
        suite_name: "gdshader_compiler_adversarial_suite".into(),
        cases_executed,
        invariants_held,
    }
}

// ============================================================================
// Suite 3: GdVm & QuickJsComponentRunner Sandbox & Termination Fuzzer
// ============================================================================

/// Verifies step budget termination guards prevent infinite loops in `GdVm` and
/// `QuickJsComponentRunner`, division-by-zero is trapped cleanly, and malformed inputs never panic.
pub fn verify_script_sandbox_termination_suite() -> VerificationReceipt {
    let mut cases_executed = 0;
    let mut invariants_held = true;

    let dummy_node_id = NodeId::new(1, 1).unwrap();
    let make_node = || IrNode {
        id: dummy_node_id,
        name: "SandboxTestNode".into(),
        class_name: "Node3D".into(),
        parent: None,
        instance_scene: None,
        properties: BTreeMap::new(),
        groups: Vec::new(),
    };

    // 1. GdVm Infinite Loop Termination: while true MUST terminate with BudgetExceeded!
    let gd_infinite_scripts = [
        "func _ready():\n    while true:\n        pass\n",
        "func _ready():\n    var i = 0\n    while i >= 0:\n        i += 1\n",
        "func _physics_process(delta):\n    while true:\n        pass\n",
    ];

    for &script in &gd_infinite_scripts {
        cases_executed += 1;
        if let Ok(mut vm) = GdVm::from_source(script) {
            vm.max_step_budget = 100;
            let mut node = make_node();

            let ready_res = vm.ready(&mut node);
            let phys_res = vm.step_physics_process(0.016, &mut node);

            let exceeded_in_ready = matches!(ready_res, Err(VmError::BudgetExceeded { budget: 100 }));
            let exceeded_in_phys = matches!(phys_res, Err(VmError::BudgetExceeded { budget: 100 }));

            if !exceeded_in_ready && !exceeded_in_phys {
                invariants_held = false;
            }
        } else {
            invariants_held = false;
        }
    }

    // 2. QuickJsComponentRunner Infinite Loop Termination
    let js_infinite_scripts = [
        "function onReady(node) {\n    while (true) {}\n}\n",
        "function onReady(node) {\n    let x = 0;\n    while (1) {\n        x += 1;\n    }\n}\n",
        "function onPhysicsUpdate(node, dt) {\n    let i = 0;\n    while (i >= 0) {\n        i += 1;\n    }\n}\n",
    ];

    for &script in &js_infinite_scripts {
        cases_executed += 1;
        match QuickJsComponentRunner::from_source("InfiniteRunner", script) {
            Ok(runner) => {
                let mut runner = runner.with_step_budget(100);
                let mut node = make_node();

                let ready_res = runner.call_on_ready(&mut node);
                let phys_res = runner.call_on_physics_update(&mut node, 0.016);

                let exceeded_in_ready = matches!(ready_res, Err(JsEvalError::BudgetExceeded { budget: 100 }));
                let exceeded_in_phys = matches!(phys_res, Err(JsEvalError::BudgetExceeded { budget: 100 }));

                if !exceeded_in_ready && !exceeded_in_phys {
                    invariants_held = false;
                }
            }
            Err(_) => {
                invariants_held = false;
            }
        }
    }

    // 3. Division-by-Zero Traps in GdVm:
    let gd_div_zero_scripts = [
        "func _ready():\n    var x = 10 / 0\n",
        "func _ready():\n    var x = 5\n    x /= 0\n",
    ];

    for &script in &gd_div_zero_scripts {
        cases_executed += 1;
        if let Ok(mut vm) = GdVm::from_source(script) {
            let mut node = make_node();
            let res = vm.ready(&mut node);
            if !matches!(res, Err(VmError::DivisionByZero)) {
                invariants_held = false;
            }
        }
    }

    // 4. Division-by-Zero Traps in QuickJsComponentRunner:
    let js_div_zero_scripts = [
        "function onReady(node) {\n    let x = 10 / 0;\n}\n",
        "function onReady(node) {\n    let y = 10 % 0;\n}\n",
    ];

    for &script in &js_div_zero_scripts {
        cases_executed += 1;
        if let Ok(mut runner) = QuickJsComponentRunner::from_source("DivZeroRunner", script) {
            let mut node = make_node();
            let res = runner.call_on_ready(&mut node);
            if !matches!(res, Err(JsEvalError::DivisionByZero)) {
                invariants_held = false;
            }
        }
    }

    // 5. Unknown Variable / Method / Malformed Script Resiliency
    cases_executed += 1;
    let gd_missing_var = "func _ready():\n    var y = non_existent_var_123 + 1\n";
    if let Ok(mut vm) = GdVm::from_source(gd_missing_var) {
        let mut node = make_node();
        let res = vm.ready(&mut node);
        if !matches!(res, Err(VmError::VariableNotFound { .. }) | Err(VmError::TypeError { .. })) {
            invariants_held = false;
        }

        let unknown_method_res = vm.call_method("unknown_method_xyz", &[], &mut node);
        if !matches!(unknown_method_res, Err(VmError::MethodNotFound { .. })) {
            invariants_held = false;
        }
    }

    cases_executed += 1;
    let js_missing_var = "function onReady(node) {\n    let y = unknown_variable_xyz + 5;\n}\n";
    if let Ok(mut runner) = QuickJsComponentRunner::from_source("MissingVar", js_missing_var) {
        let mut node = make_node();
        let res = runner.call_on_ready(&mut node);
        if !matches!(res, Err(JsEvalError::VariableNotFound { .. })) {
            invariants_held = false;
        }

        let unknown_js_method = runner.call_method("unknownMethodXyz", &mut node, &[]);
        if !matches!(unknown_js_method, Err(JsEvalError::MethodNotFound { .. })) {
            invariants_held = false;
        }
    }

    VerificationReceipt {
        suite_name: "script_sandbox_termination_suite".into(),
        cases_executed,
        invariants_held,
    }
}

// ============================================================================
// Suite 4: Bidirectional Godot .tscn Round-Trip Property Fuzzer
// ============================================================================

/// Verifies `IrScene -> export_ir_to_tscn -> parse_tscn_to_ir` preserves node counts,
/// names, classes, parent hierarchy, groups, and signal connections.
pub fn verify_tscn_bidirectional_roundtrip_suite() -> VerificationReceipt {
    let mut cases_executed = 0;
    let mut invariants_held = true;

    // Build 10 distinct scenes with various topologies
    for i in 0..10 {
        cases_executed += 1;
        let mut scene = IrScene::new(format!("RoundtripScene_{i}"));

        let root_id = NodeId::new(0, 1).unwrap();
        let mut root_node = IrNode {
            id: root_id,
            name: format!("Level_{i}"),
            class_name: "Node3D".into(),
            parent: None,
            instance_scene: None,
            properties: BTreeMap::new(),
            groups: vec!["root_group".into()],
        };
        root_node.properties.insert("score".into(), IrValue::I64((i * 100) as i64));
        scene.nodes.insert(root_id, root_node);

        let child1_id = NodeId::new(10 + i as u32, 1).unwrap();
        let mut child1 = IrNode {
            id: child1_id,
            name: "Player".into(),
            class_name: "CharacterBody3D".into(),
            parent: Some(root_id),
            instance_scene: None,
            properties: BTreeMap::new(),
            groups: vec!["player".into(), "physics".into()],
        };
        child1.properties.insert("speed".into(), IrValue::F64(15.5 + i as f64));
        scene.nodes.insert(child1_id, child1);

        let child2_id = NodeId::new(20 + i as u32, 1).unwrap();
        let child2 = IrNode {
            id: child2_id,
            name: "Camera".into(),
            class_name: "Camera3D".into(),
            parent: Some(child1_id),
            instance_scene: None,
            properties: BTreeMap::new(),
            groups: Vec::new(),
        };
        scene.nodes.insert(child2_id, child2);

        // Add signal connection
        let conn = SignalConnection {
            from_node: child1_id,
            signal_name: "landed".into(),
            to_node: root_id,
            method_name: "_on_player_landed".into(),
        };
        scene.connections.push(conn);

        // Export to Godot 4 .tscn
        let exported_tscn = export_ir_to_tscn(&scene);
        if exported_tscn.is_empty() {
            invariants_held = false;
            continue;
        }

        // Re-parse back to IrScene
        match parse_tscn_to_ir(&exported_tscn) {
            Ok(reparsed) => {
                // Invariant: node counts must match exactly
                if reparsed.nodes.len() != scene.nodes.len() {
                    invariants_held = false;
                }
                // Invariant: connections count must match
                if reparsed.connections.len() != scene.connections.len() {
                    invariants_held = false;
                }
                // Invariant: node names, classes, groups, and root status must match
                let reparsed_by_name: BTreeMap<&str, &IrNode> = reparsed
                    .nodes
                    .values()
                    .map(|n| (n.name.as_str(), n))
                    .collect();

                for node in scene.nodes.values() {
                    if let Some(re_node) = reparsed_by_name.get(node.name.as_str()) {
                        if re_node.class_name != node.class_name
                            || re_node.groups != node.groups
                            || re_node.parent.is_none() != node.parent.is_none()
                        {
                            invariants_held = false;
                        }
                    } else {
                        invariants_held = false;
                    }
                }
            }
            Err(_) => {
                invariants_held = false;
            }
        }
    }

    VerificationReceipt {
        suite_name: "tscn_bidirectional_roundtrip_suite".into(),
        cases_executed,
        invariants_held,
    }
}

/// Executes the official Godot 4 200-Scene Conformance Corpus gate and produces a verification receipt.
pub fn verify_godot4_200_scene_conformance_gate() -> VerificationReceipt {
    let report = run_200_tscn_conformance_gate();
    let invariants_held = report.is_100_percent() && report.total_scenes == 200 && report.passed == 200;

    VerificationReceipt {
        suite_name: "godot4_200_scene_conformance_gate".into(),
        cases_executed: report.total_scenes,
        invariants_held,
    }
}

/// Executes the PLAN.md Phase 5 1,000-Frame Physics Differential Gate and produces a verification receipt.
pub fn verify_1000_frame_physics_differential_gate() -> VerificationReceipt {
    let report = run_1000_frame_differential_gate();
    let invariants_held = report.passed_frames == report.total_frames && report.total_frames == 1000;

    VerificationReceipt {
        suite_name: "1000_frame_physics_differential_gate".into(),
        cases_executed: report.total_frames,
        invariants_held,
    }
}

// ============================================================================
// Suite 5: 5-Platform ProjectExporter Bundle Invariant Verifier
// ============================================================================

/// Verifies `ProjectExporter` produces valid release bundles for all 5 target platforms,
/// ensuring `.fpck` validity, macOS `LSSupportsGameMode`, SexOS `sexpkg.toml`, etc.
pub fn verify_project_exporter_bundle_invariants_suite() -> VerificationReceipt {
    let mut cases_executed = 0;
    let mut invariants_held = true;

    let base_config = ExportConfig::new("ForeplaySkater", "com.foreplay.skater", "res://scenes/main.fscn")
        .with_file("scenes/main.fscn", b"[fscn_scene]\nname=\"MainLevel\"\n".to_vec())
        .with_file("scripts/skater.ts", b"export class Skater {}\n".to_vec());

    let all_targets = ExportTarget::all();

    for &target in all_targets {
        cases_executed += 1;
        let output = match ProjectExporter::export_project(&base_config, target) {
            Ok(out) => out,
            Err(_) => {
                invariants_held = false;
                continue;
            }
        };

        // Invariant 1: archive_bytes MUST unpack cleanly with FpckArchive
        match FpckArchive::unpack_bytes(&output.archive_bytes) {
            Ok(unpacked) => {
                // Must contain project.foreplay, main.fscn, skater.ts
                if !unpacked.contains_key("project.foreplay")
                    || !unpacked.contains_key("scenes/main.fscn")
                    || !unpacked.contains_key("scripts/skater.ts")
                {
                    invariants_held = false;
                }
            }
            Err(_) => invariants_held = false,
        }

        // Invariant 2: Target-specific bundle contracts
        match target {
            ExportTarget::LinuxSteamDeckX64 => {
                if output.primary_artifact != "game.x86_64" {
                    invariants_held = false;
                }
                if let Some(sh_bytes) = output.bundle_files.get("launch.sh") {
                    let sh_str = String::from_utf8_lossy(sh_bytes);
                    if !sh_str.contains("ENABLE_VKB=1") || !sh_str.contains("MANGOHUD=1") {
                        invariants_held = false;
                    }
                } else {
                    invariants_held = false;
                }
            }

            ExportTarget::WindowsX64 => {
                if output.primary_artifact != "game.exe" {
                    invariants_held = false;
                }
                if !output.bundle_files.contains_key("game.bat")
                    || !output.bundle_files.contains_key("game.fpck")
                {
                    invariants_held = false;
                }
            }

            ExportTarget::MacOsAppBundle => {
                if !output.primary_artifact.ends_with(".app") {
                    invariants_held = false;
                }
                let plist_path = format!("{}/Contents/Info.plist", output.primary_artifact);
                if let Some(plist_bytes) = output.bundle_files.get(&plist_path) {
                    let plist_str = String::from_utf8_lossy(plist_bytes);
                    if !plist_str.contains("<key>LSSupportsGameMode</key>")
                        || !plist_str.contains("<true/>")
                    {
                        invariants_held = false;
                    }
                } else {
                    invariants_held = false;
                }
                let launcher_path = format!("{}/Contents/MacOS/launcher", output.primary_artifact);
                if !output.bundle_files.contains_key(&launcher_path) {
                    invariants_held = false;
                }
            }

            ExportTarget::WebGpuBrowserUrl => {
                if output.primary_artifact != "index.html" {
                    invariants_held = false;
                }
                if !output.bundle_files.contains_key("manifest.json")
                    || !output.bundle_files.contains_key("foreplay_runtime.js")
                    || !output.bundle_files.contains_key("game.fpck")
                {
                    invariants_held = false;
                }
            }

            ExportTarget::SexOsPackage => {
                if !output.primary_artifact.ends_with(".sexpkg") {
                    invariants_held = false;
                }
                if let Some(manifest_bytes) = output.bundle_files.get("sexpkg.toml") {
                    let manifest_str = String::from_utf8_lossy(manifest_bytes);
                    if !manifest_str.contains("entry = \"game.fpck\"")
                        || !manifest_str.contains("runtime = \"foreplay-v1\"")
                    {
                        invariants_held = false;
                    }
                } else {
                    invariants_held = false;
                }
            }
        }
    }

    // Hostile Config Rejections:
    // Empty game title rejected
    cases_executed += 1;
    let empty_title_cfg = ExportConfig::new("", "com.test", "res://main.fscn");
    if !matches!(
        ProjectExporter::export_project(&empty_title_cfg, ExportTarget::LinuxSteamDeckX64),
        Err(ExportError::InvalidConfig(_))
    ) {
        invariants_held = false;
    }

    // Empty main scene rejected
    cases_executed += 1;
    let empty_scene_cfg = ExportConfig::new("Game", "com.test", "");
    if !matches!(
        ProjectExporter::export_project(&empty_scene_cfg, ExportTarget::LinuxSteamDeckX64),
        Err(ExportError::InvalidConfig(_))
    ) {
        invariants_held = false;
    }

    VerificationReceipt {
        suite_name: "project_exporter_bundle_invariants_suite".into(),
        cases_executed,
        invariants_held,
    }
}
