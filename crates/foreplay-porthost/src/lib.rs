//! `PortHost` — Clean-room game port hosting (`skate2-rust-port` BYO-ROM + `rustports.org` differential oracle).
//!
//! Provides Godot 4 `.tscn` text importer, zero-panic endian-aware binary archive reader,
//! `port://` VFS mounts, and the multi-frame Differential Oracle.

pub mod archive;
pub mod oracle;
pub mod tscn;

pub use archive::{BinaryReadError, BinaryReader, DirectoryPortMount, Endian, SimplePakMount};
pub use oracle::{
    DifferentialReport, FrameDifferentialVerdict, OracleFrameSample, evaluate_trace,
};
pub use tscn::{TscnParseError, parse_godot_value, parse_tscn_to_ir};

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
}
