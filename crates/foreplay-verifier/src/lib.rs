//! Independent Adversarial Verifier (`foreplay-verifier` — Track E Gemini 4 Carbon).
//!
//! Attacks parsers, Undo/Redo state machines, hierarchy invariants, and differential frames.

pub mod adversarial;
pub mod scene_tree_fuzzer;
pub mod undo_redo_fuzzer;

pub use adversarial::run_all_adversarial_suites;

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
