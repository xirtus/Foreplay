//! First-class `Callable` and lambda support for GDScript 2.0.

use foreplay_ir::IrValue;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// First-class `Callable` referencing a method on a target or an anonymous lambda function.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct GdCallable {
    /// Object reference / scope name (e.g. "self" or specific node identifier).
    pub target: Option<String>,
    /// Method name or anonymous lambda symbol name.
    pub method: String,
    /// Bound arguments appended/prepended via `.bind(...)`.
    pub bound_args: Vec<IrValue>,
}

impl GdCallable {
    /// Creates a new callable referencing `target.method(...)`.
    pub fn new(target: Option<String>, method: impl Into<String>) -> Self {
        Self {
            target,
            method: method.into(),
            bound_args: Vec::new(),
        }
    }

    /// Creates a partially bound copy of this callable.
    pub fn bind(&self, extra_args: &[IrValue]) -> Self {
        let mut new_bound = self.bound_args.clone();
        new_bound.extend_from_slice(extra_args);
        Self {
            target: self.target.clone(),
            method: self.method.clone(),
            bound_args: new_bound,
        }
    }

    /// Returns whether the callable has a non-empty method name.
    pub fn is_valid(&self) -> bool {
        !self.method.trim().is_empty()
    }

    /// Prepares final argument slice combining call arguments and bound arguments.
    /// In Godot 4, call arguments come first, followed by bound arguments.
    pub fn combine_arguments(&self, call_args: &[IrValue]) -> Vec<IrValue> {
        let mut combined = Vec::with_capacity(call_args.len() + self.bound_args.len());
        combined.extend_from_slice(call_args);
        combined.extend_from_slice(&self.bound_args);
        combined
    }

    /// Encodes this `GdCallable` as an `IrValue::Map`.
    pub fn to_ir_value(&self) -> IrValue {
        let mut map = BTreeMap::new();
        map.insert("__type__".into(), IrValue::String("Callable".into()));
        if let Some(ref t) = self.target {
            map.insert("target".into(), IrValue::String(t.clone()));
        }
        map.insert("method".into(), IrValue::String(self.method.clone()));
        map.insert("bound_args".into(), IrValue::Array(self.bound_args.clone()));
        IrValue::Map(map)
    }

    /// Attempts to extract a `GdCallable` from an `IrValue::Map`.
    pub fn from_ir_value(val: &IrValue) -> Option<Self> {
        if let IrValue::Map(m) = val
            && m.get("__type__") == Some(&IrValue::String("Callable".into()))
        {
            let target = match m.get("target") {
                Some(IrValue::String(s)) => Some(s.clone()),
                _ => None,
            };
            let method = match m.get("method") {
                Some(IrValue::String(s)) => s.clone(),
                _ => String::new(),
            };
            let bound_args = match m.get("bound_args") {
                Some(IrValue::Array(arr)) => arr.clone(),
                _ => Vec::new(),
            };
            return Some(Self {
                target,
                method,
                bound_args,
            });
        }
        None
    }
}
