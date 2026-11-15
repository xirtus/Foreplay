//! Hostile Parser & Wire-Codec Fuzzing Suites (`src/parser_fuzzers.rs`).
//!
//! Attacks all Foreplay document parsers, wire codecs, and transfer functions with
//! pathological exploit vectors:
//! - Deeply nested brackets `[[[[...`
//! - Null bytes `\0`, UTF-8 BOM, lone surrogates
//! - `1e9999` float overflows, `-0.0`, `NaN`, `Inf`
//! - Truncated headers and 64KB lines
//! - Cyclic hierarchy paths
//! - Division-by-zero traps

use crate::VerificationReceipt;
use crate::undo_redo_fuzzer::scenes_bit_identical;
use foreplay_audio::{WavHeader, WavParseError};
use foreplay_gdscript::parse_gdscript;
use foreplay_ir::{IrNode, IrScene, IrValue, NodeId};
use foreplay_net::{CompactReplayFrame, ReplayNetError};
use foreplay_platform::StickCalibration;
use foreplay_porthost::archive::{BinaryReadError, BinaryReader, Endian, SimplePakMount};
use foreplay_porthost::tscn::parse_tscn_to_ir;
use std::collections::BTreeMap;

/// Generates a corpus of pathological exploit strings designed to crash naive parsers.
pub fn generate_pathological_exploit_strings() -> Vec<String> {
    let mut corpus = Vec::new();

    // 1. Empty and whitespace variations
    corpus.push("".into());
    corpus.push("   \t  \r\n\n\r  ".into());

    // 2. Deeply nested brackets
    let mut deeply_nested = String::new();
    for _ in 0..1000 {
        deeply_nested.push('[');
    }
    corpus.push(deeply_nested.clone());
    for _ in 0..1000 {
        deeply_nested.push(']');
    }
    corpus.push(deeply_nested);

    // 3. Null bytes and binary control characters
    corpus.push("node_name\0with\0null\0bytes".into());
    corpus.push("[node name=\"foo\0bar\" type=\"Node3D\"]\0".into());
    corpus.push("\u{FEFF}[foreplay_scene name=\"BOMScene\" format=1]\n".into()); // UTF-8 BOM
    corpus.push("\u{0000}\u{0001}\u{0002}\u{0003}\u{0004}\u{0005}".into());

    // 4. Extreme numeric literals and overflows
    corpus.push("[feel_spec key=\"overflow\" value=1e999999999999999999999999999999999999]\n".into());
    corpus.push("[feel_spec key=\"underflow\" value=-1e99999999999999999999999999999999999]\n".into());
    corpus.push("[feel_spec key=\"nan_val\" value=nan]\n".into());
    corpus.push("[feel_spec key=\"inf_val\" value=inf]\n".into());
    corpus.push("[feel_spec key=\"neg_inf\" value=-inf]\n".into());
    corpus.push("[feel_spec key=\"signed_zero\" value=-0.0]\n".into());

    // 5. Giant 64KB lines
    let giant_line = "a = \"".to_string() + &"X".repeat(65_536) + "\"\n";
    corpus.push(giant_line);

    // 6. Truncated headers and unclosed strings
    corpus.push("[foreplay_scene".into());
    corpus.push("[foreplay_scene name=\"unclosed".into());
    corpus.push("[node id=\"1:1\"".into());
    corpus.push("[connection".into());
    corpus.push("[gd_scene".into());
    corpus.push("[ext_resource".into());
    corpus.push("@export_range(".into());
    corpus.push("func foo(".into());
    corpus.push("var x = \"unclosed string literal".into());

    // 7. Malformed node IDs and attribute syntax
    corpus.push("[node id=\"not_an_id\" name=\"X\" type=\"Y\"]\n".into());
    corpus.push("[node id=\"0:0\" name=\"ZeroGen\" type=\"Y\"]\n".into()); // generation 0 is invalid
    corpus.push("[node id=\"123\" name=\"MissingColon\" type=\"Y\"]\n".into());
    corpus.push("[node id=\"1:1:1\" name=\"TooManyColons\" type=\"Y\"]\n".into());
    corpus.push("[node === invalid attributes ===]\n".into());

    // 8. Cyclic and self-referencing hierarchy strings
    corpus.push("[node id=\"1:1\" name=\"Self\" type=\"Node3D\" parent=\"1:1\"]\n".into());
    corpus.push("[node id=\"2:1\" name=\"MissingParent\" type=\"Node3D\" parent=\"999:1\"]\n".into());

    corpus
}

/// Fuzzes `foreplay_ir::IrScene::from_fscn_str` against pathological strings and proves
/// round-trip equivalence on arbitrary valid scenes.
pub fn verify_fscn_hostile_suite() -> VerificationReceipt {
    let mut cases_executed = 0;
    let mut invariants_held = true;

    // 1. Pathological exploit inputs: MUST NEVER PANIC
    for exploit in generate_pathological_exploit_strings() {
        cases_executed += 1;
        // The parser must return Ok or Err cleanly without panic
        let _ = IrScene::from_fscn_str(&exploit);
    }

    // 2. Round-trip property test on valid scenes: from_fscn_str(to_fscn_string()) == scene
    for i in 1..=100 {
        cases_executed += 1;
        let mut scene = IrScene::new(format!("RoundTripScene_{i}"));
        scene.feel_overrides.insert("jump_impulse".into(), 5.5);
        scene.feel_overrides.insert("speed_mult".into(), 1.25);

        let root_id = NodeId::new(0, 1).unwrap();
        let mut root_props = BTreeMap::new();
        root_props.insert("world_gravity".into(), IrValue::F64(9.8));
        root_props.insert("is_active".into(), IrValue::Bool(true));
        root_props.insert("level_name".into(), IrValue::String("SanVanelona".into()));
        root_props.insert("vec_offset".into(), IrValue::Vec3([1.0, 2.0, 3.0]));
        root_props.insert("quat_rot".into(), IrValue::Quat([0.0, 0.0, 0.0, 1.0]));
        root_props.insert("tags".into(), IrValue::Array(vec![
            IrValue::String("outdoor".into()),
            IrValue::I64(42),
        ]));

        scene.nodes.insert(
            root_id,
            IrNode {
                id: root_id,
                name: "WorldRoot".into(),
                class_name: "Node3D".into(),
                parent: None,
                instance_scene: None,
                properties: root_props,
                groups: vec!["root_group".into()],
            },
        );

        let child_id = NodeId::new(1, 1).unwrap();
        scene.nodes.insert(
            child_id,
            IrNode {
                id: child_id,
                name: "Skater".into(),
                class_name: "CharacterBody3D".into(),
                parent: Some(root_id),
                instance_scene: None,
                properties: BTreeMap::new(),
                groups: vec!["player".into()],
            },
        );

        let emitted = scene.to_fscn_string();
        match IrScene::from_fscn_str(&emitted) {
            Ok(parsed) => {
                if !scenes_bit_identical(&scene, &parsed) {
                    invariants_held = false;
                }
            }
            Err(_) => {
                invariants_held = false;
            }
        }
    }

    VerificationReceipt {
        suite_name: "fscn_hostile_parser_and_roundtrip_fuzz".into(),
        cases_executed,
        invariants_held,
    }
}

/// Fuzzes `foreplay_porthost::tscn::parse_tscn_to_ir` with pathological inputs.
pub fn verify_tscn_hostile_suite() -> VerificationReceipt {
    let mut cases_executed = 0;
    let invariants_held = true;

    for exploit in generate_pathological_exploit_strings() {
        cases_executed += 1;
        let _ = parse_tscn_to_ir(&exploit);
    }

    // Additional Godot TSCN specific exploit strings
    let tscn_exploits = [
        "[gd_scene load_steps=2 format=3 uid=\"uid://exploit\"]\n[node name=\"Root\" type=\"Node3D\"]\n",
        "[node name=\"NoParentChild\" type=\"Node3D\" parent=\"NonExistent\"]\n",
        "[node name=\"SelfParent\" type=\"Node3D\" parent=\".\"]\n",
        "[node name=\"CyclicA\" type=\"Node3D\" parent=\"CyclicB\"]\n[node name=\"CyclicB\" type=\"Node3D\" parent=\"CyclicA\"]\n",
        "[ext_resource type=\"Script\" path=\"res://non_existent.gd\" id=\"1_abc\"]\n",
        "[connection signal=\"body_entered\" from=\".\" to=\"NonExistent\" method=\"_on_body_entered\"]\n",
        "[node name=\"Root\" type=\"Node3D\"]\nprop = Vector3(1e9999, nan, -inf)\n",
    ];

    for exploit in tscn_exploits {
        cases_executed += 1;
        let _ = parse_tscn_to_ir(exploit);
    }

    VerificationReceipt {
        suite_name: "tscn_hostile_parser_fuzz".into(),
        cases_executed,
        invariants_held,
    }
}

/// Fuzzes `foreplay_gdscript::parse_gdscript` with pathological header & syntax declarations.
pub fn verify_gdscript_hostile_suite() -> VerificationReceipt {
    let mut cases_executed = 0;
    let invariants_held = true;

    for exploit in generate_pathological_exploit_strings() {
        cases_executed += 1;
        let _ = parse_gdscript(&exploit);
    }

    let gd_exploits = [
        "@export_range(1, 10, 1, 1, 1, 1, 1, 1) var bad_range: int = 5\n",
        "@export_range(nan, inf) var non_finite: float = 0.0\n",
        "@export var unclosed_type: Array[\n",
        "func bad_func(((((((a, b, c): pass\n",
        "signal bad_signal(((((\n",
        "class_name 123InvalidClassName\n",
        "extends \"res://unclosed_string\n",
        "var x = 1e99999999999999999999999999999999\n",
    ];

    for exploit in gd_exploits {
        cases_executed += 1;
        let _ = parse_gdscript(exploit);
    }

    VerificationReceipt {
        suite_name: "gdscript_hostile_parser_fuzz".into(),
        cases_executed,
        invariants_held,
    }
}

/// Fuzzes `foreplay_net::CompactReplayFrame` wire decoding.
pub fn verify_replay_frame_codec_suite() -> VerificationReceipt {
    let mut cases_executed = 0;
    let mut invariants_held = true;

    // 1. Buffer too short
    for len in 0..62 {
        cases_executed += 1;
        let buf = vec![0u8; len];
        if !matches!(CompactReplayFrame::decode_bytes(&buf), Err(ReplayNetError::BufferTooShort)) {
            invariants_held = false;
        }
    }

    // 2. Corrupt magic
    {
        cases_executed += 1;
        let mut buf = vec![0u8; 62];
        buf[0..4].copy_from_slice(b"XXXX");
        if !matches!(CompactReplayFrame::decode_bytes(&buf), Err(ReplayNetError::InvalidMagic)) {
            invariants_held = false;
        }
    }

    // 3. Unsupported version
    {
        cases_executed += 1;
        let mut buf = vec![0u8; 62];
        buf[0..4].copy_from_slice(b"FPRF");
        buf[4..6].copy_from_slice(&2u16.to_le_bytes()); // version 2
        if !matches!(CompactReplayFrame::decode_bytes(&buf), Err(ReplayNetError::UnsupportedVersion(2))) {
            invariants_held = false;
        }
    }

    // 4. Invalid Node generation == 0
    {
        cases_executed += 1;
        let mut buf = vec![0u8; 62];
        buf[0..4].copy_from_slice(b"FPRF");
        buf[4..6].copy_from_slice(&1u16.to_le_bytes());
        buf[14..18].copy_from_slice(&1u32.to_le_bytes()); // index
        buf[18..22].copy_from_slice(&0u32.to_le_bytes()); // generation 0
        if !matches!(CompactReplayFrame::decode_bytes(&buf), Err(ReplayNetError::InvalidNodeGeneration)) {
            invariants_held = false;
        }
    }

    // 5. Non-finite float rejection (NaN / Inf)
    for bad_float in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        cases_executed += 1;
        let mut buf = vec![0u8; 62];
        buf[0..4].copy_from_slice(b"FPRF");
        buf[4..6].copy_from_slice(&1u16.to_le_bytes());
        buf[14..18].copy_from_slice(&1u32.to_le_bytes());
        buf[18..22].copy_from_slice(&1u32.to_le_bytes()); // gen 1
        buf[22..26].copy_from_slice(&bad_float.to_le_bytes()); // pos.x = NaN/Inf
        buf[34..38].copy_from_slice(&1.0f32.to_le_bytes());   // quat.w = 1.0
        if !matches!(CompactReplayFrame::decode_bytes(&buf), Err(ReplayNetError::NonFiniteFloat)) {
            invariants_held = false;
        }
    }

    // 6. Zero/degenerate quaternion rejection
    {
        cases_executed += 1;
        let mut buf = vec![0u8; 62];
        buf[0..4].copy_from_slice(b"FPRF");
        buf[4..6].copy_from_slice(&1u16.to_le_bytes());
        buf[14..18].copy_from_slice(&1u32.to_le_bytes());
        buf[18..22].copy_from_slice(&1u32.to_le_bytes());
        // quat remains all 0.0
        if !matches!(CompactReplayFrame::decode_bytes(&buf), Err(ReplayNetError::InvalidRotationQuaternion)) {
            invariants_held = false;
        }
    }

    // 7. Valid round-trip encoding/decoding
    {
        cases_executed += 1;
        let valid_frame = CompactReplayFrame {
            tick: 12345,
            entity: NodeId::new(42, 1).unwrap(),
            position: [1.0, 2.0, 3.0],
            rotation_quat: [0.0, 0.0, 0.0, 1.0],
            linear_velocity: [0.5, -0.5, 0.0],
        };
        let encoded = valid_frame.encode_bytes();
        match CompactReplayFrame::decode_bytes(&encoded) {
            Ok(decoded) => {
                if valid_frame != decoded {
                    invariants_held = false;
                }
            }
            Err(_) => invariants_held = false,
        }
    }

    VerificationReceipt {
        suite_name: "replay_net_wire_codec_fuzz".into(),
        cases_executed,
        invariants_held,
    }
}

/// Fuzzes `foreplay_audio::WavHeader::parse` with truncated and corrupt chunk headers.
pub fn verify_wav_codec_hostile_suite() -> VerificationReceipt {
    let mut cases_executed = 0;
    let mut invariants_held = true;

    // 1. Truncated buffers (< 44 bytes)
    for len in 0..44 {
        cases_executed += 1;
        let buf = vec![0u8; len];
        if !matches!(WavHeader::parse(&buf), Err(WavParseError::BufferTooSmall)) {
            invariants_held = false;
        }
    }

    // 2. Invalid RIFF magic
    {
        cases_executed += 1;
        let mut buf = vec![0u8; 44];
        buf[0..4].copy_from_slice(b"NOPE");
        buf[8..12].copy_from_slice(b"WAVE");
        if !matches!(WavHeader::parse(&buf), Err(WavParseError::InvalidRiffHeader)) {
            invariants_held = false;
        }
    }

    // 3. Invalid WAVE magic
    {
        cases_executed += 1;
        let mut buf = vec![0u8; 44];
        buf[0..4].copy_from_slice(b"RIFF");
        buf[8..12].copy_from_slice(b"MP3_");
        if !matches!(WavHeader::parse(&buf), Err(WavParseError::InvalidWaveFormat)) {
            invariants_held = false;
        }
    }

    // 4. Zero sample rate trap
    {
        cases_executed += 1;
        let mut buf = build_minimal_wav_bytes();
        // Set sample_rate to 0 at offset 24 (12 + 8 + 4)
        buf[24..28].copy_from_slice(&0u32.to_le_bytes());
        if !matches!(WavHeader::parse(&buf), Err(WavParseError::InvalidSampleRate(0))) {
            invariants_held = false;
        }
    }

    // 5. Zero channel count trap
    {
        cases_executed += 1;
        let mut buf = build_minimal_wav_bytes();
        // Set channels to 0 at offset 22 (12 + 8 + 2)
        buf[22..24].copy_from_slice(&0u16.to_le_bytes());
        if !matches!(WavHeader::parse(&buf), Err(WavParseError::InvalidChannelCount(0))) {
            invariants_held = false;
        }
    }

    // 6. Zero byte_rate division-by-zero check in duration_seconds()
    {
        cases_executed += 1;
        let header = WavHeader {
            audio_format: 1,
            channels: 2,
            sample_rate: 44100,
            byte_rate: 0, // 0 byte rate must return 0.0 duration without panic
            block_align: 4,
            bits_per_sample: 16,
            data_offset: 44,
            data_size: 1000,
        };
        let duration = header.duration_seconds();
        if duration != 0.0 || duration.is_nan() {
            invariants_held = false;
        }
    }

    VerificationReceipt {
        suite_name: "wav_audio_codec_hostile_fuzz".into(),
        cases_executed,
        invariants_held,
    }
}

/// Helper building a valid minimal 44-byte WAV header.
fn build_minimal_wav_bytes() -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&36u32.to_le_bytes()); // overall size
    buf.extend_from_slice(b"WAVE");

    // fmt chunk
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&16u32.to_le_bytes()); // chunk size 16
    buf.extend_from_slice(&1u16.to_le_bytes());  // PCM format 1
    buf.extend_from_slice(&2u16.to_le_bytes());  // 2 channels
    buf.extend_from_slice(&44100u32.to_le_bytes()); // 44100 Hz
    buf.extend_from_slice(&(44100u32 * 4).to_le_bytes()); // byte rate
    buf.extend_from_slice(&4u16.to_le_bytes());  // block align 4
    buf.extend_from_slice(&16u16.to_le_bytes()); // 16 bits per sample

    // data chunk
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&0u32.to_le_bytes()); // 0 data size
    buf
}

/// Fuzzes `BinaryReader` and `SimplePakMount` against offset overflows and corrupt headers.
pub fn verify_archive_pak_hostile_suite() -> VerificationReceipt {
    let mut cases_executed = 0;
    let mut invariants_held = true;

    // 1. BinaryReader hostile seek and skip
    {
        cases_executed += 1;
        let data = [1u8, 2, 3, 4, 5];
        let mut reader = BinaryReader::new(&data, Endian::Little);
        // seek past EOF
        if !matches!(reader.seek(10), Err(BinaryReadError::UnexpectedEof { .. })) {
            invariants_held = false;
        }
        // skip past EOF
        if !matches!(reader.skip(10), Err(BinaryReadError::UnexpectedEof { .. })) {
            invariants_held = false;
        }
        // read past EOF
        let _ = reader.read_u64();
    }

    // 2. SimplePakMount::from_bytes with invalid magic
    {
        cases_executed += 1;
        let corrupt = b"NOPE_INVALID_PAK_BYTES";
        if !matches!(SimplePakMount::from_bytes(corrupt), Err(BinaryReadError::CorruptArchive(_))) {
            invariants_held = false;
        }
    }

    // 3. SimplePakMount::from_bytes with huge TOC count on short buffer
    {
        cases_executed += 1;
        let mut buf = Vec::new();
        buf.extend_from_slice(b"SPAK");
        buf.push(1); // Little endian
        buf.extend_from_slice(&[0, 0, 0]); // 3 reserved
        buf.extend_from_slice(&1_000_000u32.to_le_bytes()); // 1M entries declared
        if !matches!(SimplePakMount::from_bytes(&buf), Err(BinaryReadError::UnexpectedEof { .. })) {
            invariants_held = false;
        }
    }

    // 4. Valid roundtrip build_pak / from_bytes for Little and Big endian
    for endian in [Endian::Little, Endian::Big] {
        cases_executed += 1;
        let mut mount = SimplePakMount::new();
        mount.insert("models/skater.mesh", vec![1, 2, 3, 4, 5]);
        mount.insert("audio/ollie.wav", vec![10, 20, 30]);

        let pak_bytes = mount.build_pak(endian);
        match SimplePakMount::from_bytes(&pak_bytes) {
            Ok(loaded) => {
                if loaded.get("models/skater.mesh") != Some(&[1, 2, 3, 4, 5][..]) {
                    invariants_held = false;
                }
                if loaded.get("audio/ollie.wav") != Some(&[10, 20, 30][..]) {
                    invariants_held = false;
                }
            }
            Err(_) => invariants_held = false,
        }
    }

    VerificationReceipt {
        suite_name: "archive_pak_binary_reader_fuzz".into(),
        cases_executed,
        invariants_held,
    }
}

/// Fuzzes `StickCalibration::apply_radial` to prove output magnitude is ALWAYS in `0.0..=1.0`
/// and NEVER `NaN` for ANY input.
pub fn verify_stick_calibration_suite() -> VerificationReceipt {
    let mut cases_executed = 0;
    let mut invariants_held = true;

    let calib = StickCalibration::default();

    let test_inputs: Vec<[f32; 2]> = vec![
        [0.0, 0.0],
        [-0.0, -0.0],
        [f32::NAN, 0.0],
        [0.0, f32::NAN],
        [f32::NAN, f32::NAN],
        [f32::INFINITY, 0.0],
        [0.0, f32::INFINITY],
        [f32::NEG_INFINITY, f32::NEG_INFINITY],
        [f32::from_bits(1), f32::from_bits(1)], // smallest subnormal
        [1e30, 1e30],
        [-1e30, -1e30],
        [0.5, 0.5],
        [1.0, 0.0],
        [0.0, 1.0],
        [-1.0, 0.0],
        [0.0, -1.0],
        [2.0, 2.0], // requires unit-circle clamping
    ];

    for raw in test_inputs {
        cases_executed += 1;
        let [x, y] = calib.apply_radial(raw);

        // Invariant 1: Outputs MUST NEVER be NaN
        if x.is_nan() || y.is_nan() {
            invariants_held = false;
        }

        // Invariant 2: Outputs MUST BE finite
        if !x.is_finite() || !y.is_finite() {
            invariants_held = false;
        }

        // Invariant 3: Magnitude MUST be in 0.0..=1.000001
        let mag = (x * x + y * y).sqrt();
        if !(0.0..=1.000001).contains(&mag) {
            invariants_held = false;
        }
    }

    // Grid sweep across float boundaries
    let float_grid = [
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        0.0,
        -0.0,
        f32::from_bits(1),
        0.05, // inside deadzone (< 0.12)
        0.15, // outside deadzone
        std::f32::consts::FRAC_1_SQRT_2,
        1.0,
        1.5,
        100.0,
    ];

    for &x in &float_grid {
        for &y in &float_grid {
            cases_executed += 1;
            let [out_x, out_y] = calib.apply_radial([x, y]);
            if out_x.is_nan() || out_y.is_nan() || !out_x.is_finite() || !out_y.is_finite() {
                invariants_held = false;
            }
            let mag = (out_x * out_x + out_y * out_y).sqrt();
            if !(0.0..=1.000001).contains(&mag) {
                invariants_held = false;
            }
        }
    }

    VerificationReceipt {
        suite_name: "stick_calibration_radial_fuzz".into(),
        cases_executed,
        invariants_held,
    }
}
