//! Coroutines and `await` execution frames for GDScript 2.0.

use crate::vm::VmExecutionResult;
use foreplay_ir::IrValue;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Unique identifier for an in-flight suspended coroutine.
pub type CoroutineId = u64;

/// Target event/condition that a suspended coroutine is waiting for.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub enum AwaitTarget {
    Signal {
        emitter: Option<String>,
        signal_name: String,
    },
    Timer {
        remaining_sec: f64,
    },
    Coroutine(CoroutineId),
}

/// A saved stack frame within a coroutine execution context.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct GdStackFrame {
    pub function_name: String,
    pub statement_cursor: usize,
    pub locals: BTreeMap<String, IrValue>,
}

/// Suspended coroutine state preserved across async yields.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct GdCoroutineFrame {
    pub id: CoroutineId,
    pub function_name: String,
    pub statement_cursor: usize,
    pub locals: BTreeMap<String, IrValue>,
    pub call_stack: Vec<GdStackFrame>,
    pub await_target: AwaitTarget,
}

/// Outcome of invoking a function via `GdVm::call_or_suspend`.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub enum GdCoroutineOutcome {
    Completed(VmExecutionResult),
    Suspended {
        coroutine_id: CoroutineId,
        target: AwaitTarget,
    },
}
