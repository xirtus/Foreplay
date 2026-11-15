//! Safe deterministic GDScript execution VM (`GdVm`).
//!
//! Evaluates GDScript AST with bounded execution budgets, mutating `foreplay_ir::IrNode`
//! properties, dispatching lifecycle methods (`_ready`, `_physics_process`), coroutine
//! resumption via `await`, first-class `Callable`/lambdas, `super` dispatch, and structural `match`.

use crate::builtins_catalog::{eval_global_builtin, eval_method, eval_node_builtin, ir_to_string};
use crate::callable::GdCallable;
use crate::coroutines::{AwaitTarget, CoroutineId, GdCoroutineFrame, GdCoroutineOutcome};
use crate::types::GdType;
use crate::{GdScriptModule, parse_gdscript};
use foreplay_ir::{IrNode, IrValue};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

static LAMBDA_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Execution errors produced by `GdVm`.
#[derive(Clone, PartialEq, Eq, Debug, thiserror::Error, Serialize, Deserialize)]
pub enum VmError {
    #[error("execution budget exceeded: maximum {budget} steps allowed")]
    BudgetExceeded { budget: usize },
    #[error("variable '{name}' not found")]
    VariableNotFound { name: String },
    #[error("method '{name}' not found")]
    MethodNotFound { name: String },
    #[error("type error: {message}")]
    TypeError { message: String },
    #[error("division by zero")]
    DivisionByZero,
    #[error("syntax error parsing function body: {message}")]
    ParseError { message: String },
}

/// Execution outcome of a VM function invocation.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct VmExecutionResult {
    pub return_value: Option<IrValue>,
    pub emitted_signals: Vec<(String, Vec<IrValue>)>,
    pub logs: Vec<String>,
    pub steps_executed: usize,
}

#[derive(Clone, PartialEq, Debug)]
pub enum AssignOp {
    Assign,
    AddAssign,
    SubAssign,
    MulAssign,
    DivAssign,
}

#[derive(Clone, PartialEq, Debug)]
pub enum AssignTarget {
    Variable(String),
    Property { base: String, prop: String },
    Subscript { base: String, index: Box<Expr> },
}

#[derive(Clone, PartialEq, Debug)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

#[derive(Clone, PartialEq, Debug)]
pub enum UnaryOp {
    Neg,
    Not,
}

#[derive(Clone, PartialEq, Debug)]
pub enum GdPattern {
    Wildcard,
    Literal(IrValue),
    Bind(String),
    Array {
        patterns: Vec<GdPattern>,
        has_rest: bool,
    },
    Dict {
        pairs: Vec<(String, GdPattern)>,
    },
}

#[derive(Clone, PartialEq, Debug)]
pub struct MatchArm {
    pub patterns: Vec<GdPattern>,
    pub guard: Option<Expr>,
    pub body: Vec<Stmt>,
}

#[derive(Clone, PartialEq, Debug)]
pub enum Expr {
    Literal(IrValue),
    Var(String),
    PropAccess {
        base: Box<Expr>,
        prop: String,
    },
    Binary {
        op: BinOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
    },
    Call {
        func_name: String,
        args: Vec<Expr>,
    },
    MethodCall {
        target: Box<Expr>,
        method: String,
        args: Vec<Expr>,
    },
    Lambda {
        parameters: Vec<(String, Option<IrValue>)>,
        body: Vec<Stmt>,
    },
    SuperCall {
        method: Option<String>,
        args: Vec<Expr>,
    },
    Await(Box<Expr>),
    ArrayLit(Vec<Expr>),
    DictLit(Vec<(Expr, Expr)>),
    Subscript {
        base: Box<Expr>,
        index: Box<Expr>,
    },
}

#[derive(Clone, PartialEq, Debug)]
pub enum Stmt {
    Pass,
    VarDecl {
        name: String,
        init: Option<Expr>,
    },
    Assign {
        target: AssignTarget,
        op: AssignOp,
        expr: Expr,
    },
    If {
        condition: Expr,
        body: Vec<Stmt>,
        elifs: Vec<(Expr, Vec<Stmt>)>,
        else_body: Option<Vec<Stmt>>,
    },
    While {
        condition: Expr,
        body: Vec<Stmt>,
    },
    For {
        var_name: String,
        range_start: Expr,
        range_end: Expr,
        body: Vec<Stmt>,
    },
    ForIn {
        var_name: String,
        collection: Expr,
        body: Vec<Stmt>,
    },
    Match {
        expr: Expr,
        arms: Vec<MatchArm>,
    },
    Call(Expr),
    Return(Option<Expr>),
    Await(Expr),
}

#[derive(Clone, PartialEq, Debug)]
pub struct FunctionAst {
    pub name: String,
    pub parameters: Vec<String>,
    pub default_args: BTreeMap<String, IrValue>,
    pub body: Vec<Stmt>,
}

/// Deterministic GDScript Virtual Machine executing scripts bound to `IrNode` instances.
#[derive(Clone, PartialEq, Debug)]
pub struct GdVm {
    pub module: GdScriptModule,
    pub member_vars: BTreeMap<String, IrValue>,
    pub functions: BTreeMap<String, FunctionAst>,
    pub max_step_budget: usize,
    pub suspended_coroutines: BTreeMap<CoroutineId, GdCoroutineFrame>,
    pub next_coroutine_id: CoroutineId,
    pub base_classes: BTreeMap<String, Box<GdVm>>,
    pub var_types: BTreeMap<String, GdType>,
    pub bound_node: Option<IrNode>,
}

impl GdVm {
    pub const DEFAULT_STEP_BUDGET: usize = 10_000;

    /// Compiles GDScript source into a `GdVm` instance.
    pub fn from_source(source: &str) -> Result<Self, VmError> {
        let module = parse_gdscript(source)
            .map_err(|e| VmError::ParseError { message: e.to_string() })?;

        let mut member_vars = BTreeMap::new();
        let mut var_types = BTreeMap::new();

        for export in &module.exports {
            if let Some(hint) = &export.type_hint {
                var_types.insert(export.name.clone(), GdType::parse(hint));
            }
            if let Some(val) = &export.default_value {
                let final_val = if let Some(t) = var_types.get(&export.name) {
                    t.coerce_value(val.clone())
                        .map_err(|e| VmError::TypeError { message: e })?
                } else {
                    val.clone()
                };
                member_vars.insert(export.name.clone(), final_val);
            }
        }

        // Initialize top-level member variables (both @export and plain var)
        for line in source.lines() {
            let trimmed = line.trim();
            let indent = line.len() - line.trim_start().len();
            if indent == 0
                && let Some(rest) = trimmed.strip_prefix("var ")
            {
                let (name, type_str, expr_str) = if let Some(eq) = rest.find('=') {
                    let n = rest[..eq].trim();
                    let (clean_n, t) = if let Some(col) = n.find(':') {
                        (n[..col].trim(), Some(n[col + 1..].trim()))
                    } else {
                        (n, None)
                    };
                    (clean_n, t, Some(rest[eq + 1..].trim()))
                } else {
                    let (clean_n, t) = if let Some(col) = rest.find(':') {
                        (rest[..col].trim(), Some(rest[col + 1..].trim()))
                    } else {
                        (rest.trim(), None)
                    };
                    (clean_n, t, None)
                };

                if let Some(t) = type_str {
                    var_types.insert(name.to_string(), GdType::parse(t));
                }

                if let Some(expr_s) = expr_str
                    && let Ok(expr) = parse_expression(expr_s)
                {
                    let mut dummy_ctx = ExecutionContext {
                        steps: 0,
                        max_budget: 100,
                        emitted_signals: Vec::new(),
                        logs: Vec::new(),
                        current_func: String::new(),
                    };
                    let dummy_locals = BTreeMap::new();
                    let mut vm_init = GdVm {
                        module: module.clone(),
                        member_vars: BTreeMap::new(),
                        functions: BTreeMap::new(),
                        max_step_budget: 100,
                        suspended_coroutines: BTreeMap::new(),
                        next_coroutine_id: 1,
                        base_classes: BTreeMap::new(),
                        var_types: var_types.clone(),
                        bound_node: None,
                    };
                    if let Ok(val) = vm_init.eval_expr(&expr, &dummy_locals, &mut dummy_ctx) {
                        let final_val = if let Some(t) = var_types.get(name) {
                            t.coerce_value(val).unwrap_or(IrValue::Nil)
                        } else {
                            val
                        };
                        member_vars.insert(name.to_string(), final_val);
                    }
                }
            }
        }

        let functions = parse_source_functions(source)?;

        Ok(Self {
            module,
            member_vars,
            functions,
            max_step_budget: Self::DEFAULT_STEP_BUDGET,
            suspended_coroutines: BTreeMap::new(),
            next_coroutine_id: 1,
            base_classes: BTreeMap::new(),
            var_types,
            bound_node: None,
        })
    }

    /// Registers a base class VM for `super` dispatch.
    pub fn register_base_class(&mut self, class_name: &str, base_vm: GdVm) {
        self.base_classes.insert(class_name.to_string(), Box::new(base_vm));
    }

    /// Invokes `_ready()` on the bound `IrNode`.
    pub fn ready(&mut self, node: &mut IrNode) -> Result<VmExecutionResult, VmError> {
        self.call_method("_ready", &[], node)
    }

    /// Invokes `_process(delta)` on the bound `IrNode`.
    pub fn step_process(&mut self, delta: f64, node: &mut IrNode) -> Result<VmExecutionResult, VmError> {
        self.call_method("_process", &[IrValue::F64(delta)], node)
    }

    /// Invokes `_physics_process(delta)` on the bound `IrNode`.
    pub fn step_physics_process(&mut self, delta: f64, node: &mut IrNode) -> Result<VmExecutionResult, VmError> {
        self.call_method("_physics_process", &[IrValue::F64(delta)], node)
    }

    /// Invokes a named function with arguments on the bound `IrNode`.
    pub fn call_method(
        &mut self,
        method_name: &str,
        args: &[IrValue],
        node: &mut IrNode,
    ) -> Result<VmExecutionResult, VmError> {
        match self.call_or_suspend(method_name, args, node)? {
            GdCoroutineOutcome::Completed(res) => Ok(res),
            GdCoroutineOutcome::Suspended { .. } => Ok(VmExecutionResult::default()),
        }
    }

    /// Invokes a function, either completing or suspending into a coroutine upon reaching an `await`.
    pub fn call_or_suspend(
        &mut self,
        method_name: &str,
        args: &[IrValue],
        node: &mut IrNode,
    ) -> Result<GdCoroutineOutcome, VmError> {
        let func = match self.functions.get(method_name) {
            Some(f) => f.clone(),
            None => {
                if method_name.starts_with('_') {
                    return Ok(GdCoroutineOutcome::Completed(VmExecutionResult::default()));
                }
                return Err(VmError::MethodNotFound { name: method_name.to_string() });
            }
        };

        if !node.properties.contains_key("position") {
            node.properties.insert("position".into(), IrValue::Vec3([0.0, 0.0, 0.0]));
        }

        for (k, v) in &node.properties {
            self.member_vars.insert(k.clone(), v.clone());
        }

        self.bound_node = Some(node.clone());

        let mut local_vars = BTreeMap::new();
        for (idx, param) in func.parameters.iter().enumerate() {
            let arg_val = args
                .get(idx)
                .cloned()
                .or_else(|| func.default_args.get(param).cloned())
                .unwrap_or(IrValue::Nil);
            local_vars.insert(param.clone(), arg_val);
        }

        let mut ctx = ExecutionContext {
            steps: 0,
            max_budget: self.max_step_budget,
            emitted_signals: Vec::new(),
            logs: Vec::new(),
            current_func: method_name.to_string(),
        };

        let exec_outcome = self.execute_block_suspendable(&func.body, 0, &mut local_vars, &mut ctx)?;

        for (k, v) in &self.member_vars {
            node.properties.insert(k.clone(), v.clone());
        }
        self.bound_node = Some(node.clone());

        match exec_outcome {
            BlockOutcome::Completed(ret_val) => Ok(GdCoroutineOutcome::Completed(VmExecutionResult {
                return_value: ret_val,
                emitted_signals: ctx.emitted_signals,
                logs: ctx.logs,
                steps_executed: ctx.steps,
            })),
            BlockOutcome::Suspended { cursor, target } => {
                let coroutine_id = self.next_coroutine_id;
                self.next_coroutine_id += 1;
                let frame = GdCoroutineFrame {
                    id: coroutine_id,
                    function_name: method_name.to_string(),
                    statement_cursor: cursor,
                    locals: local_vars,
                    call_stack: Vec::new(),
                    await_target: target.clone(),
                };
                self.suspended_coroutines.insert(coroutine_id, frame);
                Ok(GdCoroutineOutcome::Suspended { coroutine_id, target })
            }
        }
    }

    /// Advances active timer countdowns on suspended coroutines and resumes any that expired.
    pub fn tick_timers(&mut self, dt: f64) -> Result<Vec<VmExecutionResult>, VmError> {
        let mut expired = Vec::new();
        for (cid, frame) in &mut self.suspended_coroutines {
            if let AwaitTarget::Timer { remaining_sec } = &mut frame.await_target {
                *remaining_sec -= dt;
                if *remaining_sec <= 0.0 {
                    expired.push(*cid);
                }
            }
        }

        let mut results = Vec::new();
        for cid in expired {
            if let Some(frame) = self.suspended_coroutines.remove(&cid) {
                let res = self.resume_coroutine(frame)?;
                if let GdCoroutineOutcome::Completed(r) = res {
                    results.push(r);
                }
            }
        }
        Ok(results)
    }

    /// Resumes any suspended coroutines waiting for the specified signal.
    pub fn emit_signal_resume(&mut self, signal_name: &str, _payload: &[IrValue]) -> Result<Vec<VmExecutionResult>, VmError> {
        let mut matching = Vec::new();
        for (cid, frame) in &self.suspended_coroutines {
            if let AwaitTarget::Signal { signal_name: sig, .. } = &frame.await_target
                && sig == signal_name
            {
                matching.push(*cid);
            }
        }

        let mut results = Vec::new();
        for cid in matching {
            if let Some(frame) = self.suspended_coroutines.remove(&cid) {
                let res = self.resume_coroutine(frame)?;
                if let GdCoroutineOutcome::Completed(r) = res {
                    results.push(r);
                }
            }
        }
        Ok(results)
    }

    fn resume_coroutine(&mut self, mut frame: GdCoroutineFrame) -> Result<GdCoroutineOutcome, VmError> {
        let func = match self.functions.get(&frame.function_name) {
            Some(f) => f.clone(),
            None => return Err(VmError::MethodNotFound { name: frame.function_name }),
        };

        let mut ctx = ExecutionContext {
            steps: 0,
            max_budget: self.max_step_budget,
            emitted_signals: Vec::new(),
            logs: Vec::new(),
            current_func: frame.function_name.clone(),
        };

        let outcome = self.execute_block_suspendable(&func.body, frame.statement_cursor, &mut frame.locals, &mut ctx)?;

        if let Some(node) = &mut self.bound_node {
            for (k, v) in &self.member_vars {
                node.properties.insert(k.clone(), v.clone());
            }
        }

        match outcome {
            BlockOutcome::Completed(ret_val) => Ok(GdCoroutineOutcome::Completed(VmExecutionResult {
                return_value: ret_val,
                emitted_signals: ctx.emitted_signals,
                logs: ctx.logs,
                steps_executed: ctx.steps,
            })),
            BlockOutcome::Suspended { cursor, target } => {
                let new_cid = self.next_coroutine_id;
                self.next_coroutine_id += 1;
                frame.statement_cursor = cursor;
                frame.await_target = target.clone();
                self.suspended_coroutines.insert(new_cid, frame);
                Ok(GdCoroutineOutcome::Suspended { coroutine_id: new_cid, target })
            }
        }
    }

    fn execute_block_suspendable(
        &mut self,
        block: &[Stmt],
        start_cursor: usize,
        locals: &mut BTreeMap<String, IrValue>,
        ctx: &mut ExecutionContext,
    ) -> Result<BlockOutcome, VmError> {
        let mut idx = start_cursor;
        while idx < block.len() {
            ctx.tick()?;
            let stmt = &block[idx];
            match stmt {
                Stmt::Pass => {
                    idx += 1;
                }
                Stmt::VarDecl { name, init } => {
                    let val = match init {
                        Some(expr) => self.eval_expr(expr, locals, ctx)?,
                        None => IrValue::Nil,
                    };
                    let coerced = if let Some(t) = self.var_types.get(name) {
                        t.coerce_value(val).map_err(|e| VmError::TypeError { message: e })?
                    } else {
                        val
                    };
                    locals.insert(name.clone(), coerced);
                    idx += 1;
                }
                Stmt::Assign { target, op, expr } => {
                    let rhs = self.eval_expr(expr, locals, ctx)?;
                    match target {
                        AssignTarget::Variable(var_name) => {
                            let old_val = if let Some(v) = locals.get(var_name) {
                                v.clone()
                            } else if let Some(v) = self.member_vars.get(var_name) {
                                v.clone()
                            } else {
                                IrValue::Nil
                            };

                            let new_val = apply_assign_op(&old_val, &rhs, op)?;
                            let final_val = if let Some(t) = self.var_types.get(var_name) {
                                t.coerce_value(new_val).map_err(|e| VmError::TypeError { message: e })?
                            } else {
                                new_val
                            };

                            if locals.contains_key(var_name) {
                                locals.insert(var_name.clone(), final_val);
                            } else {
                                self.member_vars.insert(var_name.clone(), final_val);
                            }
                        }
                        AssignTarget::Property { base, prop } => {
                            let mut base_val = if let Some(v) = locals.get(base) {
                                v.clone()
                            } else if let Some(v) = self.member_vars.get(base) {
                                v.clone()
                            } else {
                                IrValue::Nil
                            };

                            let old_prop = get_component_prop(&base_val, prop)?;
                            let new_prop = apply_assign_op(&old_prop, &rhs, op)?;
                            set_component_prop(&mut base_val, prop, new_prop)?;

                            if locals.contains_key(base) {
                                locals.insert(base.clone(), base_val);
                            } else {
                                self.member_vars.insert(base.clone(), base_val);
                            }
                        }
                        AssignTarget::Subscript { base, index } => {
                            let idx_val = self.eval_expr(index, locals, ctx)?;
                            let mut base_val = if let Some(v) = locals.get(base) {
                                v.clone()
                            } else if let Some(v) = self.member_vars.get(base) {
                                v.clone()
                            } else {
                                IrValue::Nil
                            };

                            match &mut base_val {
                                IrValue::Array(arr) => {
                                    let i = to_i64(&idx_val)?;
                                    if i >= 0 && (i as usize) < arr.len() {
                                        let old = arr[i as usize].clone();
                                        arr[i as usize] = apply_assign_op(&old, &rhs, op)?;
                                    }
                                }
                                IrValue::Map(m) => {
                                    let k = ir_to_string(&idx_val);
                                    let old = m.get(&k).cloned().unwrap_or(IrValue::Nil);
                                    m.insert(k, apply_assign_op(&old, &rhs, op)?);
                                }
                                other => return Err(VmError::TypeError { message: format!("cannot subscript assign {other:?}") }),
                            }

                            if locals.contains_key(base) {
                                locals.insert(base.clone(), base_val);
                            } else {
                                self.member_vars.insert(base.clone(), base_val);
                            }
                        }
                    }
                    idx += 1;
                }
                Stmt::If { condition, body, elifs, else_body } => {
                    let cond_val = self.eval_expr(condition, locals, ctx)?;
                    if is_truthy(&cond_val) {
                        let out = self.execute_block_suspendable(body, 0, locals, ctx)?;
                        if let BlockOutcome::Completed(Some(r)) = out {
                            return Ok(BlockOutcome::Completed(Some(r)));
                        }
                    } else {
                        let mut executed_elif = false;
                        for (elif_cond, elif_body) in elifs {
                            let v = self.eval_expr(elif_cond, locals, ctx)?;
                            if is_truthy(&v) {
                                executed_elif = true;
                                let out = self.execute_block_suspendable(elif_body, 0, locals, ctx)?;
                                if let BlockOutcome::Completed(Some(r)) = out {
                                    return Ok(BlockOutcome::Completed(Some(r)));
                                }
                                break;
                            }
                        }
                        if !executed_elif
                            && let Some(else_b) = else_body
                        {
                            let out = self.execute_block_suspendable(else_b, 0, locals, ctx)?;
                            if let BlockOutcome::Completed(Some(r)) = out {
                                return Ok(BlockOutcome::Completed(Some(r)));
                            }
                        }
                    }
                    idx += 1;
                }
                Stmt::While { condition, body } => {
                    while is_truthy(&self.eval_expr(condition, locals, ctx)?) {
                        ctx.tick()?;
                        let out = self.execute_block_suspendable(body, 0, locals, ctx)?;
                        if let BlockOutcome::Completed(Some(r)) = out {
                            return Ok(BlockOutcome::Completed(Some(r)));
                        }
                    }
                    idx += 1;
                }
                Stmt::For { var_name, range_start, range_end, body } => {
                    let start = to_i64(&self.eval_expr(range_start, locals, ctx)?)?;
                    let end = to_i64(&self.eval_expr(range_end, locals, ctx)?)?;
                    for i in start..end {
                        ctx.tick()?;
                        locals.insert(var_name.clone(), IrValue::I64(i));
                        let out = self.execute_block_suspendable(body, 0, locals, ctx)?;
                        if let BlockOutcome::Completed(Some(r)) = out {
                            return Ok(BlockOutcome::Completed(Some(r)));
                        }
                    }
                    idx += 1;
                }
                Stmt::ForIn { var_name, collection, body } => {
                    let col_val = self.eval_expr(collection, locals, ctx)?;
                    let items = match col_val {
                        IrValue::Array(arr) => arr,
                        _ => Vec::new(),
                    };
                    for item in items {
                        ctx.tick()?;
                        locals.insert(var_name.clone(), item);
                        let out = self.execute_block_suspendable(body, 0, locals, ctx)?;
                        if let BlockOutcome::Completed(Some(r)) = out {
                            return Ok(BlockOutcome::Completed(Some(r)));
                        }
                    }
                    idx += 1;
                }
                Stmt::Match { expr, arms } => {
                    let val = self.eval_expr(expr, locals, ctx)?;
                    for arm in arms {
                        let mut arm_matched = false;
                        for pat in &arm.patterns {
                            if let Some(bindings) = match_pattern(&val, pat) {
                                let mut test_locals = locals.clone();
                                test_locals.extend(bindings.clone());
                                let guard_passed = match &arm.guard {
                                    Some(g) => is_truthy(&self.eval_expr(g, &test_locals, ctx)?),
                                    None => true,
                                };
                                if guard_passed {
                                    arm_matched = true;
                                    locals.extend(bindings);
                                    let out = self.execute_block_suspendable(&arm.body, 0, locals, ctx)?;
                                    if let BlockOutcome::Completed(Some(r)) = out {
                                        return Ok(BlockOutcome::Completed(Some(r)));
                                    }
                                    break;
                                }
                            }
                        }
                        if arm_matched {
                            break;
                        }
                    }
                    idx += 1;
                }
                Stmt::Call(expr) => {
                    self.eval_expr(expr, locals, ctx)?;
                    idx += 1;
                }
                Stmt::Return(maybe_expr) => {
                    let val = match maybe_expr {
                        Some(expr) => self.eval_expr(expr, locals, ctx)?,
                        None => IrValue::Nil,
                    };
                    return Ok(BlockOutcome::Completed(Some(val)));
                }
                Stmt::Await(expr) => {
                    let target_val = self.eval_expr(expr, locals, ctx)?;
                    let target = determine_await_target(&target_val);
                    return Ok(BlockOutcome::Suspended {
                        cursor: idx + 1,
                        target,
                    });
                }
            }
        }
        Ok(BlockOutcome::Completed(None))
    }

    fn eval_expr(
        &mut self,
        expr: &Expr,
        locals: &BTreeMap<String, IrValue>,
        ctx: &mut ExecutionContext,
    ) -> Result<IrValue, VmError> {
        match expr {
            Expr::Literal(val) => Ok(val.clone()),
            Expr::Var(name) => {
                if name == "self" {
                    let mut map = BTreeMap::new();
                    map.insert("__type__".to_string(), IrValue::String("Node".into()));
                    return Ok(IrValue::Map(map));
                }
                if let Some(v) = locals.get(name) {
                    return Ok(v.clone());
                }
                if let Some(v) = self.member_vars.get(name) {
                    return Ok(v.clone());
                }
                if self.module.signals.iter().any(|s| &s.name == name) {
                    let mut sig_map = BTreeMap::new();
                    sig_map.insert("__type__".into(), IrValue::String("Signal".into()));
                    sig_map.insert("signal_name".into(), IrValue::String(name.clone()));
                    return Ok(IrValue::Map(sig_map));
                }
                Ok(IrValue::Nil)
            }
            Expr::PropAccess { base, prop } => {
                let base_val = self.eval_expr(base, locals, ctx)?;
                if let IrValue::Map(ref m) = base_val {
                    if m.get("__type__") == Some(&IrValue::String("SceneTreeTimer".into()))
                        && prop == "timeout"
                    {
                        let mut sig_map = BTreeMap::new();
                        sig_map.insert("__type__".into(), IrValue::String("Signal".into()));
                        sig_map.insert("signal_name".into(), IrValue::String("timeout".into()));
                        if let Some(rem) = m.get("remaining_sec") {
                            sig_map.insert("remaining_sec".into(), rem.clone());
                        }
                        return Ok(IrValue::Map(sig_map));
                    }
                    if m.get("__type__") == Some(&IrValue::String("Node".into()))
                        && !self.member_vars.contains_key(prop)
                    {
                        let mut sig_map = BTreeMap::new();
                        sig_map.insert("__type__".into(), IrValue::String("Signal".into()));
                        sig_map.insert("signal_name".into(), IrValue::String(prop.clone()));
                        return Ok(IrValue::Map(sig_map));
                    }
                }
                get_component_prop(&base_val, prop)
            }
            Expr::Subscript { base, index } => {
                let b = self.eval_expr(base, locals, ctx)?;
                let idx = self.eval_expr(index, locals, ctx)?;
                match b {
                    IrValue::Array(arr) => {
                        let i = to_i64(&idx)?;
                        if i >= 0 && (i as usize) < arr.len() {
                            Ok(arr[i as usize].clone())
                        } else {
                            Ok(IrValue::Nil)
                        }
                    }
                    IrValue::Map(m) => {
                        let k = ir_to_string(&idx);
                        Ok(m.get(&k).cloned().unwrap_or(IrValue::Nil))
                    }
                    other => Err(VmError::TypeError { message: format!("cannot index into {other:?}") }),
                }
            }
            Expr::Unary { op, expr } => {
                let v = self.eval_expr(expr, locals, ctx)?;
                match op {
                    UnaryOp::Neg => match v {
                        IrValue::I64(i) => Ok(IrValue::I64(-i)),
                        IrValue::F64(f) => Ok(IrValue::F64(-f)),
                        other => Err(VmError::TypeError { message: format!("cannot negate {other:?}") }),
                    },
                    UnaryOp::Not => Ok(IrValue::Bool(!is_truthy(&v))),
                }
            }
            Expr::Binary { op, left, right } => {
                let lhs = self.eval_expr(left, locals, ctx)?;
                let rhs = self.eval_expr(right, locals, ctx)?;
                eval_binary_op(&lhs, &rhs, op)
            }
            Expr::Call { func_name, args } => {
                let mut evaluated_args = Vec::with_capacity(args.len());
                for a in args {
                    evaluated_args.push(self.eval_expr(a, locals, ctx)?);
                }
                self.eval_call(func_name, &evaluated_args, locals, ctx)
            }
            Expr::MethodCall { target, method, args } => {
                let mut evaluated_args = Vec::with_capacity(args.len());
                for a in args {
                    evaluated_args.push(self.eval_expr(a, locals, ctx)?);
                }
                self.eval_method_call(target, method, &evaluated_args, locals, ctx)
            }
            Expr::Lambda { parameters, body } => {
                let lambda_id = LAMBDA_COUNTER.fetch_add(1, Ordering::SeqCst);
                let func_name = format!("__lambda_{lambda_id}");
                let mut param_names = Vec::new();
                let mut default_args = BTreeMap::new();
                for (n, def) in parameters {
                    param_names.push(n.clone());
                    if let Some(d) = def {
                        default_args.insert(n.clone(), d.clone());
                    }
                }
                self.functions.insert(
                    func_name.clone(),
                    FunctionAst {
                        name: func_name.clone(),
                        parameters: param_names,
                        default_args,
                        body: body.clone(),
                    },
                );
                let callable = GdCallable::new(Some("self".into()), func_name);
                Ok(callable.to_ir_value())
            }
            Expr::SuperCall { method, args } => {
                let mut evaluated_args = Vec::with_capacity(args.len());
                for a in args {
                    evaluated_args.push(self.eval_expr(a, locals, ctx)?);
                }
                let target_method = method.clone().unwrap_or_else(|| ctx.current_func.clone());
                self.eval_super_call(&target_method, &evaluated_args)
            }
            Expr::Await(inner) => {
                let val = self.eval_expr(inner, locals, ctx)?;
                Ok(val)
            }
            Expr::ArrayLit(elements) => {
                let mut items = Vec::with_capacity(elements.len());
                for e in elements {
                    items.push(self.eval_expr(e, locals, ctx)?);
                }
                Ok(IrValue::Array(items))
            }
            Expr::DictLit(pairs) => {
                let mut map = BTreeMap::new();
                for (k_expr, v_expr) in pairs {
                    let k = self.eval_expr(k_expr, locals, ctx)?;
                    let v = self.eval_expr(v_expr, locals, ctx)?;
                    map.insert(ir_to_string(&k), v);
                }
                Ok(IrValue::Map(map))
            }
        }
    }

    fn eval_call(
        &mut self,
        func_name: &str,
        args: &[IrValue],
        locals: &BTreeMap<String, IrValue>,
        ctx: &mut ExecutionContext,
    ) -> Result<IrValue, VmError> {
        if func_name == "print" {
            let msg = args.iter().map(ir_to_string).collect::<Vec<_>>().join(" ");
            ctx.logs.push(msg);
            return Ok(IrValue::Nil);
        }
        if func_name == "emit_signal" {
            let sig_name = args.first().map(ir_to_string).unwrap_or_default();
            let sig_args = if args.len() > 1 { args[1..].to_vec() } else { Vec::new() };
            ctx.emitted_signals.push((sig_name, sig_args));
            return Ok(IrValue::Nil);
        }
        if func_name == "Vector2" {
            let x = to_f32(args.first().unwrap_or(&IrValue::F64(0.0)));
            let y = to_f32(args.get(1).unwrap_or(&IrValue::F64(0.0)));
            return Ok(IrValue::Vec2([x, y]));
        }
        if func_name == "Vector3" {
            let x = to_f32(args.first().unwrap_or(&IrValue::F64(0.0)));
            let y = to_f32(args.get(1).unwrap_or(&IrValue::F64(0.0)));
            let z = to_f32(args.get(2).unwrap_or(&IrValue::F64(0.0)));
            return Ok(IrValue::Vec3([x, y, z]));
        }
        if func_name == "Color" {
            let r = to_f32(args.first().unwrap_or(&IrValue::F64(0.0)));
            let g = to_f32(args.get(1).unwrap_or(&IrValue::F64(0.0)));
            let b = to_f32(args.get(2).unwrap_or(&IrValue::F64(0.0)));
            let a = to_f32(args.get(3).unwrap_or(&IrValue::F64(1.0)));
            return Ok(IrValue::Color([r, g, b, a]));
        }
        if func_name == "Callable" {
            let target = args.first().map(ir_to_string);
            let method = args.get(1).map(ir_to_string).unwrap_or_default();
            let c = GdCallable::new(target, method);
            return Ok(c.to_ir_value());
        }

        // Global builtins catalog
        if let Some(res) = eval_global_builtin(func_name, args, &mut ctx.logs) {
            return res.map_err(|e| VmError::TypeError { message: e });
        }

        // Node / self builtins
        if let Some(node) = &mut self.bound_node
            && let Some(res) = eval_node_builtin(node, func_name, args, &mut ctx.emitted_signals)
        {
            return res.map_err(|e| VmError::TypeError { message: e });
        }

        // User-defined function
        if let Some(func) = self.functions.get(func_name).cloned() {
            let mut sub_locals = BTreeMap::new();
            for (idx, p) in func.parameters.iter().enumerate() {
                let v = args
                    .get(idx)
                    .cloned()
                    .or_else(|| func.default_args.get(p).cloned())
                    .unwrap_or(IrValue::Nil);
                sub_locals.insert(p.clone(), v);
            }
            let prev_func = std::mem::replace(&mut ctx.current_func, func_name.to_string());
            let out = self.execute_block_suspendable(&func.body, 0, &mut sub_locals, ctx)?;
            ctx.current_func = prev_func;
            return match out {
                BlockOutcome::Completed(v) => Ok(v.unwrap_or(IrValue::Nil)),
                BlockOutcome::Suspended { .. } => Ok(IrValue::Nil),
            };
        }

        // Fallback variable Callable invocation
        if let Some(val) = locals.get(func_name).or_else(|| self.member_vars.get(func_name))
            && let Some(callable) = GdCallable::from_ir_value(val)
        {
            return self.invoke_callable(&callable, args, ctx);
        }

        Err(VmError::MethodNotFound { name: func_name.to_string() })
    }

    fn eval_method_call(
        &mut self,
        target_expr: &Expr,
        method: &str,
        args: &[IrValue],
        locals: &BTreeMap<String, IrValue>,
        ctx: &mut ExecutionContext,
    ) -> Result<IrValue, VmError> {
        let mut target_val = self.eval_expr(target_expr, locals, ctx)?;

        // Higher-order Array methods: map, filter, reduce, any, all
        if let IrValue::Array(ref arr) = target_val {
            match method {
                "map" => {
                    let callable_arg = args.first().ok_or_else(|| VmError::TypeError { message: "map requires Callable argument".into() })?;
                    let callable = GdCallable::from_ir_value(callable_arg).ok_or_else(|| VmError::TypeError { message: "map expected Callable".into() })?;
                    let mut result_items = Vec::with_capacity(arr.len());
                    for it in arr {
                        let res = self.invoke_callable(&callable, std::slice::from_ref(it), ctx)?;
                        result_items.push(res);
                    }
                    return Ok(IrValue::Array(result_items));
                }
                "filter" => {
                    let callable_arg = args.first().ok_or_else(|| VmError::TypeError { message: "filter requires Callable argument".into() })?;
                    let callable = GdCallable::from_ir_value(callable_arg).ok_or_else(|| VmError::TypeError { message: "filter expected Callable".into() })?;
                    let mut result_items = Vec::new();
                    for it in arr {
                        let res = self.invoke_callable(&callable, std::slice::from_ref(it), ctx)?;
                        if is_truthy(&res) {
                            result_items.push(it.clone());
                        }
                    }
                    return Ok(IrValue::Array(result_items));
                }
                "reduce" => {
                    let callable_arg = args.first().ok_or_else(|| VmError::TypeError { message: "reduce requires Callable argument".into() })?;
                    let callable = GdCallable::from_ir_value(callable_arg).ok_or_else(|| VmError::TypeError { message: "reduce expected Callable".into() })?;
                    let mut accum = args.get(1).cloned().unwrap_or(IrValue::Nil);
                    for it in arr {
                        accum = self.invoke_callable(&callable, &[accum, it.clone()], ctx)?;
                    }
                    return Ok(accum);
                }
                "any" => {
                    let callable_arg = args.first().ok_or_else(|| VmError::TypeError { message: "any requires Callable argument".into() })?;
                    let callable = GdCallable::from_ir_value(callable_arg).ok_or_else(|| VmError::TypeError { message: "any expected Callable".into() })?;
                    for it in arr {
                        let res = self.invoke_callable(&callable, std::slice::from_ref(it), ctx)?;
                        if is_truthy(&res) {
                            return Ok(IrValue::Bool(true));
                        }
                    }
                    return Ok(IrValue::Bool(false));
                }
                "all" => {
                    let callable_arg = args.first().ok_or_else(|| VmError::TypeError { message: "all requires Callable argument".into() })?;
                    let callable = GdCallable::from_ir_value(callable_arg).ok_or_else(|| VmError::TypeError { message: "all expected Callable".into() })?;
                    for it in arr {
                        let res = self.invoke_callable(&callable, std::slice::from_ref(it), ctx)?;
                        if !is_truthy(&res) {
                            return Ok(IrValue::Bool(false));
                        }
                    }
                    return Ok(IrValue::Bool(true));
                }
                _ => {}
            }
        }

        // Callable invocations
        if let Some(callable) = GdCallable::from_ir_value(&target_val) {
            match method {
                "call" => {
                    return self.invoke_callable(&callable, args, ctx);
                }
                "callv" => {
                    let callv_args = match args.first() {
                        Some(IrValue::Array(a)) => a.as_slice(),
                        _ => &[],
                    };
                    return self.invoke_callable(&callable, callv_args, ctx);
                }
                "bind" => {
                    let bound = callable.bind(args);
                    return Ok(bound.to_ir_value());
                }
                "is_valid" => {
                    return Ok(IrValue::Bool(callable.is_valid()));
                }
                _ => {}
            }
        }

        // Type methods from builtins catalog
        if let Some(res) = eval_method(&mut target_val, method, args) {
            let ret = res.map_err(|e| VmError::TypeError { message: e })?;
            // If target was a variable, sync mutated target back to variable
            if let Expr::Var(var_name) = target_expr {
                self.member_vars.insert(var_name.clone(), target_val);
            }
            return Ok(ret);
        }

        // Node methods
        if let Some(node) = &mut self.bound_node
            && let Some(res) = eval_node_builtin(node, method, args, &mut ctx.emitted_signals)
        {
            return res.map_err(|e| VmError::TypeError { message: e });
        }

        Err(VmError::MethodNotFound { name: method.to_string() })
    }

    fn invoke_callable(
        &mut self,
        callable: &GdCallable,
        args: &[IrValue],
        ctx: &mut ExecutionContext,
    ) -> Result<IrValue, VmError> {
        let combined_args = callable.combine_arguments(args);
        let dummy_locals = BTreeMap::new();
        self.eval_call(&callable.method, &combined_args, &dummy_locals, ctx)
    }

    fn eval_super_call(&mut self, method_name: &str, args: &[IrValue]) -> Result<IrValue, VmError> {
        let extends_name = self.module.extends.clone();
        if let Some(base_vm) = self.base_classes.get_mut(&extends_name) {
            let mut dummy_node = self.bound_node.clone().unwrap_or_else(|| IrNode {
                id: foreplay_ir::NodeId::new(1, 1).unwrap(),
                name: "Base".into(),
                class_name: extends_name.clone(),
                parent: None,
                instance_scene: None,
                properties: self.member_vars.clone(),
                groups: Vec::new(),
            });

            let res = base_vm.call_method(method_name, args, &mut dummy_node)?;
            for (k, v) in base_vm.member_vars.clone() {
                self.member_vars.insert(k, v);
            }
            Ok(res.return_value.unwrap_or(IrValue::Nil))
        } else {
            Ok(IrValue::Nil)
        }
    }
}

enum BlockOutcome {
    Completed(Option<IrValue>),
    Suspended {
        cursor: usize,
        target: AwaitTarget,
    },
}

struct ExecutionContext {
    steps: usize,
    max_budget: usize,
    emitted_signals: Vec<(String, Vec<IrValue>)>,
    logs: Vec<String>,
    current_func: String,
}

impl ExecutionContext {
    fn tick(&mut self) -> Result<(), VmError> {
        self.steps += 1;
        if self.steps > self.max_budget {
            return Err(VmError::BudgetExceeded { budget: self.max_budget });
        }
        Ok(())
    }
}

fn determine_await_target(val: &IrValue) -> AwaitTarget {
    match val {
        IrValue::Map(m) => {
            if m.get("__type__") == Some(&IrValue::String("SceneTreeTimer".into())) {
                let rem = match m.get("remaining_sec") {
                    Some(IrValue::F64(f)) => *f,
                    Some(IrValue::I64(i)) => *i as f64,
                    _ => 0.0,
                };
                AwaitTarget::Timer { remaining_sec: rem }
            } else if m.get("__type__") == Some(&IrValue::String("Signal".into())) {
                let sig_name = match m.get("signal_name") {
                    Some(IrValue::String(s)) => s.clone(),
                    _ => String::new(),
                };
                if let Some(rem_val) = m.get("remaining_sec") {
                    let rem = match rem_val {
                        IrValue::F64(f) => *f,
                        IrValue::I64(i) => *i as f64,
                        _ => 0.0,
                    };
                    AwaitTarget::Timer { remaining_sec: rem }
                } else {
                    AwaitTarget::Signal { emitter: None, signal_name: sig_name }
                }
            } else {
                AwaitTarget::Timer { remaining_sec: 0.0 }
            }
        }
        IrValue::String(s) => AwaitTarget::Signal {
            emitter: None,
            signal_name: s.clone(),
        },
        IrValue::F64(sec) => AwaitTarget::Timer { remaining_sec: *sec },
        IrValue::I64(sec) => AwaitTarget::Timer { remaining_sec: *sec as f64 },
        _ => AwaitTarget::Timer { remaining_sec: 0.0 },
    }
}

fn match_pattern(val: &IrValue, pat: &GdPattern) -> Option<BTreeMap<String, IrValue>> {
    match pat {
        GdPattern::Wildcard => Some(BTreeMap::new()),
        GdPattern::Bind(name) => {
            let mut map = BTreeMap::new();
            map.insert(name.clone(), val.clone());
            Some(map)
        }
        GdPattern::Literal(lit) => {
            if val == lit {
                Some(BTreeMap::new())
            } else {
                None
            }
        }
        GdPattern::Array { patterns, has_rest } => {
            if let IrValue::Array(items) = val {
                if *has_rest {
                    if items.len() < patterns.len() {
                        return None;
                    }
                    let mut map = BTreeMap::new();
                    for (i, p) in patterns.iter().enumerate() {
                        let sub = match_pattern(&items[i], p)?;
                        map.extend(sub);
                    }
                    Some(map)
                } else {
                    if items.len() != patterns.len() {
                        return None;
                    }
                    let mut map = BTreeMap::new();
                    for (i, p) in patterns.iter().enumerate() {
                        let sub = match_pattern(&items[i], p)?;
                        map.extend(sub);
                    }
                    Some(map)
                }
            } else {
                None
            }
        }
        GdPattern::Dict { pairs } => {
            if let IrValue::Map(m) = val {
                let mut map = BTreeMap::new();
                for (k, p) in pairs {
                    let item = m.get(k)?;
                    let sub = match_pattern(item, p)?;
                    map.extend(sub);
                }
                Some(map)
            } else {
                None
            }
        }
    }
}

fn apply_assign_op(old: &IrValue, rhs: &IrValue, op: &AssignOp) -> Result<IrValue, VmError> {
    match op {
        AssignOp::Assign => Ok(rhs.clone()),
        AssignOp::AddAssign => eval_binary_op(old, rhs, &BinOp::Add),
        AssignOp::SubAssign => eval_binary_op(old, rhs, &BinOp::Sub),
        AssignOp::MulAssign => eval_binary_op(old, rhs, &BinOp::Mul),
        AssignOp::DivAssign => eval_binary_op(old, rhs, &BinOp::Div),
    }
}

fn eval_binary_op(lhs: &IrValue, rhs: &IrValue, op: &BinOp) -> Result<IrValue, VmError> {
    match op {
        BinOp::Add => match (lhs, rhs) {
            (IrValue::I64(a), IrValue::I64(b)) => Ok(IrValue::I64(a + b)),
            (IrValue::F64(a), IrValue::F64(b)) => Ok(IrValue::F64(a + b)),
            (IrValue::I64(a), IrValue::F64(b)) => Ok(IrValue::F64(*a as f64 + b)),
            (IrValue::F64(a), IrValue::I64(b)) => Ok(IrValue::F64(a + *b as f64)),
            (IrValue::String(a), IrValue::String(b)) => Ok(IrValue::String(format!("{a}{b}"))),
            (IrValue::String(a), other) => Ok(IrValue::String(format!("{a}{}", ir_to_string(other)))),
            (other, IrValue::String(b)) => Ok(IrValue::String(format!("{}{b}", ir_to_string(other)))),
            (IrValue::Vec3([x1, y1, z1]), IrValue::Vec3([x2, y2, z2])) => {
                Ok(IrValue::Vec3([x1 + x2, y1 + y2, z1 + z2]))
            }
            (IrValue::Vec2([x1, y1]), IrValue::Vec2([x2, y2])) => {
                Ok(IrValue::Vec2([x1 + x2, y1 + y2]))
            }
            (IrValue::Array(a1), IrValue::Array(a2)) => {
                let mut combined = a1.clone();
                combined.extend(a2.clone());
                Ok(IrValue::Array(combined))
            }
            _ => Err(VmError::TypeError { message: format!("cannot add {lhs:?} and {rhs:?}") }),
        },
        BinOp::Sub => match (lhs, rhs) {
            (IrValue::I64(a), IrValue::I64(b)) => Ok(IrValue::I64(a - b)),
            (IrValue::F64(a), IrValue::F64(b)) => Ok(IrValue::F64(a - b)),
            (IrValue::I64(a), IrValue::F64(b)) => Ok(IrValue::F64(*a as f64 - b)),
            (IrValue::F64(a), IrValue::I64(b)) => Ok(IrValue::F64(a - *b as f64)),
            (IrValue::Vec3([x1, y1, z1]), IrValue::Vec3([x2, y2, z2])) => {
                Ok(IrValue::Vec3([x1 - x2, y1 - y2, z1 - z2]))
            }
            (IrValue::Vec2([x1, y1]), IrValue::Vec2([x2, y2])) => {
                Ok(IrValue::Vec2([x1 - x2, y1 - y2]))
            }
            _ => Err(VmError::TypeError { message: format!("cannot subtract {rhs:?} from {lhs:?}") }),
        },
        BinOp::Mul => match (lhs, rhs) {
            (IrValue::I64(a), IrValue::I64(b)) => Ok(IrValue::I64(a * b)),
            (IrValue::F64(a), IrValue::F64(b)) => Ok(IrValue::F64(a * b)),
            (IrValue::I64(a), IrValue::F64(b)) => Ok(IrValue::F64(*a as f64 * b)),
            (IrValue::F64(a), IrValue::I64(b)) => Ok(IrValue::F64(a * *b as f64)),
            (IrValue::Vec3([x, y, z]), IrValue::F64(s)) => {
                let sf = *s as f32;
                Ok(IrValue::Vec3([x * sf, y * sf, z * sf]))
            }
            (IrValue::Vec3([x, y, z]), IrValue::I64(s)) => {
                let sf = *s as f32;
                Ok(IrValue::Vec3([x * sf, y * sf, z * sf]))
            }
            (IrValue::Vec2([x, y]), IrValue::F64(s)) => {
                let sf = *s as f32;
                Ok(IrValue::Vec2([x * sf, y * sf]))
            }
            (IrValue::Vec2([x, y]), IrValue::I64(s)) => {
                let sf = *s as f32;
                Ok(IrValue::Vec2([x * sf, y * sf]))
            }
            _ => Err(VmError::TypeError { message: format!("cannot multiply {lhs:?} and {rhs:?}") }),
        },
        BinOp::Div => match (lhs, rhs) {
            (IrValue::I64(a), IrValue::I64(b)) => {
                if *b == 0 { return Err(VmError::DivisionByZero); }
                Ok(IrValue::I64(a / b))
            }
            (IrValue::F64(a), IrValue::F64(b)) => {
                if *b == 0.0 { return Err(VmError::DivisionByZero); }
                Ok(IrValue::F64(a / b))
            }
            (IrValue::F64(a), IrValue::I64(b)) => {
                if *b == 0 { return Err(VmError::DivisionByZero); }
                Ok(IrValue::F64(a / *b as f64))
            }
            (IrValue::I64(a), IrValue::F64(b)) => {
                if *b == 0.0 { return Err(VmError::DivisionByZero); }
                Ok(IrValue::F64(*a as f64 / b))
            }
            _ => Err(VmError::TypeError { message: format!("cannot divide {lhs:?} by {rhs:?}") }),
        },
        BinOp::Mod => match (lhs, rhs) {
            (IrValue::I64(a), IrValue::I64(b)) => {
                if *b == 0 { return Err(VmError::DivisionByZero); }
                Ok(IrValue::I64(a % b))
            }
            _ => Err(VmError::TypeError { message: format!("cannot modulo {lhs:?} by {rhs:?}") }),
        },
        BinOp::Eq => Ok(IrValue::Bool(lhs == rhs)),
        BinOp::Ne => Ok(IrValue::Bool(lhs != rhs)),
        BinOp::Lt => Ok(IrValue::Bool(to_f64(lhs)? < to_f64(rhs)?)),
        BinOp::Le => Ok(IrValue::Bool(to_f64(lhs)? <= to_f64(rhs)?)),
        BinOp::Gt => Ok(IrValue::Bool(to_f64(lhs)? > to_f64(rhs)?)),
        BinOp::Ge => Ok(IrValue::Bool(to_f64(lhs)? >= to_f64(rhs)?)),
        BinOp::And => Ok(IrValue::Bool(is_truthy(lhs) && is_truthy(rhs))),
        BinOp::Or => Ok(IrValue::Bool(is_truthy(lhs) || is_truthy(rhs))),
    }
}

fn is_truthy(val: &IrValue) -> bool {
    match val {
        IrValue::Nil => false,
        IrValue::Bool(b) => *b,
        IrValue::I64(i) => *i != 0,
        IrValue::F64(f) => *f != 0.0,
        IrValue::String(s) => !s.is_empty(),
        IrValue::Array(a) => !a.is_empty(),
        IrValue::Map(m) => !m.is_empty(),
        _ => true,
    }
}

fn to_f64(val: &IrValue) -> Result<f64, VmError> {
    match val {
        IrValue::F64(f) => Ok(*f),
        IrValue::I64(i) => Ok(*i as f64),
        other => Err(VmError::TypeError { message: format!("expected number, got {other:?}") }),
    }
}

fn to_i64(val: &IrValue) -> Result<i64, VmError> {
    match val {
        IrValue::I64(i) => Ok(*i),
        IrValue::F64(f) => Ok(*f as i64),
        other => Err(VmError::TypeError { message: format!("expected integer, got {other:?}") }),
    }
}

fn to_f32(val: &IrValue) -> f32 {
    match val {
        IrValue::F64(f) => *f as f32,
        IrValue::I64(i) => *i as f32,
        _ => 0.0,
    }
}

fn get_component_prop(val: &IrValue, prop: &str) -> Result<IrValue, VmError> {
    match (val, prop) {
        (IrValue::Vec3([x, _, _]), "x") => Ok(IrValue::F64(*x as f64)),
        (IrValue::Vec3([_, y, _]), "y") => Ok(IrValue::F64(*y as f64)),
        (IrValue::Vec3([_, _, z]), "z") => Ok(IrValue::F64(*z as f64)),
        (IrValue::Vec2([x, _]), "x") => Ok(IrValue::F64(*x as f64)),
        (IrValue::Vec2([_, y]), "y") => Ok(IrValue::F64(*y as f64)),
        (IrValue::Color([r, _, _, _]), "r") => Ok(IrValue::F64(*r as f64)),
        (IrValue::Color([_, g, _, _]), "g") => Ok(IrValue::F64(*g as f64)),
        (IrValue::Color([_, _, b, _]), "b") => Ok(IrValue::F64(*b as f64)),
        (IrValue::Color([_, _, _, a]), "a") => Ok(IrValue::F64(*a as f64)),
        (IrValue::Map(m), key) => Ok(m.get(key).cloned().unwrap_or(IrValue::Nil)),
        (other, _) => Err(VmError::TypeError { message: format!("cannot access property '{prop}' on {other:?}") }),
    }
}

fn set_component_prop(val: &mut IrValue, prop: &str, new_val: IrValue) -> Result<(), VmError> {
    let f = to_f32(&new_val);
    match (val, prop) {
        (IrValue::Vec3([x, _, _]), "x") => { *x = f; Ok(()) }
        (IrValue::Vec3([_, y, _]), "y") => { *y = f; Ok(()) }
        (IrValue::Vec3([_, _, z]), "z") => { *z = f; Ok(()) }
        (IrValue::Vec2([x, _]), "x") => { *x = f; Ok(()) }
        (IrValue::Vec2([_, y]), "y") => { *y = f; Ok(()) }
        (IrValue::Color([r, _, _, _]), "r") => { *r = f; Ok(()) }
        (IrValue::Color([_, g, _, _]), "g") => { *g = f; Ok(()) }
        (IrValue::Color([_, _, b, _]), "b") => { *b = f; Ok(()) }
        (IrValue::Color([_, _, _, a]), "a") => { *a = f; Ok(()) }
        (IrValue::Map(m), key) => { m.insert(key.to_string(), new_val); Ok(()) }
        (other, _) => Err(VmError::TypeError { message: format!("cannot set property '{prop}' on {other:?}") }),
    }
}

/// Parses function bodies from GDScript source based on indentation.
fn parse_source_functions(source: &str) -> Result<BTreeMap<String, FunctionAst>, VmError> {
    let mut functions = BTreeMap::new();
    let lines: Vec<&str> = source.lines().collect();
    let mut idx = 0;

    while idx < lines.len() {
        let line = lines[idx];
        let trimmed = line.trim();

        if let Some(rest) = trimmed.strip_prefix("func ") {
            let func_decl = rest.trim_end_matches(':').trim();
            let (func_name, params, default_args) = parse_func_signature(func_decl);

            idx += 1;
            let mut body_lines = Vec::new();

            while idx < lines.len() {
                let body_line = lines[idx];
                let body_trimmed = body_line.trim();

                if body_trimmed.is_empty() || body_trimmed.starts_with('#') {
                    idx += 1;
                    continue;
                }

                let indent = body_line.len() - body_line.trim_start().len();
                if indent == 0 && !body_trimmed.is_empty() {
                    break;
                }

                body_lines.push(body_line);
                idx += 1;
            }

            let body_stmts = parse_statements(&body_lines)?;
            functions.insert(
                func_name.clone(),
                FunctionAst {
                    name: func_name,
                    parameters: params,
                    default_args,
                    body: body_stmts,
                },
            );
        } else {
            idx += 1;
        }
    }

    Ok(functions)
}

fn parse_func_signature(decl: &str) -> (String, Vec<String>, BTreeMap<String, IrValue>) {
    let clean = if let Some(idx) = decl.find("->") {
        decl[..idx].trim()
    } else {
        decl.trim()
    };

    if let Some(open) = clean.find('(') {
        let name = clean[..open].trim().to_string();
        let close = clean.rfind(')').unwrap_or(clean.len());
        let params_str = clean[open + 1..close].trim();
        let mut params = Vec::new();
        let mut default_args = BTreeMap::new();
        for (p_name, def) in parse_lambda_params(params_str) {
            params.push(p_name.clone());
            if let Some(val) = def {
                default_args.insert(p_name, val);
            }
        }
        (name, params, default_args)
    } else {
        (clean.to_string(), Vec::new(), BTreeMap::new())
    }
}

fn parse_statements(lines: &[&str]) -> Result<Vec<Stmt>, VmError> {
    let mut stmts = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let raw = lines[i];
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            i += 1;
            continue;
        }

        let cur_indent = raw.len() - raw.trim_start().len();

        if line == "pass" {
            stmts.push(Stmt::Pass);
            i += 1;
            continue;
        }

        if let Some(rest) = line.strip_prefix("while ")
            && rest.ends_with(':')
        {
            let cond_str = rest.trim_end_matches(':').trim();
            let cond = parse_expression(cond_str)?;

            let mut body_lines = Vec::new();
            i += 1;
            while i < lines.len() {
                let next_raw = lines[i];
                let next_trimmed = next_raw.trim();
                if next_trimmed.is_empty() || next_trimmed.starts_with('#') {
                    i += 1;
                    continue;
                }
                let next_indent = next_raw.len() - next_raw.trim_start().len();
                if next_indent <= cur_indent {
                    break;
                }
                body_lines.push(next_raw);
                i += 1;
            }
            let body = parse_statements(&body_lines)?;
            stmts.push(Stmt::While { condition: cond, body });
            continue;
        }

        if let Some(rest) = line.strip_prefix("if ")
            && rest.ends_with(':')
        {
            let cond_str = rest.trim_end_matches(':').trim();
            let cond = parse_expression(cond_str)?;

            let mut body_lines = Vec::new();
            i += 1;
            while i < lines.len() {
                let next_raw = lines[i];
                let next_trimmed = next_raw.trim();
                if next_trimmed.is_empty() || next_trimmed.starts_with('#') {
                    i += 1;
                    continue;
                }
                let next_indent = next_raw.len() - next_raw.trim_start().len();
                if next_indent <= cur_indent {
                    break;
                }
                body_lines.push(next_raw);
                i += 1;
            }
            let body = parse_statements(&body_lines)?;

            let mut elifs = Vec::new();
            let mut else_body = None;

            while i < lines.len() {
                let check_raw = lines[i];
                let check_trimmed = check_raw.trim();
                if check_trimmed.is_empty() || check_trimmed.starts_with('#') {
                    i += 1;
                    continue;
                }
                let check_indent = check_raw.len() - check_raw.trim_start().len();
                if check_indent != cur_indent {
                    break;
                }

                if let Some(elif_rest) = check_trimmed.strip_prefix("elif ")
                    && elif_rest.ends_with(':')
                {
                    let elif_cond_str = elif_rest.trim_end_matches(':').trim();
                    let elif_cond = parse_expression(elif_cond_str)?;
                    let mut elif_lines = Vec::new();
                    i += 1;
                    while i < lines.len() {
                        let inner_raw = lines[i];
                        let inner_trimmed = inner_raw.trim();
                        if inner_trimmed.is_empty() || inner_trimmed.starts_with('#') {
                            i += 1;
                            continue;
                        }
                        if (inner_raw.len() - inner_raw.trim_start().len()) <= cur_indent {
                            break;
                        }
                        elif_lines.push(inner_raw);
                        i += 1;
                    }
                    elifs.push((elif_cond, parse_statements(&elif_lines)?));
                } else if check_trimmed == "else:" {
                    let mut else_lines = Vec::new();
                    i += 1;
                    while i < lines.len() {
                        let inner_raw = lines[i];
                        let inner_trimmed = inner_raw.trim();
                        if inner_trimmed.is_empty() || inner_trimmed.starts_with('#') {
                            i += 1;
                            continue;
                        }
                        if (inner_raw.len() - inner_raw.trim_start().len()) <= cur_indent {
                            break;
                        }
                        else_lines.push(inner_raw);
                        i += 1;
                    }
                    else_body = Some(parse_statements(&else_lines)?);
                    break;
                } else {
                    break;
                }
            }

            stmts.push(Stmt::If {
                condition: cond,
                body,
                elifs,
                else_body,
            });
            continue;
        }

        if let Some(rest) = line.strip_prefix("for ")
            && rest.ends_with(':')
        {
            let header = rest.trim_end_matches(':').trim();
            let parts: Vec<&str> = header.split(" in ").collect();
            if parts.len() == 2 {
                let var_name = parts[0].trim().to_string();
                let col_part = parts[1].trim();

                let mut body_lines = Vec::new();
                i += 1;
                while i < lines.len() {
                    let next_raw = lines[i];
                    let next_trimmed = next_raw.trim();
                    if next_trimmed.is_empty() || next_trimmed.starts_with('#') {
                        i += 1;
                        continue;
                    }
                    if (next_raw.len() - next_raw.trim_start().len()) <= cur_indent {
                        break;
                    }
                    body_lines.push(next_raw);
                    i += 1;
                }
                let body = parse_statements(&body_lines)?;

                if let Some(inner) = col_part.strip_prefix("range(")
                    && inner.ends_with(')')
                {
                    let r_args = split_comma_args(&inner[..inner.len() - 1]);
                    let (start_expr, end_expr) = if r_args.len() == 1 {
                        (Expr::Literal(IrValue::I64(0)), parse_expression(r_args[0].trim())?)
                    } else if r_args.len() >= 2 {
                        (parse_expression(r_args[0].trim())?, parse_expression(r_args[1].trim())?)
                    } else {
                        (Expr::Literal(IrValue::I64(0)), Expr::Literal(IrValue::I64(0)))
                    };
                    stmts.push(Stmt::For {
                        var_name,
                        range_start: start_expr,
                        range_end: end_expr,
                        body,
                    });
                } else {
                    let col_expr = parse_expression(col_part)?;
                    stmts.push(Stmt::ForIn {
                        var_name,
                        collection: col_expr,
                        body,
                    });
                }
                continue;
            }
        }

        // Godot 4 `match` statement
        if let Some(rest) = line.strip_prefix("match ")
            && rest.ends_with(':')
        {
            let expr_str = rest.trim_end_matches(':').trim();
            let match_expr = parse_expression(expr_str)?;

            let mut match_lines = Vec::new();
            i += 1;
            while i < lines.len() {
                let next_raw = lines[i];
                let next_trimmed = next_raw.trim();
                if next_trimmed.is_empty() || next_trimmed.starts_with('#') {
                    i += 1;
                    continue;
                }
                if (next_raw.len() - next_raw.trim_start().len()) <= cur_indent {
                    break;
                }
                match_lines.push(next_raw);
                i += 1;
            }

            let arms = parse_match_arms(&match_lines)?;
            stmts.push(Stmt::Match {
                expr: match_expr,
                arms,
            });
            continue;
        }

        if let Some(rest) = line.strip_prefix("await ") {
            let expr = parse_expression(rest.trim())?;
            stmts.push(Stmt::Await(expr));
            i += 1;
            continue;
        }

        if let Some(rest) = line.strip_prefix("return") {
            let expr_str = rest.trim();
            if expr_str.is_empty() {
                stmts.push(Stmt::Return(None));
            } else {
                let expr = parse_expression(expr_str)?;
                stmts.push(Stmt::Return(Some(expr)));
            }
            i += 1;
            continue;
        }

        if let Some(rest) = line.strip_prefix("var ") {
            let (name, expr) = if let Some(eq) = rest.find('=') {
                let n = rest[..eq].trim();
                let clean_n = if let Some(col) = n.find(':') { n[..col].trim() } else { n };
                let e = parse_expression(rest[eq + 1..].trim())?;
                (clean_n.to_string(), Some(e))
            } else {
                let n = rest.trim();
                let clean_n = if let Some(col) = n.find(':') { n[..col].trim() } else { n };
                (clean_n.to_string(), None)
            };
            stmts.push(Stmt::VarDecl { name, init: expr });
            i += 1;
            continue;
        }

        if let Some((target, op, expr_str)) = split_assignment(line) {
            let expr = parse_expression(expr_str)?;
            stmts.push(Stmt::Assign { target, op, expr });
            i += 1;
            continue;
        }

        // Call expression or await expression statement
        if line.contains('(') || line.starts_with("super") || line.contains('.') {
            let expr = parse_expression(line)?;
            stmts.push(Stmt::Call(expr));
            i += 1;
            continue;
        }

        i += 1;
    }

    Ok(stmts)
}

fn parse_match_arms(lines: &[&str]) -> Result<Vec<MatchArm>, VmError> {
    let mut arms = Vec::new();
    let mut idx = 0;

    let arm_indent = lines
        .iter()
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(4);

    while idx < lines.len() {
        let raw = lines[idx];
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            idx += 1;
            continue;
        }

        let cur_indent = raw.len() - raw.trim_start().len();
        if cur_indent != arm_indent {
            idx += 1;
            continue;
        }

        let header = trimmed.trim_end_matches(':');
        let (pat_str, guard_expr) = if let Some(when_idx) = header.find(" when ") {
            let pat = header[..when_idx].trim();
            let guard = header[when_idx + 6..].trim();
            (pat, Some(parse_expression(guard)?))
        } else {
            (header.trim(), None)
        };

        let patterns = parse_comma_patterns(pat_str)?;

        idx += 1;
        let mut body_lines = Vec::new();
        while idx < lines.len() {
            let inner_raw = lines[idx];
            let inner_trimmed = inner_raw.trim();
            if inner_trimmed.is_empty() || inner_trimmed.starts_with('#') {
                idx += 1;
                continue;
            }
            if (inner_raw.len() - inner_raw.trim_start().len()) <= arm_indent {
                break;
            }
            body_lines.push(inner_raw);
            idx += 1;
        }

        let body = parse_statements(&body_lines)?;
        arms.push(MatchArm {
            patterns,
            guard: guard_expr,
            body,
        });
    }

    Ok(arms)
}

fn parse_comma_patterns(s: &str) -> Result<Vec<GdPattern>, VmError> {
    let parts = split_comma_args(s);
    let mut patterns = Vec::new();
    for p in parts {
        patterns.push(parse_single_pattern(p.trim())?);
    }
    Ok(patterns)
}

fn parse_single_pattern(s: &str) -> Result<GdPattern, VmError> {
    if s == "_" {
        return Ok(GdPattern::Wildcard);
    }
    if let Some(rest) = s.strip_prefix("var ") {
        return Ok(GdPattern::Bind(rest.trim().to_string()));
    }
    if s.starts_with('[') && s.ends_with(']') {
        let inner = &s[1..s.len() - 1].trim();
        let parts = split_comma_args(inner);
        let mut patterns = Vec::new();
        let mut has_rest = false;
        for p in parts {
            let clean = p.trim();
            if clean == ".." {
                has_rest = true;
            } else if !clean.is_empty() {
                patterns.push(parse_single_pattern(clean)?);
            }
        }
        return Ok(GdPattern::Array { patterns, has_rest });
    }
    if s.starts_with('{') && s.ends_with('}') {
        let inner = &s[1..s.len() - 1].trim();
        let parts = split_comma_args(inner);
        let mut pairs = Vec::new();
        for p in parts {
            let clean = p.trim();
            if let Some(col) = clean.find(':') {
                let k_str = clean[..col].trim().trim_matches('"').trim_matches('\'');
                let pat = parse_single_pattern(clean[col + 1..].trim())?;
                pairs.push((k_str.to_string(), pat));
            }
        }
        return Ok(GdPattern::Dict { pairs });
    }

    if let Ok(expr) = parse_expression(s)
        && let Expr::Literal(lit) = expr
    {
        return Ok(GdPattern::Literal(lit));
    }

    if let Ok(i) = s.parse::<i64>() {
        return Ok(GdPattern::Literal(IrValue::I64(i)));
    }
    if let Ok(f) = s.parse::<f64>() {
        return Ok(GdPattern::Literal(IrValue::F64(f)));
    }

    Ok(GdPattern::Literal(IrValue::String(s.trim_matches('"').trim_matches('\'').to_string())))
}

fn split_assignment(line: &str) -> Option<(AssignTarget, AssignOp, &str)> {
    let ops = [
        ("+=", AssignOp::AddAssign),
        ("-=", AssignOp::SubAssign),
        ("*=", AssignOp::MulAssign),
        ("/=", AssignOp::DivAssign),
        ("=", AssignOp::Assign),
    ];

    for (op_str, op) in ops {
        if let Some(idx) = line.find(op_str) {
            if op == AssignOp::Assign && (line[..idx].ends_with(['!', '<', '>', '=']) || line[idx + 1..].starts_with('=')) {
                continue;
            }

            let lhs = line[..idx].trim();
            let rhs = line[idx + op_str.len()..].trim();

            let target = if let Some(b_open) = lhs.find('[')
                && lhs.ends_with(']')
            {
                let base = lhs[..b_open].trim().to_string();
                let inner = &lhs[b_open + 1..lhs.len() - 1];
                let index_expr = parse_expression(inner).ok()?;
                AssignTarget::Subscript {
                    base,
                    index: Box::new(index_expr),
                }
            } else if let Some(dot) = lhs.find('.') {
                AssignTarget::Property {
                    base: lhs[..dot].trim().to_string(),
                    prop: lhs[dot + 1..].trim().to_string(),
                }
            } else {
                AssignTarget::Variable(lhs.to_string())
            };

            return Some((target, op, rhs));
        }
    }
    None
}

/// Parses an expression string into AST.
pub fn parse_expression(expr_str: &str) -> Result<Expr, VmError> {
    let mut s = expr_str.trim();

    // Strip outer parentheses if balanced
    while s.starts_with('(') && s.ends_with(')') && is_bracket_balanced(&s[1..s.len() - 1]) {
        s = s[1..s.len() - 1].trim();
    }

    if s == "true" { return Ok(Expr::Literal(IrValue::Bool(true))); }
    if s == "false" { return Ok(Expr::Literal(IrValue::Bool(false))); }
    if s == "null" { return Ok(Expr::Literal(IrValue::Nil)); }

    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        let inner = &s[1..s.len() - 1];
        return Ok(Expr::Literal(IrValue::String(inner.to_string())));
    }

    // Number check before operators
    if let Ok(i) = s.parse::<i64>() {
        return Ok(Expr::Literal(IrValue::I64(i)));
    }
    if let Ok(f) = s.parse::<f64>() {
        return Ok(Expr::Literal(IrValue::F64(f)));
    }

    // `await <expr>`
    if let Some(rest) = s.strip_prefix("await ") {
        let inner = parse_expression(rest.trim())?;
        return Ok(Expr::Await(Box::new(inner)));
    }

    // Inline lambda: func(x, y = 1): return x + y
    if let Some(rest) = s.strip_prefix("func(")
        && let Some(close_paren) = rest.find(')')
    {
        let params_part = &rest[..close_paren];
        let after = rest[close_paren + 1..].trim();
        if let Some(body_part) = after.strip_prefix(':') {
            let params = parse_lambda_params(params_part);
            let body_stmts = parse_statements(&[body_part.trim()])?;
            return Ok(Expr::Lambda {
                parameters: params,
                body: body_stmts,
            });
        }
    }

    // Array literal: [1, 2, 3]
    if s.starts_with('[') && s.ends_with(']') && is_bracket_balanced(&s[1..s.len() - 1]) {
        let inner = &s[1..s.len() - 1].trim();
        if inner.is_empty() {
            return Ok(Expr::ArrayLit(Vec::new()));
        }
        let parts = split_comma_args(inner);
        let mut elems = Vec::new();
        for p in parts {
            if !p.trim().is_empty() {
                elems.push(parse_expression(p.trim())?);
            }
        }
        return Ok(Expr::ArrayLit(elems));
    }

    // Dictionary literal: {"k": v, ...}
    if s.starts_with('{') && s.ends_with('}') && is_bracket_balanced(&s[1..s.len() - 1]) {
        let inner = &s[1..s.len() - 1].trim();
        if inner.is_empty() {
            return Ok(Expr::DictLit(Vec::new()));
        }
        let parts = split_comma_args(inner);
        let mut pairs = Vec::new();
        for p in parts {
            let clean = p.trim();
            if let Some(col) = clean.find(':') {
                let k = parse_expression(clean[..col].trim())?;
                let v = parse_expression(clean[col + 1..].trim())?;
                pairs.push((k, v));
            }
        }
        return Ok(Expr::DictLit(pairs));
    }

    // Binary operations (from lowest to highest precedence)
    let bin_ops = [
        (" or ", BinOp::Or),
        (" and ", BinOp::And),
        (" == ", BinOp::Eq),
        (" != ", BinOp::Ne),
        (" <= ", BinOp::Le),
        (" >= ", BinOp::Ge),
        (" < ", BinOp::Lt),
        (" > ", BinOp::Gt),
        (" + ", BinOp::Add),
        (" - ", BinOp::Sub),
        (" * ", BinOp::Mul),
        (" / ", BinOp::Div),
        (" % ", BinOp::Mod),
    ];

    for (op_str, op) in bin_ops {
        if let Some(idx) = find_top_level_operator(s, op_str) {
            let left = parse_expression(&s[..idx])?;
            let right = parse_expression(&s[idx + op_str.len()..])?;
            return Ok(Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            });
        }
    }

    // Subscript: base[index]
    if s.ends_with(']')
        && let Some(open) = find_matching_open_bracket(s)
    {
        let base_str = s[..open].trim();
        let idx_str = s[open + 1..s.len() - 1].trim();
        if !base_str.is_empty() && !idx_str.is_empty() {
            let base_expr = parse_expression(base_str)?;
            let idx_expr = parse_expression(idx_str)?;
            return Ok(Expr::Subscript {
                base: Box::new(base_expr),
                index: Box::new(idx_expr),
            });
        }
    }

    // Function call or method call: ends with ')'
    if s.ends_with(')')
        && let Some(open) = find_matching_open_paren(s)
    {
        let caller = s[..open].trim();
        let args_str = &s[open + 1..s.len() - 1].trim();
        let args_strs = split_comma_args(args_str);
        let mut args = Vec::with_capacity(args_strs.len());
        for a in args_strs {
            if !a.trim().is_empty() {
                args.push(parse_expression(a.trim())?);
            }
        }

        if caller == "super" {
            return Ok(Expr::SuperCall { method: None, args });
        }

        if let Some(rest) = caller.strip_prefix("super.") {
            return Ok(Expr::SuperCall { method: Some(rest.trim().to_string()), args });
        }

        if let Some(dot) = find_top_level_dot(caller) {
            let target = parse_expression(&caller[..dot])?;
            let method = caller[dot + 1..].trim().to_string();
            return Ok(Expr::MethodCall {
                target: Box::new(target),
                method,
                args,
            });
        }

        return Ok(Expr::Call {
            func_name: caller.to_string(),
            args,
        });
    }

    // Property access: base.prop
    if let Some(dot) = find_top_level_dot(s) {
        let base_str = s[..dot].trim();
        let prop = s[dot + 1..].trim();
        if prop.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') {
            let base_expr = parse_expression(base_str)?;
            return Ok(Expr::PropAccess {
                base: Box::new(base_expr),
                prop: prop.to_string(),
            });
        }
    }

    Ok(Expr::Var(s.to_string()))
}

fn parse_lambda_params(p_str: &str) -> Vec<(String, Option<IrValue>)> {
    let mut params = Vec::new();
    for p in split_comma_args(p_str) {
        let clean = p.trim();
        if clean.is_empty() {
            continue;
        }
        if let Some(eq) = clean.find('=') {
            let name_part = clean[..eq].trim();
            let def_part = clean[eq + 1..].trim();
            let clean_name = name_part.split(':').next().unwrap_or(name_part).trim();
            let def_val = if let Ok(Expr::Literal(lit)) = parse_expression(def_part) {
                Some(lit)
            } else {
                None
            };
            params.push((clean_name.to_string(), def_val));
        } else {
            let clean_name = clean.split(':').next().unwrap_or(clean).trim();
            params.push((clean_name.to_string(), None));
        }
    }
    params
}

fn find_top_level_operator(s: &str, op: &str) -> Option<usize> {
    let mut depth = 0;
    let mut in_quote = false;
    let mut quote_char = '"';
    let bytes = s.as_bytes();
    let op_bytes = op.as_bytes();

    let mut i = 0;
    while i + op_bytes.len() <= bytes.len() {
        let b = bytes[i];
        if (b == b'"' || b == b'\'') && (i == 0 || bytes[i - 1] != b'\\') {
            if !in_quote {
                in_quote = true;
                quote_char = b as char;
            } else if b as char == quote_char {
                in_quote = false;
            }
        } else if !in_quote {
            match b {
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => if depth > 0 { depth -= 1 },
                _ => {
                    if depth == 0 && &bytes[i..i + op_bytes.len()] == op_bytes {
                        return Some(i);
                    }
                }
            }
        }
        i += 1;
    }
    None
}

fn find_top_level_dot(s: &str) -> Option<usize> {
    let mut depth = 0;
    let mut in_quote = false;
    let mut quote_char = '"';
    let mut last_dot = None;
    let bytes = s.as_bytes();

    for (i, &b) in bytes.iter().enumerate() {
        if (b == b'"' || b == b'\'') && (i == 0 || bytes[i - 1] != b'\\') {
            if !in_quote {
                in_quote = true;
                quote_char = b as char;
            } else if b as char == quote_char {
                in_quote = false;
            }
        } else if !in_quote {
            match b {
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => if depth > 0 { depth -= 1 },
                b'.' if depth == 0 => {
                    let next = bytes.get(i + 1).copied();
                    if let Some(c) = next
                        && ((c as char).is_ascii_alphabetic() || c == b'_')
                    {
                        last_dot = Some(i);
                    }
                }
                _ => {}
            }
        }
    }
    last_dot
}

fn find_matching_open_paren(s: &str) -> Option<usize> {
    if !s.ends_with(')') {
        return None;
    }
    let mut depth = 0;
    let mut in_quote = false;
    let mut quote_char = '"';
    let bytes = s.as_bytes();

    for i in (0..bytes.len()).rev() {
        let b = bytes[i];
        if (b == b'"' || b == b'\'') && (i == 0 || bytes[i - 1] != b'\\') {
            if !in_quote {
                in_quote = true;
                quote_char = b as char;
            } else if b as char == quote_char {
                in_quote = false;
            }
        } else if !in_quote {
            match b {
                b')' => depth += 1,
                b'(' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i);
                    }
                }
                _ => {}
            }
        }
    }
    None
}

fn find_matching_open_bracket(s: &str) -> Option<usize> {
    if !s.ends_with(']') {
        return None;
    }
    let mut depth = 0;
    let mut in_quote = false;
    let mut quote_char = '"';
    let bytes = s.as_bytes();

    for i in (0..bytes.len()).rev() {
        let b = bytes[i];
        if (b == b'"' || b == b'\'') && (i == 0 || bytes[i - 1] != b'\\') {
            if !in_quote {
                in_quote = true;
                quote_char = b as char;
            } else if b as char == quote_char {
                in_quote = false;
            }
        } else if !in_quote {
            match b {
                b']' => depth += 1,
                b'[' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i);
                    }
                }
                _ => {}
            }
        }
    }
    None
}

fn is_bracket_balanced(s: &str) -> bool {
    let mut depth = 0;
    let mut in_quote = false;
    let mut quote_char = '"';
    let bytes = s.as_bytes();

    for (i, &b) in bytes.iter().enumerate() {
        if (b == b'"' || b == b'\'') && (i == 0 || bytes[i - 1] != b'\\') {
            if !in_quote {
                in_quote = true;
                quote_char = b as char;
            } else if b as char == quote_char {
                in_quote = false;
            }
        } else if !in_quote {
            match b {
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => {
                    if depth == 0 {
                        return false;
                    }
                    depth -= 1;
                }
                _ => {}
            }
        }
    }
    depth == 0
}

fn split_comma_args(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0;
    let mut in_quote = false;
    let mut quote_char = '"';
    let mut start = 0;

    for (idx, c) in s.char_indices() {
        if (c == '"' || c == '\'') && (idx == 0 || s.as_bytes()[idx - 1] != b'\\') {
            if !in_quote {
                in_quote = true;
                quote_char = c;
            } else if c == quote_char {
                in_quote = false;
            }
        } else if !in_quote {
            match c {
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => if depth > 0 { depth -= 1 },
                ',' if depth == 0 => {
                    out.push(&s[start..idx]);
                    start = idx + 1;
                }
                _ => {}
            }
        }
    }

    if start <= s.len() {
        out.push(&s[start..]);
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use foreplay_ir::NodeId;

    const SKATER_SCRIPT: &str = r#"
class_name SkaterController
extends CharacterBody3D

@export var max_speed: float = 14.5
@export var is_grounded: bool = true
var velocity = Vector3(0.0, 0.0, 0.0)

func _ready() -> void:
    velocity.x = 5.0
    print("Skater ready!")

func _physics_process(delta: float) -> void:
    position.x += max_speed * delta
    if position.x > 10.0:
        emit_signal("trick_landed", "kickflip", 100)
"#;

    #[test]
    fn vm_ready_and_physics_process_lifecycle() {
        let mut vm = GdVm::from_source(SKATER_SCRIPT).expect("vm compilation");
        let mut node = IrNode {
            id: NodeId::new(1, 1).unwrap(),
            name: "Skater".into(),
            class_name: "CharacterBody3D".into(),
            parent: None,
            instance_scene: None,
            properties: BTreeMap::new(),
            groups: Vec::new(),
        };
        node.properties.insert("position".into(), IrValue::Vec3([0.0, 0.0, 0.0]));

        let ready_res = vm.ready(&mut node).expect("ready execution");
        assert_eq!(ready_res.logs, vec!["Skater ready!"]);

        let step1 = vm.step_physics_process(0.1, &mut node).expect("step 1");
        assert!(step1.emitted_signals.is_empty());
        assert_eq!(node.properties.get("position"), Some(&IrValue::Vec3([1.45, 0.0, 0.0])));

        let step2 = vm.step_physics_process(1.0, &mut node).expect("step 2");
        assert_eq!(step2.emitted_signals.len(), 1);
        assert_eq!(step2.emitted_signals[0].0, "trick_landed");
        assert_eq!(step2.emitted_signals[0].1, vec![IrValue::String("kickflip".into()), IrValue::I64(100)]);
    }

    #[test]
    fn vm_budget_guard_prevents_infinite_loops() {
        let infinite_script = r#"
func _ready():
    while true:
        pass
"#;
        let mut vm = GdVm::from_source(infinite_script).expect("compile");
        vm.max_step_budget = 50;
        let mut node = IrNode {
            id: NodeId::new(1, 1).unwrap(),
            name: "LoopNode".into(),
            class_name: "Node".into(),
            parent: None,
            instance_scene: None,
            properties: BTreeMap::new(),
            groups: Vec::new(),
        };

        let err = vm.ready(&mut node).unwrap_err();
        assert!(matches!(err, VmError::BudgetExceeded { budget: 50 }));
    }
}
