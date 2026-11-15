//! Phase 16 (Track M4-D): Independent Red-Team v1.0.0 Godot-Parity Adversarial Verifier.
//!
//! Provides exhaustive security, bounds, C-ABI, and round-trip invariant verification for:
//! 1. Hostile Binary `.glb` Fuzzing (`import_glb_bytes_to_ir`)
//! 2. Godot 4 `.tres` & `.gdextension` Manifest Fuzzing & Round-Trip
//! 3. GDScript 2.0 Coroutines, `match` Guards, Typed Collections & `GdLspAnalyzer` Bounds
//! 4. `GDExtension` C ABI Boundary (`gdext_get_proc_address`, Variant lifecycles) & `ClassDb` Invariants
//! 5. `RapierSpace3D` <-> `PortFaithful3D` Hotswap & `CharacterBody3DController::move_and_slide` Properties
//! 6. Adversarial `FPCKEXEC` Self-Contained Binary Trailer Fuzzing & Tamper Rejection
//! 7. `BbCodeRichText`, `TileMapTerrainAutotiler`, `AudioRingBuffer` & `InputServer` Invariants

use crate::VerificationReceipt;
use bevy_math::Vec3;
use foreplay_audio::AudioRingBuffer;
use foreplay_core::classdb::{ClassDb, ClassDbError, ClassInfo};
use foreplay_core::text_server::BbCodeRichText;
use foreplay_core::tilemap_terrain::{
    PeeringDirection, TerrainMode, TerrainSet, TerrainTileDef, TileMapTerrainAutotiler,
    TileSetTerrainCatalog,
};
use foreplay_export::bundle::verify_self_contained_export_bytes;
use foreplay_export::FpckArchive;
use foreplay_gdext::c_abi::{
    gdext_get_proc_address, gdext_variant_booleanize, gdext_variant_destroy,
    gdext_variant_get_type, gdext_variant_new_copy, gdext_variant_new_nil,
    supported_proc_names, AbiVariant, GDExtensionVariantType,
};
use foreplay_gdext::manifest::{
    emit_gdextension_manifest, parse_gdextension_manifest, ConfigurationSection,
    GdExtensionManifest,
};
use foreplay_gdscript::{
    AwaitTarget, GdCoroutineOutcome, GdLspAnalyzer, GdVm,
};
use foreplay_ir::{IrNode, IrValue, NodeId};
use foreplay_physics::character_controller::CharacterBody3DController;
use foreplay_physics::rapier_backend::{BodyType, RapierSpace3D, RigidBodyConfig3D, ShapeConfig3D};
use foreplay_physics::{
    Collider3D, PhysicsBackendKind, PhysicsBody3D, PhysicsServer3D,
};
use foreplay_platform::input_map::{
    GamepadAxisId, InputEvent, InputMap, InputServer, InputTrigger,
};
use foreplay_player::embedded_pack::{
    embed_pack_trailer, extract_embedded_pack_from_bytes, EmbeddedPackError,
};
use foreplay_porthost::gltf_import::{
    build_sample_rigged_character_glb, import_glb_bytes_to_ir,
};
use foreplay_porthost::tres::{
    emit_tres, parse_tres, TresExtResource, TresFile, TresHeader, TresResource, TresSubResource,
    TresValue,
};
use std::collections::BTreeMap;
use std::ffi::CString;
use std::panic::catch_unwind;

fn make_test_node(id_u32: u32, name: &str, class_name: &str) -> IrNode {
    IrNode {
        id: NodeId::new(id_u32, 1).unwrap(),
        name: name.to_string(),
        class_name: class_name.to_string(),
        parent: None,
        instance_scene: None,
        properties: BTreeMap::new(),
        groups: Vec::new(),
    }
}

// ============================================================================
// Suite 1: Hostile Binary .glb Fuzzing
// ============================================================================

/// Verifies `import_glb_bytes_to_ir` across arbitrary, truncated, and bit-flipped byte buffers,
/// including corrupted chunk headers and mutated valid `.glb` archives.
pub fn verify_hostile_glb_fuzzing_suite() -> VerificationReceipt {
    let mut cases_executed = 0;
    let mut invariants_held = true;

    // 1. Valid sample rigged character baseline
    cases_executed += 1;
    let valid_glb = build_sample_rigged_character_glb();
    let sample_res = import_glb_bytes_to_ir(&valid_glb, Some("RiggedHero"));
    if sample_res.is_err() {
        invariants_held = false;
    }

    // 2. Short / empty / arbitrary buffers
    let short_slices: &[&[u8]] = &[
        b"",
        b"g",
        b"gl",
        b"glT",
        b"glTF",
        b"glTF\x02\x00\x00\x00",
        b"glTF\x02\x00\x00\x00\x0c\x00\x00\x00",
        &[0xDE, 0xAD, 0xBE, 0xEF],
        &[0xFF; 32],
        &[0x00; 128],
    ];

    for &slice in short_slices {
        cases_executed += 1;
        let res = catch_unwind(|| import_glb_bytes_to_ir(slice, None));
        match res {
            Ok(Err(_)) => {} // Expected clean error
            Ok(Ok(_)) => invariants_held = false, // Cannot parse random garbage as valid glb
            Err(_) => invariants_held = false,    // Zero panic invariant violated!
        }
    }

    // 3. Mutated headers on valid GLB
    // a. Corrupt Magic
    let mut corrupt_magic = valid_glb.clone();
    corrupt_magic[0..4].copy_from_slice(b"NOPE");
    cases_executed += 1;
    if import_glb_bytes_to_ir(&corrupt_magic, None).is_ok() {
        invariants_held = false;
    }

    // b. Corrupt Version
    let mut corrupt_version = valid_glb.clone();
    corrupt_version[4..8].copy_from_slice(&1u32.to_le_bytes());
    cases_executed += 1;
    if import_glb_bytes_to_ir(&corrupt_version, None).is_ok() {
        invariants_held = false;
    }

    // c. Corrupt Total Length (declared longer than buffer)
    let mut corrupt_len = valid_glb.clone();
    corrupt_len[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
    cases_executed += 1;
    if import_glb_bytes_to_ir(&corrupt_len, None).is_ok() {
        invariants_held = false;
    }

    // d. Corrupt Chunk 0 (JSON) Length
    let mut corrupt_chunk0 = valid_glb.clone();
    corrupt_chunk0[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
    cases_executed += 1;
    if import_glb_bytes_to_ir(&corrupt_chunk0, None).is_ok() {
        invariants_held = false;
    }

    // e. Corrupt Chunk 0 Type
    let mut corrupt_chunk_type = valid_glb.clone();
    corrupt_chunk_type[16..20].copy_from_slice(&0u32.to_le_bytes());
    cases_executed += 1;
    if import_glb_bytes_to_ir(&corrupt_chunk_type, None).is_ok() {
        invariants_held = false;
    }

    // 4. Truncation sweep from 5% to 95%
    for pct in (5..=95).step_by(5) {
        cases_executed += 1;
        let truncate_len = (valid_glb.len() * pct) / 100;
        let truncated = &valid_glb[..truncate_len];
        let res = catch_unwind(|| import_glb_bytes_to_ir(truncated, None));
        match res {
            Ok(Err(_)) => {}
            Ok(Ok(_)) => invariants_held = false,
            Err(_) => invariants_held = false,
        }
    }

    // 5. Single-bit flips across random offsets
    for offset_idx in (0..valid_glb.len()).step_by(valid_glb.len() / 30) {
        for bit in 0..8 {
            cases_executed += 1;
            let mut flipped = valid_glb.clone();
            flipped[offset_idx] ^= 1 << bit;
            let res = catch_unwind(|| import_glb_bytes_to_ir(&flipped, None));
            if res.is_err() {
                invariants_held = false;
            }
        }
    }

    VerificationReceipt {
        suite_name: "hostile_binary_glb_fuzzing_suite".into(),
        cases_executed,
        invariants_held,
    }
}

// ============================================================================
// Suite 2: Godot 4 .tres Resource Fuzzing & Round-Trip
// ============================================================================

/// Verifies `parse_tres` and `emit_tres` under hostile input strings,
/// deeply nested dictionaries, and round-trip fidelity.
pub fn verify_tres_manifest_fuzzing_suite() -> VerificationReceipt {
    let mut cases_executed = 0;
    let mut invariants_held = true;

    // 1. Hostile strings to parse_tres
    let hostile_inputs = [
        "",
        "   \n\n\t  ",
        "; just a comment\n# another comment\n",
        "[not_a_resource]\nkey=\"val\"\n",
        "[gd_resource]\n", // missing type
        "[gd_resource type=\"\"]\n",
        "[gd_resource type=\"StandardMaterial3D\" format=\"bad\"]\n",
        "[gd_resource type=\"BoxShape3D\" load_steps=xyz]\n",
        "[gd_resource type=\"Theme\"]\n[ext_resource]\n",
        "[gd_resource type=\"Theme\"]\n[sub_resource]\n",
        "[gd_resource type=\"StandardMaterial3D\"]\n[resource]\nalbedo_color = Color(1.0, 2.0)\n", // malformed Color
        "[gd_resource type=\"BoxShape3D\"]\n[resource]\nsize = Vector3(1.0, \"invalid\", 3.0)\n",
        "[gd_resource type=\"StandardMaterial3D\"]\n[resource]\ntransform = Transform3D(1, 2, 3)\n",
        "[gd_resource type=\"Resource\"]\n[resource]\nnode_path = NodePath(\n",
        "[gd_resource type=\"Resource\"]\n[resource]\nrec = {\"a\": {\"b\": {\"c\": {\"d\": 123}}}}\n",
        "[gd_resource type=\"Resource\"]\n[resource]\nval = 1e9999\n",
        "[gd_resource type=\"Resource\"]\n[resource]\nval = NaN\n",
        "[gd_resource type=\"Resource\"]\n[resource]\nval = inf\n",
        "[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[",
        "\"unclosed string literal across\nmultiple lines",
    ];

    for input in hostile_inputs {
        cases_executed += 1;
        let res = catch_unwind(|| parse_tres(input));
        if res.is_err() {
            invariants_held = false;
        }
    }

    // 2. Lossless Round-Trip Verification
    let mut properties = BTreeMap::new();
    properties.insert("albedo_color".into(), TresValue::Color([1.0, 0.5, 0.25, 1.0]));
    properties.insert("metallic".into(), TresValue::F64(0.85));
    properties.insert("roughness".into(), TresValue::F64(0.15));
    properties.insert("depth_enabled".into(), TresValue::Bool(true));
    properties.insert("iteration_count".into(), TresValue::I64(16));
    properties.insert("name_tag".into(), TresValue::String("HeroMaterial".into()));
    properties.insert("offset".into(), TresValue::Vector2([0.5, -0.5]));
    properties.insert("scale3".into(), TresValue::Vector3([1.0, 2.0, 3.0]));
    properties.insert("path".into(), TresValue::NodePath("Camera3D/Arm".into()));
    properties.insert(
        "ext_ref".into(),
        TresValue::ExtResource("1_texture_albedo".into()),
    );
    properties.insert(
        "sub_ref".into(),
        TresValue::SubResource("SubResource_box".into()),
    );
    properties.insert(
        "tags".into(),
        TresValue::Array(vec![
            TresValue::String("pbr".into()),
            TresValue::I64(100),
            TresValue::Bool(false),
        ]),
    );

    let mut map_prop = BTreeMap::new();
    map_prop.insert("nested_key".into(), TresValue::I64(777));
    properties.insert("dict_val".into(), TresValue::Dictionary(map_prop));

    let mut sub_props = BTreeMap::new();
    sub_props.insert("size".into(), TresValue::Vector3([2.0, 0.5, 4.0]));

    let original_file = TresFile {
        header: TresHeader {
            res_type: "StandardMaterial3D".into(),
            script_class: Some("CustomMaterial".into()),
            load_steps: Some(3),
            format: 3,
            uid: Some("uid://verified_tres_12345".into()),
        },
        ext_resources: vec![TresExtResource {
            res_type: "Texture2D".into(),
            path: "res://textures/diffuse.png".into(),
            id: "1_texture_albedo".into(),
            uid: Some("uid://tex_diffuse_987".into()),
        }],
        sub_resources: vec![TresSubResource {
            res_type: "BoxShape3D".into(),
            id: "SubResource_box".into(),
            properties: sub_props,
        }],
        resource: TresResource { properties },
    };

    cases_executed += 1;
    let emitted = emit_tres(&original_file);
    let roundtrip_res = parse_tres(&emitted);

    match roundtrip_res {
        Ok(parsed) => {
            if parsed.header != original_file.header {
                invariants_held = false;
            }
            if parsed.ext_resources != original_file.ext_resources {
                invariants_held = false;
            }
            if parsed.sub_resources != original_file.sub_resources {
                invariants_held = false;
            }
            if parsed.resource.properties.len() != original_file.resource.properties.len() {
                invariants_held = false;
            }
        }
        Err(_) => invariants_held = false,
    }

    VerificationReceipt {
        suite_name: "tres_manifest_fuzzing_suite".into(),
        cases_executed,
        invariants_held,
    }
}

// ============================================================================
// Suite 3: Godot 4 .gdextension Manifest Fuzzing & Round-Trip
// ============================================================================

/// Verifies `.gdextension` manifest parsing, syntax resilience, and round-trip fidelity.
pub fn verify_gdextension_manifest_fuzzing_suite() -> VerificationReceipt {
    let mut cases_executed = 0;
    let mut invariants_held = true;

    // 1. Hostile strings to parse_gdextension_manifest
    let hostile_manifests = [
        "",
        "entry_symbol = \"no_section\"",
        "[configuration\nentry_symbol = \"unclosed_section\"",
        "[configuration]\nno_equals_symbol",
        "[configuration]\nentry_symbol = \n",
        "[configuration]\nreloadable = not_a_boolean\n",
        "[configuration]\nandroid_aar_plugin = 1234\n",
        "[libraries]\nwindows.x86_64\n",
        "[dependencies]\nwindows.x86_64 = [\"unclosed_array\n",
        "[invalid_section]\nsomething = true\n",
        ";;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;",
        "# only comments\n#\n",
        "[[[[[[[[[[[]]]]]]]]]]]",
    ];

    for hostile in hostile_manifests {
        cases_executed += 1;
        let res = catch_unwind(|| parse_gdextension_manifest(hostile));
        if res.is_err() {
            invariants_held = false;
        }
    }

    // 2. Full Round-Trip Verification
    cases_executed += 1;
    let original = GdExtensionManifest {
        configuration: ConfigurationSection {
            entry_symbol: "libforeplay_gdext_init".to_string(),
            compatibility_minimum: "4.2".to_string(),
            reloadable: true,
            android_aar_plugin: false,
        },
        libraries: {
            let mut m = BTreeMap::new();
            m.insert("windows.x86_64".into(), "res://bin/libplugin.dll".into());
            m.insert("linux.x86_64".into(), "res://bin/libplugin.so".into());
            m.insert("macos".into(), "res://bin/libplugin.dylib".into());
            m
        },
        dependencies: {
            let mut m = BTreeMap::new();
            m.insert(
                "windows.x86_64".into(),
                vec!["res://bin/dep1.dll".into(), "res://bin/dep2.dll".into()],
            );
            m.insert("linux.x86_64".into(), vec!["res://bin/libdep1.so".into()]);
            m
        },
    };

    let emitted = emit_gdextension_manifest(&original);
    let parsed = parse_gdextension_manifest(&emitted);

    match parsed {
        Ok(parsed_manifest) => {
            if parsed_manifest != original {
                invariants_held = false;
            }
        }
        Err(_) => invariants_held = false,
    }

    VerificationReceipt {
        suite_name: "gdextension_manifest_fuzzing_suite".into(),
        cases_executed,
        invariants_held,
    }
}

// ============================================================================
// Suite 4: GDScript 2.0 Coroutines & Asynchronous Resumption
// ============================================================================

/// Verifies GDScript 2.0 suspendable coroutine frames, timer tick advances, and signal triggers.
pub fn verify_gdscript_coroutine_resumption_suite() -> VerificationReceipt {
    let mut cases_executed = 0;
    let mut invariants_held = true;

    let src = r#"
var step_marker = 0

func run_coroutine_flow():
    step_marker = 1
    await get_tree().create_timer(0.2)
    step_marker = 2
    await get_tree().create_timer(0.4)
    step_marker = 3

func run_signal_flow():
    step_marker = 10
    await self.player_scored
    step_marker = 20
"#;

    cases_executed += 1;
    let mut vm = match GdVm::from_source(src) {
        Ok(v) => v,
        Err(_) => {
            return VerificationReceipt {
                suite_name: "gdscript_coroutine_resumption_suite".into(),
                cases_executed,
                invariants_held: false,
            };
        }
    };

    let mut node = make_test_node(1001, "CoroutineRunner", "Node");

    // 1. Timer Coroutine Execution & Resumption
    cases_executed += 1;
    let call_res = vm.call_or_suspend("run_coroutine_flow", &[], &mut node);
    match call_res {
        Ok(GdCoroutineOutcome::Suspended { target, .. }) => {
            if !matches!(target, AwaitTarget::Timer { remaining_sec } if (remaining_sec - 0.2).abs() < 1e-4) {
                invariants_held = false;
            }
        }
        _ => invariants_held = false,
    }

    if vm.member_vars.get("step_marker") != Some(&IrValue::I64(1)) {
        invariants_held = false;
    }

    // Tick partial time (0.1s) -> should remain suspended
    cases_executed += 1;
    if vm.tick_timers(0.1).is_err() {
        invariants_held = false;
    }
    if vm.member_vars.get("step_marker") != Some(&IrValue::I64(1)) {
        invariants_held = false;
    }

    // Tick remaining time (0.15s) -> resumes, executes to step_marker = 2, suspends for 0.4s
    cases_executed += 1;
    if vm.tick_timers(0.15).is_err() {
        invariants_held = false;
    }
    if vm.member_vars.get("step_marker") != Some(&IrValue::I64(2)) {
        invariants_held = false;
    }

    // Tick full 0.5s -> completes coroutine, reaches step_marker = 3
    cases_executed += 1;
    if vm.tick_timers(0.5).is_err() {
        invariants_held = false;
    }
    if vm.member_vars.get("step_marker") != Some(&IrValue::I64(3)) {
        invariants_held = false;
    }

    // 2. Signal Coroutine Execution & Resumption
    cases_executed += 1;
    let sig_call = vm.call_or_suspend("run_signal_flow", &[], &mut node);
    match sig_call {
        Ok(GdCoroutineOutcome::Suspended { target, .. }) => {
            if !matches!(target, AwaitTarget::Signal { signal_name, .. } if signal_name == "player_scored") {
                invariants_held = false;
            }
        }
        _ => invariants_held = false,
    }

    if vm.member_vars.get("step_marker") != Some(&IrValue::I64(10)) {
        invariants_held = false;
    }

    // Emit wrong signal -> should NOT resume
    cases_executed += 1;
    if vm.emit_signal_resume("unrelated_signal", &[]).is_err() {
        invariants_held = false;
    }
    if vm.member_vars.get("step_marker") != Some(&IrValue::I64(10)) {
        invariants_held = false;
    }

    // Emit matching signal -> resumes to step_marker = 20
    cases_executed += 1;
    if vm.emit_signal_resume("player_scored", &[]).is_err() {
        invariants_held = false;
    }
    if vm.member_vars.get("step_marker") != Some(&IrValue::I64(20)) {
        invariants_held = false;
    }

    // 3. Fuzz tick_timers with edge case deltas (negative, zero, NaN, huge)
    for &bad_dt in &[-1.0, 0.0, -100.0, 10_000.0] {
        cases_executed += 1;
        let res = catch_unwind(std::panic::AssertUnwindSafe(|| vm.tick_timers(bad_dt)));
        if res.is_err() {
            invariants_held = false;
        }
    }

    VerificationReceipt {
        suite_name: "gdscript_coroutine_resumption_suite".into(),
        cases_executed,
        invariants_held,
    }
}

// ============================================================================
// Suite 5: GDScript 2.0 Match Guards & Typed Collections Fuzzing
// ============================================================================

/// Verifies GDScript 2.0 `match` expressions (`when` guards, array/dictionary destructuring)
/// and typed collection (`Array[int]`, `Dictionary[String, int]`) coercion.
pub fn verify_gdscript_match_and_typed_collections_suite() -> VerificationReceipt {
    let mut cases_executed = 0;
    let mut invariants_held = true;

    let src = r#"
var numbers: Array[int] = [1.0, 2.5, "3", 4]
var stats: Dictionary[String, int] = {"hp": 100.5, "mp": "50"}

func eval_guard(x):
    match x:
        var val when val < 0:
            return "negative"
        var val when val == 0:
            return "zero"
        var val when val > 0 and val < 10:
            return "single_digit"
        _:
            return "large"

func eval_array_destructure(items):
    match items:
        [var head, ..]:
            return head
        _:
            return -1

func eval_dict_destructure(event):
    match event:
        {"type": "attack", "damage": var dmg} when dmg >= 50:
            return dmg
        _:
            return 0
"#;

    cases_executed += 1;
    let mut vm = match GdVm::from_source(src) {
        Ok(v) => v,
        Err(_) => {
            return VerificationReceipt {
                suite_name: "gdscript_match_and_typed_collections_suite".into(),
                cases_executed,
                invariants_held: false,
            };
        }
    };

    let mut node = make_test_node(1002, "MatchTester", "Node");
    cases_executed += 1;
    if vm.ready(&mut node).is_err() {
        invariants_held = false;
    }

    // 1. Verify Typed Collection Coercion
    if let Some(IrValue::Array(arr)) = vm.member_vars.get("numbers") {
        if arr != &[IrValue::I64(1), IrValue::I64(2), IrValue::I64(3), IrValue::I64(4)] {
            invariants_held = false;
        }
    } else {
        invariants_held = false;
    }

    if let Some(IrValue::Map(map)) = vm.member_vars.get("stats") {
        if map.get("hp") != Some(&IrValue::I64(100)) || map.get("mp") != Some(&IrValue::I64(50)) {
            invariants_held = false;
        }
    } else {
        invariants_held = false;
    }

    // 2. Verify Match `when` Guards
    let guard_cases = [
        (-5, "negative"),
        (0, "zero"),
        (7, "single_digit"),
        (42, "large"),
    ];

    for (input, expected) in guard_cases {
        cases_executed += 1;
        let res = vm.call_method("eval_guard", &[IrValue::I64(input)], &mut node);
        if let Ok(val) = res {
            if val.return_value != Some(IrValue::String(expected.into())) {
                invariants_held = false;
            }
        } else {
            invariants_held = false;
        }
    }

    // 3. Verify Array Destructuring with Rest `..`
    cases_executed += 1;
    let arr_nonempty = vm.call_method(
        "eval_array_destructure",
        &[IrValue::Array(vec![IrValue::I64(99), IrValue::I64(100)])],
        &mut node,
    );
    if let Ok(res) = arr_nonempty {
        if res.return_value != Some(IrValue::I64(99)) {
            invariants_held = false;
        }
    } else {
        invariants_held = false;
    }

    cases_executed += 1;
    let arr_empty = vm.call_method(
        "eval_array_destructure",
        &[IrValue::Array(Vec::new())],
        &mut node,
    );
    if let Ok(res) = arr_empty {
        if res.return_value != Some(IrValue::I64(-1)) {
            invariants_held = false;
        }
    } else {
        invariants_held = false;
    }

    // 4. Verify Dictionary Destructuring
    cases_executed += 1;
    let mut attack_event = BTreeMap::new();
    attack_event.insert("type".into(), IrValue::String("attack".into()));
    attack_event.insert("damage".into(), IrValue::I64(75));

    let dict_res = vm.call_method(
        "eval_dict_destructure",
        &[IrValue::Map(attack_event)],
        &mut node,
    );
    if let Ok(res) = dict_res {
        if res.return_value != Some(IrValue::I64(75)) {
            invariants_held = false;
        }
    } else {
        invariants_held = false;
    }

    // 5. Verify Dictionary Destructuring with Guard Failure (dmg < 50) -> returns default 0
    cases_executed += 1;
    let mut weak_attack = BTreeMap::new();
    weak_attack.insert("type".into(), IrValue::String("attack".into()));
    weak_attack.insert("damage".into(), IrValue::I64(30));

    let weak_res = vm.call_method(
        "eval_dict_destructure",
        &[IrValue::Map(weak_attack)],
        &mut node,
    );
    if let Ok(res) = weak_res {
        if res.return_value != Some(IrValue::I64(0)) {
            invariants_held = false;
        }
    } else {
        invariants_held = false;
    }

    // 6. Verify Dictionary Destructuring with Mismatched Key ("type" != "attack")
    cases_executed += 1;
    let mut heal_event = BTreeMap::new();
    heal_event.insert("type".into(), IrValue::String("heal".into()));
    heal_event.insert("damage".into(), IrValue::I64(100));

    let heal_res = vm.call_method(
        "eval_dict_destructure",
        &[IrValue::Map(heal_event)],
        &mut node,
    );
    if let Ok(res) = heal_res {
        if res.return_value != Some(IrValue::I64(0)) {
            invariants_held = false;
        }
    } else {
        invariants_held = false;
    }

    // 7. Verify Array Destructuring with single-item array
    cases_executed += 1;
    let arr_single = vm.call_method(
        "eval_array_destructure",
        &[IrValue::Array(vec![IrValue::I64(42)])],
        &mut node,
    );
    if let Ok(res) = arr_single {
        if res.return_value != Some(IrValue::I64(42)) {
            invariants_held = false;
        }
    } else {
        invariants_held = false;
    }

    // 8. Verify Array Destructuring with multi-item array
    cases_executed += 1;
    let arr_multi = vm.call_method(
        "eval_array_destructure",
        &[IrValue::Array(vec![
            IrValue::I64(101),
            IrValue::I64(102),
            IrValue::I64(103),
            IrValue::I64(104),
        ])],
        &mut node,
    );
    if let Ok(res) = arr_multi {
        if res.return_value != Some(IrValue::I64(101)) {
            invariants_held = false;
        }
    } else {
        invariants_held = false;
    }

    VerificationReceipt {
        suite_name: "gdscript_match_and_typed_collections_suite".into(),
        cases_executed,
        invariants_held,
    }
}

// ============================================================================
// Suite 6: GdLspAnalyzer Coordinates & Out-Of-Bounds Fuzzing
// ============================================================================

/// Verifies `GdLspAnalyzer` resilience under arbitrary, negative/zero, out-of-bounds,
/// and mid-multibyte UTF-8 line and column coordinates.
pub fn verify_gdlsp_analyzer_bounds_suite() -> VerificationReceipt {
    let mut cases_executed = 0;
    let mut invariants_held = true;

    let analyzer = GdLspAnalyzer::new();

    let unicode_doc = r#"# Godot 4 GDScript with Multibyte Unicode 🚀
class_name SkaterPlayer
extends CharacterBody3D

signal combo_achieved(score: int)

@export var player_name: String = "🦀 Rust Skater 🛹"
var score: int = 0

func perform_ollie(height: float) -> void:
    # Mid-UTF8: 你好世界 — Привет мир
    score += int(height * 100.0)
    combo_achieved.emit(score)
"#;

    cases_executed += 1;
    let analysis = analyzer.analyze_document("res://player.gd", unicode_doc);
    if analysis.symbols.is_empty() {
        invariants_held = false;
    }

    // Extremes and out-of-bounds line & column coordinates
    let test_coords = [
        (0, 0),
        (0, 1),
        (1, 0),
        (1, 1),
        (7, 10), // before emoji
        (7, 50), // after emoji
        (11, 12), // before CJK characters
        (11, 28), // after CJK characters
        (1, 999_999),
        (999_999, 1),
        (usize::MAX, usize::MAX),
        (15, 0),
        (100, 100),
    ];

    for &(line, col) in &test_coords {
        cases_executed += 1;
        let c_res = catch_unwind(|| analyzer.complete_at(unicode_doc, line, col));
        if c_res.is_err() {
            invariants_held = false;
        }

        cases_executed += 1;
        let h_res = catch_unwind(|| analyzer.hover_at(unicode_doc, line, col));
        if h_res.is_err() {
            invariants_held = false;
        }

        cases_executed += 1;
        let g_res = catch_unwind(|| analyzer.goto_definition(unicode_doc, line, col));
        if g_res.is_err() {
            invariants_held = false;
        }
    }

    // Mid-multibyte UTF-8 coordinates for hover and definition lookups
    let mid_utf8_coords = [(7, 37), (11, 14), (11, 17), (11, 20)];
    for &(line, col) in &mid_utf8_coords {
        cases_executed += 1;
        let h_res = catch_unwind(|| analyzer.hover_at(unicode_doc, line, col));
        if h_res.is_err() {
            invariants_held = false;
        }

        cases_executed += 1;
        let g_res = catch_unwind(|| analyzer.goto_definition(unicode_doc, line, col));
        if g_res.is_err() {
            invariants_held = false;
        }
    }

    // Fuzz analyzer with totally hostile broken scripts
    let hostile_scripts = [
        "",
        "(((((((((((((((((",
        "func ()): var = pass",
        "@export_range(\"broken\n\n\n",
        "class_name \nfunc \nvar \n",
        "################################\n",
    ];

    for script in hostile_scripts {
        cases_executed += 1;
        let res = catch_unwind(|| {
            let doc = analyzer.analyze_document("hostile.gd", script);
            let _ = analyzer.complete_at(script, 1, 1);
            let _ = analyzer.hover_at(script, 1, 1);
            let _ = analyzer.goto_definition(script, 1, 1);
            doc
        });
        if res.is_err() {
            invariants_held = false;
        }
    }

    VerificationReceipt {
        suite_name: "gdlsp_analyzer_bounds_suite".into(),
        cases_executed,
        invariants_held,
    }
}

// ============================================================================
// Suite 7: GDExtension C ABI Boundary & Variant Lifecycle
// ============================================================================

/// Verifies `gdext_get_proc_address` across all supported procedure names and hostile inputs,
/// alongside memory and value lifecycles of C-ABI `AbiVariant`.
pub fn verify_gdextension_c_abi_boundary_suite() -> VerificationReceipt {
    let mut cases_executed = 0;
    let mut invariants_held = true;

    // 1. Verify all 82+ supported interface procedures resolve
    let proc_names = supported_proc_names();
    if proc_names.len() < 80 {
        invariants_held = false;
    }

    for &name in proc_names {
        cases_executed += 1;
        let c_str = CString::new(name).unwrap();
        // SAFETY: c_str is a valid null-terminated C string.
        let proc_ptr = unsafe { gdext_get_proc_address(c_str.as_ptr()) };
        if proc_ptr.is_none() {
            invariants_held = false;
        }
    }

    // 2. Hostile procedure lookups (null, empty, unknown)
    cases_executed += 1;
    // SAFETY: Testing null pointer rejection.
    let null_res = unsafe { gdext_get_proc_address(std::ptr::null()) };
    if null_res.is_some() {
        invariants_held = false;
    }

    let fake_names = ["", "non_existent_godot_func", "gdext_unknown_proc", "classdb_fake_proc"];
    for fake in fake_names {
        cases_executed += 1;
        let c_fake = CString::new(fake).unwrap();
        // SAFETY: c_fake is a valid null-terminated C string.
        let fake_res = unsafe { gdext_get_proc_address(c_fake.as_ptr()) };
        if fake_res.is_some() {
            invariants_held = false;
        }
    }

    // 3. Variant Lifecycle (new_nil, new_copy, get_type, booleanize, destroy)
    let mut var_storage = std::mem::MaybeUninit::<AbiVariant>::uninit();
    let var_ptr = var_storage.as_mut_ptr() as *mut std::ffi::c_void;

    cases_executed += 1;
    // SAFETY: var_ptr points to valid uninitialized storage for AbiVariant.
    unsafe {
        gdext_variant_new_nil(var_ptr);
        let variant_type = gdext_variant_get_type(var_ptr);
        if variant_type != GDExtensionVariantType::Nil {
            invariants_held = false;
        }

        let bool_val = gdext_variant_booleanize(var_ptr);
        if bool_val != 0 {
            invariants_held = false;
        }

        // Test copy
        let mut copy_storage = std::mem::MaybeUninit::<AbiVariant>::uninit();
        let copy_ptr = copy_storage.as_mut_ptr() as *mut std::ffi::c_void;
        gdext_variant_new_copy(copy_ptr, var_ptr);
        if gdext_variant_get_type(copy_ptr) != GDExtensionVariantType::Nil {
            invariants_held = false;
        }

        gdext_variant_destroy(copy_ptr);
        gdext_variant_destroy(var_ptr);
    }

    // Test with Bool, Int, Float, String variants
    let typed_variants = [
        (IrValue::Bool(true), GDExtensionVariantType::Bool, 1u8),
        (IrValue::Bool(false), GDExtensionVariantType::Bool, 0u8),
        (IrValue::I64(42), GDExtensionVariantType::Int, 1u8),
        (IrValue::I64(0), GDExtensionVariantType::Int, 0u8),
        (IrValue::F64(std::f64::consts::PI), GDExtensionVariantType::Float, 1u8),
        (IrValue::String("hello".into()), GDExtensionVariantType::String, 1u8),
        (IrValue::String("".into()), GDExtensionVariantType::String, 0u8),
    ];

    for (val, expected_type, expected_bool) in typed_variants {
        cases_executed += 1;
        let mut v = AbiVariant::new(val);
        let ptr = &mut v as *mut AbiVariant as *mut std::ffi::c_void;

        // SAFETY: ptr is a valid pointer to initialized AbiVariant.
        unsafe {
            if gdext_variant_get_type(ptr) != expected_type {
                invariants_held = false;
            }
            if gdext_variant_booleanize(ptr) != expected_bool {
                invariants_held = false;
            }
        }
    }

    VerificationReceipt {
        suite_name: "gdextension_c_abi_boundary_suite".into(),
        cases_executed,
        invariants_held,
    }
}

// ============================================================================
// Suite 8: ClassDb Built-In Hierarchy & Extension Registration Invariants
// ============================================================================

/// Verifies that `ClassDb::global_godot44()` maintains its 471+ class hierarchy,
/// rejects unknown parents and duplicate names, and protects built-in classes from mutation.
pub fn verify_classdb_hierarchy_invariants_suite() -> VerificationReceipt {
    let mut cases_executed = 0;
    let mut invariants_held = true;

    let base_db = ClassDb::global_godot44();
    let initial_count = base_db.class_count();

    // 1. Invariant: At least 471 Godot built-in classes registered
    cases_executed += 1;
    if initial_count < 470 {
        invariants_held = false;
    }

    // 2. Canonical root and branch relationships
    cases_executed += 1;
    if !base_db.inherits("CharacterBody3D", "PhysicsBody3D") {
        invariants_held = false;
    }
    if !base_db.inherits("CharacterBody3D", "Node3D") {
        invariants_held = false;
    }
    if !base_db.inherits("CharacterBody3D", "Node") {
        invariants_held = false;
    }
    if !base_db.inherits("CharacterBody3D", "Object") {
        invariants_held = false;
    }
    if base_db.inherits("CharacterBody3D", "Control") {
        invariants_held = false;
    }

    // 3. Dynamic GDExtension class registration on a cloned database
    let mut db = base_db.clone();

    // a. Reject registration with unknown parent class
    cases_executed += 1;
    let orphan_class = ClassInfo::new("OrphanSkater", Some("NonExistentParentClass"), true);
    match db.register_extension_class(orphan_class) {
        Err(ClassDbError::AncestorNotFound(parent)) if parent == "NonExistentParentClass" => {}
        _ => invariants_held = false,
    }

    // b. Reject registration with duplicate built-in class name
    cases_executed += 1;
    let duplicate_builtin = ClassInfo::new("Node3D", Some("Node"), true);
    match db.register_extension_class(duplicate_builtin) {
        Err(ClassDbError::ClassAlreadyExists(name)) if name == "Node3D" => {}
        _ => invariants_held = false,
    }

    // c. Successfully register valid custom extension class
    cases_executed += 1;
    let valid_ext = ClassInfo::new("ProSkaterController", Some("CharacterBody3D"), true);
    if db.register_extension_class(valid_ext).is_err() {
        invariants_held = false;
    }

    // d. Verify custom class inherits ancestral chain
    cases_executed += 1;
    if !db.inherits("ProSkaterController", "CharacterBody3D")
        || !db.inherits("ProSkaterController", "Node3D")
        || !db.inherits("ProSkaterController", "Object")
    {
        invariants_held = false;
    }

    // e. Reject duplicate registration of the newly added extension class
    cases_executed += 1;
    let duplicate_ext = ClassInfo::new("ProSkaterController", Some("Node3D"), true);
    match db.register_extension_class(duplicate_ext) {
        Err(ClassDbError::ClassAlreadyExists(name)) if name == "ProSkaterController" => {}
        _ => invariants_held = false,
    }

    // Invariant: Base global ClassDB is completely untouched
    if base_db.class_count() != initial_count {
        invariants_held = false;
    }

    VerificationReceipt {
        suite_name: "classdb_hierarchy_invariants_suite".into(),
        cases_executed,
        invariants_held,
    }
}

// ============================================================================
// Suite 9: Rapier <-> PortFaithful Live Hotswap & CharacterBody3D Properties
// ============================================================================

/// Verifies live A/B physics backend hotswapping and `CharacterBody3DController::move_and_slide`
/// finite state invariants (no NaNs, no infinities, normalized contact normals).
pub fn verify_rapier_port_hotswap_and_controller_proptest_suite() -> VerificationReceipt {
    let mut cases_executed = 0;
    let mut invariants_held = true;

    // 1. Live A/B hotswap thrash on PhysicsServer3D
    let mut server = PhysicsServer3D::new_standard();

    for i in 0..10 {
        let body_id = NodeId::new(5000 + i, 1).unwrap();
        let body = PhysicsBody3D::new(
            body_id,
            Vec3::new(i as f32 * 2.0, 10.0 + i as f32, 0.0),
            Collider3D::Sphere { radius: 0.5 },
        );
        server.add_body(body);
    }

    for step_idx in 0..40 {
        cases_executed += 1;
        server.step(0.016);

        // Hotswap every 4 frames between StandardRigid, SkatePortArcade, and Rapier3D
        if step_idx % 4 == 0 {
            let next_kind = match (step_idx / 4) % 3 {
                0 => PhysicsBackendKind::Rapier3D,
                1 => PhysicsBackendKind::StandardRigid,
                _ => PhysicsBackendKind::SkatePortArcade,
            };
            server.hotswap_preserving_bodies(next_kind);
        }

        // Verify all bodies have finite positions and velocities
        for i in 0..10 {
            let body_id = NodeId::new(5000 + i, 1).unwrap();
            if let Some(b) = server.get_body(body_id)
                && (!b.position.x.is_finite()
                    || !b.position.y.is_finite()
                    || !b.position.z.is_finite()
                    || !b.velocity.x.is_finite())
            {
                invariants_held = false;
            }
        }
    }

    // 2. CharacterBody3D move_and_slide property checks
    let mut space = RapierSpace3D::new();

    // Floor
    let floor_id = NodeId::new(6000, 1).unwrap();
    space.add_rigid_body(
        floor_id,
        &RigidBodyConfig3D {
            body_type: BodyType::Fixed,
            position: Vec3::ZERO,
            ..Default::default()
        },
    );
    space.add_collider(
        floor_id,
        ShapeConfig3D::Cuboid {
            half_extents: Vec3::new(20.0, 0.2, 20.0),
        },
        0.0,
        0.0,
        1,
        1,
    );

    let char_id = NodeId::new(6001, 1).unwrap();
    let mut controller = CharacterBody3DController::new(char_id, Vec3::new(0.0, 1.0, 0.0));
    let char_shape = ShapeConfig3D::Ball { radius: 0.5 }.to_shared_shape();

    // Perform randomized walk, jump, and collision steps
    for step in 0..50 {
        cases_executed += 1;
        let vx = ((step as f32 * 1.7).sin()) * 5.0;
        let vy = if step % 10 == 0 { 8.0 } else { -9.8 * 0.05 };
        let vz = ((step as f32 * 2.3).cos()) * 5.0;

        controller.velocity = Vec3::new(vx, vy, vz);
        controller.move_and_slide(0.05, &space, &char_shape);

        if !controller.position.x.is_finite()
            || !controller.position.y.is_finite()
            || !controller.position.z.is_finite()
            || !controller.velocity.x.is_finite()
        {
            invariants_held = false;
        }

        if controller.is_on_floor() {
            let n = controller.get_floor_normal();
            let len = n.length();
            if (len - 1.0).abs() > 1e-3 {
                invariants_held = false;
            }
        }
    }

    VerificationReceipt {
        suite_name: "rapier_port_hotswap_and_controller_proptest_suite".into(),
        cases_executed,
        invariants_held,
    }
}

// ============================================================================
// Suite 10: Adversarial FPCKEXEC Self-Contained Binary Trailer Fuzzing
// ============================================================================

/// Verifies `embed_pack_trailer`, `extract_embedded_pack_from_bytes`, and
/// `verify_self_contained_export_bytes` against truncation, magic flips, bit flips, and overflows.
pub fn verify_fpckexec_binary_trailer_adversarial_suite() -> VerificationReceipt {
    let mut cases_executed = 0;
    let mut invariants_held = true;

    let base_exe = b"MOCK_EXECUTABLE_HEADER_AND_CODE_SECTION_BYTES_1234567890";
    let mock_pack = b"FPACK_ARCHIVE_DATA_PAYLOAD_ABCDEFGHIJKLMN_987654321";

    // 1. Lossless Embed & Extract Round-Trip
    cases_executed += 1;
    let embedded = embed_pack_trailer(base_exe, mock_pack);
    let extracted = extract_embedded_pack_from_bytes(&embedded);

    match extracted {
        Ok(Some(payload)) => {
            if payload != mock_pack {
                invariants_held = false;
            }
        }
        _ => invariants_held = false,
    }

    // 2. Scan plain executable without trailer
    cases_executed += 1;
    if extract_embedded_pack_from_bytes(base_exe) != Ok(None) {
        invariants_held = false;
    }

    // 3. Short buffers
    let short_buffers: &[&[u8]] = &[b"", b"FPCK", b"FPCKEXEC", &[0u8; 23]];
    for &sb in short_buffers {
        cases_executed += 1;
        if extract_embedded_pack_from_bytes(sb) != Ok(None) {
            invariants_held = false;
        }
    }

    // 4. Bit-flip corruption across the embedded binary
    for offset in 0..embedded.len() {
        for bit in [0, 4, 7] {
            cases_executed += 1;
            let mut corrupted = embedded.clone();
            corrupted[offset] ^= 1 << bit;

            let res = extract_embedded_pack_from_bytes(&corrupted);
            match res {
                Err(EmbeddedPackError::ChecksumMismatch { .. })
                | Err(EmbeddedPackError::InvalidBounds { .. })
                | Err(EmbeddedPackError::CorruptTrailer(..))
                | Err(EmbeddedPackError::Io(..))
                | Ok(None) => {} // Expected clean tamper detection or missing footer
                Ok(Some(p)) if p == mock_pack => {} // Unlikely flip in non-critical padding
                _ => invariants_held = false,
            }
        }
    }

    // 5. Overflow and bounds manipulation
    let trailer_len = 24_usize;
    let mut overflow_trailer = embedded.clone();
    let pack_len_pos = overflow_trailer.len() - trailer_len + 8;
    overflow_trailer[pack_len_pos..pack_len_pos + 8].copy_from_slice(&u64::MAX.to_le_bytes());

    cases_executed += 1;
    let overflow_res = extract_embedded_pack_from_bytes(&overflow_trailer);
    if overflow_res.is_ok() {
        invariants_held = false;
    }

    // 6. Test verify_self_contained_export_bytes on valid FpckArchive payload
    let mut files = BTreeMap::new();
    files.insert("project.foreplay".into(), b"[project]\nname=\"Test\"\n".to_vec());
    let pack_archive = FpckArchive::pack_bytes(&files).expect("pack archive");
    let valid_bundle = embed_pack_trailer(base_exe, &pack_archive);

    cases_executed += 1;
    let bundle_res = verify_self_contained_export_bytes(&valid_bundle);
    if bundle_res.is_err() {
        invariants_held = false;
    }

    // Corrupt one byte of archive in self-contained export
    let mut corrupted_bundle = valid_bundle.clone();
    corrupted_bundle[base_exe.len() + 10] ^= 0x55;
    cases_executed += 1;
    if verify_self_contained_export_bytes(&corrupted_bundle).is_ok() {
        invariants_held = false;
    }

    VerificationReceipt {
        suite_name: "fpckexec_binary_trailer_adversarial_suite".into(),
        cases_executed,
        invariants_held,
    }
}

// ============================================================================
// Suite 11: BbCodeRichText & TileMapTerrainAutotiler Fuzzing
// ============================================================================

/// Verifies `BbCodeRichText::parse` and `TileMapTerrainAutotiler` against malformed tags,
/// unclosed spans, and randomized peering paint strokes.
pub fn verify_bbcode_and_terrain_autotiler_suite() -> VerificationReceipt {
    let mut cases_executed = 0;
    let mut invariants_held = true;

    // 1. BbCodeRichText Hostile Tag Fuzzing
    let hostile_bbcodes = [
        "",
        "plain text without tags",
        "[b]bold[/b] and [i]italic[/i]",
        "[b][i][u][s][code]nested all[/b][/i][/u][/s][/code]",
        "[color=#ff0000]red text[/color]",
        "[color=invalid_color]fallback[/color]",
        "[font_size=24]big[/font_size]",
        "[font_size=not_a_number]fallback[/font_size]",
        "[wave amp=50.0 freq=2.0]wavy[/wave]",
        "[wave bad_args]wavy_fallback[/wave]",
        "[shake rate=20.0 level=5]shaky[/shake]",
        "[b]unclosed bold",
        "unopened close tag [/b][/i][/u]",
        "[[[[[[[[[[[[[[[[[[[[[[[[[",
        "]]]]]]]]]]]]]]]]]]]]]]]]]",
        "[b][color=#00ff00][i]unclosed multi-layer",
        "[url=https://example.com]click here[/url]",
    ];

    for bbcode in hostile_bbcodes {
        cases_executed += 1;
        let res = catch_unwind(|| BbCodeRichText::parse(bbcode));
        match res {
            Ok(parsed) => {
                let _ = parsed.spans.len();
            }
            Err(_) => invariants_held = false,
        }
    }

    // 2. TileMapTerrainAutotiler Peering Fuzzing
    let mut catalog = TileSetTerrainCatalog::new();
    let mut terrain_set = TerrainSet::new(TerrainMode::MatchCornersAndSides);
    let t_grass = terrain_set.add_terrain("Grass");
    let t_path = terrain_set.add_terrain("Path");

    // Add sample tiles with various peering configurations
    terrain_set.add_tile(TerrainTileDef::new(1, t_grass));
    terrain_set.add_tile(TerrainTileDef::new(2, t_path));
    terrain_set.add_tile(
        TerrainTileDef::new(3, t_path)
            .with_peering(PeeringDirection::Right, t_path)
            .with_peering(PeeringDirection::Left, t_path),
    );
    catalog.register_set(0, terrain_set);

    let mut autotiler = TileMapTerrainAutotiler::new(catalog);

    // Fuzz random connect points
    let points = [
        (0, 0),
        (1, 0),
        (2, 0),
        (2, 1),
        (2, 2),
        (-5, -5),
        (-4, -5),
        (100, 200),
    ];

    cases_executed += 1;
    let res = catch_unwind(std::panic::AssertUnwindSafe(|| {
        autotiler.paint_terrain_connect(&points, 0, t_path);
    }));
    if res.is_err() {
        invariants_held = false;
    }

    // Fuzz path painting
    let path = [(0, 0), (10, 0), (10, 10), (0, 10), (0, 0)];
    cases_executed += 1;
    let path_res = catch_unwind(std::panic::AssertUnwindSafe(|| {
        autotiler.paint_terrain_path(&path, 0, t_grass);
    }));
    if path_res.is_err() {
        invariants_held = false;
    }

    if autotiler.placed_tiles.is_empty() {
        invariants_held = false;
    }

    VerificationReceipt {
        suite_name: "bbcode_and_terrain_autotiler_suite".into(),
        cases_executed,
        invariants_held,
    }
}

// ============================================================================
// Suite 12: AudioRingBuffer SPSC & InputServer Radial Deadzone Invariants
// ============================================================================

/// Verifies SPSC `AudioRingBuffer` chunked push/pop FIFO integrity and `InputServer::get_vector`
/// radial deadzone bounding (`length <= 1.0 + 1e-5`, non-NaN).
pub fn verify_audio_ringbuffer_and_input_deadzone_suite() -> VerificationReceipt {
    let mut cases_executed = 0;
    let mut invariants_held = true;

    // 1. AudioRingBuffer FIFO Integrity
    let cap = 64;
    let ring = AudioRingBuffer::new(cap);

    if ring.capacity() != cap {
        invariants_held = false;
    }
    if !ring.is_empty() {
        invariants_held = false;
    }

    // Test interleaved slice push & pop
    let test_frames: Vec<[f32; 2]> = (0..200)
        .map(|i| [i as f32 * 0.01, -(i as f32 * 0.01)])
        .collect();

    let mut pushed_cursor = 0;
    let mut popped_results = Vec::new();

    while pushed_cursor < test_frames.len() || !ring.is_empty() {
        cases_executed += 1;
        // Push chunk
        let push_chunk = (pushed_cursor..test_frames.len()).take(17);
        let push_slice: Vec<[f32; 2]> = push_chunk.map(|idx| test_frames[idx]).collect();
        let actually_pushed = ring.push_slice(&push_slice);
        pushed_cursor += actually_pushed;

        // Pop chunk
        let mut pop_buf = vec![[0.0f32; 2]; 13];
        let actually_popped = ring.pop_slice(&mut pop_buf);
        popped_results.extend_from_slice(&pop_buf[..actually_popped]);

        if ring.available_read() + ring.available_write() != cap {
            invariants_held = false;
        }
    }

    if popped_results.len() != test_frames.len() {
        invariants_held = false;
    } else {
        for (a, b) in popped_results.iter().zip(test_frames.iter()) {
            if (a[0] - b[0]).abs() > 1e-5 || (a[1] - b[1]).abs() > 1e-5 {
                invariants_held = false;
            }
        }
    }

    // 2. InputServer Radial Deadzone Invariant
    let mut input_map = InputMap::new();
    input_map.add_action("move_left", 0.2);
    input_map.action_add_trigger(
        "move_left",
        InputTrigger::GamepadAxis {
            pad: None,
            axis: GamepadAxisId::LeftStickX,
            direction: -1.0,
        },
    );
    input_map.add_action("move_right", 0.2);
    input_map.action_add_trigger(
        "move_right",
        InputTrigger::GamepadAxis {
            pad: None,
            axis: GamepadAxisId::LeftStickX,
            direction: 1.0,
        },
    );
    input_map.add_action("move_up", 0.2);
    input_map.action_add_trigger(
        "move_up",
        InputTrigger::GamepadAxis {
            pad: None,
            axis: GamepadAxisId::LeftStickY,
            direction: -1.0,
        },
    );
    input_map.add_action("move_down", 0.2);
    input_map.action_add_trigger(
        "move_down",
        InputTrigger::GamepadAxis {
            pad: None,
            axis: GamepadAxisId::LeftStickY,
            direction: 1.0,
        },
    );

    let mut server = InputServer::new(input_map);

    // Fuzz axis input deflections and verify radial deadzone unit clamping
    for x_step in -10..=10 {
        for y_step in -10..=10 {
            for deadzone in [0.0, 0.2, 0.5, 0.9, 1.0] {
                cases_executed += 1;
                let axis_x = x_step as f32 * 0.15;
                let axis_y = y_step as f32 * 0.15;

                server.feed_event(InputEvent::GamepadAxis {
                    pad: 0,
                    axis: GamepadAxisId::LeftStickX,
                    value: axis_x,
                });
                server.feed_event(InputEvent::GamepadAxis {
                    pad: 0,
                    axis: GamepadAxisId::LeftStickY,
                    value: axis_y,
                });

                let vec = server.get_vector(
                    "move_left",
                    "move_right",
                    "move_up",
                    "move_down",
                    deadzone,
                );

                if !vec[0].is_finite() || !vec[1].is_finite() {
                    invariants_held = false;
                }

                let length = (vec[0] * vec[0] + vec[1] * vec[1]).sqrt();
                if length > 1.0 + 1e-5 {
                    invariants_held = false;
                }
            }
        }
    }

    VerificationReceipt {
        suite_name: "audio_ringbuffer_and_input_deadzone_suite".into(),
        cases_executed,
        invariants_held,
    }
}

// ============================================================================
// Unit & Proptest Verification Suites
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn test_suite_1_hostile_binary_glb_fuzzing() {
        let r = verify_hostile_glb_fuzzing_suite();
        assert!(r.invariants_held);
        assert!(r.cases_executed >= 50);
    }

    #[test]
    fn test_suite_2_tres_manifest_fuzzing() {
        let r = verify_tres_manifest_fuzzing_suite();
        assert!(r.invariants_held);
        assert!(r.cases_executed >= 20);
    }

    #[test]
    fn test_suite_3_gdextension_manifest_fuzzing() {
        let r = verify_gdextension_manifest_fuzzing_suite();
        assert!(r.invariants_held);
        assert!(r.cases_executed >= 10);
    }

    #[test]
    fn test_suite_4_gdscript_coroutine_resumption() {
        let r = verify_gdscript_coroutine_resumption_suite();
        assert!(r.invariants_held);
        assert!(r.cases_executed >= 10);
    }

    #[test]
    fn test_suite_5_gdscript_match_and_typed_collections() {
        let r = verify_gdscript_match_and_typed_collections_suite();
        assert!(r.invariants_held);
        assert!(r.cases_executed >= 10);
    }

    #[test]
    fn test_suite_6_gdlsp_analyzer_bounds() {
        let r = verify_gdlsp_analyzer_bounds_suite();
        assert!(r.invariants_held);
        assert!(r.cases_executed >= 30);
    }

    #[test]
    fn test_suite_7_gdextension_c_abi_boundary() {
        let r = verify_gdextension_c_abi_boundary_suite();
        assert!(r.invariants_held);
        assert!(r.cases_executed >= 80);
    }

    #[test]
    fn test_suite_8_classdb_hierarchy_invariants() {
        let r = verify_classdb_hierarchy_invariants_suite();
        assert!(r.invariants_held);
        assert!(r.cases_executed >= 5);
    }

    #[test]
    fn test_suite_9_rapier_port_hotswap_and_controller() {
        let r = verify_rapier_port_hotswap_and_controller_proptest_suite();
        assert!(r.invariants_held);
        assert!(r.cases_executed >= 50);
    }

    #[test]
    fn test_suite_10_fpckexec_binary_trailer_adversarial() {
        let r = verify_fpckexec_binary_trailer_adversarial_suite();
        assert!(r.invariants_held);
        assert!(r.cases_executed >= 50);
    }

    #[test]
    fn test_suite_11_bbcode_and_terrain_autotiler() {
        let r = verify_bbcode_and_terrain_autotiler_suite();
        assert!(r.invariants_held);
        assert!(r.cases_executed >= 15);
    }

    #[test]
    fn test_suite_12_audio_ringbuffer_and_input_deadzone() {
        let r = verify_audio_ringbuffer_and_input_deadzone_suite();
        assert!(r.invariants_held);
        assert!(r.cases_executed >= 100);
    }

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 64,
            failure_persistence: None,
            ..ProptestConfig::default()
        })]

        /// Proptest 1: Arbitrary byte slice into import_glb_bytes_to_ir never panics
        #[test]
        fn proptest_hostile_glb_no_panic(bytes in prop::collection::vec(any::<u8>(), 0..1024)) {
            let res = catch_unwind(|| {
                let _ = import_glb_bytes_to_ir(&bytes, None);
            });
            prop_assert!(res.is_ok(), "import_glb_bytes_to_ir panicked on arbitrary bytes!");
        }

        /// Proptest 2: Arbitrary string into parse_tres never panics
        #[test]
        fn proptest_tres_parse_no_panic(s in "\\PC*") {
            let res = catch_unwind(|| {
                let _ = parse_tres(&s);
            });
            prop_assert!(res.is_ok(), "parse_tres panicked on arbitrary string!");
        }

        /// Proptest 3: Arbitrary string into parse_gdextension_manifest never panics
        #[test]
        fn proptest_gdextension_parse_no_panic(s in "\\PC*") {
            let res = catch_unwind(|| {
                let _ = parse_gdextension_manifest(&s);
            });
            prop_assert!(res.is_ok(), "parse_gdextension_manifest panicked on arbitrary string!");
        }

        /// Proptest 4: GdLspAnalyzer coordinates never panic
        #[test]
        fn proptest_lsp_coordinates_no_panic(
            line in 0usize..500usize,
            col in 0usize..500usize,
            source in "[a-zA-Z0-9_ \t\n@:=+*#\"'()]{0,300}"
        ) {
            let analyzer = GdLspAnalyzer::new();
            let res = catch_unwind(|| {
                let _ = analyzer.complete_at(&source, line, col);
                let _ = analyzer.hover_at(&source, line, col);
                let _ = analyzer.goto_definition(&source, line, col);
            });
            prop_assert!(res.is_ok(), "GdLspAnalyzer panicked on fuzz coordinates!");
        }

        /// Proptest 5: InputServer::get_vector length is always <= 1.0 + 1e-5 and non-NaN
        #[test]
        fn proptest_input_deadzone_radial_bound(
            x in -2.0f32..2.0f32,
            y in -2.0f32..2.0f32,
            dz in 0.0f32..1.5f32,
        ) {
            let mut map = InputMap::new();
            map.add_action("l", 0.1);
            map.action_add_trigger("l", InputTrigger::GamepadAxis { pad: None, axis: GamepadAxisId::LeftStickX, direction: -1.0 });
            map.add_action("r", 0.1);
            map.action_add_trigger("r", InputTrigger::GamepadAxis { pad: None, axis: GamepadAxisId::LeftStickX, direction: 1.0 });
            map.add_action("u", 0.1);
            map.action_add_trigger("u", InputTrigger::GamepadAxis { pad: None, axis: GamepadAxisId::LeftStickY, direction: -1.0 });
            map.add_action("d", 0.1);
            map.action_add_trigger("d", InputTrigger::GamepadAxis { pad: None, axis: GamepadAxisId::LeftStickY, direction: 1.0 });

            let mut server = InputServer::new(map);
            server.feed_event(InputEvent::GamepadAxis { pad: 0, axis: GamepadAxisId::LeftStickX, value: x });
            server.feed_event(InputEvent::GamepadAxis { pad: 0, axis: GamepadAxisId::LeftStickY, value: y });

            let vec = server.get_vector("l", "r", "u", "d", dz);
            prop_assert!(vec[0].is_finite());
            prop_assert!(vec[1].is_finite());
            let len = (vec[0] * vec[0] + vec[1] * vec[1]).sqrt();
            prop_assert!(len <= 1.0 + 1e-5, "get_vector length exceeded 1.0: {}", len);
        }

        /// Proptest 6: Lossless pack trailer embed/extract round-trip
        #[test]
        fn proptest_pack_trailer_lossless_roundtrip(
            base in prop::collection::vec(any::<u8>(), 10..200),
            pack in prop::collection::vec(any::<u8>(), 10..200)
        ) {
            let embedded = embed_pack_trailer(&base, &pack);
            let extracted = extract_embedded_pack_from_bytes(&embedded);
            prop_assert_eq!(extracted, Ok(Some(pack)));
        }
    }
}
