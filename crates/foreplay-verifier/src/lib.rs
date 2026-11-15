//! Independent Adversarial Verifier (`foreplay-verifier` — Track E Gemini 4 Carbon).
//!
//! Attacks parsers, Undo/Redo state machines, hierarchy invariants, and differential frames.

pub mod adversarial;
pub mod parser_fuzzers;
pub mod scene_tree_fuzzer;
pub mod undo_redo_fuzzer;
pub mod wave2b_fuzzers;

pub use adversarial::run_all_adversarial_suites;
pub use wave2b_fuzzers::{
    verify_1000_frame_physics_differential_gate, verify_fpck_archive_adversarial_suite,
    verify_gdshader_compiler_adversarial_suite, verify_godot4_200_scene_conformance_gate,
    verify_project_exporter_bundle_invariants_suite, verify_script_sandbox_termination_suite,
    verify_tscn_bidirectional_roundtrip_suite,
};

/// Summary receipt produced by an adversarial verification run.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct VerificationReceipt {
    pub suite_name: String,
    pub cases_executed: usize,
    pub invariants_held: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_all_adversarial_suites_passes_100_percent() {
        let receipts = run_all_adversarial_suites();
        assert!(!receipts.is_empty(), "Receipts must not be empty");
        for r in &receipts {
            assert!(
                r.invariants_held,
                "Suite '{}' failed adversarial invariants!",
                r.suite_name
            );
            assert!(
                r.cases_executed > 0,
                "Suite '{}' executed 0 cases!",
                r.suite_name
            );
        }
    }
}
