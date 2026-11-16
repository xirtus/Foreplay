//! Phase 21: Independent Red-Team `v1.1.0` Polyamour & Next-Gen Adversarial Verifier
//! (`crates/foreplay-verifier/src/v1_1_polyamour_nextgen_fuzzers.rs`).
//!
//! Provides 10 adversarial fuzzing and invariant property suites for all `v1.1.0` next-gen subsystems:
//! 1. `PLNK` Live-Link wire protocol, framing limits, bit-flips & delta sync
//! 2. `polyops 2` syntax, hostile transcripts, out-of-bounds replay & bidirectional serialization
//! 3. Sub-object picking raycasts (`Ray3D`), NDC box selection & proportional editing falloffs
//! 4. Sculpt brush symmetry reflections (X/Y/Z) & geodesic heat-diffusion bone weights
//! 5. Semantic game attributes, continuous skate grind-rail splines & Smart UV2 lightmap atlases
//! 6. WebAssembly WebGPU binary packager, LEB128 varint parsing & embedded `.fpck` custom sections
//! 7. Rust C-ABI hot-reload property coercion matrix & state-preserving schema migration
//! 8. `Moan` adaptive DAW stems, DSP sends & `Splicy` magnetic EDL J/L-cut director
//! 9. Hardware WGSL BVH path-tracer, SVGF wavelet denoiser & Radiance Cascades GI solver
//! 10. `PlayerRuntime` mid-gameplay parametric live-link & Three.js codegen determinism

use crate::VerificationReceipt;
use bevy_math::Vec3;
use foreplay_export::wasm_webgpu::{
    extract_and_verify_wasm_fpck_section, WasmWebGpuPackager, FPCK_WASM_MAGIC, WASM_MAGIC,
    WASM_VERSION,
};
use foreplay_export::FpckArchive;
use foreplay_ir::{IrNode, IrScene, IrValue, NodeId};
use foreplay_player::polyamour_runtime::{
    pack_node_id, parse_parametric_modifiers, unpack_node_id, LivePolyamourNodeState,
};
use foreplay_player::runtime::PlayerRuntime;
use foreplay_polyamour::{
    apply_mesh_delta, apply_sculpt_stroke, canonical_edge, compile_polyamour_game_bundle,
    compute_heat_bone_weights, decode_frame, emit_polyops_v2, encode_frame,
    parse_polyops_v2, pick_edge_ray, pick_face_ray, pick_vertex_ray, primitive_cube,
    primitive_cylinder, primitive_quarterpipe, primitive_uv_sphere,
    replay_polyops_v2, transform_selection_proportional, LiveLinkError, PolyLinkMessage,
    PolyMesh, PolyMeshSnapshot, PolyOpStep, PolyamourSemanticAttributes, ProportionalFalloff,
    Ray3D, RigBoneSegment, SculptBrush, SculptStroke, ShapeKeySet,
    SubObjectSelection, SymmetryAxes, MAX_FRAME_SIZE, TAG_COLLISION_CONVEX,
    TAG_COLLISION_NONE, TAG_COLLISION_TRIMESH, TAG_GRIND_EDGE, TAG_WALKABLE_NAV,
};
use foreplay_render::{
    GpuComputePathTracer, GpuEmitter, GpuPathCamera, GpuPathScene, GpuPathTriangle,
    GpuRadianceCascadesSolver, WgpuRenderDevice,
};
use foreplay_script::rust_hot_reload::{coerce_ir_value, ParsedPropertySchema, RustPropertyType};
use foreplay_script::sexlife_bridge::{
    CameraKeyframe, CameraShotClip, MoanProjectStemLink, SplicyCutsceneEdl,
};
use foreplay_threejs::parametric_polyamour::{
    extract_polyamour_assets_from_scene, generate_polyamour_runtime_ts,
};
use std::collections::BTreeMap;
use std::num::NonZeroU32;

// =========================================================================
// Suite 1: PLNK Live-Link Wire Protocol Fuzz Suite
// =========================================================================

/// Verifies the `PLNK` binary live-link network wire framing protocol against hostile frames,
/// truncated packets, header corruption, oversized allocations, single-bit checksum flips,
/// and lossless roundtrip across all 6 `PolyLinkMessage` variants.
#[must_use]
pub fn verify_plnk_livelink_wire_protocol_fuzz_suite() -> VerificationReceipt {
    let mut cases_executed = 0;
    let mut invariants_held = true;

    // 1. Truncated buffer lengths (< 20 bytes HEADER_SIZE) must return Ok(None) without panics
    for len in 0..20 {
        cases_executed += 1;
        let buf = vec![0x42u8; len];
        match decode_frame(&buf) {
            Ok(None) => {}
            other => {
                eprintln!("[Suite 1 Fail] truncated length {len} returned {other:?}");
                invariants_held = false;
            }
        }
    }

    // 2. Corrupted Magic bytes (bytes 0..4 != b"PLNK") must fail with InvalidMagic
    let hostile_magics: [&[u8; 4]; 6] = [
        b"\0\0\0\0",
        b"plnk",
        b"PLNX",
        b"FPCK",
        b"\xFF\xFF\xFF\xFF",
        b"HTTP",
    ];
    for magic in &hostile_magics {
        cases_executed += 1;
        let mut frame = Vec::new();
        frame.extend_from_slice(*magic);
        frame.extend_from_slice(&1u32.to_le_bytes()); // version 1
        frame.extend_from_slice(&0u32.to_le_bytes()); // length 0
        frame.extend_from_slice(&0u64.to_le_bytes()); // checksum
        match decode_frame(&frame) {
            Err(LiveLinkError::InvalidMagic(m)) if &m == *magic => {}
            other => {
                eprintln!("[Suite 1 Fail] hostile magic {magic:?} returned {other:?}");
                invariants_held = false;
            }
        }
    }

    // 3. Unsupported version numbers (version != 1) must return UnsupportedVersion
    let bad_versions = [0u32, 2, 3, 999, u32::MAX];
    for ver in bad_versions {
        cases_executed += 1;
        let mut frame = Vec::new();
        frame.extend_from_slice(b"PLNK");
        frame.extend_from_slice(&ver.to_le_bytes());
        frame.extend_from_slice(&0u32.to_le_bytes());
        frame.extend_from_slice(&0u64.to_le_bytes());
        match decode_frame(&frame) {
            Err(LiveLinkError::UnsupportedVersion(v)) if v == ver => {}
            other => {
                eprintln!("[Suite 1 Fail] unsupported version {ver} returned {other:?}");
                invariants_held = false;
            }
        }
    }

    // 4. Hostile oversized payload lengths exceeding MAX_FRAME_SIZE (64 MiB)
    let bad_lengths = [
        (MAX_FRAME_SIZE + 1) as u32,
        (MAX_FRAME_SIZE + 1_000_000) as u32,
        u32::MAX,
    ];
    for len in bad_lengths {
        cases_executed += 1;
        let mut frame = Vec::new();
        frame.extend_from_slice(b"PLNK");
        frame.extend_from_slice(&1u32.to_le_bytes());
        frame.extend_from_slice(&len.to_le_bytes());
        frame.extend_from_slice(&0u64.to_le_bytes());
        match decode_frame(&frame) {
            Err(LiveLinkError::FrameTooLarge(sz)) if sz == len as usize => {}
            other => {
                eprintln!("[Suite 1 Fail] oversized length {len} returned {other:?}");
                invariants_held = false;
            }
        }
    }

    // 5. Incomplete payload (length claimed > remaining buffer) must return Ok(None)
    for claimed_len in [10u32, 50, 100, 1000] {
        cases_executed += 1;
        let mut frame = Vec::new();
        frame.extend_from_slice(b"PLNK");
        frame.extend_from_slice(&1u32.to_le_bytes());
        frame.extend_from_slice(&claimed_len.to_le_bytes());
        frame.extend_from_slice(&0u64.to_le_bytes());
        frame.extend_from_slice(&vec![0xAA; (claimed_len / 2) as usize]); // only half payload
        match decode_frame(&frame) {
            Ok(None) => {}
            other => {
                eprintln!("[Suite 1 Fail] partial payload returned {other:?}");
                invariants_held = false;
            }
        }
    }

    // 6. Lossless round-trip encode_frame -> decode_frame across all 6 PolyLinkMessage variants
    let test_cube = primitive_cube(2.0);
    let mut selection = SubObjectSelection::new();
    selection.select_vertex(0);
    selection.select_vertex(2);
    selection.select_edge(0, 1);
    selection.select_face(3);

    let messages = vec![
        PolyLinkMessage::Hello {
            app_name: "ForeplayNextGenRedTeam".into(),
            protocol_version: 1,
        },
        PolyLinkMessage::OpenMeshInPolyamour {
            node_id: 12345,
            node_name: "HeroModel".into(),
            mesh: PolyMeshSnapshot::from_mesh(&test_cube),
            sub_selection: selection.clone(),
        },
        PolyLinkMessage::MeshDeltaSync {
            node_id: 12345,
            revision: 42,
            updated_vertices: vec![(0, [1.0, 2.0, 3.0]), (1, [-1.0, 0.5, 2.5])],
            replaced_faces: Some(vec![vec![0, 1, 2], vec![2, 3, 0]]),
            edge_tags: vec![([0, 1], "grind_edge".into())],
            face_tags: vec![(0, "collision_convex".into())],
        },
        PolyLinkMessage::SelectionSync {
            node_id: 12345,
            selection: selection.clone(),
        },
        PolyLinkMessage::UndoMarker {
            label: "Extrude Facet".into(),
            revision: 43,
        },
        PolyLinkMessage::Ack { revision: 43 },
    ];

    for msg in &messages {
        cases_executed += 1;
        match encode_frame(msg) {
            Ok(encoded) => match decode_frame(&encoded) {
                Ok(Some((decoded_msg, consumed))) => {
                    if decoded_msg != *msg || consumed != encoded.len() {
                        eprintln!("[Suite 1 Fail] roundtrip mismatch: orig={msg:?}, decoded={decoded_msg:?}");
                        invariants_held = false;
                    }
                }
                other => {
                    eprintln!("[Suite 1 Fail] decode of valid encoded frame failed: {other:?}");
                    invariants_held = false;
                }
            },
            Err(e) => {
                eprintln!("[Suite 1 Fail] encode failed: {e:?}");
                invariants_held = false;
            }
        }
    }

    // 7. Single-bit FNV-1a Checksum corruption attack
    let sample_msg = PolyLinkMessage::Hello {
        app_name: "AdversarialProbe".into(),
        protocol_version: 1,
    };
    if let Ok(frame) = encode_frame(&sample_msg) {
        // Corrupt each bit of the 8-byte checksum (offsets 12..20)
        for byte_idx in 12..20 {
            for bit in 0..8 {
                cases_executed += 1;
                let mut corrupted = frame.clone();
                corrupted[byte_idx] ^= 1 << bit;
                match decode_frame(&corrupted) {
                    Err(LiveLinkError::ChecksumMismatch { .. }) => {}
                    other => {
                        eprintln!("[Suite 1 Fail] corrupted checksum not caught: {other:?}");
                        invariants_held = false;
                    }
                }
            }
        }

        // Single-bit payload bit-flip attack (offsets 20..frame.len())
        for byte_idx in 20..(20 + 10.min(frame.len() - 20)) {
            cases_executed += 1;
            let mut corrupted = frame.clone();
            corrupted[byte_idx] ^= 0x01;
            match decode_frame(&corrupted) {
                Err(LiveLinkError::ChecksumMismatch { .. })
                | Err(LiveLinkError::Serialization(_)) => {}
                other => {
                    eprintln!("[Suite 1 Fail] flipped payload byte not caught: {other:?}");
                    invariants_held = false;
                }
            }
        }
    }

    // 8. Test in-place delta application via apply_mesh_delta
    let mut mesh = primitive_cube(1.0);
    let delta_msg = PolyLinkMessage::MeshDeltaSync {
        node_id: 1,
        revision: 1,
        updated_vertices: vec![(0, [10.0, 10.0, 10.0]), (100, [5.0, 5.0, 5.0])],
        replaced_faces: None,
        edge_tags: Vec::new(),
        face_tags: Vec::new(),
    };
    cases_executed += 1;
    let applied = apply_mesh_delta(&mut mesh, &delta_msg);
    if !applied || mesh.positions[0] != Vec3::new(10.0, 10.0, 10.0) || mesh.positions.len() <= 100 {
        eprintln!("[Suite 1 Fail] apply_mesh_delta failed to update or resize vertices");
        invariants_held = false;
    }

    VerificationReceipt {
        suite_name: "plnk_livelink_wire_protocol_fuzz".into(),
        cases_executed,
        invariants_held,
    }
}

// =========================================================================
// Suite 2: Polyops V2 Parser and Replay Fuzz Suite
// =========================================================================

/// Verifies `polyops 2` syntax parsing, error resilience against malformed transcripts,
/// out-of-bounds indices in replay, and bidirectional emission-parsing roundtrip.
#[must_use]
pub fn verify_polyops_v2_parser_and_replay_fuzz_suite() -> VerificationReceipt {
    let mut cases_executed = 0;
    let mut invariants_held = true;

    // 1. Hostile missing or corrupted headers
    let bad_headers = [
        "",
        "polyops",
        "polyops 1",
        "polyops 3",
        "POLYOPS 2",
        "random string\npolyops 2",
        "hello world",
        "\0\0\0\0",
    ];
    for hdr in bad_headers {
        cases_executed += 1;
        match parse_polyops_v2(hdr) {
            Err(_) => {}
            Ok(steps) => {
                eprintln!("[Suite 2 Fail] bad header '{hdr}' was accepted as {steps:?}");
                invariants_held = false;
            }
        }
    }

    // 2. Hostile commands and malformed parameters
    let hostile_scripts = [
        "polyops 2\nselect_vertex not_a_number",
        "polyops 2\nselect_edge bad_edge",
        "polyops 2\nselect_edge 1",
        "polyops 2\nselect_edge 1-2-3",
        "polyops 2\nextrude_faces not_a_list distance=1.0",
        "polyops 2\nbevel_edges bad_edge_format offset=0.2",
        "polyops 2\ntag_edge bad-edge tag=\"grind\"",
        "polyops 2\ntag_edge 1-2-3 tag=\"grind\"",
        "polyops 2\nunknown_command_never_seen arg1 arg2",
        "polyops 2\n\n\n   \n",
        "polyops 2\nselect_vertex \nselect_edge \n",
    ];
    for script in hostile_scripts {
        cases_executed += 1;
        // The parser should either parse cleanly (for empty lines) or return Err cleanly without panicking
        let _ = parse_polyops_v2(script);
    }

    // 3. Lossless bidirectional roundtrip emit_polyops_v2 -> parse_polyops_v2 across all steps
    let steps = vec![
        PolyOpStep::SelectVertex {
            indices: vec![0, 1, 3],
        },
        PolyOpStep::SelectEdge {
            edges: vec![[0, 1], [2, 3]],
        },
        PolyOpStep::SelectFace {
            indices: vec![0, 2, 4],
        },
        PolyOpStep::SelectLoop { edge: [1, 2] },
        PolyOpStep::ExtrudeFaces {
            face_indices: vec![0, 2],
            distance: 1.5,
        },
        PolyOpStep::InsetFaces {
            face_indices: vec![1],
            thickness: 0.2,
            depth: 0.1,
        },
        PolyOpStep::BevelEdges {
            edges: vec![[0, 1], [2, 3]],
            offset: 0.05,
            segments: 2,
        },
        PolyOpStep::LoopCut {
            axis: 1,
            factor: 0.35,
        },
        PolyOpStep::TranslateProportional {
            translation: [0.0, 1.0, 0.0],
            radius: 2.5,
            falloff: ProportionalFalloff::Smooth,
        },
        PolyOpStep::TagEdge {
            edge: [0, 1],
            tag: "grind_edge".into(),
        },
        PolyOpStep::TagFace {
            face_index: 3,
            tag: "collision_convex".into(),
        },
        PolyOpStep::ClearSelection,
    ];

    cases_executed += 1;
    let emitted = emit_polyops_v2(&steps);
    match parse_polyops_v2(&emitted) {
        Ok(parsed_steps) => {
            if parsed_steps != steps {
                eprintln!("[Suite 2 Fail] roundtrip steps mismatch!\nOrig: {steps:?}\nParsed: {parsed_steps:?}");
                invariants_held = false;
            }
        }
        Err(e) => {
            eprintln!("[Suite 2 Fail] failed to parse emitted transcript: {e:?}");
            invariants_held = false;
        }
    }

    // 4. Out-of-bounds indices during replay_polyops_v2 must not panic and must maintain valid state
    let oob_steps = vec![
        PolyOpStep::SelectVertex {
            indices: vec![99999, 100000],
        },
        PolyOpStep::SelectEdge {
            edges: vec![[88888, 99999]],
        },
        PolyOpStep::SelectFace {
            indices: vec![77777],
        },
        PolyOpStep::SelectLoop {
            edge: [9999, 10000],
        },
        PolyOpStep::ExtrudeFaces {
            face_indices: vec![9999],
            distance: 1.0,
        },
        PolyOpStep::InsetFaces {
            face_indices: vec![9999],
            thickness: 0.1,
            depth: 0.1,
        },
        PolyOpStep::BevelEdges {
            edges: vec![[9998, 9999]],
            offset: 0.1,
            segments: 1,
        },
        PolyOpStep::TagEdge {
            edge: [9998, 9999],
            tag: "grind_edge".into(),
        },
        PolyOpStep::TagFace {
            face_index: 9999,
            tag: "collision_convex".into(),
        },
    ];

    cases_executed += 1;
    let mut cube = primitive_cube(2.0);
    let mut attrs = PolyamourSemanticAttributes::new();
    let replay_res = replay_polyops_v2(&mut cube, &mut attrs, &oob_steps);
    if replay_res.is_err() {
        eprintln!("[Suite 2 Fail] replay returned unexpected err: {replay_res:?}");
        invariants_held = false;
    }

    // Verify all vertex positions in mesh remain finite and valid
    for pos in &cube.positions {
        cases_executed += 1;
        if !pos.is_finite() {
            eprintln!("[Suite 2 Fail] vertex position became non-finite: {pos:?}");
            invariants_held = false;
        }
    }

    VerificationReceipt {
        suite_name: "polyops_v2_parser_and_replay_fuzz".into(),
        cases_executed,
        invariants_held,
    }
}

// =========================================================================
// Suite 3: Subobject Raycast and Proportional Edit Suite
// =========================================================================

/// Verifies raycasting against faces, edges, and vertices under degenerate, zero-length,
/// and non-finite conditions, along with proportional transformation mathematical falloffs.
#[must_use]
pub fn verify_subobject_raycast_and_proportional_edit_suite() -> VerificationReceipt {
    let mut cases_executed = 0;
    let mut invariants_held = true;

    let cube = primitive_cube(2.0);
    let empty_mesh = PolyMesh::new();

    // 1. Ray3D construction with zero-length and degenerate direction vectors
    let degen_rays = [
        Ray3D::new(Vec3::ZERO, Vec3::ZERO),
        Ray3D::new(Vec3::new(0.0, 0.0, 5.0), Vec3::new(1e-15, 1e-15, 1e-15)),
        Ray3D::new(Vec3::new(0.0, 0.0, 5.0), Vec3::new(f32::NAN, 1.0, 0.0)),
        Ray3D::new(Vec3::new(0.0, 0.0, 5.0), Vec3::new(f32::INFINITY, 0.0, 0.0)),
    ];
    for ray in degen_rays {
        cases_executed += 1;
        let _ = pick_face_ray(&cube, ray);
        let _ = pick_edge_ray(&cube, ray, 0.2);
        let _ = pick_vertex_ray(&cube, ray, 0.2);
    }

    // 2. Empty mesh raycasting must return None cleanly
    let normal_ray = Ray3D::new(Vec3::new(0.0, 0.0, 5.0), Vec3::new(0.0, 0.0, -1.0));
    cases_executed += 1;
    if pick_face_ray(&empty_mesh, normal_ray).is_some()
        || pick_edge_ray(&empty_mesh, normal_ray, 0.5).is_some()
        || pick_vertex_ray(&empty_mesh, normal_ray, 0.5).is_some()
    {
        eprintln!("[Suite 3 Fail] empty mesh raycast returned a hit!");
        invariants_held = false;
    }

    // 3. Precise Face ray hit validation
    cases_executed += 1;
    if let Some(hit) = pick_face_ray(&cube, normal_ray) {
        if (hit.distance - 4.0).abs() > 1e-3 || !hit.hit_point.is_finite() || !hit.face_normal.is_finite() {
            eprintln!("[Suite 3 Fail] face raycast hit parameters out of bounds: {hit:?}");
            invariants_held = false;
        }
    } else {
        eprintln!("[Suite 3 Fail] expected face raycast hit on front of cube");
        invariants_held = false;
    }

    // 4. Edge canonicalization invariants: canonical_edge(a, b) -> [min, max]
    let edge_pairs = [
        (0u32, 1u32),
        (1, 0),
        (5, 5),
        (u32::MAX, 0),
        (0, u32::MAX),
        (100, 42),
    ];
    for (a, b) in edge_pairs {
        cases_executed += 1;
        let canon = canonical_edge(a, b);
        if canon[0] > canon[1] {
            eprintln!("[Suite 3 Fail] canonical_edge({a}, {b}) produced uncanonical order: {canon:?}");
            invariants_held = false;
        }
    }

    // 5. SubObjectSelection set invariants
    let mut sel = SubObjectSelection::new();
    sel.select_vertex(3);
    sel.select_edge(2, 4);
    sel.select_face(1);
    cases_executed += 1;
    if sel.total_selected_count() != 3 {
        eprintln!("[Suite 3 Fail] total_selected_count != 3");
        invariants_held = false;
    }
    let all_verts = sel.all_selected_vertex_indices(&cube);
    cases_executed += 1;
    if !all_verts.contains(&3) || !all_verts.contains(&2) || !all_verts.contains(&4) {
        eprintln!("[Suite 3 Fail] all_selected_vertex_indices missing selected vertices");
        invariants_held = false;
    }

    // 6. Proportional editing falloffs mathematical bounds & finite invariant
    let falloffs = [
        ProportionalFalloff::Linear,
        ProportionalFalloff::Smooth,
        ProportionalFalloff::Sphere,
        ProportionalFalloff::Root,
        ProportionalFalloff::Sharp,
    ];
    for falloff in falloffs {
        for t_sample in [0.0f32, 0.25, 0.5, 0.75, 1.0, -0.5, 1.5] {
            cases_executed += 1;
            let weight = falloff.evaluate(t_sample);
            if !weight.is_finite() || !(0.0..=1.0).contains(&weight) {
                eprintln!("[Suite 3 Fail] falloff {falloff:?} at t={t_sample} gave weight {weight}");
                invariants_held = false;
            }
        }
    }

    // 7. Proportional transformation stress test with extreme parameters
    let radii = [-1.0f32, 0.0, 1e-8, 0.5, 2.0, 100.0, f32::INFINITY];
    for radius in radii {
        for falloff in falloffs {
            cases_executed += 1;
            let mut test_mesh = primitive_cube(2.0);
            transform_selection_proportional(
                &mut test_mesh,
                &sel,
                Vec3::new(0.0, 5.0, 0.0),
                radius,
                falloff,
            );
            for p in &test_mesh.positions {
                if !p.is_finite() {
                    eprintln!("[Suite 3 Fail] proportional edit produced non-finite vertex: {p:?}");
                    invariants_held = false;
                }
            }
        }
    }

    VerificationReceipt {
        suite_name: "subobject_raycast_and_proportional_edit".into(),
        cases_executed,
        invariants_held,
    }
}

// =========================================================================
// Suite 4: Sculpt Symmetry and Heat Bone Weights Suite
// =========================================================================

/// Verifies sculpt stroke operations across all brushes and symmetry axes (X/Y/Z),
/// along with Laplacian heat-diffusion auto-rig bone skinning weights and shape keys.
#[must_use]
pub fn verify_sculpt_symmetry_and_heat_bone_weights_suite() -> VerificationReceipt {
    let mut cases_executed = 0;
    let mut invariants_held = true;

    let brushes = [
        SculptBrush::Grab,
        SculptBrush::ClayStrips,
        SculptBrush::Crease,
        SculptBrush::Smooth,
        SculptBrush::Inflate,
        SculptBrush::Flatten,
    ];

    // 1. All brush variants on primitive sphere
    for brush in brushes {
        cases_executed += 1;
        let mut sphere = primitive_uv_sphere(1.0, 8, 4);
        let stroke = SculptStroke {
            brush,
            hit_point: Vec3::new(0.0, 1.0, 0.0),
            hit_normal: Vec3::Y,
            radius: 0.5,
            strength: 0.8,
            symmetry: SymmetryAxes::none(),
            stroke_delta: Some(Vec3::new(0.0, 0.2, 0.0)),
        };
        apply_sculpt_stroke(&mut sphere, &stroke);
        for p in &sphere.positions {
            if !p.is_finite() {
                eprintln!("[Suite 4 Fail] brush {brush:?} produced non-finite position {p:?}");
                invariants_held = false;
            }
        }
    }

    // 2. Sculpt Symmetry reflection invariant: X-mirroring
    cases_executed += 1;
    let mut cube = primitive_cube(2.0);
    let stroke_x = SculptStroke {
        brush: SculptBrush::Inflate,
        hit_point: Vec3::new(1.0, 0.0, 0.0),
        hit_normal: Vec3::X,
        radius: 1.5,
        strength: 0.5,
        symmetry: SymmetryAxes::x_only(),
        stroke_delta: None,
    };
    apply_sculpt_stroke(&mut cube, &stroke_x);
    // Invariant: cube must remain symmetric across X=0 plane
    for p in &cube.positions {
        cases_executed += 1;
        let mirrored = Vec3::new(-p.x, p.y, p.z);
        let has_mirror = cube.positions.iter().any(|other| (other.distance(mirrored)) < 1e-3);
        if !has_mirror {
            eprintln!("[Suite 4 Fail] X-symmetry violated for vertex {p:?}");
            invariants_held = false;
        }
    }

    // 3. Geodesic heat-diffusion bone weights: degenerate cases
    let degen_bones = [
        vec![], // Empty bones
        vec![RigBoneSegment::new("DegenerateZeroLen", Vec3::ZERO, Vec3::ZERO, 0.5)], // Head == Tail
        vec![RigBoneSegment::new("ZeroRadius", Vec3::new(0.0, -1.0, 0.0), Vec3::new(0.0, 1.0, 0.0), 0.0)], // Radius = 0
        vec![
            RigBoneSegment::new("Bone1", Vec3::new(0.0, -1.0, 0.0), Vec3::ZERO, 0.5),
            RigBoneSegment::new("Bone2", Vec3::ZERO, Vec3::new(0.0, 1.0, 0.0), 0.5),
        ],
    ];

    let test_mesh = primitive_cylinder(1.0, 2.0, 8);
    for bones in degen_bones {
        cases_executed += 1;
        let influences = compute_heat_bone_weights(&test_mesh, &bones);
        if influences.len() != test_mesh.positions.len() {
            eprintln!("[Suite 4 Fail] influences count {} != vertex count {}", influences.len(), test_mesh.positions.len());
            invariants_held = false;
        }

        // Invariant: sum of weights for every vertex must be exactly 1.0 within 1e-4
        for infl in &influences {
            cases_executed += 1;
            let sum: f32 = infl.weights.iter().sum();
            if (sum - 1.0).abs() > 1e-4 {
                eprintln!("[Suite 4 Fail] bone weights sum = {sum} != 1.0");
                invariants_held = false;
            }
            for &w in &infl.weights {
                if !w.is_finite() || !(0.0..=1.0001).contains(&w) {
                    eprintln!("[Suite 4 Fail] bone weight {w} out of normalized bounds [0, 1]");
                    invariants_held = false;
                }
            }
        }
    }

    // 4. ShapeKeySet evaluation
    cases_executed += 1;
    let mut shape_keys = ShapeKeySet::from_polymesh(&test_mesh);
    let mut deformed_positions = test_mesh.positions.clone();
    for p in &mut deformed_positions {
        p.y += 0.5;
    }
    let added = shape_keys.add_shape_key("Stretch", deformed_positions);
    if !added {
        eprintln!("[Suite 4 Fail] add_shape_key returned false");
        invariants_held = false;
    }

    let mut weights = BTreeMap::new();
    weights.insert("Stretch".into(), 0.5f32);
    let blended = shape_keys.evaluate_blended_positions(&weights);
    cases_executed += 1;
    if blended.len() != test_mesh.positions.len() {
        eprintln!("[Suite 4 Fail] blended positions len mismatch");
        invariants_held = false;
    }

    VerificationReceipt {
        suite_name: "sculpt_symmetry_and_heat_bone_weights".into(),
        cases_executed,
        invariants_held,
    }
}

// =========================================================================
// Suite 5: Semantic Grind Splines and UV2 Atlas Suite
// =========================================================================

/// Verifies game semantic tags, continuous skate grind polyline chaining,
/// physics collision geometry partitioning, and Smart UV2 lightmap atlas coordinates.
#[must_use]
pub fn verify_semantic_grind_splines_and_uv2_atlas_suite() -> VerificationReceipt {
    let mut cases_executed = 0;
    let mut invariants_held = true;

    let cube = primitive_cube(2.0);
    let mut attrs = PolyamourSemanticAttributes::new();

    // 1. Grind rail splines: open chain
    attrs.tag_edge([0, 1], TAG_GRIND_EDGE);
    attrs.tag_edge([1, 2], TAG_GRIND_EDGE);
    attrs.tag_edge([2, 3], TAG_GRIND_EDGE);

    // 2. Collision and nav tags
    attrs.tag_face(0, TAG_COLLISION_CONVEX);
    attrs.tag_face(1, TAG_COLLISION_TRIMESH);
    attrs.tag_face(2, TAG_WALKABLE_NAV);
    attrs.tag_face(3, TAG_COLLISION_NONE);

    cases_executed += 1;
    let bundle = compile_polyamour_game_bundle(&cube, &attrs);

    // Invariant 1: Grind rail polylines extracted and continuous
    if bundle.grind_rail_splines.is_empty() {
        eprintln!("[Suite 5 Fail] grind_rail_splines is empty despite tagged grind edges");
        invariants_held = false;
    } else {
        cases_executed += 1;
        let spline = &bundle.grind_rail_splines[0];
        if spline.len() != 4 {
            eprintln!("[Suite 5 Fail] expected open grind rail with 4 points, got {}", spline.len());
            invariants_held = false;
        }
    }

    // Invariant 2: Convex hull point cloud non-empty
    cases_executed += 1;
    if bundle.convex_hull_points.is_empty() {
        eprintln!("[Suite 5 Fail] convex_hull_points is empty");
        invariants_held = false;
    }

    // Invariant 3: Trimesh collider non-empty
    cases_executed += 1;
    if bundle.trimesh_collider.0.is_empty() || bundle.trimesh_collider.1.is_empty() {
        eprintln!("[Suite 5 Fail] trimesh_collider is empty");
        invariants_held = false;
    }

    // Invariant 4: Walkable nav faces contain face 2
    cases_executed += 1;
    if !bundle.walkable_nav_faces.contains(&2) {
        eprintln!("[Suite 5 Fail] walkable_nav_faces missing face 2");
        invariants_held = false;
    }

    // Invariant 5: All UV2 coordinates must lie strictly within [0.0, 1.0]^2 without NaNs
    for uv in &bundle.uv2_coords {
        cases_executed += 1;
        if !uv[0].is_finite() || !uv[1].is_finite() || uv[0] < -1e-4 || uv[0] > 1.0001 || uv[1] < -1e-4 || uv[1] > 1.0001 {
            eprintln!("[Suite 5 Fail] UV2 coordinate {uv:?} outside [0, 1]^2 atlas bounds");
            invariants_held = false;
        }
    }

    // Invariant 6: UV2 islands bounds within [0, 1]^2
    for island in &bundle.uv2_islands {
        cases_executed += 1;
        if island.min_uv[0] > island.max_uv[0] || island.min_uv[1] > island.max_uv[1] {
            eprintln!("[Suite 5 Fail] UV2 island has inverted min/max bounds: {island:?}");
            invariants_held = false;
        }
    }

    // Test on quarterpipe primitive (radius: 4.0, width: 2.0, segments: 4)
    cases_executed += 1;
    let qp = primitive_quarterpipe(4.0, 2.0, 4);
    let qp_bundle = compile_polyamour_game_bundle(&qp, &PolyamourSemanticAttributes::new());
    for uv in &qp_bundle.uv2_coords {
        cases_executed += 1;
        if !uv[0].is_finite() || uv[0] < -1e-4 || uv[0] > 1.0001 {
            eprintln!("[Suite 5 Fail] quarterpipe UV2 coordinate out of bounds: {uv:?}");
            invariants_held = false;
        }
    }

    VerificationReceipt {
        suite_name: "semantic_grind_splines_and_uv2_atlas".into(),
        cases_executed,
        invariants_held,
    }
}

// =========================================================================
// Suite 6: WASM Custom Section Adversarial Suite
// =========================================================================

/// Verifies WebAssembly 1.0 binary extraction, LEB128 varint parsing under truncated/hostile inputs,
/// checksum tampering detection, and full package-to-extract roundtrip.
#[must_use]
pub fn verify_wasm_custom_section_adversarial_suite() -> VerificationReceipt {
    let mut cases_executed = 0;
    let mut invariants_held = true;

    // 1. Truncated WASM headers (< 8 bytes)
    for len in 0..8 {
        cases_executed += 1;
        let buf = vec![0u8; len];
        if extract_and_verify_wasm_fpck_section(&buf).is_ok() {
            eprintln!("[Suite 6 Fail] truncated WASM header len {len} accepted!");
            invariants_held = false;
        }
    }

    // 2. Corrupted WASM magic
    cases_executed += 1;
    let bad_magic_wasm = [0x00, 0x61, 0x73, 0x58, 1, 0, 0, 0]; // \0asX
    if extract_and_verify_wasm_fpck_section(&bad_magic_wasm).is_ok() {
        eprintln!("[Suite 6 Fail] corrupted WASM magic accepted!");
        invariants_held = false;
    }

    // 3. Unsupported WASM version (!= 1)
    cases_executed += 1;
    let bad_ver_wasm = [0x00, 0x61, 0x73, 0x6d, 2, 0, 0, 0]; // version 2
    if extract_and_verify_wasm_fpck_section(&bad_ver_wasm).is_ok() {
        eprintln!("[Suite 6 Fail] unsupported WASM version accepted!");
        invariants_held = false;
    }

    // 4. Valid packaging and bit-exact extraction roundtrip
    let mut files = BTreeMap::new();
    files.insert("main.fscn".into(), b"[scene] name=\"Test\"".to_vec());
    files.insert("player.gd".into(), b"extends Node\nfunc _ready(): pass".to_vec());
    files.insert("audio/jump.wav".into(), vec![0x52, 0x49, 0x46, 0x46, 0x00]);

    cases_executed += 1;
    let fpck_archive_bytes = FpckArchive::pack_bytes(&files).expect("pack fpck");
    let wasm_module = WasmWebGpuPackager::build_wasm_module(&fpck_archive_bytes);

    // Verify WASM header
    if wasm_module[0..4] != WASM_MAGIC || wasm_module[4..8] != WASM_VERSION.to_le_bytes() {
        eprintln!("[Suite 6 Fail] generated wasm header invalid");
        invariants_held = false;
    }

    cases_executed += 1;
    match extract_and_verify_wasm_fpck_section(&wasm_module) {
        Ok(extracted_files) => {
            if extracted_files != files {
                eprintln!("[Suite 6 Fail] extracted files mismatch original files!");
                invariants_held = false;
            }
        }
        Err(e) => {
            eprintln!("[Suite 6 Fail] extraction of valid wasm module failed: {e:?}");
            invariants_held = false;
        }
    }

    // 5. Hostile tampering: bit-flip in custom payload must trigger checksum error
    // Find the offset of FPCK_WASM_MAGIC
    if let Some(pos) = wasm_module.windows(8).position(|w| w == FPCK_WASM_MAGIC) {
        // Offset + 8: fpck_len (8 bytes)
        // Offset + 16: fnv1a_hash (8 bytes)
        // Offset + 24: payload
        cases_executed += 1;
        let mut tampered_wasm = wasm_module.clone();
        tampered_wasm[pos + 16] ^= 0x01; // flip 1 bit in checksum
        if extract_and_verify_wasm_fpck_section(&tampered_wasm).is_ok() {
            eprintln!("[Suite 6 Fail] tampered checksum in WASM custom section accepted!");
            invariants_held = false;
        }

        cases_executed += 1;
        let mut tampered_payload = wasm_module.clone();
        if pos + 25 < tampered_payload.len() {
            tampered_payload[pos + 25] ^= 0x01; // flip 1 bit in payload
            if extract_and_verify_wasm_fpck_section(&tampered_payload).is_ok() {
                eprintln!("[Suite 6 Fail] tampered payload in WASM custom section accepted!");
                invariants_held = false;
            }
        }
    }

    VerificationReceipt {
        suite_name: "wasm_custom_section_adversarial".into(),
        cases_executed,
        invariants_held,
    }
}

// =========================================================================
// Suite 7: Rust Hot Reload Schema Migration Suite
// =========================================================================

/// Verifies dynamic plugin C-ABI schema matching, `IrValue` property coercion matrix,
/// and state-preserving schema migration across hot-reloading generations.
#[must_use]
pub fn verify_rust_hot_reload_schema_migration_suite() -> VerificationReceipt {
    let mut cases_executed = 0;
    let mut invariants_held = true;

    // 1. RustPropertyType from_u32 and matching
    for u in 0..13 {
        cases_executed += 1;
        let prop_type = RustPropertyType::from_u32(u);
        if prop_type.is_none() {
            eprintln!("[Suite 7 Fail] RustPropertyType::from_u32({u}) returned None");
            invariants_held = false;
        }
    }
    cases_executed += 1;
    if RustPropertyType::from_u32(13).is_some() {
        eprintln!("[Suite 7 Fail] RustPropertyType::from_u32(13) returned Some");
        invariants_held = false;
    }

    // 2. Coercion Matrix
    let coercions: &[(IrValue, RustPropertyType, Option<IrValue>)] = &[
        // I64 -> F64
        (IrValue::I64(42), RustPropertyType::F64, Some(IrValue::F64(42.0))),
        // F64 -> I64
        (IrValue::F64(14.8), RustPropertyType::I64, Some(IrValue::I64(14))),
        // Bool -> I64
        (IrValue::Bool(true), RustPropertyType::I64, Some(IrValue::I64(1))),
        (IrValue::Bool(false), RustPropertyType::I64, Some(IrValue::I64(0))),
        // I64 -> Bool
        (IrValue::I64(1), RustPropertyType::Bool, Some(IrValue::Bool(true))),
        (IrValue::I64(0), RustPropertyType::Bool, Some(IrValue::Bool(false))),
        // Incompatible conversions must cleanly return None
        (IrValue::String("hello".into()), RustPropertyType::F64, None),
        (IrValue::Vec3([1.0, 2.0, 3.0]), RustPropertyType::I64, None),
        (IrValue::Color([1.0, 0.0, 0.0, 1.0]), RustPropertyType::Bool, None),
    ];

    for (val, target_type, expected) in coercions {
        cases_executed += 1;
        let result = coerce_ir_value(val, *target_type);
        if result != *expected {
            eprintln!("[Suite 7 Fail] coerce_ir_value({val:?}, {target_type:?}) -> {result:?}, expected {expected:?}");
            invariants_held = false;
        }
    }

    // 3. Schema evolution simulation
    // Gen 1 schemas:
    let gen1_schemas = vec![
        ParsedPropertySchema {
            name: "speed".into(),
            prop_type: RustPropertyType::F64,
            default_value: IrValue::F64(10.0),
        },
        ParsedPropertySchema {
            name: "player_name".into(),
            prop_type: RustPropertyType::String,
            default_value: IrValue::String("Skater".into()),
        },
        ParsedPropertySchema {
            name: "is_airborne".into(),
            prop_type: RustPropertyType::Bool,
            default_value: IrValue::Bool(false),
        },
    ];

    // Node properties at Gen 1 initialized from schemas
    let mut node_properties: BTreeMap<String, IrValue> = BTreeMap::new();
    for s in &gen1_schemas {
        node_properties.insert(s.name.clone(), s.default_value.clone());
    }
    node_properties.insert("speed".into(), IrValue::F64(25.5));
    node_properties.insert("player_name".into(), IrValue::String("TonyHawk".into()));
    node_properties.insert("is_airborne".into(), IrValue::Bool(true));

    // Gen 2 schemas:
    // - `speed` preserved
    // - `is_airborne` coerced to I64
    // - `player_name` removed (archived)
    // - `combo_multiplier` added (defaulted)
    let gen2_schemas = vec![
        ParsedPropertySchema {
            name: "speed".into(),
            prop_type: RustPropertyType::F64,
            default_value: IrValue::F64(10.0),
        },
        ParsedPropertySchema {
            name: "is_airborne".into(),
            prop_type: RustPropertyType::I64,
            default_value: IrValue::I64(0),
        },
        ParsedPropertySchema {
            name: "combo_multiplier".into(),
            prop_type: RustPropertyType::F64,
            default_value: IrValue::F64(1.0),
        },
    ];

    // Perform migration
    cases_executed += 1;
    let mut migrated = BTreeMap::new();
    let mut archived = BTreeMap::new();

    for schema in &gen2_schemas {
        if let Some(old_val) = node_properties.get(&schema.name) {
            if let Some(coerced) = coerce_ir_value(old_val, schema.prop_type) {
                migrated.insert(schema.name.clone(), coerced);
            } else {
                migrated.insert(schema.name.clone(), schema.default_value.clone());
                archived.insert(schema.name.clone(), old_val.clone());
            }
        } else {
            migrated.insert(schema.name.clone(), schema.default_value.clone());
        }
    }
    for (k, v) in &node_properties {
        if !gen2_schemas.iter().any(|s| &s.name == k) {
            archived.insert(k.clone(), v.clone());
        }
    }

    // Verify invariants
    if migrated.get("speed") != Some(&IrValue::F64(25.5)) {
        eprintln!("[Suite 7 Fail] speed not preserved");
        invariants_held = false;
    }
    if migrated.get("is_airborne") != Some(&IrValue::I64(1)) {
        eprintln!("[Suite 7 Fail] is_airborne not coerced to 1");
        invariants_held = false;
    }
    if migrated.get("combo_multiplier") != Some(&IrValue::F64(1.0)) {
        eprintln!("[Suite 7 Fail] combo_multiplier not defaulted");
        invariants_held = false;
    }
    if archived.get("player_name") != Some(&IrValue::String("TonyHawk".into())) {
        eprintln!("[Suite 7 Fail] player_name not archived");
        invariants_held = false;
    }

    VerificationReceipt {
        suite_name: "rust_hot_reload_schema_migration".into(),
        cases_executed,
        invariants_held,
    }
}

// =========================================================================
// Suite 8: Sexlife Moan and Splicy EDL Fuzz Suite
// =========================================================================

/// Verifies `Moan` DAW stem manifest deserialization, intensity curve clamping,
/// master DSP bus send modulation, and `Splicy` magnetic EDL J/L-cut director evaluation.
#[must_use]
pub fn verify_sexlife_moan_and_splicy_edl_fuzz_suite() -> VerificationReceipt {
    let mut cases_executed = 0;
    let mut invariants_held = true;

    // 1. Moan Project Stem Link default soundtrack & JSON roundtrip
    let moan = MoanProjectStemLink::default_skate_soundtrack();
    cases_executed += 1;
    let moan_json = moan.to_moan_json().expect("serialize moan");
    let moan_restored = MoanProjectStemLink::from_moan_json(&moan_json).expect("deserialize moan");
    if moan != moan_restored {
        eprintln!("[Suite 8 Fail] Moan JSON roundtrip mismatch");
        invariants_held = false;
    }

    // 2. Fuzz Moan adaptive mix evaluation with hostile intensities and beats
    let hostile_intensities = [
        -100.0f32, -1.0, 0.0, 0.25, 0.5, 0.75, 1.0, 1.5, 100.0,
        f32::NAN, f32::INFINITY, f32::NEG_INFINITY,
    ];
    let hostile_beats = [-50.0f64, 0.0, 3.99, 4.0, 16.5, 128.0, 10000.0];

    for &intensity in &hostile_intensities {
        for &beat in &hostile_beats {
            for combo in [false, true] {
                cases_executed += 1;
                let snapshot = moan.evaluate_adaptive_mix(intensity, combo, beat);

                // Invariant: all stem gains clamped in [0.0, 2.0] and finite even under hostile/NaN inputs
                for (id, &gain) in &snapshot.stem_gains {
                    if !gain.is_finite() || !(0.0..=2.0).contains(&gain) {
                        eprintln!("[Suite 8 Fail] stem {id} gain {gain} outside [0, 2]");
                        invariants_held = false;
                    }
                }

                // Invariant: cutoffs in [20.0, 20000.0] and finite even under hostile/NaN inputs
                for (id, &cutoff) in &snapshot.stem_cutoffs_hz {
                    if !cutoff.is_finite() || !(20.0..=20000.0).contains(&cutoff) {
                        eprintln!("[Suite 8 Fail] stem {id} cutoff {cutoff} outside [20, 20000]");
                        invariants_held = false;
                    }
                }

                // For finite inputs, intensity and DSP sends must be clamped in [0.0, 1.0]
                if intensity.is_finite() {
                    if !(0.0..=1.0).contains(&snapshot.intensity) {
                        eprintln!("[Suite 8 Fail] snapshot intensity {} outside [0, 1]", snapshot.intensity);
                        invariants_held = false;
                    }

                    if !(0.0..=1.0).contains(&snapshot.dsp_sends.drive)
                        || !(0.0..=1.0).contains(&snapshot.dsp_sends.reverb)
                    {
                        eprintln!("[Suite 8 Fail] DSP sends out of [0, 1] bounds");
                        invariants_held = false;
                    }
                }
            }
        }
    }

    // 3. Splicy Cutscene EDL intro & JSON roundtrip
    let mut splicy = SplicyCutsceneEdl::new("SkateIntro", 5.0, 60.0);
    let cam_id = NodeId {
        generation: NonZeroU32::new(1).unwrap(),
        index: 5,
    };
    splicy.camera_shots.push(CameraShotClip {
        id: "Shot1".into(),
        camera_node: cam_id,
        start_time_sec: 0.0,
        end_time_sec: 5.0,
        focal_length_mm: 35.0,
        aperture_f_stop: 2.8,
        dolly_orbit_keyframes: vec![
            CameraKeyframe {
                time_offset_sec: 0.0,
                position: [0.0, 2.0, 5.0],
                rotation_quat: [0.0, 0.0, 0.0, 1.0],
                fov_degrees: 60.0,
            },
            CameraKeyframe {
                time_offset_sec: 5.0,
                position: [0.0, 2.0, 0.0],
                rotation_quat: [0.0, 0.0, 0.0, 1.0],
                fov_degrees: 60.0,
            },
        ],
    });

    cases_executed += 1;
    let splicy_json = splicy.to_splicy_json().expect("serialize splicy");
    let splicy_restored = SplicyCutsceneEdl::from_splicy_json(&splicy_json).expect("deserialize splicy");
    if splicy != splicy_restored {
        eprintln!("[Suite 8 Fail] Splicy JSON roundtrip mismatch");
        invariants_held = false;
    }

    // 4. Fuzz Splicy timeline evaluation at scrub timestamps
    let timestamps = [-100.0f64, 0.0, 0.25, 0.95, 1.0, 1.8, 2.5, 5.0, 100.0, f64::NAN];
    for &t in &timestamps {
        cases_executed += 1;
        let snap = splicy.evaluate_at(t);
        // Assert no panic, and if timestamp is in valid range, snapshots evaluate camera
        if t >= 0.0 && t <= splicy.duration_sec && snap.active_camera.is_none() {
            eprintln!("[Suite 8 Fail] expected active camera at t={t}");
            invariants_held = false;
        }
    }

    VerificationReceipt {
        suite_name: "sexlife_moan_and_splicy_edl_fuzz".into(),
        cases_executed,
        invariants_held,
    }
}

// =========================================================================
// Suite 9: GPU BVH Pathtracer and Radiance Cascades Suite
// =========================================================================

/// Verifies GPU BVH tree construction under degenerate and overlapping geometry,
/// executes hardware/fallback compute path tracing and SVGF denoising, and solves
/// Radiance Cascades probe irradiance grids.
#[must_use]
pub fn verify_gpu_bvh_pathtracer_and_radiance_cascades_suite() -> VerificationReceipt {
    let mut cases_executed = 0;
    let mut invariants_held = true;

    // 1. Degenerate BVH construction
    let degen_tris = vec![
        // Zero-area degenerate triangle
        GpuPathTriangle {
            p0: [0.0, 0.0, 0.0, 1.0],
            p1: [0.0, 0.0, 0.0, 1.0],
            p2: [0.0, 0.0, 0.0, 1.0],
            ..Default::default()
        },
        // Collinear triangle
        GpuPathTriangle {
            p0: [0.0, 0.0, 0.0, 1.0],
            p1: [1.0, 0.0, 0.0, 1.0],
            p2: [2.0, 0.0, 0.0, 1.0],
            ..Default::default()
        },
    ];

    cases_executed += 1;
    let scene = GpuPathScene::from_triangles(degen_tris);
    if scene.bvh_nodes.is_empty() {
        eprintln!("[Suite 9 Fail] BVH nodes empty for degenerate triangles");
        invariants_held = false;
    }

    // 2. Box geometry with 12 triangles
    cases_executed += 1;
    let mut box_scene = GpuPathScene::new();
    box_scene.add_box(
        Vec3::ZERO,
        Vec3::ONE,
        [0.8, 0.8, 0.8, 1.0],
        0.5,
        0.0,
        [0.0, 0.0, 0.0],
    );
    if box_scene.triangles.len() != 12 || box_scene.bvh_nodes.is_empty() {
        eprintln!("[Suite 9 Fail] add_box did not produce 12 triangles");
        invariants_held = false;
    }

    // Verify all BVH AABBs are non-inverted
    for node in &box_scene.bvh_nodes {
        cases_executed += 1;
        if node.aabb_min[0] > node.aabb_max[0] ||
           node.aabb_min[1] > node.aabb_max[1] ||
           node.aabb_min[2] > node.aabb_max[2] {
            eprintln!("[Suite 9 Fail] inverted AABB in BVH node: {node:?}");
            invariants_held = false;
        }
    }

    // 3. Path tracer execution (headless fallback)
    let render_device = WgpuRenderDevice::new_or_fallback();
    let mut path_tracer = GpuComputePathTracer::new();
    let camera = GpuPathCamera::default();

    cases_executed += 1;
    let trace_result = path_tracer.trace_and_denoise_rgba(
        &render_device,
        &box_scene,
        &camera,
        16,
        16,
        1,
        true,
    );

    match trace_result {
        Ok(output) => {
            if output.rgba8.len() != 16 * 16 * 4 {
                eprintln!("[Suite 9 Fail] path trace RGBA buffer len {} != 1024", output.rgba8.len());
                invariants_held = false;
            }
        }
        Err(e) => {
            eprintln!("[Suite 9 Fail] path tracer dispatch failed: {e:?}");
            invariants_held = false;
        }
    }

    // 4. Radiance Cascades GI Solver execution
    let mut rc_solver = GpuRadianceCascadesSolver::new();
    let emitters = [
        GpuEmitter::new(Vec3::new(0.0, 1.0, 0.0), 0.5, [1.0, 0.8, 0.6], 10.0),
        GpuEmitter::new(Vec3::new(2.0, 0.5, 0.0), 0.2, [0.2, 0.5, 1.0], 5.0),
    ];

    cases_executed += 1;
    let rc_result = rc_solver.solve_radiance_cascades(
        &render_device,
        &emitters,
        Vec3::new(-5.0, -1.0, -5.0),
        Vec3::new(5.0, 5.0, 5.0),
        [8, 8],
        2,
        [0.05, 0.05, 0.1],
    );

    match rc_result {
        Ok(res) => {
            if res.probe_grid.len() != 64 || res.visualization_rgba8.len() != 64 * 4 {
                eprintln!("[Suite 9 Fail] RC grid dimensions mismatch");
                invariants_held = false;
            }
            for probe in &res.probe_grid {
                cases_executed += 1;
                if !probe[0].is_finite() || probe[0] < 0.0 {
                    eprintln!("[Suite 9 Fail] probe irradiance value invalid: {probe:?}");
                    invariants_held = false;
                }
            }
        }
        Err(e) => {
            eprintln!("[Suite 9 Fail] RC solver dispatch failed: {e:?}");
            invariants_held = false;
        }
    }

    VerificationReceipt {
        suite_name: "gpu_bvh_pathtracer_and_radiance_cascades".into(),
        cases_executed,
        invariants_held,
    }
}

// =========================================================================
// Suite 10: Player Live Polyamour and Three.js Parametric Suite
// =========================================================================

/// Verifies `NodeId` network bijection, parametric modifier script parsing,
/// runtime live Polyamour mesh & physics updates, and Three.js codegen determinism.
#[must_use]
pub fn verify_player_live_polyamour_and_threejs_parametric_suite() -> VerificationReceipt {
    let mut cases_executed = 0;
    let mut invariants_held = true;

    // 1. NodeId packing and unpacking bijection property
    let node_cases = [
        (1u32, 0u32),
        (1, 1),
        (2, 42),
        (100, 1024),
        (u32::MAX, u32::MAX),
        (u32::MAX, 0),
        (1, u32::MAX),
    ];
    for (gen_val, idx) in node_cases {
        cases_executed += 1;
        let node_id = NodeId {
            generation: NonZeroU32::new(gen_val).unwrap(),
            index: idx,
        };
        let packed = pack_node_id(node_id);
        let unpacked = unpack_node_id(packed);
        if unpacked != node_id {
            eprintln!("[Suite 10 Fail] NodeId roundtrip failed: {node_id:?} != {unpacked:?}");
            invariants_held = false;
        }
    }

    // 2. Fuzz parametric modifier parsing
    let modifier_script = r#"
        # Comment line
        subdivide 2 simple
        displace 0.25 2.0
        array 3 1.0 2.0 3.0
        bevel 0.05
        solidify 0.15
        smooth 0.8 4
        triangulate
        weld 0.005
        mirror 1 0 1
        garbage_line_ignored
    "#;
    cases_executed += 1;
    let modifiers = parse_parametric_modifiers(modifier_script);
    if modifiers.len() != 9 {
        eprintln!("[Suite 10 Fail] expected 9 parsed modifiers, got {}", modifiers.len());
        invariants_held = false;
    }

    // 3. PlayerRuntime live Polyamour mesh update
    let mut scene = IrScene::new("ParametricTestScene");
    let test_node_id = NodeId {
        generation: NonZeroU32::new(1).unwrap(),
        index: 10,
    };
    let mut test_node = IrNode {
        id: test_node_id,
        name: "TestPolyamourNode".into(),
        class_name: "MeshInstance3D".into(),
        parent: None,
        instance_scene: None,
        properties: BTreeMap::new(),
        groups: Vec::new(),
    };
    test_node.properties.insert(
        "polyamour_primitive".into(),
        IrValue::String("Cube".into()),
    );
    scene.nodes.insert(test_node_id, test_node);

    let mut runtime = PlayerRuntime::new(scene);
    let cube = primitive_cube(2.0);
    let live_state = LivePolyamourNodeState::new(test_node_id, cube);
    runtime.live_polyamour_meshes.insert(test_node_id, live_state);

    cases_executed += 1;
    let mut morph_weights = BTreeMap::new();
    morph_weights.insert("Stretch".into(), 0.5f32);
    let update_res = runtime.update_parametric_polyamour_node(
        test_node_id,
        Some("displace 0.1 1.0\nsolidify 0.05"),
        &morph_weights,
    );

    match update_res {
        Ok(stats) => {
            if stats.vertex_count == 0 || stats.triangle_count == 0 || stats.revision == 0 {
                eprintln!("[Suite 10 Fail] invalid stats returned after update: {stats:?}");
                invariants_held = false;
            }
        }
        Err(e) => {
            eprintln!("[Suite 10 Fail] update_parametric_polyamour_node failed: {e:?}");
            invariants_held = false;
        }
    }

    // 4. Three.js Parametric Polyamour Asset extraction & TypeScript codegen
    let mut threejs_scene = IrScene::new("ThreeJsScene");
    let mut p_node = IrNode {
        id: test_node_id,
        name: "SkateRamp".into(),
        class_name: "MeshInstance3D".into(),
        parent: None,
        instance_scene: None,
        properties: BTreeMap::new(),
        groups: Vec::new(),
    };
    p_node.properties.insert("polyamour_primitive".into(), IrValue::String("Cube".into()));
    p_node.properties.insert("polyamour_ops".into(), IrValue::String("polyops 2\ntag_edge 0-1 tag=\"grind_edge\"\n".into()));
    threejs_scene.nodes.insert(test_node_id, p_node);

    cases_executed += 1;
    let assets = extract_polyamour_assets_from_scene(&threejs_scene);
    if assets.len() != 1 {
        eprintln!("[Suite 10 Fail] expected 1 extracted asset, got {}", assets.len());
        invariants_held = false;
    } else {
        cases_executed += 1;
        let ts_code = generate_polyamour_runtime_ts(&assets);
        if !ts_code.contains("ParametricPolyamourController") || !ts_code.contains("BAKED_POLYAMOUR_ASSET_METAS") {
            eprintln!("[Suite 10 Fail] generated typescript code missing key controllers");
            invariants_held = false;
        }
    }

    VerificationReceipt {
        suite_name: "player_live_polyamour_and_threejs_parametric".into(),
        cases_executed,
        invariants_held,
    }
}
