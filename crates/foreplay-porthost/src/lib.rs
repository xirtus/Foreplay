//! `PortHost` — Clean-room game port hosting (`skate2-rust-port` BYO-ROM + `rustports.org` differential oracle).
//!
//! Provides Godot 4 `.tscn` text importer and exporter, 200-scene conformance corpus,
//! zero-panic endian-aware binary archive reader, `port://` VFS mounts, BYO-ROM validator,
//! world cell streamer, and the multi-frame Differential Oracle.

pub mod archive;
pub mod conformance;
pub mod gltf_import;
pub mod oracle;
pub mod resource_loader;
pub mod streaming;
pub mod tres;
pub mod tscn;

pub use archive::{BinaryReadError, BinaryReader, DirectoryPortMount, Endian, SimplePakMount};
pub use conformance::{
    ConformanceReport, generate_godot4_conformance_corpus, run_200_tscn_conformance_gate,
};
pub use gltf_import::{
    build_sample_rigged_character_glb, import_glb_bytes_to_ir, GlbAnimationClip,
    GlbAnimationProperty, GlbAnimationTrack, GlbImportError, GlbImportedBundle, GlbInterpolation,
    GlbMaterialData, GlbMeshData, GlbPrimitiveData, GlbSkinData, GlbTrackValues,
};
pub use oracle::{
    DifferentialReport, FrameDifferentialVerdict, OracleFrameSample, evaluate_trace,
    generate_1000_frame_skater_trace, run_1000_frame_differential_gate,
};
pub use resource_loader::{
    generate_fnv1a_uid, LoadedResource, ResourceLoaderError, ThreadLoadStatus,
    ThreadedResourceLoader, UidRegistry,
};
pub use streaming::{
    ByoRomWizard, CellCoord, CellStreamDelta, RomSignature, RomValidationResult, WorldCellStreamer,
};
pub use tres::{
    emit_tres, format_tres_value, parse_tres, parse_tres_value, TresExtResource, TresFile,
    TresHeader, TresParseError, TresResource, TresSubResource, TresValue,
};
pub use tscn::{TscnParseError, export_ir_to_tscn, parse_godot_value, parse_tscn_to_ir};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn differential_oracle_flags_drift_above_epsilon() {
        let pass = FrameDifferentialVerdict::evaluate(42, 0.0001, 0.0002, 0.001);
        assert!(pass.within_epsilon);

        let drift = FrameDifferentialVerdict::evaluate(43, 0.05, 0.0, 0.001);
        assert!(!drift.within_epsilon);
    }

    #[test]
    fn template_2d_platformer_roundtrip() {
        let tscn = include_str!("../../../templates/2d-platformer/level_01.tscn");
        let ir = parse_tscn_to_ir(tscn).expect("level_01.tscn should parse");
        assert_eq!(ir.name, "Level01");
        assert_eq!(ir.nodes.len(), 9);
        assert_eq!(ir.connections.len(), 1);

        let exported = export_ir_to_tscn(&ir);
        let re_parsed = parse_tscn_to_ir(&exported).expect("exported level_01.tscn should re-parse");
        assert_eq!(re_parsed.nodes.len(), ir.nodes.len());
        assert_eq!(re_parsed.connections.len(), ir.connections.len());
    }
}
