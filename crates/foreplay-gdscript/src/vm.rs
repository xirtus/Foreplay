//! Safe deterministic GDScript execution VM (`GdVm`).
//!
//! Evaluates GDScript AST with bounded execution budgets, mutating `foreplay_ir::IrNode`
//! properties, dispatching lifecycle methods (`_ready`, `_physics_process`), and handling signals.

use crate::{GdScriptModule, parse_gdscript};
use foreplay_ir::{IrNode, IrValue};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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
pub enum Expr {
    Literal(IrValue),
    Var(String),
    PropAccess { base: Box<Expr>, prop: String },
    Binary { op: BinOp, left: Box<Expr>, right: Box<Expr> },
    Unary { op: UnaryOp, expr: Box<Expr> },
    Call { func_name: String, args: Vec<Expr> },
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
    Call(Expr),
    Return(Option<Expr>),
}

#[derive(Clone, PartialEq, Debug)]
pub struct FunctionAst {
    pub name: String,
    pub parameters: Vec<String>,
    pub body: Vec<Stmt>,
}

/// Deterministic GDScript Virtual Machine executing scripts bound to `IrNode` instances.
#[derive(Clone, PartialEq, Debug)]
pub struct GdVm {
    pub module: GdScriptModule,
    pub member_vars: BTreeMap<String, IrValue>,
    pub functions: BTreeMap<String, FunctionAst>,
    pub max_step_budget: usize,
}

impl GdVm {
    pub const DEFAULT_STEP_BUDGET: usize = 10_000;

    /// Compiles GDScript source into a `GdVm` instance.
    pub fn from_source(source: &str) -> Result<Self, VmError> {
        let module = parse_gdscript(source)
            .map_err(|e| VmError::ParseError { message: e.to_string() })?;

        let mut member_vars = BTreeMap::new();
        for export in &module.exports {
            if let Some(val) = &export.default_value {
                member_vars.insert(export.name.clone(), val.clone());
            }
        }

        // Initialize top-level member variables (both @export and plain var)
        for line in source.lines() {
            let trimmed = line.trim();
            let indent = line.len() - line.trim_start().len();
            if indent == 0
                && let Some(rest) = trimmed.strip_prefix("var ")
                && let Some(eq) = rest.find('=')
            {
                let var_name = rest[..eq].trim();
                let clean_name = if let Some(col) = var_name.find(':') {
                    var_name[..col].trim()
                } else {
                    var_name
                };
                let expr_str = rest[eq + 1..].trim();
                if let Ok(expr) = parse_expression(expr_str) {
                    let mut dummy_ctx = ExecutionContext {
                        steps: 0,
                        max_budget: 100,
                        emitted_signals: Vec::new(),
                        logs: Vec::new(),
                    };
                    let dummy_locals = BTreeMap::new();
                    let mut vm_init = GdVm {
                        module: module.clone(),
                        member_vars: BTreeMap::new(),
                        functions: BTreeMap::new(),
                        max_step_budget: 100,
                    };
                    if let Ok(val) = vm_init.eval_expr(&expr, &dummy_locals, &mut dummy_ctx) {
                        member_vars.insert(clean_name.to_string(), val);
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
        })
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
        let func = match self.functions.get(method_name) {
            Some(f) => f.clone(),
            None => {
                // If it's an un-implemented lifecycle method, return empty success
                if method_name.starts_with('_') {
                    return Ok(VmExecutionResult::default());
                }
                return Err(VmError::MethodNotFound { name: method_name.to_string() });
            }
        };

        // Ensure position is present
        if !node.properties.contains_key("position") {
            node.properties.insert("position".into(), IrValue::Vec3([0.0, 0.0, 0.0]));
        }

        // 1. Sync node properties into member_vars
        for (k, v) in &node.properties {
            self.member_vars.insert(k.clone(), v.clone());
        }

        // 2. Setup local scope
        let mut local_vars = BTreeMap::new();
        for (idx, param) in func.parameters.iter().enumerate() {
            let arg_val = args.get(idx).cloned().unwrap_or(IrValue::Nil);
            local_vars.insert(param.clone(), arg_val);
        }

        let mut ctx = ExecutionContext {
            steps: 0,
            max_budget: self.max_step_budget,
            emitted_signals: Vec::new(),
            logs: Vec::new(),
        };

        let ret_val = self.execute_block(&func.body, &mut local_vars, &mut ctx)?;

        // 3. Sync member_vars back into node properties
        for (k, v) in &self.member_vars {
            node.properties.insert(k.clone(), v.clone());
        }

        Ok(VmExecutionResult {
            return_value: ret_val,
            emitted_signals: ctx.emitted_signals,
            logs: ctx.logs,
            steps_executed: ctx.steps,
        })
    }

    fn execute_block(
        &mut self,
        block: &[Stmt],
        locals: &mut BTreeMap<String, IrValue>,
        ctx: &mut ExecutionContext,
    ) -> Result<Option<IrValue>, VmError> {
        for stmt in block {
            ctx.tick()?;
            match stmt {
                Stmt::Pass => {}
                Stmt::VarDecl { name, init } => {
                    let val = match init {
                        Some(expr) => self.eval_expr(expr, locals, ctx)?,
                        None => IrValue::Nil,
                    };
                    locals.insert(name.clone(), val);
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
                            if locals.contains_key(var_name) {
                                locals.insert(var_name.clone(), new_val);
                            } else {
                                self.member_vars.insert(var_name.clone(), new_val);
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
                    }
                }
                Stmt::If { condition, body, elifs, else_body } => {
                    let cond_val = self.eval_expr(condition, locals, ctx)?;
                    if is_truthy(&cond_val) {
                        if let Some(r) = self.execute_block(body, locals, ctx)? {
                            return Ok(Some(r));
                        }
                    } else {
                        let mut executed_elif = false;
                        for (elif_cond, elif_body) in elifs {
                            let v = self.eval_expr(elif_cond, locals, ctx)?;
                            if is_truthy(&v) {
                                executed_elif = true;
                                if let Some(r) = self.execute_block(elif_body, locals, ctx)? {
                                    return Ok(Some(r));
                                }
                                break;
                            }
                        }
                        if !executed_elif
                            && let Some(else_b) = else_body
                            && let Some(r) = self.execute_block(else_b, locals, ctx)?
                        {
                            return Ok(Some(r));
                        }
                    }
                }
                Stmt::While { condition, body } => {
                    while is_truthy(&self.eval_expr(condition, locals, ctx)?) {
                        ctx.tick()?;
                        if let Some(r) = self.execute_block(body, locals, ctx)? {
                            return Ok(Some(r));
                        }
                    }
                }
                Stmt::For { var_name, range_start, range_end, body } => {
                    let start = to_i64(&self.eval_expr(range_start, locals, ctx)?)?;
                    let end = to_i64(&self.eval_expr(range_end, locals, ctx)?)?;
                    for i in start..end {
                        ctx.tick()?;
                        locals.insert(var_name.clone(), IrValue::I64(i));
                        if let Some(r) = self.execute_block(body, locals, ctx)? {
                            return Ok(Some(r));
                        }
                    }
                }
                Stmt::Call(expr) => {
                    self.eval_expr(expr, locals, ctx)?;
                }
                Stmt::Return(maybe_expr) => {
                    let val = match maybe_expr {
                        Some(expr) => self.eval_expr(expr, locals, ctx)?,
                        None => IrValue::Nil,
                    };
                    return Ok(Some(val));
                }
            }
        }
        Ok(None)
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
                if let Some(v) = locals.get(name) {
                    return Ok(v.clone());
                }
                if let Some(v) = self.member_vars.get(name) {
                    return Ok(v.clone());
                }
                Ok(IrValue::Nil)
            }
            Expr::PropAccess { base, prop } => {
                let base_val = self.eval_expr(base, locals, ctx)?;
                get_component_prop(&base_val, prop)
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
                self.eval_call(func_name, &evaluated_args, ctx)
            }
        }
    }

    fn eval_call(
        &mut self,
        func_name: &str,
        args: &[IrValue],
        ctx: &mut ExecutionContext,
    ) -> Result<IrValue, VmError> {
        match func_name {
            "print" => {
                let msg = args.iter().map(value_to_string).collect::<Vec<_>>().join(" ");
                ctx.logs.push(msg);
                Ok(IrValue::Nil)
            }
            "emit_signal" => {
                let sig_name = args
                    .first()
                    .and_then(|v| match v {
                        IrValue::String(s) => Some(s.clone()),
                        _ => None,
                    })
                    .unwrap_or_default();
                let sig_args = if args.len() > 1 { args[1..].to_vec() } else { Vec::new() };
                ctx.emitted_signals.push((sig_name, sig_args));
                Ok(IrValue::Nil)
            }
            "Vector2" => {
                let x = to_f32(args.first().unwrap_or(&IrValue::F64(0.0)));
                let y = to_f32(args.get(1).unwrap_or(&IrValue::F64(0.0)));
                Ok(IrValue::Vec2([x, y]))
            }
            "Vector3" => {
                let x = to_f32(args.first().unwrap_or(&IrValue::F64(0.0)));
                let y = to_f32(args.get(1).unwrap_or(&IrValue::F64(0.0)));
                let z = to_f32(args.get(2).unwrap_or(&IrValue::F64(0.0)));
                Ok(IrValue::Vec3([x, y, z]))
            }
            "clamp" => {
                let val = to_f64(args.first().unwrap_or(&IrValue::F64(0.0)))?;
                let min = to_f64(args.get(1).unwrap_or(&IrValue::F64(0.0)))?;
                let max = to_f64(args.get(2).unwrap_or(&IrValue::F64(1.0)))?;
                Ok(IrValue::F64(val.clamp(min, max)))
            }
            "lerp" => {
                let a = to_f64(args.first().unwrap_or(&IrValue::F64(0.0)))?;
                let b = to_f64(args.get(1).unwrap_or(&IrValue::F64(0.0)))?;
                let t = to_f64(args.get(2).unwrap_or(&IrValue::F64(0.0)))?;
                Ok(IrValue::F64(a + (b - a) * t))
            }
            "abs" => {
                let v = to_f64(args.first().unwrap_or(&IrValue::F64(0.0)))?;
                Ok(IrValue::F64(v.abs()))
            }
            "min" => {
                let a = to_f64(args.first().unwrap_or(&IrValue::F64(0.0)))?;
                let b = to_f64(args.get(1).unwrap_or(&IrValue::F64(0.0)))?;
                Ok(IrValue::F64(a.min(b)))
            }
            "max" => {
                let a = to_f64(args.first().unwrap_or(&IrValue::F64(0.0)))?;
                let b = to_f64(args.get(1).unwrap_or(&IrValue::F64(0.0)))?;
                Ok(IrValue::F64(a.max(b)))
            }
            "sqrt" => {
                let v = to_f64(args.first().unwrap_or(&IrValue::F64(0.0)))?;
                Ok(IrValue::F64(v.sqrt()))
            }
            "sin" => {
                let v = to_f64(args.first().unwrap_or(&IrValue::F64(0.0)))?;
                Ok(IrValue::F64(v.sin()))
            }
            "cos" => {
                let v = to_f64(args.first().unwrap_or(&IrValue::F64(0.0)))?;
                Ok(IrValue::F64(v.cos()))
            }
            other => Err(VmError::MethodNotFound { name: other.to_string() }),
        }
    }
}

struct ExecutionContext {
    steps: usize,
    max_budget: usize,
    emitted_signals: Vec<(String, Vec<IrValue>)>,
    logs: Vec<String>,
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
            (IrValue::Vec3([x1, y1, z1]), IrValue::Vec3([x2, y2, z2])) => {
                Ok(IrValue::Vec3([x1 + x2, y1 + y2, z1 + z2]))
            }
            (IrValue::Vec2([x1, y1]), IrValue::Vec2([x2, y2])) => {
                Ok(IrValue::Vec2([x1 + x2, y1 + y2]))
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
            (IrValue::Vec2([x, y]), IrValue::F64(s)) => {
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

fn value_to_string(val: &IrValue) -> String {
    match val {
        IrValue::String(s) => s.clone(),
        IrValue::I64(i) => format!("{i}"),
        IrValue::F64(f) => format!("{f}"),
        IrValue::Bool(b) => format!("{b}"),
        IrValue::Nil => "null".into(),
        other => format!("{other:?}"),
    }
}

fn get_component_prop(val: &IrValue, prop: &str) -> Result<IrValue, VmError> {
    match (val, prop) {
        (IrValue::Vec3([x, _, _]), "x") => Ok(IrValue::F64(*x as f64)),
        (IrValue::Vec3([_, y, _]), "y") => Ok(IrValue::F64(*y as f64)),
        (IrValue::Vec3([_, _, z]), "z") => Ok(IrValue::F64(*z as f64)),
        (IrValue::Vec2([x, _]), "x") => Ok(IrValue::F64(*x as f64)),
        (IrValue::Vec2([_, y]), "y") => Ok(IrValue::F64(*y as f64)),
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
            let (func_name, params) = parse_func_signature(func_decl);

            idx += 1;
            let mut body_lines = Vec::new();

            while idx < lines.len() {
                let body_line = lines[idx];
                let body_trimmed = body_line.trim();

                if body_trimmed.is_empty() || body_trimmed.starts_with('#') {
                    idx += 1;
                    continue;
                }

                // Check indentation
                let indent = body_line.len() - body_line.trim_start().len();
                if indent == 0 && !body_trimmed.is_empty() {
                    // Start of new top-level declaration
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
                    body: body_stmts,
                },
            );
        } else {
            idx += 1;
        }
    }

    Ok(functions)
}

fn parse_func_signature(decl: &str) -> (String, Vec<String>) {
    let clean = if let Some(idx) = decl.find("->") {
        decl[..idx].trim()
    } else {
        decl.trim()
    };

    if let Some(open) = clean.find('(') {
        let name = clean[..open].trim().to_string();
        let close = clean.rfind(')').unwrap_or(clean.len());
        let params_str = &clean[open + 1..close].trim();
        let params = if params_str.is_empty() {
            Vec::new()
        } else {
            params_str
                .split(',')
                .map(|p| {
                    let s = p.trim();
                    if let Some(col) = s.find(':') {
                        s[..col].trim().to_string()
                    } else if let Some(eq) = s.find('=') {
                        s[..eq].trim().to_string()
                    } else {
                        s.to_string()
                    }
                })
                .filter(|s| !s.is_empty())
                .collect()
        };
        (name, params)
    } else {
        (clean.to_string(), Vec::new())
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

            // Check elif / else
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
            // e.g. "i in range(10)" or "i in range(0, 10)"
            let parts: Vec<&str> = header.split(" in ").collect();
            if parts.len() == 2 {
                let var_name = parts[0].trim().to_string();
                let range_part = parts[1].trim();
                let (start_expr, end_expr) = if let Some(inner) = range_part.strip_prefix("range(")
                    && inner.ends_with(')')
                {
                    let r_args = split_comma_args(&inner[..inner.len() - 1]);
                    if r_args.len() == 1 {
                        (Expr::Literal(IrValue::I64(0)), parse_expression(r_args[0].trim())?)
                    } else if r_args.len() >= 2 {
                        (parse_expression(r_args[0].trim())?, parse_expression(r_args[1].trim())?)
                    } else {
                        (Expr::Literal(IrValue::I64(0)), Expr::Literal(IrValue::I64(0)))
                    }
                } else {
                    (Expr::Literal(IrValue::I64(0)), Expr::Literal(IrValue::I64(0)))
                };

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
                stmts.push(Stmt::For {
                    var_name,
                    range_start: start_expr,
                    range_end: end_expr,
                    body,
                });
                continue;
            }
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

        // Assignments: +=, -=, *=, /=, =
        if let Some((target, op, expr_str)) = split_assignment(line) {
            let expr = parse_expression(expr_str)?;
            stmts.push(Stmt::Assign { target, op, expr });
            i += 1;
            continue;
        }

        // Function call statement (e.g. print(...) or emit_signal(...))
        if line.contains('(') && line.ends_with(')') {
            let expr = parse_expression(line)?;
            stmts.push(Stmt::Call(expr));
            i += 1;
            continue;
        }

        i += 1;
    }

    Ok(stmts)
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
            // Ensure not ==, <=, >=, !=
            if op == AssignOp::Assign && (line[..idx].ends_with(['!', '<', '>', '=']) || line[idx + 1..].starts_with('=')) {
                continue;
            }

            let lhs = line[..idx].trim();
            let rhs = line[idx + op_str.len()..].trim();

            let target = if let Some(dot) = lhs.find('.') {
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

fn parse_expression(expr_str: &str) -> Result<Expr, VmError> {
    let s = expr_str.trim();

    // Literal checks
    if s == "true" { return Ok(Expr::Literal(IrValue::Bool(true))); }
    if s == "false" { return Ok(Expr::Literal(IrValue::Bool(false))); }
    if s == "null" { return Ok(Expr::Literal(IrValue::Nil)); }

    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        let inner = &s[1..s.len() - 1];
        return Ok(Expr::Literal(IrValue::String(inner.to_string())));
    }

    // Numbers first so 5.0 is parsed as F64, not property access
    if let Ok(i) = s.parse::<i64>() {
        return Ok(Expr::Literal(IrValue::I64(i)));
    }
    if let Ok(f) = s.parse::<f64>() {
        return Ok(Expr::Literal(IrValue::F64(f)));
    }

    // Function / constructor call: name(arg1, arg2, ...)
    if let Some(open) = s.find('(')
        && s.ends_with(')')
        && !s.starts_with('(')
    {
        let func_name = s[..open].trim().to_string();
        let inner = &s[open + 1..s.len() - 1];
        let args_strs = split_comma_args(inner);
        let mut args = Vec::with_capacity(args_strs.len());
        for a in args_strs {
            if !a.trim().is_empty() {
                args.push(parse_expression(a.trim())?);
            }
        }
        return Ok(Expr::Call { func_name, args });
    }

    // Binary operations (lowest precedence to highest)
    let bin_ops = [
        (" and ", BinOp::And),
        (" or ", BinOp::Or),
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
        if let Some(idx) = s.find(op_str) {
            let left = parse_expression(&s[..idx])?;
            let right = parse_expression(&s[idx + op_str.len()..])?;
            return Ok(Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            });
        }
    }

    // Property access: base.prop (ensure prop starts with alphabetic or underscore)
    if let Some(dot) = s.find('.') {
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

    // Identifier / variable name
    Ok(Expr::Var(s.to_string()))
}

fn split_comma_args(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0;
    let mut in_quote = false;
    let mut start = 0;

    for (idx, c) in s.char_indices() {
        if c == '"' {
            in_quote = !in_quote;
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

        // 1. Run ready
        let ready_res = vm.ready(&mut node).expect("ready execution");
        assert_eq!(ready_res.logs, vec!["Skater ready!"]);

        // 2. Step 1 frame delta = 0.1s: position.x += 14.5 * 0.1 = 1.45
        let step1 = vm.step_physics_process(0.1, &mut node).expect("step 1");
        assert!(step1.emitted_signals.is_empty());
        assert_eq!(node.properties.get("position"), Some(&IrValue::Vec3([1.45, 0.0, 0.0])));

        // 3. Step 10 more frames: position.x reaches 1.45 + 14.5 = 15.95 > 10.0 -> emit signal
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
