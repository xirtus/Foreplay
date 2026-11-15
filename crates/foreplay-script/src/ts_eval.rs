//! Zero-compile TypeScript / JavaScript component runner (`QuickJsComponentRunner`).
//!
//! Evaluates TypeScript/JavaScript game component scripts with bounded execution budgets,
//! mutating `foreplay_ir::IrNode` properties, dispatching lifecycle methods (`onReady`,
//! `onUpdate`, `onPhysicsUpdate`), and integrating with 3JSE feel tuning knobs.
//!
//! Mirrors Godot / QuickJS component evaluation patterns in pure safe Rust.

use foreplay_ir::{IrNode, IrValue};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const DEFAULT_STEP_BUDGET: usize = 10_000;

/// Errors produced during TypeScript/JavaScript evaluation.
#[derive(Clone, PartialEq, Eq, Debug, thiserror::Error, Serialize, Deserialize)]
pub enum JsEvalError {
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
    #[error("syntax parse error: {message}")]
    ParseError { message: String },
}

/// Execution outcome of a JS/TS function invocation.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct JsExecutionResult {
    pub return_value: Option<IrValue>,
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
    Property { base: Box<Expr>, prop: String },
    Index { base: Box<Expr>, index: Box<Expr> },
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
    Identifier(String),
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
    },
    Binary {
        left: Box<Expr>,
        op: BinOp,
        right: Box<Expr>,
    },
    PropertyAccess {
        base: Box<Expr>,
        prop: String,
    },
    IndexAccess {
        base: Box<Expr>,
        index: Box<Expr>,
    },
    Call {
        callee: String,
        args: Vec<Expr>,
    },
    MethodCall {
        base: Box<Expr>,
        method: String,
        args: Vec<Expr>,
    },
    Array(Vec<Expr>),
    Object(Vec<(String, Expr)>),
}

#[derive(Clone, PartialEq, Debug)]
pub enum Stmt {
    VarDecl {
        name: String,
        init: Option<Expr>,
    },
    Assign {
        target: AssignTarget,
        op: AssignOp,
        value: Expr,
    },
    Expr(Expr),
    If {
        condition: Expr,
        then_branch: Vec<Stmt>,
        else_branch: Option<Vec<Stmt>>,
    },
    While {
        condition: Expr,
        body: Vec<Stmt>,
    },
    For {
        init: Option<Box<Stmt>>,
        condition: Option<Expr>,
        step: Option<Box<Stmt>>,
        body: Vec<Stmt>,
    },
    Return(Option<Expr>),
}

#[derive(Clone, PartialEq, Debug)]
pub struct JsFunction {
    pub name: String,
    pub params: Vec<String>,
    pub body: Vec<Stmt>,
}

/// Zero-compile TypeScript / JavaScript component runner.
#[derive(Clone, PartialEq, Debug)]
pub struct QuickJsComponentRunner {
    pub module_name: String,
    pub script_source: String,
    pub step_budget: usize,
    pub instance_state: BTreeMap<String, IrValue>,
    pub functions: BTreeMap<String, JsFunction>,
}

impl QuickJsComponentRunner {
    /// Creates a new runner from TypeScript / JavaScript source code.
    pub fn from_source(
        module_name: impl Into<String>,
        source: &str,
    ) -> Result<Self, JsEvalError> {
        let module_name = module_name.into();
        let (functions, initial_state) = parse_ts_source(source)?;

        Ok(Self {
            module_name,
            script_source: source.to_string(),
            step_budget: DEFAULT_STEP_BUDGET,
            instance_state: initial_state,
            functions,
        })
    }

    /// Sets the maximum execution step budget.
    pub fn with_step_budget(mut self, budget: usize) -> Self {
        self.step_budget = budget;
        self
    }

    /// Dispatches `onReady(node)` lifecycle hook if defined.
    pub fn call_on_ready(&mut self, node: &mut IrNode) -> Result<JsExecutionResult, JsEvalError> {
        let method = if self.functions.contains_key("onReady") {
            "onReady"
        } else if self.functions.contains_key("ready") {
            "ready"
        } else {
            return Ok(JsExecutionResult::default());
        };

        self.call_method(method, node, &[])
    }

    /// Dispatches `onUpdate(node, dt, feel)` lifecycle hook if defined.
    pub fn call_on_update(
        &mut self,
        node: &mut IrNode,
        dt: f64,
        feel: &BTreeMap<String, f64>,
    ) -> Result<JsExecutionResult, JsEvalError> {
        let method = if self.functions.contains_key("onUpdate") {
            "onUpdate"
        } else if self.functions.contains_key("update") {
            "update"
        } else if self.functions.contains_key("process") {
            "process"
        } else {
            return Ok(JsExecutionResult::default());
        };

        let feel_map = feel
            .iter()
            .map(|(k, &v)| (k.clone(), IrValue::F64(v)))
            .collect();
        let args = vec![IrValue::F64(dt), IrValue::Map(feel_map)];
        self.call_method(method, node, &args)
    }

    /// Dispatches `onPhysicsUpdate(node, dt)` lifecycle hook if defined.
    pub fn call_on_physics_update(
        &mut self,
        node: &mut IrNode,
        dt: f64,
    ) -> Result<JsExecutionResult, JsEvalError> {
        let method = if self.functions.contains_key("onPhysicsUpdate") {
            "onPhysicsUpdate"
        } else if self.functions.contains_key("physicsProcess") {
            "physicsProcess"
        } else if self.functions.contains_key("physics_process") {
            "physics_process"
        } else {
            return Ok(JsExecutionResult::default());
        };

        let args = vec![IrValue::F64(dt)];
        self.call_method(method, node, &args)
    }

    /// Dispatches an arbitrary method on the component.
    pub fn call_method(
        &mut self,
        method: &str,
        node: &mut IrNode,
        args: &[IrValue],
    ) -> Result<JsExecutionResult, JsEvalError> {
        let func = self
            .functions
            .get(method)
            .cloned()
            .ok_or_else(|| JsEvalError::MethodNotFound {
                name: method.to_string(),
            })?;

        let mut exec = ExecutionContext {
            budget: self.step_budget,
            steps_taken: 0,
            locals: BTreeMap::new(),
            instance_state: self.instance_state.clone(),
            node_properties: node.properties.clone(),
            logs: Vec::new(),
        };

        // Ensure "position" exists in node properties if accessed
        if !exec.node_properties.contains_key("position") {
            exec.node_properties
                .insert("position".into(), IrValue::Vec3([0.0, 0.0, 0.0]));
        }

        // Bind parameters: if the method takes 'node' as first param, bind it
        let mut arg_idx = 0;
        for (i, param) in func.params.iter().enumerate() {
            if i == 0 && (param == "node" || param == "_node") {
                // First param represents the node itself
                exec.locals.insert(
                    param.clone(),
                    IrValue::String(node.name.clone()),
                );
            } else if arg_idx < args.len() {
                exec.locals.insert(param.clone(), args[arg_idx].clone());
                arg_idx += 1;
            } else {
                exec.locals.insert(param.clone(), IrValue::Nil);
            }
        }

        let ret = exec.run_block(&func.body)?;

        // Synchronize back instance state & node properties
        self.instance_state = exec.instance_state;
        node.properties = exec.node_properties;

        Ok(JsExecutionResult {
            return_value: ret,
            logs: exec.logs,
            steps_executed: exec.steps_taken,
        })
    }

    /// Evaluates a standalone TS/JS expression against a node.
    pub fn eval_expression(&self, expr_str: &str, node: &IrNode) -> Result<IrValue, JsEvalError> {
        let cleaned = strip_ts_types(expr_str.trim().trim_end_matches(';'));
        let expr = parse_expression(&cleaned)?;
        let mut exec = ExecutionContext {
            budget: self.step_budget,
            steps_taken: 0,
            locals: BTreeMap::new(),
            instance_state: self.instance_state.clone(),
            node_properties: node.properties.clone(),
            logs: Vec::new(),
        };
        exec.eval_expr(&expr)
    }
}

struct ExecutionContext {
    budget: usize,
    steps_taken: usize,
    locals: BTreeMap<String, IrValue>,
    instance_state: BTreeMap<String, IrValue>,
    node_properties: BTreeMap<String, IrValue>,
    logs: Vec<String>,
}

impl ExecutionContext {
    fn tick(&mut self) -> Result<(), JsEvalError> {
        self.steps_taken += 1;
        if self.steps_taken > self.budget {
            Err(JsEvalError::BudgetExceeded {
                budget: self.budget,
            })
        } else {
            Ok(())
        }
    }

    fn run_block(&mut self, stmts: &[Stmt]) -> Result<Option<IrValue>, JsEvalError> {
        for stmt in stmts {
            self.tick()?;
            if let Some(val) = self.run_stmt(stmt)? {
                return Ok(Some(val));
            }
        }
        Ok(None)
    }

    fn run_stmt(&mut self, stmt: &Stmt) -> Result<Option<IrValue>, JsEvalError> {
        match stmt {
            Stmt::VarDecl { name, init } => {
                let val = match init {
                    Some(expr) => self.eval_expr(expr)?,
                    None => IrValue::Nil,
                };
                self.locals.insert(name.clone(), val);
                Ok(None)
            }
            Stmt::Assign { target, op, value } => {
                let rhs = self.eval_expr(value)?;
                self.apply_assign(target, op, rhs)?;
                Ok(None)
            }
            Stmt::Expr(expr) => {
                self.eval_expr(expr)?;
                Ok(None)
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let cond_val = self.eval_expr(condition)?;
                if is_truthy(&cond_val) {
                    self.run_block(then_branch)
                } else if let Some(else_b) = else_branch {
                    self.run_block(else_b)
                } else {
                    Ok(None)
                }
            }
            Stmt::While { condition, body } => {
                loop {
                    self.tick()?;
                    let cond_val = self.eval_expr(condition)?;
                    if !is_truthy(&cond_val) {
                        break;
                    }
                    if let Some(ret) = self.run_block(body)? {
                        return Ok(Some(ret));
                    }
                }
                Ok(None)
            }
            Stmt::For {
                init,
                condition,
                step,
                body,
            } => {
                if let Some(init_stmt) = init {
                    self.run_stmt(init_stmt)?;
                }
                loop {
                    self.tick()?;
                    if let Some(cond) = condition {
                        let c = self.eval_expr(cond)?;
                        if !is_truthy(&c) {
                            break;
                        }
                    }
                    if let Some(ret) = self.run_block(body)? {
                        return Ok(Some(ret));
                    }
                    if let Some(step_stmt) = step {
                        self.run_stmt(step_stmt)?;
                    }
                }
                Ok(None)
            }
            Stmt::Return(expr) => {
                let val = match expr {
                    Some(e) => self.eval_expr(e)?,
                    None => IrValue::Nil,
                };
                Ok(Some(val))
            }
        }
    }

    fn apply_assign(
        &mut self,
        target: &AssignTarget,
        op: &AssignOp,
        rhs: IrValue,
    ) -> Result<(), JsEvalError> {
        match target {
            AssignTarget::Variable(name) => {
                let cur = if let Some(v) = self.locals.get(name) {
                    v.clone()
                } else if let Some(v) = self.instance_state.get(name) {
                    v.clone()
                } else if let Some(v) = self.node_properties.get(name) {
                    v.clone()
                } else {
                    IrValue::Nil
                };

                let new_val = self.compute_assign_op(&cur, op, rhs)?;
                if self.locals.contains_key(name) {
                    self.locals.insert(name.clone(), new_val);
                } else if self.instance_state.contains_key(name) {
                    self.instance_state.insert(name.clone(), new_val);
                } else {
                    self.locals.insert(name.clone(), new_val);
                }
            }
            AssignTarget::Property { base, prop } => {
                match &**base {
                    Expr::Identifier(id) if id == "this" => {
                        let cur = self
                            .instance_state
                            .get(prop)
                            .cloned()
                            .unwrap_or(IrValue::Nil);
                        let new_val = self.compute_assign_op(&cur, op, rhs)?;
                        self.instance_state.insert(prop.clone(), new_val);
                    }
                    Expr::Identifier(id) if id == "node" => {
                        let cur = self
                            .node_properties
                            .get(prop)
                            .cloned()
                            .unwrap_or(IrValue::Nil);
                        let new_val = self.compute_assign_op(&cur, op, rhs)?;
                        self.node_properties.insert(prop.clone(), new_val);
                    }
                    Expr::PropertyAccess {
                        base: inner_base,
                        prop: inner_prop,
                    } => {
                        // Nested property access e.g. node.position.x
                        if let Expr::Identifier(id) = &**inner_base
                            && id == "node"
                        {
                            let mut inner_val = self
                                .node_properties
                                .get(inner_prop)
                                .cloned()
                                .unwrap_or(IrValue::Vec3([0.0, 0.0, 0.0]));

                            let sub_val = get_prop_val(&inner_val, prop)?;
                            let calculated = self.compute_assign_op(&sub_val, op, rhs)?;
                            set_prop_val(&mut inner_val, prop, calculated)?;
                            self.node_properties.insert(inner_prop.clone(), inner_val);
                        } else {
                            return Err(JsEvalError::TypeError {
                                message: format!("unsupported nested property assignment on {base:?}"),
                            });
                        }
                    }
                    _ => {
                        return Err(JsEvalError::TypeError {
                            message: format!("cannot assign property on {base:?}"),
                        });
                    }
                }
            }
            AssignTarget::Index { base, index } => {
                let idx_val = self.eval_expr(index)?;
                let key_str = match idx_val {
                    IrValue::String(s) => s,
                    IrValue::I64(i) => i.to_string(),
                    _ => {
                        return Err(JsEvalError::TypeError {
                            message: "invalid index type".into(),
                        });
                    }
                };

                match &**base {
                    Expr::PropertyAccess {
                        base: inner_base,
                        prop,
                    } if prop == "properties" => {
                        if let Expr::Identifier(id) = &**inner_base
                            && id == "node"
                        {
                            let cur = self
                                .node_properties
                                .get(&key_str)
                                .cloned()
                                .unwrap_or(IrValue::Nil);
                            let new_val = self.compute_assign_op(&cur, op, rhs)?;
                            self.node_properties.insert(key_str, new_val);
                        }
                    }
                    Expr::Identifier(id) if id == "node" => {
                        let cur = self
                            .node_properties
                            .get(&key_str)
                            .cloned()
                            .unwrap_or(IrValue::Nil);
                        let new_val = self.compute_assign_op(&cur, op, rhs)?;
                        self.node_properties.insert(key_str, new_val);
                    }
                    _ => {
                        return Err(JsEvalError::TypeError {
                            message: "unsupported indexed assignment".into(),
                        });
                    }
                }
            }
        }
        Ok(())
    }

    fn compute_assign_op(
        &self,
        current: &IrValue,
        op: &AssignOp,
        rhs: IrValue,
    ) -> Result<IrValue, JsEvalError> {
        match op {
            AssignOp::Assign => Ok(rhs),
            AssignOp::AddAssign => eval_bin_op(current, &BinOp::Add, &rhs),
            AssignOp::SubAssign => eval_bin_op(current, &BinOp::Sub, &rhs),
            AssignOp::MulAssign => eval_bin_op(current, &BinOp::Mul, &rhs),
            AssignOp::DivAssign => eval_bin_op(current, &BinOp::Div, &rhs),
        }
    }

    fn eval_expr(&mut self, expr: &Expr) -> Result<IrValue, JsEvalError> {
        match expr {
            Expr::Literal(val) => Ok(val.clone()),
            Expr::Identifier(name) => {
                if name == "undefined" || name == "null" {
                    return Ok(IrValue::Nil);
                }
                if name == "true" {
                    return Ok(IrValue::Bool(true));
                }
                if name == "false" {
                    return Ok(IrValue::Bool(false));
                }
                if name == "Math" {
                    return Ok(IrValue::String("Math".into()));
                }
                if let Some(val) = self.locals.get(name) {
                    return Ok(val.clone());
                }
                if let Some(val) = self.instance_state.get(name) {
                    return Ok(val.clone());
                }
                if let Some(val) = self.node_properties.get(name) {
                    return Ok(val.clone());
                }
                Err(JsEvalError::VariableNotFound { name: name.clone() })
            }
            Expr::Unary { op, expr } => {
                let inner = self.eval_expr(expr)?;
                match op {
                    UnaryOp::Neg => match inner {
                        IrValue::I64(i) => Ok(IrValue::I64(-i)),
                        IrValue::F64(f) => Ok(IrValue::F64(-f)),
                        _ => Err(JsEvalError::TypeError {
                            message: "unary minus requires numeric value".into(),
                        }),
                    },
                    UnaryOp::Not => Ok(IrValue::Bool(!is_truthy(&inner))),
                }
            }
            Expr::Binary { left, op, right } => {
                let l = self.eval_expr(left)?;
                let r = self.eval_expr(right)?;
                eval_bin_op(&l, op, &r)
            }
            Expr::PropertyAccess { base, prop } => {
                if let Expr::Identifier(id) = &**base {
                    if id == "this" {
                        return Ok(self
                            .instance_state
                            .get(prop)
                            .cloned()
                            .unwrap_or(IrValue::Nil));
                    }
                    if id == "node" {
                        return Ok(self
                            .node_properties
                            .get(prop)
                            .cloned()
                            .unwrap_or(IrValue::Nil));
                    }
                    if id == "Math" && prop == "PI" {
                        return Ok(IrValue::F64(std::f64::consts::PI));
                    }
                }

                let base_val = self.eval_expr(base)?;
                get_prop_val(&base_val, prop)
            }
            Expr::IndexAccess { base, index } => {
                let base_val = self.eval_expr(base)?;
                let idx_val = self.eval_expr(index)?;
                match (base_val, idx_val) {
                    (IrValue::Array(arr), IrValue::I64(i)) => {
                        let idx = i as usize;
                        Ok(arr.get(idx).cloned().unwrap_or(IrValue::Nil))
                    }
                    (IrValue::Map(map), IrValue::String(k)) => {
                        Ok(map.get(&k).cloned().unwrap_or(IrValue::Nil))
                    }
                    _ => Err(JsEvalError::TypeError {
                        message: "invalid index access".into(),
                    }),
                }
            }
            Expr::Call { callee, args } => {
                let evaluated_args: Vec<IrValue> = args
                    .iter()
                    .map(|a| self.eval_expr(a))
                    .collect::<Result<_, _>>()?;

                self.eval_call(callee, &evaluated_args)
            }
            Expr::MethodCall { base, method, args } => {
                let evaluated_args: Vec<IrValue> = args
                    .iter()
                    .map(|a| self.eval_expr(a))
                    .collect::<Result<_, _>>()?;

                if let Expr::Identifier(id) = &**base {
                    if id == "console" && method == "log" {
                        let log_str = evaluated_args
                            .iter()
                            .map(ir_val_to_display_string)
                            .collect::<Vec<_>>()
                            .join(" ");
                        self.logs.push(log_str);
                        return Ok(IrValue::Nil);
                    }
                    if id == "Math" {
                        return eval_math_func(method, &evaluated_args);
                    }
                }

                let base_val = self.eval_expr(base)?;
                match method.as_str() {
                    "toString" => Ok(IrValue::String(ir_val_to_display_string(&base_val))),
                    "toFixed" => {
                        let precision = evaluated_args
                            .first()
                            .and_then(|v| match v {
                                IrValue::I64(i) => Some(*i as usize),
                                _ => None,
                            })
                            .unwrap_or(0);
                        let f = match base_val {
                            IrValue::F64(v) => v,
                            IrValue::I64(v) => v as f64,
                            _ => 0.0,
                        };
                        Ok(IrValue::String(format!("{f:.precision$}")))
                    }
                    _ => Err(JsEvalError::MethodNotFound {
                        name: method.clone(),
                    }),
                }
            }
            Expr::Array(elements) => {
                let vals = elements
                    .iter()
                    .map(|e| self.eval_expr(e))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(IrValue::Array(vals))
            }
            Expr::Object(entries) => {
                let mut map = BTreeMap::new();
                for (k, v) in entries {
                    map.insert(k.clone(), self.eval_expr(v)?);
                }
                Ok(IrValue::Map(map))
            }
        }
    }

    fn eval_call(&mut self, callee: &str, args: &[IrValue]) -> Result<IrValue, JsEvalError> {
        match callee {
            "Vector2" | "vec2" => {
                let x = args.first().and_then(val_to_f32).unwrap_or(0.0);
                let y = args.get(1).and_then(val_to_f32).unwrap_or(0.0);
                Ok(IrValue::Vec2([x, y]))
            }
            "Vector3" | "vec3" => {
                let x = args.first().and_then(val_to_f32).unwrap_or(0.0);
                let y = args.get(1).and_then(val_to_f32).unwrap_or(0.0);
                let z = args.get(2).and_then(val_to_f32).unwrap_or(0.0);
                Ok(IrValue::Vec3([x, y, z]))
            }
            "clamp" => {
                if args.len() < 3 {
                    return Err(JsEvalError::TypeError {
                        message: "clamp requires 3 arguments (val, min, max)".into(),
                    });
                }
                let val = val_to_f64(&args[0])?;
                let min = val_to_f64(&args[1])?;
                let max = val_to_f64(&args[2])?;
                Ok(IrValue::F64(val.clamp(min, max)))
            }
            "lerp" => {
                if args.len() < 3 {
                    return Err(JsEvalError::TypeError {
                        message: "lerp requires 3 arguments (from, to, weight)".into(),
                    });
                }
                let a = val_to_f64(&args[0])?;
                let b = val_to_f64(&args[1])?;
                let t = val_to_f64(&args[2])?;
                Ok(IrValue::F64(a + (b - a) * t))
            }
            "print" => {
                let log_str = args
                    .iter()
                    .map(ir_val_to_display_string)
                    .collect::<Vec<_>>()
                    .join(" ");
                self.logs.push(log_str);
                Ok(IrValue::Nil)
            }
            _ => Err(JsEvalError::MethodNotFound {
                name: callee.to_string(),
            }),
        }
    }
}

fn eval_math_func(method: &str, args: &[IrValue]) -> Result<IrValue, JsEvalError> {
    match method {
        "abs" => {
            let val = val_to_f64(args.first().unwrap_or(&IrValue::F64(0.0)))?;
            Ok(IrValue::F64(val.abs()))
        }
        "sin" => {
            let val = val_to_f64(args.first().unwrap_or(&IrValue::F64(0.0)))?;
            Ok(IrValue::F64(val.sin()))
        }
        "cos" => {
            let val = val_to_f64(args.first().unwrap_or(&IrValue::F64(0.0)))?;
            Ok(IrValue::F64(val.cos()))
        }
        "sqrt" => {
            let val = val_to_f64(args.first().unwrap_or(&IrValue::F64(0.0)))?;
            Ok(IrValue::F64(val.sqrt()))
        }
        "floor" => {
            let val = val_to_f64(args.first().unwrap_or(&IrValue::F64(0.0)))?;
            Ok(IrValue::F64(val.floor()))
        }
        "ceil" => {
            let val = val_to_f64(args.first().unwrap_or(&IrValue::F64(0.0)))?;
            Ok(IrValue::F64(val.ceil()))
        }
        "round" => {
            let val = val_to_f64(args.first().unwrap_or(&IrValue::F64(0.0)))?;
            Ok(IrValue::F64(val.round()))
        }
        "min" => {
            let a = val_to_f64(args.first().unwrap_or(&IrValue::F64(0.0)))?;
            let b = val_to_f64(args.get(1).unwrap_or(&IrValue::F64(0.0)))?;
            Ok(IrValue::F64(a.min(b)))
        }
        "max" => {
            let a = val_to_f64(args.first().unwrap_or(&IrValue::F64(0.0)))?;
            let b = val_to_f64(args.get(1).unwrap_or(&IrValue::F64(0.0)))?;
            Ok(IrValue::F64(a.max(b)))
        }
        _ => Err(JsEvalError::MethodNotFound {
            name: format!("Math.{method}"),
        }),
    }
}

fn get_prop_val(val: &IrValue, prop: &str) -> Result<IrValue, JsEvalError> {
    match val {
        IrValue::Vec2(v) => match prop {
            "x" => Ok(IrValue::F64(v[0] as f64)),
            "y" => Ok(IrValue::F64(v[1] as f64)),
            _ => Err(JsEvalError::TypeError {
                message: format!("Vector2 has no property '{prop}'"),
            }),
        },
        IrValue::Vec3(v) => match prop {
            "x" => Ok(IrValue::F64(v[0] as f64)),
            "y" => Ok(IrValue::F64(v[1] as f64)),
            "z" => Ok(IrValue::F64(v[2] as f64)),
            _ => Err(JsEvalError::TypeError {
                message: format!("Vector3 has no property '{prop}'"),
            }),
        },
        IrValue::Map(map) => Ok(map.get(prop).cloned().unwrap_or(IrValue::Nil)),
        _ => Err(JsEvalError::TypeError {
            message: format!("cannot access property '{prop}' on non-object {val:?}"),
        }),
    }
}

fn set_prop_val(target: &mut IrValue, prop: &str, new_val: IrValue) -> Result<(), JsEvalError> {
    let f = val_to_f32(&new_val).unwrap_or(0.0);
    match target {
        IrValue::Vec2(v) => match prop {
            "x" => v[0] = f,
            "y" => v[1] = f,
            _ => {
                return Err(JsEvalError::TypeError {
                    message: format!("cannot set property '{prop}' on Vector2"),
                });
            }
        },
        IrValue::Vec3(v) => match prop {
            "x" => v[0] = f,
            "y" => v[1] = f,
            "z" => v[2] = f,
            _ => {
                return Err(JsEvalError::TypeError {
                    message: format!("cannot set property '{prop}' on Vector3"),
                });
            }
        },
        IrValue::Map(map) => {
            map.insert(prop.to_string(), new_val);
        }
        _ => {
            return Err(JsEvalError::TypeError {
                message: format!("cannot set property '{prop}' on non-object"),
            });
        }
    }
    Ok(())
}

fn val_to_f64(val: &IrValue) -> Result<f64, JsEvalError> {
    match val {
        IrValue::F64(f) => Ok(*f),
        IrValue::I64(i) => Ok(*i as f64),
        _ => Err(JsEvalError::TypeError {
            message: format!("expected number, got {val:?}"),
        }),
    }
}

fn val_to_f32(val: &IrValue) -> Option<f32> {
    match val {
        IrValue::F64(f) => Some(*f as f32),
        IrValue::I64(i) => Some(*i as f32),
        _ => None,
    }
}

fn is_truthy(val: &IrValue) -> bool {
    match val {
        IrValue::Nil => false,
        IrValue::Bool(b) => *b,
        IrValue::I64(i) => *i != 0,
        IrValue::F64(f) => *f != 0.0 && !f.is_nan(),
        IrValue::String(s) => !s.is_empty(),
        _ => true,
    }
}

fn ir_val_to_display_string(val: &IrValue) -> String {
    match val {
        IrValue::Nil => "null".to_string(),
        IrValue::Bool(b) => b.to_string(),
        IrValue::I64(i) => i.to_string(),
        IrValue::F64(f) => f.to_string(),
        IrValue::String(s) => s.clone(),
        IrValue::Vec2(v) => format!("Vector2({}, {})", v[0], v[1]),
        IrValue::Vec3(v) => format!("Vector3({}, {}, {})", v[0], v[1], v[2]),
        IrValue::Quat(q) => format!("Quat({}, {}, {}, {})", q[0], q[1], q[2], q[3]),
        IrValue::Color(c) => format!("Color({}, {}, {}, {})", c[0], c[1], c[2], c[3]),
        IrValue::NodePath(p) => format!("NodePath(\"{p}\")"),
        IrValue::ResourcePath(p) => format!("ResourcePath(\"{p}\")"),
        IrValue::Array(arr) => {
            let inner = arr
                .iter()
                .map(ir_val_to_display_string)
                .collect::<Vec<_>>()
                .join(", ");
            format!("[{inner}]")
        }
        IrValue::Map(map) => {
            let inner = map
                .iter()
                .map(|(k, v)| format!("{k}: {}", ir_val_to_display_string(v)))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{{{inner}}}")
        }
    }
}

fn eval_bin_op(l: &IrValue, op: &BinOp, r: &IrValue) -> Result<IrValue, JsEvalError> {
    match op {
        BinOp::Add => match (l, r) {
            (IrValue::I64(a), IrValue::I64(b)) => Ok(IrValue::I64(a + b)),
            (IrValue::I64(a), IrValue::F64(b)) => Ok(IrValue::F64(*a as f64 + b)),
            (IrValue::F64(a), IrValue::I64(b)) => Ok(IrValue::F64(a + *b as f64)),
            (IrValue::F64(a), IrValue::F64(b)) => Ok(IrValue::F64(a + b)),
            (IrValue::String(a), IrValue::String(b)) => Ok(IrValue::String(format!("{a}{b}"))),
            (IrValue::String(a), other) => {
                Ok(IrValue::String(format!("{a}{}", ir_val_to_display_string(other))))
            }
            (other, IrValue::String(b)) => {
                Ok(IrValue::String(format!("{}{b}", ir_val_to_display_string(other))))
            }
            _ => Err(JsEvalError::TypeError {
                message: format!("cannot add {l:?} and {r:?}"),
            }),
        },
        BinOp::Sub => match (l, r) {
            (IrValue::I64(a), IrValue::I64(b)) => Ok(IrValue::I64(a - b)),
            (IrValue::I64(a), IrValue::F64(b)) => Ok(IrValue::F64(*a as f64 - b)),
            (IrValue::F64(a), IrValue::I64(b)) => Ok(IrValue::F64(a - *b as f64)),
            (IrValue::F64(a), IrValue::F64(b)) => Ok(IrValue::F64(a - b)),
            _ => Err(JsEvalError::TypeError {
                message: format!("cannot subtract {l:?} and {r:?}"),
            }),
        },
        BinOp::Mul => match (l, r) {
            (IrValue::I64(a), IrValue::I64(b)) => Ok(IrValue::I64(a * b)),
            (IrValue::I64(a), IrValue::F64(b)) => Ok(IrValue::F64(*a as f64 * b)),
            (IrValue::F64(a), IrValue::I64(b)) => Ok(IrValue::F64(a * *b as f64)),
            (IrValue::F64(a), IrValue::F64(b)) => Ok(IrValue::F64(a * b)),
            _ => Err(JsEvalError::TypeError {
                message: format!("cannot multiply {l:?} and {r:?}"),
            }),
        },
        BinOp::Div => match (l, r) {
            (IrValue::I64(a), IrValue::I64(b)) => {
                if *b == 0 {
                    Err(JsEvalError::DivisionByZero)
                } else {
                    Ok(IrValue::I64(a / b))
                }
            }
            (IrValue::I64(a), IrValue::F64(b)) => {
                if *b == 0.0 {
                    Err(JsEvalError::DivisionByZero)
                } else {
                    Ok(IrValue::F64(*a as f64 / b))
                }
            }
            (IrValue::F64(a), IrValue::I64(b)) => {
                if *b == 0 {
                    Err(JsEvalError::DivisionByZero)
                } else {
                    Ok(IrValue::F64(a / *b as f64))
                }
            }
            (IrValue::F64(a), IrValue::F64(b)) => {
                if *b == 0.0 {
                    Err(JsEvalError::DivisionByZero)
                } else {
                    Ok(IrValue::F64(a / b))
                }
            }
            _ => Err(JsEvalError::TypeError {
                message: format!("cannot divide {l:?} and {r:?}"),
            }),
        },
        BinOp::Mod => match (l, r) {
            (IrValue::I64(a), IrValue::I64(b)) => {
                if *b == 0 {
                    Err(JsEvalError::DivisionByZero)
                } else {
                    Ok(IrValue::I64(a % b))
                }
            }
            _ => Err(JsEvalError::TypeError {
                message: format!("cannot modulo {l:?} and {r:?}"),
            }),
        },
        BinOp::Eq => Ok(IrValue::Bool(l == r)),
        BinOp::Ne => Ok(IrValue::Bool(l != r)),
        BinOp::Lt => compare_vals(l, r).map(|c| IrValue::Bool(c < 0)),
        BinOp::Le => compare_vals(l, r).map(|c| IrValue::Bool(c <= 0)),
        BinOp::Gt => compare_vals(l, r).map(|c| IrValue::Bool(c > 0)),
        BinOp::Ge => compare_vals(l, r).map(|c| IrValue::Bool(c >= 0)),
        BinOp::And => Ok(IrValue::Bool(is_truthy(l) && is_truthy(r))),
        BinOp::Or => Ok(IrValue::Bool(is_truthy(l) || is_truthy(r))),
    }
}

fn compare_vals(l: &IrValue, r: &IrValue) -> Result<i32, JsEvalError> {
    match (l, r) {
        (IrValue::I64(a), IrValue::I64(b)) => Ok(a.cmp(b) as i32),
        (IrValue::I64(a), IrValue::F64(b)) => {
            let fa = *a as f64;
            if fa < *b {
                Ok(-1)
            } else if fa > *b {
                Ok(1)
            } else {
                Ok(0)
            }
        }
        (IrValue::F64(a), IrValue::I64(b)) => {
            let fb = *b as f64;
            if *a < fb {
                Ok(-1)
            } else if *a > fb {
                Ok(1)
            } else {
                Ok(0)
            }
        }
        (IrValue::F64(a), IrValue::F64(b)) => {
            if *a < *b {
                Ok(-1)
            } else if *a > *b {
                Ok(1)
            } else {
                Ok(0)
            }
        }
        (IrValue::String(a), IrValue::String(b)) => Ok(a.cmp(b) as i32),
        _ => Err(JsEvalError::TypeError {
            message: format!("cannot compare {l:?} and {r:?}"),
        }),
    }
}

// ---------------------------------------------------------------------------
// TypeScript / JavaScript Parser
// ---------------------------------------------------------------------------

fn strip_ts_types(line: &str) -> String {
    // Basic type annotation cleaner: replaces `: Type` with empty when not in strings or object literals
    // e.g. "let x: number = 5;" -> "let x = 5;"
    // e.g. "speed: number = 10;" -> "speed = 10;"
    // e.g. "onUpdate(node: IrNode, dt: number)" -> "onUpdate(node, dt)"
    let mut out = String::with_capacity(line.len());
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    let mut in_str = None;

    while i < chars.len() {
        let c = chars[i];
        if in_str.is_some() {
            if Some(c) == in_str && (i == 0 || chars[i - 1] != '\\') {
                in_str = None;
            }
            out.push(c);
            i += 1;
            continue;
        }

        if c == '\'' || c == '"' || c == '`' {
            in_str = Some(c);
            out.push(c);
            i += 1;
            continue;
        }

        // Check for `: type`
        if c == ':' {
            // Peek forward to see if this is a type annotation (e.g. `: number`, `: string`, `: any`, `: IrNode`)
            let mut j = i + 1;
            while j < chars.len() && chars[j].is_whitespace() {
                j += 1;
            }
            let type_start = j;
            while j < chars.len()
                && (chars[j].is_alphanumeric()
                    || chars[j] == '_'
                    || chars[j] == '['
                    || chars[j] == ']'
                    || chars[j] == '<'
                    || chars[j] == '>')
            {
                j += 1;
            }
            let type_token: String = chars[type_start..j].iter().collect();
            // If recognized TS primitive or common type followed by '=', ',', ')', ';', '{'
            let is_type = matches!(
                type_token.as_str(),
                "number"
                    | "string"
                    | "boolean"
                    | "any"
                    | "void"
                    | "IrNode"
                    | "Record"
                    | "number[]"
                    | "string[]"
            ) || type_token.ends_with("[]");

            if is_type {
                i = j;
                continue;
            }
        }

        out.push(c);
        i += 1;
    }
    out
}

type ParsedModule = (BTreeMap<String, JsFunction>, BTreeMap<String, IrValue>);

fn parse_ts_source(source: &str) -> Result<ParsedModule, JsEvalError> {
    let mut functions = BTreeMap::new();
    let mut initial_state = BTreeMap::new();

    // Remove comments
    let lines: Vec<&str> = source.lines().collect();
    let mut clean_lines = Vec::new();
    let mut in_multiline_comment = false;

    for line in lines {
        let trimmed = line.trim();
        if in_multiline_comment {
            if let Some(pos) = trimmed.find("*/") {
                in_multiline_comment = false;
                clean_lines.push(&trimmed[pos + 2..]);
            }
            continue;
        }
        if trimmed.starts_with("/*") {
            if !trimmed.contains("*/") {
                in_multiline_comment = true;
            }
            continue;
        }
        if trimmed.starts_with("//") {
            continue;
        }
        // Remove trailing // comment if not inside string
        let clean = if let Some(idx) = trimmed.find("//") {
            &trimmed[..idx]
        } else {
            trimmed
        };
        if !clean.is_empty() {
            clean_lines.push(clean);
        }
    }

    let code = clean_lines.join("\n");

    // Scan for class body or top-level functions and variables
    let mut cursor = 0;
    let chars: Vec<char> = code.chars().collect();

    while cursor < chars.len() {
        while cursor < chars.len() && chars[cursor].is_whitespace() {
            cursor += 1;
        }
        if cursor >= chars.len() {
            break;
        }

        // Check for export class Foo { ... } or class Foo { ... }
        let remaining: String = chars[cursor..].iter().collect();
        if remaining.starts_with("export default class")
            || remaining.starts_with("export class")
            || remaining.starts_with("class")
        {
            // Find opening '{'
            if let Some(brace_pos) = remaining.find('{') {
                let class_body_start = cursor + brace_pos + 1;
                // Find matching closing '}'
                let mut depth = 1;
                let mut body_end = class_body_start;
                while body_end < chars.len() && depth > 0 {
                    if chars[body_end] == '{' {
                        depth += 1;
                    } else if chars[body_end] == '}' {
                        depth -= 1;
                    }
                    body_end += 1;
                }
                let class_body: String = chars[class_body_start..body_end.saturating_sub(1)]
                    .iter()
                    .collect();
                parse_class_body(&class_body, &mut functions, &mut initial_state)?;
                cursor = body_end;
                continue;
            }
        }

        // Check for top-level function e.g. function foo(args) { ... } or export function
        if remaining.starts_with("function ")
            || remaining.starts_with("export function ")
            || remaining.starts_with("export const ")
        {
            // parse function
            let (func, next_cursor) = parse_single_function(&chars, cursor)?;
            if let Some(f) = func {
                functions.insert(f.name.clone(), f);
            }
            cursor = next_cursor;
            continue;
        }

        // Top level property/variable: let/const/var x = ...;
        if (remaining.starts_with("let ")
            || remaining.starts_with("const ")
            || remaining.starts_with("var "))
            && let Some(semi) = remaining.find(';')
        {
            let decl_str = &remaining[..semi];
            let cleaned = strip_ts_types(decl_str);
            if let Some((k, v)) = parse_var_declaration(&cleaned)? {
                initial_state.insert(k, v);
            }
            cursor += semi + 1;
            continue;
        }

        cursor += 1;
    }

    Ok((functions, initial_state))
}

fn parse_class_body(
    body: &str,
    functions: &mut BTreeMap<String, JsFunction>,
    initial_state: &mut BTreeMap<String, IrValue>,
) -> Result<(), JsEvalError> {
    let chars: Vec<char> = body.chars().collect();
    let mut cursor = 0;

    while cursor < chars.len() {
        while cursor < chars.len() && chars[cursor].is_whitespace() {
            cursor += 1;
        }
        if cursor >= chars.len() {
            break;
        }

        let remaining: String = chars[cursor..].iter().collect();

        // Check for class property declaration first if semicolon precedes opening parenthesis
        let semi_pos = remaining.find(';');
        let paren_pos = remaining.find('(');

        if let Some(semi) = semi_pos
            && paren_pos.is_none_or(|p| semi < p)
        {
            let decl_str = &remaining[..semi];
            let cleaned = strip_ts_types(decl_str);
            if let Some((k, v)) = parse_var_declaration(&cleaned)? {
                initial_state.insert(k, v);
            }
            cursor += semi + 1;
            continue;
        }

        // Check for class method e.g. "onReady(node) {" or "public onUpdate(node, dt) {"
        if let Some(paren_pos) = paren_pos {
            let prefix = remaining[..paren_pos].trim();
            // Prefix should be method name or optional keyword + method name
            let tokens: Vec<&str> = prefix.split_whitespace().collect();
            let method_name = tokens.last().copied().unwrap_or("");
            if is_valid_ident(method_name) && !tokens.contains(&"if") && !tokens.contains(&"while")
            {
                // Find closing ')'
                if let Some(close_paren) = remaining[paren_pos..].find(')') {
                    let actual_close = paren_pos + close_paren;
                    let params_str = &remaining[paren_pos + 1..actual_close];
                    let params = parse_param_list(params_str);

                    // Find opening '{' of method
                    if let Some(open_brace) = remaining[actual_close..].find('{') {
                        let method_body_start = cursor + actual_close + open_brace + 1;
                        let mut depth = 1;
                        let mut body_end = method_body_start;
                        while body_end < chars.len() && depth > 0 {
                            if chars[body_end] == '{' {
                                depth += 1;
                            } else if chars[body_end] == '}' {
                                depth -= 1;
                            }
                            body_end += 1;
                        }
                        let method_body_str: String = chars
                            [method_body_start..body_end.saturating_sub(1)]
                            .iter()
                            .collect();
                        let body_stmts = parse_statements(&method_body_str)?;
                        functions.insert(
                            method_name.to_string(),
                            JsFunction {
                                name: method_name.to_string(),
                                params,
                                body: body_stmts,
                            },
                        );
                        cursor = body_end;
                        continue;
                    }
                }
            }
        }

        cursor += 1;
    }

    Ok(())
}

fn parse_single_function(
    chars: &[char],
    start: usize,
) -> Result<(Option<JsFunction>, usize), JsEvalError> {
    let remaining: String = chars[start..].iter().collect();
    let fn_keyword = if remaining.starts_with("export function ") {
        "export function "
    } else if remaining.starts_with("function ") {
        "function "
    } else {
        return Ok((None, start + 1));
    };

    let after_kw = &remaining[fn_keyword.len()..];
    let paren_pos = after_kw
        .find('(')
        .ok_or_else(|| JsEvalError::ParseError {
            message: "expected '(' in function declaration".into(),
        })?;
    let name = after_kw[..paren_pos].trim().to_string();

    let close_paren = after_kw[paren_pos..]
        .find(')')
        .ok_or_else(|| JsEvalError::ParseError {
            message: "expected ')' in function declaration".into(),
        })?;
    let params_str = &after_kw[paren_pos + 1..paren_pos + close_paren];
    let params = parse_param_list(params_str);

    let actual_close = start + fn_keyword.len() + paren_pos + close_paren;
    let after_paren: String = chars[actual_close..].iter().collect();
    let open_brace = after_paren
        .find('{')
        .ok_or_else(|| JsEvalError::ParseError {
            message: "expected '{' starting function body".into(),
        })?;

    let body_start = actual_close + open_brace + 1;
    let mut depth = 1;
    let mut body_end = body_start;
    while body_end < chars.len() && depth > 0 {
        if chars[body_end] == '{' {
            depth += 1;
        } else if chars[body_end] == '}' {
            depth -= 1;
        }
        body_end += 1;
    }

    let body_str: String = chars[body_start..body_end.saturating_sub(1)]
        .iter()
        .collect();
    let body = parse_statements(&body_str)?;

    Ok((
        Some(JsFunction { name, params, body }),
        body_end,
    ))
}

fn parse_param_list(params_str: &str) -> Vec<String> {
    let cleaned = strip_ts_types(params_str);
    cleaned
        .split(',')
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

fn parse_var_declaration(decl: &str) -> Result<Option<(String, IrValue)>, JsEvalError> {
    let trimmed = decl
        .trim()
        .trim_start_matches("export ")
        .trim_start_matches("public ")
        .trim_start_matches("let ")
        .trim_start_matches("const ")
        .trim_start_matches("var ")
        .trim();

    if let Some((left, right)) = trimmed.split_once('=') {
        let name = left.trim();
        let val_expr = parse_expression(right.trim())?;
        let mut exec = ExecutionContext {
            budget: 100,
            steps_taken: 0,
            locals: BTreeMap::new(),
            instance_state: BTreeMap::new(),
            node_properties: BTreeMap::new(),
            logs: Vec::new(),
        };
        let evaluated = exec.eval_expr(&val_expr).unwrap_or(IrValue::Nil);
        if is_valid_ident(name) {
            return Ok(Some((name.to_string(), evaluated)));
        }
    }
    Ok(None)
}

fn parse_statements(body: &str) -> Result<Vec<Stmt>, JsEvalError> {
    let mut stmts = Vec::new();
    let chars: Vec<char> = body.chars().collect();
    let mut cursor = 0;

    while cursor < chars.len() {
        while cursor < chars.len() && (chars[cursor].is_whitespace() || chars[cursor] == ';') {
            cursor += 1;
        }
        if cursor >= chars.len() {
            break;
        }

        let remaining: String = chars[cursor..].iter().collect();

        // If statement: if (...) { ... } else { ... }
        if remaining.starts_with("if")
            && remaining[2..].starts_with(|c: char| c.is_whitespace() || c == '(')
        {
            let (if_stmt, next) = parse_if_stmt(&chars, cursor)?;
            stmts.push(if_stmt);
            cursor = next;
            continue;
        }

        // While loop: while (...) { ... }
        if remaining.starts_with("while")
            && remaining[5..].starts_with(|c: char| c.is_whitespace() || c == '(')
        {
            let (while_stmt, next) = parse_while_stmt(&chars, cursor)?;
            stmts.push(while_stmt);
            cursor = next;
            continue;
        }

        // For loop: for (...) { ... }
        if remaining.starts_with("for")
            && remaining[3..].starts_with(|c: char| c.is_whitespace() || c == '(')
        {
            let (for_stmt, next) = parse_for_stmt(&chars, cursor)?;
            stmts.push(for_stmt);
            cursor = next;
            continue;
        }

        // Single statement ending with ';' or '}'
        let mut end = cursor;
        let mut in_str = None;
        let mut paren_depth = 0;
        let mut brace_depth = 0;

        while end < chars.len() {
            let c = chars[end];
            if in_str.is_some() {
                if Some(c) == in_str && (end == 0 || chars[end - 1] != '\\') {
                    in_str = None;
                }
                end += 1;
                continue;
            }
            if c == '\'' || c == '"' || c == '`' {
                in_str = Some(c);
                end += 1;
                continue;
            }
            if c == '(' {
                paren_depth += 1;
            } else if c == ')' {
                paren_depth -= 1;
            } else if c == '{' {
                brace_depth += 1;
            } else if c == '}' {
                if brace_depth == 0 {
                    break;
                }
                brace_depth -= 1;
            } else if c == ';' && paren_depth == 0 && brace_depth == 0 {
                break;
            }
            end += 1;
        }

        let stmt_str: String = chars[cursor..end].iter().collect();
        let trimmed = stmt_str.trim();
        if !trimmed.is_empty() {
            let stmt = parse_single_stmt(trimmed)?;
            stmts.push(stmt);
        }

        cursor = end;
        if cursor < chars.len() && chars[cursor] == ';' {
            cursor += 1;
        }
    }

    Ok(stmts)
}

fn parse_if_stmt(chars: &[char], start: usize) -> Result<(Stmt, usize), JsEvalError> {
    let remaining: String = chars[start..].iter().collect();
    let paren_open = remaining
        .find('(')
        .ok_or_else(|| JsEvalError::ParseError {
            message: "expected '(' in if condition".into(),
        })?;
    let paren_close = remaining[paren_open..]
        .find(')')
        .ok_or_else(|| JsEvalError::ParseError {
            message: "expected ')' in if condition".into(),
        })?;
    let cond_str = &remaining[paren_open + 1..paren_open + paren_close];
    let condition = parse_expression(cond_str.trim())?;

    let after_paren = start + paren_open + paren_close + 1;
    let (then_branch, after_then) = parse_block_or_stmt(chars, after_paren)?;

    // Check for 'else'
    let mut check_else = after_then;
    while check_else < chars.len() && chars[check_else].is_whitespace() {
        check_else += 1;
    }

    let mut else_branch = None;
    let mut final_cursor = after_then;
    let rem_else: String = chars[check_else..].iter().collect();
    if rem_else.starts_with("else") {
        let after_else_kw = check_else + 4;
        let (else_b, after_else) = parse_block_or_stmt(chars, after_else_kw)?;
        else_branch = Some(else_b);
        final_cursor = after_else;
    }

    Ok((
        Stmt::If {
            condition,
            then_branch,
            else_branch,
        },
        final_cursor,
    ))
}

fn parse_while_stmt(chars: &[char], start: usize) -> Result<(Stmt, usize), JsEvalError> {
    let remaining: String = chars[start..].iter().collect();
    let paren_open = remaining
        .find('(')
        .ok_or_else(|| JsEvalError::ParseError {
            message: "expected '(' in while condition".into(),
        })?;
    let paren_close = remaining[paren_open..]
        .find(')')
        .ok_or_else(|| JsEvalError::ParseError {
            message: "expected ')' in while condition".into(),
        })?;
    let cond_str = &remaining[paren_open + 1..paren_open + paren_close];
    let condition = parse_expression(cond_str.trim())?;

    let after_paren = start + paren_open + paren_close + 1;
    let (body, after_body) = parse_block_or_stmt(chars, after_paren)?;

    Ok((Stmt::While { condition, body }, after_body))
}

fn parse_for_stmt(chars: &[char], start: usize) -> Result<(Stmt, usize), JsEvalError> {
    let remaining: String = chars[start..].iter().collect();
    let paren_open = remaining
        .find('(')
        .ok_or_else(|| JsEvalError::ParseError {
            message: "expected '(' in for loop".into(),
        })?;
    let paren_close = remaining[paren_open..]
        .find(')')
        .ok_or_else(|| JsEvalError::ParseError {
            message: "expected ')' in for loop".into(),
        })?;
    let header_str = &remaining[paren_open + 1..paren_open + paren_close];
    let parts: Vec<&str> = header_str.split(';').collect();

    let init = if !parts.is_empty() && !parts[0].trim().is_empty() {
        Some(Box::new(parse_single_stmt(parts[0].trim())?))
    } else {
        None
    };

    let condition = if parts.len() > 1 && !parts[1].trim().is_empty() {
        Some(parse_expression(parts[1].trim())?)
    } else {
        None
    };

    let step = if parts.len() > 2 && !parts[2].trim().is_empty() {
        Some(Box::new(parse_single_stmt(parts[2].trim())?))
    } else {
        None
    };

    let after_paren = start + paren_open + paren_close + 1;
    let (body, after_body) = parse_block_or_stmt(chars, after_paren)?;

    Ok((
        Stmt::For {
            init,
            condition,
            step,
            body,
        },
        after_body,
    ))
}

fn parse_block_or_stmt(chars: &[char], start: usize) -> Result<(Vec<Stmt>, usize), JsEvalError> {
    let mut cursor = start;
    while cursor < chars.len() && chars[cursor].is_whitespace() {
        cursor += 1;
    }
    if cursor >= chars.len() {
        return Ok((Vec::new(), cursor));
    }

    if chars[cursor] == '{' {
        let block_start = cursor + 1;
        let mut depth = 1;
        let mut end = block_start;
        while end < chars.len() && depth > 0 {
            if chars[end] == '{' {
                depth += 1;
            } else if chars[end] == '}' {
                depth -= 1;
            }
            end += 1;
        }
        let block_str: String = chars[block_start..end.saturating_sub(1)].iter().collect();
        let stmts = parse_statements(&block_str)?;
        Ok((stmts, end))
    } else {
        // Single statement without braces
        let mut end = cursor;
        while end < chars.len() && chars[end] != ';' && chars[end] != '}' {
            end += 1;
        }
        let stmt_str: String = chars[cursor..end].iter().collect();
        let stmt = parse_single_stmt(stmt_str.trim())?;
        if end < chars.len() && chars[end] == ';' {
            end += 1;
        }
        Ok((vec![stmt], end))
    }
}

fn parse_single_stmt(s: &str) -> Result<Stmt, JsEvalError> {
    let cleaned = strip_ts_types(s.trim());

    if cleaned.starts_with("return")
        && (cleaned.len() == 6 || cleaned[6..].starts_with(char::is_whitespace))
    {
        let rest = cleaned[6..].trim();
        if rest.is_empty() {
            return Ok(Stmt::Return(None));
        }
        return Ok(Stmt::Return(Some(parse_expression(rest)?)));
    }

    if cleaned.starts_with("let ") || cleaned.starts_with("const ") || cleaned.starts_with("var ") {
        let decl = cleaned
            .trim_start_matches("let ")
            .trim_start_matches("const ")
            .trim_start_matches("var ")
            .trim();
        if let Some((left, right)) = decl.split_once('=') {
            let name = left.trim().to_string();
            let init = Some(parse_expression(right.trim())?);
            return Ok(Stmt::VarDecl { name, init });
        } else {
            return Ok(Stmt::VarDecl {
                name: decl.to_string(),
                init: None,
            });
        }
    }

    // Check for assignment: +=, -=, *=, /=, =
    // (Be careful not to confuse with == or <= or >=)
    let ops = [
        ("+=", AssignOp::AddAssign),
        ("-=", AssignOp::SubAssign),
        ("*=", AssignOp::MulAssign),
        ("/=", AssignOp::DivAssign),
        ("=", AssignOp::Assign),
    ];

    for (op_str, op_enum) in ops {
        if let Some(pos) = find_assign_op(&cleaned, op_str) {
            let left = cleaned[..pos].trim();
            let right = cleaned[pos + op_str.len()..].trim();
            let target = parse_assign_target(left)?;
            let value = parse_expression(right)?;
            return Ok(Stmt::Assign {
                target,
                op: op_enum,
                value,
            });
        }
    }

    // Otherwise it is an expression statement
    let expr = parse_expression(&cleaned)?;
    Ok(Stmt::Expr(expr))
}

fn find_assign_op(s: &str, op: &str) -> Option<usize> {
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    let mut in_str = None;

    while i < chars.len() {
        let c = chars[i];
        if in_str.is_some() {
            if Some(c) == in_str && (i == 0 || chars[i - 1] != '\\') {
                in_str = None;
            }
            i += 1;
            continue;
        }
        if c == '\'' || c == '"' || c == '`' {
            in_str = Some(c);
            i += 1;
            continue;
        }

        let rem: String = chars[i..].iter().collect();
        if rem.starts_with(op) {
            // Guard against ==, ===, <=, >=, !=
            if op == "=" {
                if i > 0 && (chars[i - 1] == '=' || chars[i - 1] == '!' || chars[i - 1] == '<' || chars[i - 1] == '>') {
                    i += 1;
                    continue;
                }
                if i + 1 < chars.len() && (chars[i + 1] == '=' || chars[i + 1] == '>') {
                    i += 1;
                    continue;
                }
            }
            return Some(i);
        }
        i += 1;
    }
    None
}

fn parse_assign_target(s: &str) -> Result<AssignTarget, JsEvalError> {
    let expr = parse_expression(s)?;
    match expr {
        Expr::Identifier(id) => Ok(AssignTarget::Variable(id)),
        Expr::PropertyAccess { base, prop } => Ok(AssignTarget::Property { base, prop }),
        Expr::IndexAccess { base, index } => Ok(AssignTarget::Index { base, index }),
        _ => Err(JsEvalError::ParseError {
            message: format!("invalid assignment target: {s}"),
        }),
    }
}

// ---------------------------------------------------------------------------
// Expression Parser
// ---------------------------------------------------------------------------

fn parse_expression(s: &str) -> Result<Expr, JsEvalError> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Err(JsEvalError::ParseError {
            message: "empty expression".into(),
        });
    }

    // Binary Logical OR
    if let Some(pos) = find_binary_split(trimmed, "||") {
        return Ok(Expr::Binary {
            left: Box::new(parse_expression(&trimmed[..pos])?),
            op: BinOp::Or,
            right: Box::new(parse_expression(&trimmed[pos + 2..])?),
        });
    }

    // Binary Logical AND
    if let Some(pos) = find_binary_split(trimmed, "&&") {
        return Ok(Expr::Binary {
            left: Box::new(parse_expression(&trimmed[..pos])?),
            op: BinOp::And,
            right: Box::new(parse_expression(&trimmed[pos + 2..])?),
        });
    }

    // Comparisons: ==, !=, <=, >=, <, >
    for (op_str, op) in [
        ("==", BinOp::Eq),
        ("!=", BinOp::Ne),
        ("<=", BinOp::Le),
        (">=", BinOp::Ge),
        ("<", BinOp::Lt),
        (">", BinOp::Gt),
    ] {
        if let Some(pos) = find_binary_split(trimmed, op_str) {
            return Ok(Expr::Binary {
                left: Box::new(parse_expression(&trimmed[..pos])?),
                op,
                right: Box::new(parse_expression(&trimmed[pos + op_str.len()..])?),
            });
        }
    }

    // Binary Add / Sub
    let chars: Vec<char> = trimmed.chars().collect();
    let mut level = 0;
    let mut in_str = None;
    let mut split_pos = None;
    let mut split_op = BinOp::Add;

    // Scan backwards for lowest precedence
    let mut idx = chars.len();
    while idx > 0 {
        idx -= 1;
        let c = chars[idx];
        if in_str.is_some() {
            if Some(c) == in_str && (idx == 0 || chars[idx - 1] != '\\') {
                in_str = None;
            }
            continue;
        }
        if c == '\'' || c == '"' || c == '`' {
            in_str = Some(c);
            continue;
        }
        if c == ')' || c == ']' || c == '}' {
            level += 1;
        } else if c == '(' || c == '[' || c == '{' {
            level -= 1;
        } else if level == 0 {
            if c == '+' {
                split_pos = Some(idx);
                split_op = BinOp::Add;
                break;
            }
            if c == '-' && idx > 0 {
                // Check if binary minus (not unary)
                let prev = chars[idx - 1];
                if prev.is_alphanumeric() || prev == ')' || prev == ']' || prev == '_' {
                    split_pos = Some(idx);
                    split_op = BinOp::Sub;
                    break;
                }
            }
        }
    }

    if let Some(pos) = split_pos {
        return Ok(Expr::Binary {
            left: Box::new(parse_expression(&trimmed[..pos])?),
            op: split_op,
            right: Box::new(parse_expression(&trimmed[pos + 1..])?),
        });
    }

    // Binary Mul / Div / Mod
    idx = chars.len();
    level = 0;
    in_str = None;
    split_pos = None;

    while idx > 0 {
        idx -= 1;
        let c = chars[idx];
        if in_str.is_some() {
            if Some(c) == in_str && (idx == 0 || chars[idx - 1] != '\\') {
                in_str = None;
            }
            continue;
        }
        if c == '\'' || c == '"' || c == '`' {
            in_str = Some(c);
            continue;
        }
        if c == ')' || c == ']' || c == '}' {
            level += 1;
        } else if c == '(' || c == '[' || c == '{' {
            level -= 1;
        } else if level == 0 && (c == '*' || c == '/' || c == '%') {
            split_pos = Some(idx);
            split_op = match c {
                '*' => BinOp::Mul,
                '/' => BinOp::Div,
                _ => BinOp::Mod,
            };
            break;
        }
    }

    if let Some(pos) = split_pos {
        return Ok(Expr::Binary {
            left: Box::new(parse_expression(&trimmed[..pos])?),
            op: split_op,
            right: Box::new(parse_expression(&trimmed[pos + 1..])?),
        });
    }

    // Unary prefix: -, !
    if let Some(rest) = trimmed.strip_prefix('!') {
        return Ok(Expr::Unary {
            op: UnaryOp::Not,
            expr: Box::new(parse_expression(rest)?),
        });
    }
    if !is_numeric_literal(trimmed)
        && let Some(rest) = trimmed.strip_prefix('-')
    {
        return Ok(Expr::Unary {
            op: UnaryOp::Neg,
            expr: Box::new(parse_expression(rest)?),
        });
    }

    // Parentheses (expr)
    if trimmed.starts_with('(') && trimmed.ends_with(')') {
        let inner = &trimmed[1..trimmed.len() - 1];
        if has_balanced_parens(inner) {
            return parse_expression(inner);
        }
    }

    // Array literal: [ ... ]
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        let inner = &trimmed[1..trimmed.len() - 1];
        if inner.trim().is_empty() {
            return Ok(Expr::Array(Vec::new()));
        }
        let items = split_by_comma(inner)?;
        let exprs = items
            .iter()
            .map(|s| parse_expression(s))
            .collect::<Result<_, _>>()?;
        return Ok(Expr::Array(exprs));
    }

    // Parse numeric literal before dot / property check
    if is_numeric_literal(trimmed) {
        if trimmed.contains('.') {
            if let Ok(f) = trimmed.parse::<f64>() {
                return Ok(Expr::Literal(IrValue::F64(f)));
            }
        } else if let Ok(i) = trimmed.parse::<i64>() {
            return Ok(Expr::Literal(IrValue::I64(i)));
        }
    }

    // String literal: '...' or "..." or `...`
    if (trimmed.starts_with('"') && trimmed.ends_with('"'))
        || (trimmed.starts_with('\'') && trimmed.ends_with('\''))
        || (trimmed.starts_with('`') && trimmed.ends_with('`'))
    {
        let content = &trimmed[1..trimmed.len() - 1];
        return Ok(Expr::Literal(IrValue::String(content.to_string())));
    }

    // Boolean / Null literals
    if trimmed == "true" {
        return Ok(Expr::Literal(IrValue::Bool(true)));
    }
    if trimmed == "false" {
        return Ok(Expr::Literal(IrValue::Bool(false)));
    }
    if trimmed == "null" || trimmed == "undefined" {
        return Ok(Expr::Literal(IrValue::Nil));
    }

    // Postfix operations: index access a[b] or method call a.foo(b) or prop access a.b
    if trimmed.ends_with(']') {
        // Find matching opening bracket
        let mut depth = 0;
        let mut bracket_pos = None;
        for (i, c) in chars.iter().enumerate().rev() {
            if *c == ']' {
                depth += 1;
            } else if *c == '[' {
                depth -= 1;
                if depth == 0 {
                    bracket_pos = Some(i);
                    break;
                }
            }
        }
        if let Some(pos) = bracket_pos
            && pos > 0
        {
            let base = &trimmed[..pos];
            let idx = &trimmed[pos + 1..trimmed.len() - 1];
            return Ok(Expr::IndexAccess {
                base: Box::new(parse_expression(base)?),
                index: Box::new(parse_expression(idx)?),
            });
        }
    }

    if trimmed.ends_with(')') {
        // Find matching opening paren for function/method call
        let mut depth = 0;
        let mut paren_pos = None;
        for (i, c) in chars.iter().enumerate().rev() {
            if *c == ')' {
                depth += 1;
            } else if *c == '(' {
                depth -= 1;
                if depth == 0 {
                    paren_pos = Some(i);
                    break;
                }
            }
        }
        if let Some(pos) = paren_pos {
            let callee_part = trimmed[..pos].trim();
            let args_part = &trimmed[pos + 1..trimmed.len() - 1];
            let raw_args = if args_part.trim().is_empty() {
                Vec::new()
            } else {
                split_by_comma(args_part)?
            };
            let args = raw_args
                .iter()
                .map(|s| parse_expression(s))
                .collect::<Result<Vec<_>, _>>()?;

            if let Some(dot_idx) = callee_part.rfind('.') {
                let base_str = &callee_part[..dot_idx];
                let method = &callee_part[dot_idx + 1..];
                if is_valid_ident(method) {
                    return Ok(Expr::MethodCall {
                        base: Box::new(parse_expression(base_str)?),
                        method: method.to_string(),
                        args,
                    });
                }
            } else if is_valid_ident(callee_part) {
                return Ok(Expr::Call {
                    callee: callee_part.to_string(),
                    args,
                });
            }
        }
    }

    // Property access: a.b (make sure 'b' is a valid identifier and 'a' is not a number)
    if let Some(dot_idx) = trimmed.rfind('.') {
        let prop = &trimmed[dot_idx + 1..];
        let base_str = &trimmed[..dot_idx];
        if is_valid_ident(prop) && !base_str.chars().all(|c| c.is_ascii_digit()) {
            return Ok(Expr::PropertyAccess {
                base: Box::new(parse_expression(base_str)?),
                prop: prop.to_string(),
            });
        }
    }

    // Plain Identifier
    if is_valid_ident(trimmed) {
        return Ok(Expr::Identifier(trimmed.to_string()));
    }

    Err(JsEvalError::ParseError {
        message: format!("unrecognized expression: '{trimmed}'"),
    })
}

fn find_binary_split(s: &str, op: &str) -> Option<usize> {
    let chars: Vec<char> = s.chars().collect();
    let mut level = 0;
    let mut in_str = None;
    let op_chars: Vec<char> = op.chars().collect();

    for i in 0..chars.len() {
        let c = chars[i];
        if in_str.is_some() {
            if Some(c) == in_str && (i == 0 || chars[i - 1] != '\\') {
                in_str = None;
            }
            continue;
        }
        if c == '\'' || c == '"' || c == '`' {
            in_str = Some(c);
            continue;
        }
        if c == '(' || c == '[' || c == '{' {
            level += 1;
        } else if c == ')' || c == ']' || c == '}' {
            level -= 1;
        } else if level == 0 && chars[i..].starts_with(&op_chars) {
            return Some(i);
        }
    }
    None
}

fn split_by_comma(s: &str) -> Result<Vec<&str>, JsEvalError> {
    let mut result = Vec::new();
    let chars: Vec<char> = s.chars().collect();
    let mut level = 0;
    let mut in_str = None;
    let mut start = 0;

    for i in 0..chars.len() {
        let c = chars[i];
        if in_str.is_some() {
            if Some(c) == in_str && (i == 0 || chars[i - 1] != '\\') {
                in_str = None;
            }
            continue;
        }
        if c == '\'' || c == '"' || c == '`' {
            in_str = Some(c);
            continue;
        }
        if c == '(' || c == '[' || c == '{' {
            level += 1;
        } else if c == ')' || c == ']' || c == '}' {
            level -= 1;
        } else if level == 0 && c == ',' {
            result.push(s[start..i].trim());
            start = i + 1;
        }
    }
    if start < s.len() {
        result.push(s[start..].trim());
    }
    Ok(result)
}

fn has_balanced_parens(s: &str) -> bool {
    let mut depth = 0;
    for c in s.chars() {
        if c == '(' {
            depth += 1;
        } else if c == ')' {
            depth -= 1;
            if depth < 0 {
                return false;
            }
        }
    }
    depth == 0
}

fn is_valid_ident(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let mut chars = s.chars();
    let first = chars.next().unwrap();
    if !first.is_alphabetic() && first != '_' && first != '$' {
        return false;
    }
    chars.all(|c| c.is_alphanumeric() || c == '_' || c == '$')
}

fn is_numeric_literal(s: &str) -> bool {
    let trimmed = s.trim().trim_start_matches('-').trim_start_matches('+');
    if trimmed.is_empty() {
        return false;
    }
    let mut dot_seen = false;
    for c in trimmed.chars() {
        if c == '.' {
            if dot_seen {
                return false;
            }
            dot_seen = true;
        } else if !c.is_ascii_digit() {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use foreplay_ir::NodeId;
    use std::num::NonZeroU32;

    fn make_test_node() -> IrNode {
        let mut props = BTreeMap::new();
        props.insert("speed".into(), IrValue::F64(10.0));
        props.insert("position".into(), IrValue::Vec3([0.0, 0.0, 0.0]));
        IrNode {
            id: NodeId {
                index: 1,
                generation: NonZeroU32::new(1).unwrap(),
            },
            name: "Player".into(),
            class_name: "CharacterBody3D".into(),
            parent: None,
            instance_scene: None,
            properties: props,
            groups: vec!["players".into()],
        }
    }

    #[test]
    fn ts_eval_class_lifecycle_ready_and_physics() {
        let ts_code = r#"
export class PlayerController {
    max_jump: number = 8.5;

    onReady(node: any) {
        node.health = 100;
        node.is_alive = true;
    }

    onPhysicsUpdate(node: any, dt: number) {
        node.position.y += 5.0 * dt;
        node.position.x += 2.0 * dt;
    }
}
"#;
        let mut runner = QuickJsComponentRunner::from_source("player_controller", ts_code).unwrap();
        let mut node = make_test_node();

        // Check initial state
        assert_eq!(
            runner.instance_state.get("max_jump"),
            Some(&IrValue::F64(8.5))
        );

        // Run onReady
        let res_ready = runner.call_on_ready(&mut node).unwrap();
        assert!(res_ready.steps_executed > 0);
        assert_eq!(node.properties.get("health"), Some(&IrValue::I64(100)));
        assert_eq!(node.properties.get("is_alive"), Some(&IrValue::Bool(true)));

        // Run onPhysicsUpdate
        let res_physics = runner.call_on_physics_update(&mut node, 0.1).unwrap();
        assert!(res_physics.steps_executed > 0);

        if let Some(IrValue::Vec3(pos)) = node.properties.get("position") {
            assert!((pos[0] - 0.2).abs() < 1e-4);
            assert!((pos[1] - 0.5).abs() < 1e-4);
        } else {
            panic!("expected Vec3 position");
        }
    }

    #[test]
    fn ts_eval_feel_knob_interop_in_update() {
        let ts_code = r#"
function onUpdate(node, dt, feel) {
    let accel = feel.skate_acceleration * dt;
    node.position.x += accel;
    console.log("Updated position x by", accel);
}
"#;
        let mut runner = QuickJsComponentRunner::from_source("feel_test", ts_code).unwrap();
        let mut node = make_test_node();

        let mut feel = BTreeMap::new();
        feel.insert("skate_acceleration".into(), 12.0);

        let res = runner.call_on_update(&mut node, 0.05, &feel).unwrap();
        assert_eq!(res.logs.len(), 1);
        assert!(res.logs[0].contains("Updated position x by 0.6"));

        if let Some(IrValue::Vec3(pos)) = node.properties.get("position") {
            assert!((pos[0] - 0.6).abs() < 1e-4);
        } else {
            panic!("expected Vec3 position");
        }
    }

    #[test]
    fn ts_eval_math_builtins_and_expressions() {
        let ts_code = r#"
function compute(node) {
    let s = Math.sin(0.0);
    let c = Math.cos(0.0);
    let m = Math.max(10, 20);
    return s + c + m;
}
"#;
        let mut runner = QuickJsComponentRunner::from_source("math_test", ts_code).unwrap();
        let mut node = make_test_node();

        let res = runner.call_method("compute", &mut node, &[]).unwrap();
        assert_eq!(res.return_value, Some(IrValue::F64(21.0)));

        // Evaluate expression directly
        let expr_res = runner
            .eval_expression("Math.min(100, 50) + 5", &node)
            .unwrap();
        assert_eq!(expr_res, IrValue::F64(55.0));
    }

    #[test]
    fn ts_eval_budget_guard_prevents_infinite_loops() {
        let ts_code = r#"
function loopForever(node) {
    let x = 0;
    while (true) {
        x += 1;
    }
}
"#;
        let mut runner = QuickJsComponentRunner::from_source("loop_test", ts_code)
            .unwrap()
            .with_step_budget(50);
        let mut node = make_test_node();

        let err = runner.call_method("loopForever", &mut node, &[]).unwrap_err();
        assert!(matches!(err, JsEvalError::BudgetExceeded { budget: 50 }));
    }
}
