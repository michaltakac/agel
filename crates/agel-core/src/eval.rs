use crate::agent::{Agent, AgentStatus, Event, EventKind, FailureAction, Protocol, TypeSpec};
use crate::macro_expander::{expand, ExpansionError, MacroDef};
use crate::model::{
    model_effect_key, ModelCompletion, ModelOutcome, ModelRecord, ModelRequest, ModelRequestStatus,
};
use crate::value::{Builtin, Capability, Closure, Env};
use crate::world::{EvaluationOptions, Module, State};
use crate::{Expr, Value};
use std::collections::BTreeMap;
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Condition {
    pub kind: String,
    pub message: String,
    pub data: Value,
}

impl Condition {
    fn new(kind: impl Into<String>, message: impl Into<String>, data: Value) -> Self {
        Self {
            kind: kind.into(),
            message: message.into(),
            data,
        }
    }

    pub fn as_value(&self) -> Value {
        Value::Map(vec![
            (
                Value::Symbol("kind".into()),
                Value::Symbol(self.kind.clone()),
            ),
            (
                Value::Symbol("message".into()),
                Value::String(self.message.clone()),
            ),
            (Value::Symbol("data".into()), self.data.clone()),
        ])
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvalError {
    pub condition: Box<Condition>,
}

impl fmt::Display for EvalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.condition.kind, self.condition.message)
    }
}

impl std::error::Error for EvalError {}

#[derive(Debug)]
enum Signal {
    Condition(Box<Condition>),
    Restart { name: String, arguments: Vec<Value> },
}

impl From<Condition> for Signal {
    fn from(value: Condition) -> Self {
        Self::Condition(Box::new(value))
    }
}

struct Runtime<'a> {
    options: &'a EvaluationOptions,
    fuel_remaining: u64,
    call_depth: usize,
    macro_depth: usize,
    current_agent: Option<u64>,
    agent_capabilities: Option<Vec<Capability>>,
    world_id: u64,
    authority_epoch: u64,
}

impl<'a> Runtime<'a> {
    fn new(options: &'a EvaluationOptions, world_id: u64, authority_epoch: u64) -> Self {
        Self {
            options,
            fuel_remaining: options.budget.fuel,
            call_depth: 0,
            macro_depth: 0,
            current_agent: None,
            agent_capabilities: None,
            world_id,
            authority_epoch,
        }
    }

    fn tick(&mut self) -> Result<(), Signal> {
        if self.fuel_remaining == 0 {
            return Err(condition(
                "resource/fuel-exhausted",
                "evaluation exhausted its deterministic fuel budget",
            ));
        }
        self.fuel_remaining -= 1;
        Ok(())
    }

    fn check_collection(&self, length: usize) -> Result<(), Signal> {
        if length > self.options.budget.max_collection_len {
            Err(condition(
                "resource/collection-limit",
                format!(
                    "collection length {length} exceeds limit {}",
                    self.options.budget.max_collection_len
                ),
            ))
        } else {
            Ok(())
        }
    }

    fn charge(&mut self, amount: u64) -> Result<(), Signal> {
        self.fuel_remaining = self.fuel_remaining.checked_sub(amount).ok_or_else(|| {
            condition(
                "resource/fuel-exhausted",
                "evaluation exhausted its deterministic fuel budget",
            )
        })?;
        Ok(())
    }

    fn steps_used(&self) -> u64 {
        self.options.budget.fuel - self.fuel_remaining
    }
}

pub(crate) fn eval_all(
    expressions: &[Expr],
    state: &mut State,
    options: &EvaluationOptions,
    world_id: u64,
    authority_epoch: u64,
) -> Result<(Vec<Value>, u64), EvalError> {
    let mut runtime = Runtime::new(options, world_id, authority_epoch);
    let mut env = Env::default();
    let mut values = Vec::with_capacity(expressions.len());
    for expression in expressions {
        match eval(expression, state, &mut env, None, &mut runtime) {
            Ok(value) => values.push(value),
            Err(Signal::Condition(condition)) => return Err(EvalError { condition }),
            Err(Signal::Restart { name, .. }) => {
                return Err(EvalError {
                    condition: Box::new(Condition::new(
                        "control/unhandled-restart",
                        format!("no active restart named {name}"),
                        Value::Symbol(name),
                    )),
                });
            }
        }
    }
    Ok((values, runtime.steps_used()))
}

fn eval(
    expression: &Expr,
    state: &mut State,
    env: &mut Env,
    module: Option<&str>,
    runtime: &mut Runtime<'_>,
) -> Result<Value, Signal> {
    runtime.tick()?;
    match expression {
        Expr::Nil => Ok(Value::Nil),
        Expr::Bool(value) => Ok(Value::Bool(*value)),
        Expr::Int(value) => Ok(Value::Int(*value)),
        Expr::String(value) => Ok(Value::String(value.clone())),
        Expr::Symbol(name) => lookup(state, env, module, name)
            .ok_or_else(|| condition("name/unbound", format!("unbound symbol: {name}"))),
        Expr::ScopedSymbol { name, module } => lookup_scoped(state, module.as_deref(), name)
            .ok_or_else(|| condition("name/unbound", format!("unbound hygienic symbol: {name}"))),
        Expr::List(items) if items.is_empty() => Ok(Value::Nil),
        Expr::List(items) => eval_list(items, state, env, module, runtime),
    }
}

fn eval_list(
    items: &[Expr],
    state: &mut State,
    env: &mut Env,
    module: Option<&str>,
    runtime: &mut Runtime<'_>,
) -> Result<Value, Signal> {
    if let Expr::Symbol(name) = &items[0] {
        match name.as_str() {
            "quote" => return eval_quote(items, runtime),
            "if" => return eval_if(items, state, env, module, runtime),
            "begin" => return eval_sequence(&items[1..], state, env, module, runtime),
            "def" => return eval_def(items, state, env, module, runtime),
            "fn" => return eval_fn(items, env, module),
            "let" => return eval_let(items, state, env, module, runtime),
            "defmacro" => return eval_defmacro(items, state, module, runtime),
            "defprotocol" => return eval_defprotocol(items, state, module, runtime),
            "macroexpand-1" => return eval_macroexpand(items, state, module, runtime),
            "module" => return eval_module(items, state, env, runtime),
            "export" => return eval_export(items, state, module, runtime),
            "import" => return eval_import(items, state, module, runtime),
            "with-handler" => return eval_with_handler(items, state, env, module, runtime),
            "with-restart" => return eval_with_restart(items, state, env, module, runtime),
            "invoke-restart" => return eval_invoke_restart(items, state, env, module, runtime),
            _ => {}
        }
    }

    if let Some(definition) = find_macro(state, module, &items[0]) {
        if runtime.macro_depth >= runtime.options.budget.max_parse_depth {
            return Err(condition(
                "resource/macro-depth",
                format!(
                    "macro expansion depth exceeds limit {}",
                    runtime.options.budget.max_parse_depth
                ),
            ));
        }
        let (expanded, expansion_steps) = expand(
            &definition,
            &items[1..],
            &mut state.next_syntax_id,
            runtime.fuel_remaining,
            runtime.options.budget.max_collection_len,
        )
        .map_err(expansion_condition)?;
        runtime.charge(expansion_steps)?;
        runtime.macro_depth += 1;
        let result = eval(&expanded, state, env, module, runtime);
        runtime.macro_depth -= 1;
        return result;
    }

    let function = eval(&items[0], state, env, module, runtime)?;
    let mut arguments = Vec::with_capacity(items.len() - 1);
    for item in &items[1..] {
        arguments.push(eval(item, state, env, module, runtime)?);
    }
    apply(function, arguments, state, runtime)
}

fn eval_quote(items: &[Expr], runtime: &Runtime<'_>) -> Result<Value, Signal> {
    expect_arity("quote", items.len() - 1, 1)?;
    quote(&items[1], runtime)
}

fn quote(expression: &Expr, runtime: &Runtime<'_>) -> Result<Value, Signal> {
    match expression {
        Expr::Nil => Ok(Value::Nil),
        Expr::Bool(value) => Ok(Value::Bool(*value)),
        Expr::Int(value) => Ok(Value::Int(*value)),
        Expr::String(value) => Ok(Value::String(value.clone())),
        Expr::Symbol(value) | Expr::ScopedSymbol { name: value, .. } => {
            Ok(Value::Symbol(value.clone()))
        }
        Expr::List(values) => {
            if values.is_empty() {
                return Ok(Value::Nil);
            }
            runtime.check_collection(values.len())?;
            values
                .iter()
                .map(|value| quote(value, runtime))
                .collect::<Result<Vec<_>, _>>()
                .map(Value::List)
        }
    }
}

fn eval_if(
    items: &[Expr],
    state: &mut State,
    env: &mut Env,
    module: Option<&str>,
    runtime: &mut Runtime<'_>,
) -> Result<Value, Signal> {
    expect_arity("if", items.len() - 1, 3)?;
    if eval(&items[1], state, env, module, runtime)?.is_truthy() {
        eval(&items[2], state, env, module, runtime)
    } else {
        eval(&items[3], state, env, module, runtime)
    }
}

fn eval_def(
    items: &[Expr],
    state: &mut State,
    env: &mut Env,
    module: Option<&str>,
    runtime: &mut Runtime<'_>,
) -> Result<Value, Signal> {
    ensure_world_mutation_allowed(runtime, "def")?;
    expect_arity("def", items.len() - 1, 2)?;
    let name = expect_expr_symbol("def", &items[1])?;
    let value = eval(&items[2], state, env, module, runtime)?;
    define_value(state, module, name.to_owned(), value.clone())?;
    Ok(value)
}

fn eval_fn(items: &[Expr], env: &Env, module: Option<&str>) -> Result<Value, Signal> {
    if items.len() < 3 {
        return Err(condition("arity", "fn expects parameters and a body"));
    }
    let params = parse_params("fn", &items[1])?;
    Ok(Value::Closure(Closure {
        params,
        body: items[2..].to_vec(),
        env: env.clone(),
        module: module.map(str::to_owned),
    }))
}

fn eval_let(
    items: &[Expr],
    state: &mut State,
    env: &mut Env,
    module: Option<&str>,
    runtime: &mut Runtime<'_>,
) -> Result<Value, Signal> {
    if items.len() < 3 {
        return Err(condition("arity", "let expects bindings and a body"));
    }
    let Expr::List(bindings) = &items[1] else {
        return Err(condition("type", "let expects a binding list"));
    };
    runtime.check_collection(bindings.len())?;
    let mut evaluated = Vec::with_capacity(bindings.len());
    for binding in bindings {
        let Expr::List(pair) = binding else {
            return Err(condition("syntax", "let bindings must be pairs"));
        };
        if pair.len() != 2 {
            return Err(condition("syntax", "let bindings must be pairs"));
        }
        let name = expect_expr_symbol("let", &pair[0])?.to_owned();
        let value = eval(&pair[1], state, env, module, runtime)?;
        evaluated.push((name, value));
    }
    let mut local = env.child();
    for (name, value) in evaluated {
        local.insert(name, value);
    }
    eval_sequence(&items[2..], state, &mut local, module, runtime)
}

fn eval_defmacro(
    items: &[Expr],
    state: &mut State,
    module: Option<&str>,
    runtime: &Runtime<'_>,
) -> Result<Value, Signal> {
    ensure_world_mutation_allowed(runtime, "defmacro")?;
    expect_arity("defmacro", items.len() - 1, 3)?;
    let name = expect_expr_symbol("defmacro", &items[1])?.to_owned();
    let params = parse_params("defmacro", &items[2])?;
    let definition = MacroDef {
        params,
        template: items[3].clone(),
        definition_module: module.map(str::to_owned),
    };
    define_macro(state, module, name.clone(), definition)?;
    Ok(Value::Symbol(name))
}

fn eval_defprotocol(
    items: &[Expr],
    state: &mut State,
    module: Option<&str>,
    runtime: &Runtime<'_>,
) -> Result<Value, Signal> {
    ensure_world_mutation_allowed(runtime, "defprotocol")?;
    if items.len() < 2 {
        return Err(condition("arity", "defprotocol expects a name"));
    }
    let name = expect_expr_symbol("defprotocol", &items[1])?.to_owned();
    let mut messages = BTreeMap::new();
    for clause in &items[2..] {
        let Expr::List(parts) = clause else {
            return Err(condition(
                "protocol/syntax",
                "protocol message declarations must be lists",
            ));
        };
        let Some(tag_expr) = parts.first() else {
            return Err(condition(
                "protocol/syntax",
                "protocol message declarations cannot be empty",
            ));
        };
        let tag = expect_expr_symbol("defprotocol", tag_expr)?.to_owned();
        if tag.starts_with("system/") {
            return Err(condition(
                "protocol/reserved-tag",
                "message tags beginning with system/ are reserved",
            ));
        }
        let mut types = Vec::with_capacity(parts.len() - 1);
        for type_expr in &parts[1..] {
            let type_name = expect_expr_symbol("defprotocol", type_expr)?;
            types.push(TypeSpec::parse(type_name).ok_or_else(|| {
                condition(
                    "protocol/unknown-type",
                    format!("unknown protocol type: {type_name}"),
                )
            })?);
        }
        if messages.insert(tag.clone(), types).is_some() {
            return Err(condition(
                "protocol/duplicate-message",
                format!("protocol {name} repeats message {tag}"),
            ));
        }
    }
    let protocol = Protocol::new(name.clone(), messages);
    define_value(state, module, name, Value::Protocol(protocol.clone()))?;
    Ok(Value::Protocol(protocol))
}

fn eval_macroexpand(
    items: &[Expr],
    state: &mut State,
    module: Option<&str>,
    runtime: &mut Runtime<'_>,
) -> Result<Value, Signal> {
    ensure_world_mutation_allowed(runtime, "macroexpand-1")?;
    expect_arity("macroexpand-1", items.len() - 1, 1)?;
    let Expr::List(call) = &items[1] else {
        return quote(&items[1], runtime);
    };
    let Some(definition) = call
        .first()
        .and_then(|head| find_macro(state, module, head))
    else {
        return quote(&items[1], runtime);
    };
    let (expanded, expansion_steps) = expand(
        &definition,
        &call[1..],
        &mut state.next_syntax_id,
        runtime.fuel_remaining,
        runtime.options.budget.max_collection_len,
    )
    .map_err(expansion_condition)?;
    runtime.charge(expansion_steps)?;
    quote(&expanded, runtime)
}

fn eval_module(
    items: &[Expr],
    state: &mut State,
    env: &mut Env,
    runtime: &mut Runtime<'_>,
) -> Result<Value, Signal> {
    ensure_world_mutation_allowed(runtime, "module")?;
    if items.len() < 2 {
        return Err(condition("arity", "module expects a name and body"));
    }
    let name = expect_expr_symbol("module", &items[1])?.to_owned();
    state.modules.insert(name.clone(), Module::default());
    eval_sequence(&items[2..], state, env, Some(&name), runtime)?;
    let module = state.modules.get(&name).expect("module was inserted");
    for export in &module.exports {
        if !module.bindings.contains_key(export) && !module.macros.contains_key(export) {
            return Err(condition(
                "module/missing-export",
                format!("module {name} exports undefined name {export}"),
            ));
        }
    }
    Ok(Value::Module(name))
}

fn eval_export(
    items: &[Expr],
    state: &mut State,
    module: Option<&str>,
    runtime: &Runtime<'_>,
) -> Result<Value, Signal> {
    ensure_world_mutation_allowed(runtime, "export")?;
    let Some(module_name) = module else {
        return Err(condition(
            "module/context",
            "export is only valid inside a module",
        ));
    };
    let module = state
        .modules
        .get_mut(module_name)
        .expect("active module exists");
    for item in &items[1..] {
        module
            .exports
            .insert(expect_expr_symbol("export", item)?.to_owned());
    }
    Ok(Value::Nil)
}

fn eval_import(
    items: &[Expr],
    state: &mut State,
    target: Option<&str>,
    runtime: &Runtime<'_>,
) -> Result<Value, Signal> {
    ensure_world_mutation_allowed(runtime, "import")?;
    expect_arity("import", items.len() - 1, 1)?;
    let source_name = expect_expr_symbol("import", &items[1])?.to_owned();
    let source = state.modules.get(&source_name).cloned().ok_or_else(|| {
        condition(
            "module/not-found",
            format!("module does not exist: {source_name}"),
        )
    })?;
    for name in &source.exports {
        if let Some(value) = source.bindings.get(name) {
            define_value(state, target, name.clone(), value.clone())?;
            define_value(
                state,
                target,
                format!("{source_name}/{name}"),
                value.clone(),
            )?;
        }
        if let Some(definition) = source.macros.get(name) {
            define_macro(state, target, name.clone(), definition.clone())?;
            define_macro(
                state,
                target,
                format!("{source_name}/{name}"),
                definition.clone(),
            )?;
        }
    }
    Ok(Value::Module(source_name))
}

fn eval_with_handler(
    items: &[Expr],
    state: &mut State,
    env: &mut Env,
    module: Option<&str>,
    runtime: &mut Runtime<'_>,
) -> Result<Value, Signal> {
    if items.len() < 4 {
        return Err(condition(
            "arity",
            "with-handler expects (kind variable), a handler, and a body",
        ));
    }
    let Expr::List(spec) = &items[1] else {
        return Err(condition("syntax", "with-handler requires a handler spec"));
    };
    if spec.len() != 2 {
        return Err(condition(
            "syntax",
            "handler spec must contain a kind and variable",
        ));
    }
    let kind = expr_name(&spec[0], "with-handler")?.to_owned();
    let variable = expect_expr_symbol("with-handler", &spec[1])?.to_owned();
    match eval_sequence(&items[3..], state, env, module, runtime) {
        Err(Signal::Condition(caught)) if kind == "*" || kind == caught.kind => {
            let mut handler_env = env.child();
            handler_env.insert(variable, caught.as_value());
            eval(&items[2], state, &mut handler_env, module, runtime)
        }
        result => result,
    }
}

fn eval_with_restart(
    items: &[Expr],
    state: &mut State,
    env: &mut Env,
    module: Option<&str>,
    runtime: &mut Runtime<'_>,
) -> Result<Value, Signal> {
    if items.len() < 4 {
        return Err(condition(
            "arity",
            "with-restart expects (name parameters), a handler, and a body",
        ));
    }
    let Expr::List(spec) = &items[1] else {
        return Err(condition("syntax", "with-restart requires a restart spec"));
    };
    let Some(name_expression) = spec.first() else {
        return Err(condition("syntax", "restart spec cannot be empty"));
    };
    let name = expr_name(name_expression, "with-restart")?.to_owned();
    let mut params = Vec::with_capacity(spec.len() - 1);
    for param in &spec[1..] {
        params.push(expect_expr_symbol("with-restart", param)?.to_owned());
    }
    match eval_sequence(&items[3..], state, env, module, runtime) {
        Err(Signal::Restart {
            name: invoked,
            arguments,
        }) if invoked == name => {
            expect_arity("restart", arguments.len(), params.len())?;
            let mut restart_env = env.child();
            for (param, value) in params.into_iter().zip(arguments) {
                restart_env.insert(param, value);
            }
            eval(&items[2], state, &mut restart_env, module, runtime)
        }
        result => result,
    }
}

fn eval_invoke_restart(
    items: &[Expr],
    state: &mut State,
    env: &mut Env,
    module: Option<&str>,
    runtime: &mut Runtime<'_>,
) -> Result<Value, Signal> {
    if items.len() < 2 {
        return Err(condition("arity", "invoke-restart expects a name"));
    }
    let name = expr_name(&items[1], "invoke-restart")?.to_owned();
    let mut arguments = Vec::with_capacity(items.len() - 2);
    for argument in &items[2..] {
        arguments.push(eval(argument, state, env, module, runtime)?);
    }
    Err(Signal::Restart { name, arguments })
}

fn eval_sequence(
    items: &[Expr],
    state: &mut State,
    env: &mut Env,
    module: Option<&str>,
    runtime: &mut Runtime<'_>,
) -> Result<Value, Signal> {
    let mut result = Value::Nil;
    for item in items {
        result = eval(item, state, env, module, runtime)?;
    }
    Ok(result)
}

fn apply(
    function: Value,
    arguments: Vec<Value>,
    state: &mut State,
    runtime: &mut Runtime<'_>,
) -> Result<Value, Signal> {
    match function {
        Value::Builtin(builtin) => apply_builtin(builtin, arguments, state, runtime),
        Value::Closure(closure) => {
            expect_arity("function", arguments.len(), closure.params.len())?;
            if runtime.call_depth >= runtime.options.budget.max_call_depth {
                return Err(condition(
                    "resource/call-depth",
                    format!(
                        "call depth exceeds limit {}",
                        runtime.options.budget.max_call_depth
                    ),
                ));
            }
            let mut call_env = closure.env.child();
            for (param, value) in closure.params.into_iter().zip(arguments) {
                call_env.insert(param, value);
            }
            runtime.call_depth += 1;
            let result = eval_sequence(
                &closure.body,
                state,
                &mut call_env,
                closure.module.as_deref(),
                runtime,
            );
            runtime.call_depth -= 1;
            result
        }
        other => Err(condition(
            "type/not-callable",
            format!("value is not callable: {other}"),
        )),
    }
}

fn apply_builtin(
    builtin: Builtin,
    arguments: Vec<Value>,
    state: &mut State,
    runtime: &mut Runtime<'_>,
) -> Result<Value, Signal> {
    match builtin {
        Builtin::Add => integer_fold("+", arguments, 0, i64::checked_add),
        Builtin::Multiply => integer_fold("*", arguments, 1, i64::checked_mul),
        Builtin::Subtract => subtract(arguments),
        Builtin::Divide => divide(arguments),
        Builtin::Equal => {
            expect_arity("=", arguments.len(), 2)?;
            Ok(Value::Bool(arguments[0] == arguments[1]))
        }
        Builtin::List => {
            runtime.check_collection(arguments.len())?;
            if arguments.is_empty() {
                Ok(Value::Nil)
            } else {
                Ok(Value::List(arguments))
            }
        }
        Builtin::Cons => cons(arguments, runtime),
        Builtin::Car => car(arguments),
        Builtin::Cdr => cdr(arguments),
        Builtin::Dict => dict(arguments, runtime),
        Builtin::Get => get(arguments),
        Builtin::Assoc => assoc(arguments, runtime),
        Builtin::Dissoc => dissoc(arguments),
        Builtin::Keys => keys(arguments, runtime),
        Builtin::Count => count(arguments),
        Builtin::Spawn => spawn(arguments, state, runtime),
        Builtin::Send => send(arguments, state, runtime),
        Builtin::Receive => receive(arguments, state, runtime),
        Builtin::Run => run(arguments, state, runtime),
        Builtin::Step => step(arguments, state, runtime),
        Builtin::AgentInfo => agent_info(arguments, state, runtime),
        Builtin::EventLog => event_log(arguments, state, runtime),
        Builtin::PendingTurns => pending_turns(arguments, state),
        Builtin::ModelRequest => request_model(arguments, state, runtime),
        Builtin::PendingModelRequests => pending_model_requests(arguments, state, runtime),
        Builtin::Signal => signal_condition(arguments),
        Builtin::RequestCapability => request_capability(arguments, runtime),
        Builtin::CapabilityKind => capability_field(arguments, true),
        Builtin::CapabilityScope => capability_field(arguments, false),
    }
}

fn integer_fold(
    name: &str,
    arguments: Vec<Value>,
    identity: i64,
    operation: fn(i64, i64) -> Option<i64>,
) -> Result<Value, Signal> {
    arguments
        .into_iter()
        .try_fold(identity, |left, value| {
            let right = expect_int(name, value)?;
            operation(left, right).ok_or_else(|| {
                condition("arithmetic/overflow", format!("{name}: integer overflow"))
            })
        })
        .map(Value::Int)
}

fn subtract(arguments: Vec<Value>) -> Result<Value, Signal> {
    if arguments.is_empty() {
        return Err(condition("arity", "- expects at least one argument"));
    }
    let mut values = arguments.into_iter();
    let first = expect_int("-", values.next().expect("checked non-empty"))?;
    if values.len() == 0 {
        return first
            .checked_neg()
            .map(Value::Int)
            .ok_or_else(|| condition("arithmetic/overflow", "-: integer overflow"));
    }
    values
        .try_fold(first, |left, value| {
            left.checked_sub(expect_int("-", value)?)
                .ok_or_else(|| condition("arithmetic/overflow", "-: integer overflow"))
        })
        .map(Value::Int)
}

fn divide(arguments: Vec<Value>) -> Result<Value, Signal> {
    if arguments.len() < 2 {
        return Err(condition("arity", "/ expects at least two arguments"));
    }
    let mut values = arguments.into_iter();
    let first = expect_int("/", values.next().expect("checked non-empty"))?;
    values
        .try_fold(first, |left, value| {
            let right = expect_int("/", value)?;
            if right == 0 {
                return Err(condition("arithmetic/division-by-zero", "division by zero"));
            }
            left.checked_div(right)
                .ok_or_else(|| condition("arithmetic/overflow", "/: integer overflow"))
        })
        .map(Value::Int)
}

fn cons(arguments: Vec<Value>, runtime: &Runtime<'_>) -> Result<Value, Signal> {
    expect_arity("cons", arguments.len(), 2)?;
    let mut values = arguments.into_iter();
    let head = values.next().expect("arity checked");
    match values.next().expect("arity checked") {
        Value::List(mut tail) => {
            runtime.check_collection(tail.len() + 1)?;
            tail.insert(0, head);
            Ok(Value::List(tail))
        }
        Value::Nil => Ok(Value::List(vec![head])),
        other => Err(condition(
            "type",
            format!("cons expects a list, got {other}"),
        )),
    }
}

fn car(arguments: Vec<Value>) -> Result<Value, Signal> {
    expect_arity("car", arguments.len(), 1)?;
    match &arguments[0] {
        Value::List(values) => Ok(values.first().cloned().unwrap_or(Value::Nil)),
        Value::Nil => Ok(Value::Nil),
        other => Err(condition(
            "type",
            format!("car expects a list, got {other}"),
        )),
    }
}

fn cdr(arguments: Vec<Value>) -> Result<Value, Signal> {
    expect_arity("cdr", arguments.len(), 1)?;
    match &arguments[0] {
        Value::List(values) => {
            let tail = values.iter().skip(1).cloned().collect::<Vec<_>>();
            if tail.is_empty() {
                Ok(Value::Nil)
            } else {
                Ok(Value::List(tail))
            }
        }
        Value::Nil => Ok(Value::Nil),
        other => Err(condition(
            "type",
            format!("cdr expects a list, got {other}"),
        )),
    }
}

fn dict(arguments: Vec<Value>, runtime: &Runtime<'_>) -> Result<Value, Signal> {
    if arguments.len() % 2 != 0 {
        return Err(condition("arity", "dict expects key/value pairs"));
    }
    runtime.check_collection(arguments.len() / 2)?;
    let mut entries = Vec::new();
    for pair in arguments.chunks_exact(2) {
        map_insert(&mut entries, pair[0].clone(), pair[1].clone());
    }
    Ok(Value::Map(entries))
}

fn get(arguments: Vec<Value>) -> Result<Value, Signal> {
    expect_arity("get", arguments.len(), 2)?;
    let Value::Map(entries) = &arguments[0] else {
        return Err(condition("type", "get expects a map"));
    };
    Ok(entries
        .iter()
        .find(|(key, _)| key == &arguments[1])
        .map(|(_, value)| value.clone())
        .unwrap_or(Value::Nil))
}

fn assoc(arguments: Vec<Value>, runtime: &Runtime<'_>) -> Result<Value, Signal> {
    expect_arity("assoc", arguments.len(), 3)?;
    let Value::Map(mut entries) = arguments[0].clone() else {
        return Err(condition("type", "assoc expects a map"));
    };
    map_insert(&mut entries, arguments[1].clone(), arguments[2].clone());
    runtime.check_collection(entries.len())?;
    Ok(Value::Map(entries))
}

fn dissoc(arguments: Vec<Value>) -> Result<Value, Signal> {
    expect_arity("dissoc", arguments.len(), 2)?;
    let Value::Map(mut entries) = arguments[0].clone() else {
        return Err(condition("type", "dissoc expects a map"));
    };
    entries.retain(|(key, _)| key != &arguments[1]);
    Ok(Value::Map(entries))
}

fn keys(arguments: Vec<Value>, runtime: &Runtime<'_>) -> Result<Value, Signal> {
    expect_arity("keys", arguments.len(), 1)?;
    let Value::Map(entries) = &arguments[0] else {
        return Err(condition("type", "keys expects a map"));
    };
    runtime.check_collection(entries.len())?;
    Ok(Value::List(
        entries.iter().map(|(key, _)| key.clone()).collect(),
    ))
}

fn count(arguments: Vec<Value>) -> Result<Value, Signal> {
    expect_arity("count", arguments.len(), 1)?;
    let length = match &arguments[0] {
        Value::Nil => 0,
        Value::List(values) => values.len(),
        Value::Map(entries) => entries.len(),
        Value::String(value) => value.chars().count(),
        other => return Err(condition("type", format!("count cannot inspect {other}"))),
    };
    i64::try_from(length)
        .map(Value::Int)
        .map_err(|_| condition("arithmetic/overflow", "collection length does not fit i64"))
}

fn map_insert(entries: &mut Vec<(Value, Value)>, key: Value, value: Value) {
    if let Some((_, existing)) = entries.iter_mut().find(|(existing, _)| existing == &key) {
        *existing = value;
    } else {
        entries.push((key, value));
    }
}

fn spawn(arguments: Vec<Value>, state: &mut State, runtime: &Runtime<'_>) -> Result<Value, Signal> {
    if arguments.len() != 1 && !(4..=8).contains(&arguments.len()) {
        return Err(condition(
            "arity",
            "spawn expects a name, or name/behavior/heap/protocol plus optional supervisor/policy/max-restarts/capabilities",
        ));
    }
    let Value::String(name) = &arguments[0] else {
        return Err(condition("type", "spawn expects a string name"));
    };
    let (behavior, heap, protocol, supervisor, failure_action, max_restarts, capabilities) =
        if arguments.len() == 1 {
            (
                None,
                Value::Nil,
                None,
                None,
                FailureAction::Stop,
                0,
                Vec::new(),
            )
        } else {
            let Value::Closure(behavior) = &arguments[1] else {
                return Err(condition("type", "spawn behavior must be a closure"));
            };
            let protocol = match &arguments[3] {
                Value::Protocol(protocol) => Some(protocol.clone()),
                Value::Nil => None,
                _ => {
                    return Err(condition(
                        "type",
                        "spawn protocol must be a protocol or nil",
                    ))
                }
            };
            let supervisor = match arguments.get(4).unwrap_or(&Value::Nil) {
                Value::Agent(id) => Some(*id),
                Value::Nil => None,
                _ => {
                    return Err(condition(
                        "type",
                        "spawn supervisor must be an agent or nil",
                    ))
                }
            };
            if let Some(id) = supervisor {
                let Some(parent) = state.agents.get(&id) else {
                    return Err(condition(
                        "agent/not-found",
                        format!("supervisor does not exist: {id}"),
                    ));
                };
                if parent.status != AgentStatus::Running {
                    return Err(condition("agent/stopped", "supervisor is stopped"));
                }
            }
            let action_name = arguments
                .get(5)
                .map(|value| value_name(value, "spawn policy"))
                .transpose()?
                .unwrap_or("stop");
            let failure_action = FailureAction::parse(action_name).ok_or_else(|| {
                condition(
                    "agent/policy",
                    format!("unknown failure policy: {action_name}"),
                )
            })?;
            let max_restarts = match arguments.get(6) {
                Some(Value::Int(value)) => u32::try_from(*value).map_err(|_| {
                    condition("type", "spawn max-restarts must be a non-negative u32")
                })?,
                Some(_) => {
                    return Err(condition(
                        "type",
                        "spawn max-restarts must be a non-negative integer",
                    ));
                }
                None => 0,
            };
            let capabilities = match arguments.get(7) {
                Some(Value::List(values)) => values
                    .iter()
                    .map(|value| match value {
                        Value::Capability(capability) => Ok(capability.clone()),
                        _ => Err(condition(
                            "type",
                            "spawn capability list may contain only capabilities",
                        )),
                    })
                    .collect::<Result<Vec<_>, _>>()?,
                Some(_) => return Err(condition("type", "spawn capabilities must be a list")),
                None => Vec::new(),
            };
            (
                Some(behavior.clone()),
                arguments[2].clone(),
                protocol,
                supervisor,
                failure_action,
                max_restarts,
                capabilities,
            )
        };
    let id = state.next_agent_id;
    state.next_agent_id = state
        .next_agent_id
        .checked_add(1)
        .ok_or_else(|| condition("resource/id-exhausted", "agent id space exhausted"))?;
    state.agents.insert(
        id,
        Agent {
            name: name.clone(),
            mailbox: Default::default(),
            behavior,
            heap: heap.clone(),
            initial_heap: heap,
            protocol,
            supervisor,
            failure_action,
            max_restarts,
            restart_count: 0,
            status: AgentStatus::Running,
            capabilities,
        },
    );
    record_event(
        state,
        EventKind::Spawned,
        id,
        Value::Map(vec![
            (Value::Symbol("name".into()), Value::String(name.clone())),
            (
                Value::Symbol("supervisor".into()),
                supervisor.map(Value::Agent).unwrap_or(Value::Nil),
            ),
            (
                Value::Symbol("policy".into()),
                Value::Symbol(failure_action.name().into()),
            ),
        ]),
        runtime,
    )?;
    Ok(Value::Agent(id))
}

fn send(arguments: Vec<Value>, state: &mut State, runtime: &Runtime<'_>) -> Result<Value, Signal> {
    expect_arity("send", arguments.len(), 2)?;
    let Value::Agent(id) = arguments[0] else {
        return Err(condition(
            "type",
            "send expects an agent as its first argument",
        ));
    };
    let message = arguments[1].clone();
    enqueue_message(state, id, message.clone(), false, runtime)?;
    Ok(message)
}

fn receive(
    arguments: Vec<Value>,
    state: &mut State,
    runtime: &Runtime<'_>,
) -> Result<Value, Signal> {
    ensure_not_in_agent(runtime, "recv")?;
    expect_arity("recv", arguments.len(), 1)?;
    let Value::Agent(id) = arguments[0] else {
        return Err(condition("type", "recv expects an agent"));
    };
    let agent = state
        .agents
        .get_mut(&id)
        .ok_or_else(|| condition("agent/not-found", format!("unknown agent: {id}")))?;
    if agent.mailbox.is_empty() {
        Ok(Value::Nil)
    } else {
        Ok(agent.mailbox.pop_front().expect("mailbox is not empty"))
    }
}

fn run(
    arguments: Vec<Value>,
    state: &mut State,
    runtime: &mut Runtime<'_>,
) -> Result<Value, Signal> {
    ensure_not_in_agent(runtime, "run")?;
    if arguments.len() > 1 {
        return Err(condition("arity", "run expects zero or one argument"));
    }
    let max_turns = match arguments.first() {
        Some(Value::Int(value)) => u64::try_from(*value)
            .map_err(|_| condition("type", "run turn limit must be non-negative"))?,
        Some(_) => return Err(condition("type", "run turn limit must be an integer")),
        None => u64::try_from(runtime.options.budget.max_collection_len).unwrap_or(u64::MAX),
    };
    run_scheduler(state, runtime, max_turns)
}

fn step(
    arguments: Vec<Value>,
    state: &mut State,
    runtime: &mut Runtime<'_>,
) -> Result<Value, Signal> {
    ensure_not_in_agent(runtime, "step")?;
    expect_arity("step", arguments.len(), 0)?;
    run_scheduler(state, runtime, 1)
}

fn run_scheduler(
    state: &mut State,
    runtime: &mut Runtime<'_>,
    max_turns: u64,
) -> Result<Value, Signal> {
    let initial_events = state.events.len();
    let mut turns = 0_u64;
    while turns < max_turns {
        let Some(agent_id) = next_runnable(state) else {
            break;
        };
        execute_turn(state, runtime, agent_id)?;
        turns += 1;
    }
    let emitted_events = state.events.len().saturating_sub(initial_events);
    Ok(Value::Map(vec![
        (
            Value::Symbol("turns".into()),
            Value::Int(i64::try_from(turns).unwrap_or(i64::MAX)),
        ),
        (
            Value::Symbol("pending".into()),
            Value::Int(i64::try_from(state.ready_queue.len()).unwrap_or(i64::MAX)),
        ),
        (
            Value::Symbol("events".into()),
            Value::Int(i64::try_from(emitted_events).unwrap_or(i64::MAX)),
        ),
    ]))
}

fn next_runnable(state: &mut State) -> Option<u64> {
    while let Some(id) = state.ready_queue.pop_front() {
        if state.agents.get(&id).is_some_and(|agent| {
            agent.status == AgentStatus::Running
                && agent.behavior.is_some()
                && !agent.mailbox.is_empty()
        }) {
            return Some(id);
        }
    }
    None
}

fn execute_turn(state: &mut State, runtime: &mut Runtime<'_>, agent_id: u64) -> Result<(), Signal> {
    let (message, behavior, heap, capabilities, has_more) = {
        let agent = state
            .agents
            .get_mut(&agent_id)
            .expect("ready queue references an agent");
        let message = agent
            .mailbox
            .pop_front()
            .expect("runnable agent has a message");
        (
            message,
            agent.behavior.clone().expect("runnable agent has behavior"),
            agent.heap.clone(),
            agent.capabilities.clone(),
            !agent.mailbox.is_empty(),
        )
    };
    if has_more {
        schedule(state, agent_id);
    }
    record_event(
        state,
        EventKind::TurnStarted,
        agent_id,
        message.clone(),
        runtime,
    )?;
    let checkpoint = state.clone();
    let previous_agent = runtime.current_agent.replace(agent_id);
    let previous_capabilities = runtime.agent_capabilities.replace(capabilities);
    let outcome = apply(
        Value::Closure(behavior),
        vec![Value::Agent(agent_id), heap, message],
        state,
        runtime,
    );
    runtime.current_agent = previous_agent;
    runtime.agent_capabilities = previous_capabilities;

    match outcome {
        Ok(new_heap) => {
            state
                .agents
                .get_mut(&agent_id)
                .expect("agent survived its turn")
                .heap = new_heap.clone();
            record_event(state, EventKind::TurnCommitted, agent_id, new_heap, runtime)?;
            Ok(())
        }
        Err(Signal::Condition(failure)) if failure.kind.starts_with("resource/") => {
            Err(Signal::Condition(failure))
        }
        Err(signal) => {
            let failure = match signal {
                Signal::Condition(failure) => failure,
                Signal::Restart { name, .. } => Box::new(Condition::new(
                    "control/unhandled-restart",
                    format!("no active restart named {name}"),
                    Value::Symbol(name),
                )),
            };
            *state = checkpoint;
            record_event(
                state,
                EventKind::TurnFailed,
                agent_id,
                failure.as_value(),
                runtime,
            )?;
            supervise_failure(state, agent_id, &failure, runtime)
        }
    }
}

fn supervise_failure(
    state: &mut State,
    agent_id: u64,
    failure: &Condition,
    runtime: &Runtime<'_>,
) -> Result<(), Signal> {
    let (action, may_restart, supervisor, initial_heap, restart_count) = {
        let agent = state.agents.get(&agent_id).expect("failed agent exists");
        (
            agent.failure_action,
            agent.restart_count < agent.max_restarts,
            agent.supervisor,
            agent.initial_heap.clone(),
            agent.restart_count,
        )
    };
    if action == FailureAction::Restart && may_restart {
        let agent = state
            .agents
            .get_mut(&agent_id)
            .expect("failed agent exists");
        agent.heap = initial_heap;
        agent.restart_count = restart_count + 1;
        agent.status = AgentStatus::Running;
        record_event(
            state,
            EventKind::Restarted,
            agent_id,
            Value::Int(i64::from(restart_count + 1)),
            runtime,
        )?;
        return Ok(());
    }

    let agent = state
        .agents
        .get_mut(&agent_id)
        .expect("failed agent exists");
    agent.status = AgentStatus::Stopped;
    state.ready_queue.retain(|queued| *queued != agent_id);
    record_event(
        state,
        EventKind::Stopped,
        agent_id,
        failure.as_value(),
        runtime,
    )?;
    if action == FailureAction::Escalate || action == FailureAction::Restart {
        if let Some(supervisor_id) = supervisor {
            record_event(
                state,
                EventKind::Escalated,
                agent_id,
                Value::Agent(supervisor_id),
                runtime,
            )?;
            if state
                .agents
                .get(&supervisor_id)
                .is_some_and(|agent| agent.status == AgentStatus::Running)
            {
                let message = Value::List(vec![
                    Value::Symbol("system/child-failed".into()),
                    Value::Agent(agent_id),
                    failure.as_value(),
                ]);
                enqueue_message(state, supervisor_id, message, true, runtime)?;
            }
        }
    }
    Ok(())
}

fn enqueue_message(
    state: &mut State,
    id: u64,
    message: Value,
    system_message: bool,
    runtime: &Runtime<'_>,
) -> Result<(), Signal> {
    let active = {
        let agent = state
            .agents
            .get_mut(&id)
            .ok_or_else(|| condition("agent/not-found", format!("unknown agent: {id}")))?;
        if agent.status != AgentStatus::Running {
            return Err(condition("agent/stopped", format!("agent {id} is stopped")));
        }
        if !system_message {
            if let Some(protocol) = &agent.protocol {
                protocol
                    .validate(&message)
                    .map_err(|message| condition("protocol/violation", message))?;
            }
        }
        runtime.check_collection(agent.mailbox.len() + 1)?;
        agent.mailbox.push_back(message.clone());
        agent.behavior.is_some()
    };
    if active {
        schedule(state, id);
    }
    record_event(state, EventKind::MessageQueued, id, message, runtime)
}

fn schedule(state: &mut State, id: u64) {
    if !state.ready_queue.contains(&id) {
        state.ready_queue.push_back(id);
    }
}

fn record_event(
    state: &mut State,
    kind: EventKind,
    agent: u64,
    detail: Value,
    runtime: &Runtime<'_>,
) -> Result<(), Signal> {
    runtime.check_collection(state.events.len() + 1)?;
    let sequence = state.next_event_sequence;
    state.next_event_sequence = state
        .next_event_sequence
        .checked_add(1)
        .ok_or_else(|| condition("resource/id-exhausted", "event sequence space exhausted"))?;
    state.events.push(Event {
        sequence,
        kind,
        agent,
        detail,
    });
    Ok(())
}

fn agent_info(
    arguments: Vec<Value>,
    state: &State,
    runtime: &Runtime<'_>,
) -> Result<Value, Signal> {
    expect_arity("agent-info", arguments.len(), 1)?;
    let Value::Agent(id) = arguments[0] else {
        return Err(condition("type", "agent-info expects an agent"));
    };
    if runtime.current_agent.is_some_and(|current| current != id) {
        return Err(condition(
            "agent/isolation",
            "an agent cannot inspect another agent's heap",
        ));
    }
    let agent = state
        .agents
        .get(&id)
        .ok_or_else(|| condition("agent/not-found", format!("unknown agent: {id}")))?;
    Ok(Value::Map(vec![
        (
            Value::Symbol("name".into()),
            Value::String(agent.name.clone()),
        ),
        (
            Value::Symbol("status".into()),
            Value::Symbol(agent.status.name().into()),
        ),
        (Value::Symbol("heap".into()), agent.heap.clone()),
        (
            Value::Symbol("mailbox".into()),
            Value::Int(i64::try_from(agent.mailbox.len()).unwrap_or(i64::MAX)),
        ),
        (
            Value::Symbol("restarts".into()),
            Value::Int(i64::from(agent.restart_count)),
        ),
        (
            Value::Symbol("supervisor".into()),
            agent.supervisor.map(Value::Agent).unwrap_or(Value::Nil),
        ),
        (
            Value::Symbol("policy".into()),
            Value::Symbol(agent.failure_action.name().into()),
        ),
        (
            Value::Symbol("protocol".into()),
            agent
                .protocol
                .clone()
                .map(Value::Protocol)
                .unwrap_or(Value::Nil),
        ),
    ]))
}

fn event_log(arguments: Vec<Value>, state: &State, runtime: &Runtime<'_>) -> Result<Value, Signal> {
    ensure_not_in_agent(runtime, "event-log")?;
    expect_arity("event-log", arguments.len(), 0)?;
    runtime.check_collection(state.events.len())?;
    Ok(Value::List(
        state.events.iter().map(Event::as_value).collect(),
    ))
}

fn pending_turns(arguments: Vec<Value>, state: &State) -> Result<Value, Signal> {
    expect_arity("pending-turns", arguments.len(), 0)?;
    Ok(Value::Int(
        i64::try_from(state.ready_queue.len()).unwrap_or(i64::MAX),
    ))
}

fn request_model(
    arguments: Vec<Value>,
    state: &mut State,
    runtime: &Runtime<'_>,
) -> Result<Value, Signal> {
    expect_arity("model-request", arguments.len(), 3)?;
    let requester = runtime.current_agent.ok_or_else(|| {
        condition(
            "model/agent-required",
            "model-request may only be called during an agent turn",
        )
    })?;
    let provider = value_name(&arguments[0], "model-request provider")?.to_owned();
    let Value::String(prompt) = &arguments[1] else {
        return Err(condition("type", "model-request prompt must be a string"));
    };
    let Value::Agent(reply_to) = arguments[2] else {
        return Err(condition(
            "type",
            "model-request reply target must be an agent",
        ));
    };
    if prompt.len() > runtime.options.budget.max_model_prompt_bytes {
        return Err(condition(
            "resource/model-prompt-limit",
            format!(
                "model prompt is {} bytes; limit is {}",
                prompt.len(),
                runtime.options.budget.max_model_prompt_bytes
            ),
        ));
    }
    let pending = state
        .model_requests
        .values()
        .filter(|record| matches!(record.status, ModelRequestStatus::Pending))
        .count();
    if pending >= runtime.options.budget.max_pending_model_requests {
        return Err(condition(
            "resource/model-request-limit",
            "too many pending model requests",
        ));
    }
    let capabilities = runtime.agent_capabilities.as_deref().unwrap_or_default();
    if !capabilities.iter().any(|capability| {
        capability.permits(
            "model/infer",
            &provider,
            runtime.world_id,
            runtime.authority_epoch,
        )
    }) {
        return Err(condition(
            "capability/denied",
            format!("agent has no model/infer capability for {provider}"),
        ));
    }
    if !state
        .agents
        .get(&reply_to)
        .is_some_and(|agent| agent.status == AgentStatus::Running)
    {
        return Err(condition(
            "agent/not-found",
            format!("model reply target is unavailable: {reply_to}"),
        ));
    }
    let id = state.next_model_request_id;
    state.next_model_request_id = state
        .next_model_request_id
        .checked_add(1)
        .ok_or_else(|| condition("resource/id-exhausted", "model request id space exhausted"))?;
    let prompt_digest = agel_integrity::sha256(prompt.as_bytes());
    let effect_key = model_effect_key(runtime.world_id, id, &provider, prompt_digest);
    let request = ModelRequest {
        id,
        requester,
        reply_to,
        provider,
        prompt: prompt.clone(),
        prompt_digest,
        effect_key,
    };
    state.model_requests.insert(
        id,
        ModelRecord {
            request: request.clone(),
            status: ModelRequestStatus::Pending,
        },
    );
    record_event(
        state,
        EventKind::ModelRequested,
        requester,
        model_request_value(&request),
        runtime,
    )?;
    Ok(Value::Int(i64::try_from(id).unwrap_or(i64::MAX)))
}

fn pending_model_requests(
    arguments: Vec<Value>,
    state: &State,
    runtime: &Runtime<'_>,
) -> Result<Value, Signal> {
    ensure_not_in_agent(runtime, "pending-model-requests")?;
    expect_arity("pending-model-requests", arguments.len(), 0)?;
    let requests = state
        .model_requests
        .values()
        .filter(|record| matches!(record.status, ModelRequestStatus::Pending))
        .map(|record| model_request_value(&record.request))
        .collect::<Vec<_>>();
    runtime.check_collection(requests.len())?;
    Ok(Value::List(requests))
}

fn model_request_value(request: &ModelRequest) -> Value {
    Value::Map(vec![
        (
            Value::Symbol("id".into()),
            Value::Int(i64::try_from(request.id).unwrap_or(i64::MAX)),
        ),
        (
            Value::Symbol("requester".into()),
            Value::Agent(request.requester),
        ),
        (
            Value::Symbol("reply-to".into()),
            Value::Agent(request.reply_to),
        ),
        (
            Value::Symbol("provider".into()),
            Value::Symbol(request.provider.clone()),
        ),
        (
            Value::Symbol("prompt".into()),
            Value::String(request.prompt.clone()),
        ),
        (
            Value::Symbol("prompt-digest".into()),
            Value::String(request.prompt_digest.to_string()),
        ),
        (
            Value::Symbol("effect-key".into()),
            Value::String(request.effect_key.to_string()),
        ),
    ])
}

pub(crate) fn complete_model_request(
    state: &mut State,
    completion: &ModelCompletion,
    options: &EvaluationOptions,
    world_id: u64,
    authority_epoch: u64,
) -> Result<u64, EvalError> {
    let mut runtime = Runtime::new(options, world_id, authority_epoch);
    let request = state
        .model_requests
        .get(&completion.request_id)
        .expect("world validates model request before completion")
        .request
        .clone();
    let (kind, message) = match &completion.outcome {
        ModelOutcome::Success(text) => (
            EventKind::ModelCompleted,
            Value::List(vec![
                Value::Symbol("system/model-result".into()),
                Value::Int(i64::try_from(request.id).unwrap_or(i64::MAX)),
                Value::Symbol(request.provider.clone()),
                Value::String(text.clone()),
            ]),
        ),
        ModelOutcome::Failure { kind, message } => (
            EventKind::ModelFailed,
            Value::List(vec![
                Value::Symbol("system/model-error".into()),
                Value::Int(i64::try_from(request.id).unwrap_or(i64::MAX)),
                Value::Symbol(request.provider.clone()),
                Value::Symbol(kind.clone()),
                Value::String(message.clone()),
            ]),
        ),
    };
    runtime.tick().map_err(signal_to_eval_error)?;
    record_event(state, kind, request.requester, message.clone(), &runtime)
        .map_err(signal_to_eval_error)?;
    state
        .model_requests
        .get_mut(&completion.request_id)
        .expect("request still exists")
        .status = ModelRequestStatus::Completed(completion.outcome.clone());
    let can_deliver = state.agents.get(&request.reply_to).is_some_and(|agent| {
        agent.status == AgentStatus::Running
            && agent.mailbox.len() < options.budget.max_collection_len
    }) && state.events.len() < options.budget.max_collection_len;
    if can_deliver {
        enqueue_message(state, request.reply_to, message, true, &runtime)
            .map_err(signal_to_eval_error)?;
    } else if state.events.len() < options.budget.max_collection_len {
        record_event(
            state,
            EventKind::ModelDeliveryDropped,
            request.requester,
            Value::Map(vec![
                (
                    Value::Symbol("request-id".into()),
                    Value::Int(i64::try_from(request.id).unwrap_or(i64::MAX)),
                ),
                (
                    Value::Symbol("reply-to".into()),
                    Value::Agent(request.reply_to),
                ),
            ]),
            &runtime,
        )
        .map_err(signal_to_eval_error)?;
    }
    Ok(runtime.steps_used())
}

pub(crate) fn claim_model_request(
    state: &mut State,
    request_id: u64,
    options: &EvaluationOptions,
    world_id: u64,
    authority_epoch: u64,
) -> Result<u64, EvalError> {
    let mut runtime = Runtime::new(options, world_id, authority_epoch);
    runtime.tick().map_err(signal_to_eval_error)?;
    let request = state
        .model_requests
        .get_mut(&request_id)
        .expect("world validates model request before dispatch");
    request.status = ModelRequestStatus::Dispatching;
    let requester = request.request.requester;
    let detail = model_request_value(&request.request);
    record_event(
        state,
        EventKind::ModelDispatchStarted,
        requester,
        detail,
        &runtime,
    )
    .map_err(signal_to_eval_error)?;
    Ok(runtime.steps_used())
}

fn signal_to_eval_error(signal: Signal) -> EvalError {
    let condition = match signal {
        Signal::Condition(condition) => condition,
        Signal::Restart { name, .. } => Box::new(Condition::new(
            "control/unhandled-restart",
            format!("no active restart named {name}"),
            Value::Symbol(name),
        )),
    };
    EvalError { condition }
}

fn signal_condition(arguments: Vec<Value>) -> Result<Value, Signal> {
    if !(2..=3).contains(&arguments.len()) {
        return Err(condition(
            "arity",
            "signal expects kind, message, and optional data",
        ));
    }
    let kind = value_name(&arguments[0], "signal")?.to_owned();
    let Value::String(message) = &arguments[1] else {
        return Err(condition("type", "signal message must be a string"));
    };
    Err(Condition::new(
        kind,
        message.clone(),
        arguments.get(2).cloned().unwrap_or(Value::Nil),
    )
    .into())
}

fn request_capability(arguments: Vec<Value>, runtime: &Runtime<'_>) -> Result<Value, Signal> {
    expect_arity("request-capability", arguments.len(), 2)?;
    let kind = value_name(&arguments[0], "request-capability")?;
    let scope = value_name(&arguments[1], "request-capability")?;
    let capabilities = runtime
        .agent_capabilities
        .as_deref()
        .unwrap_or(&runtime.options.capabilities);
    capabilities
        .iter()
        .find(|capability| {
            capability.permits(kind, scope, runtime.world_id, runtime.authority_epoch)
        })
        .cloned()
        .map(Value::Capability)
        .ok_or_else(|| {
            condition(
                "capability/denied",
                format!("no supplied capability permits {kind} on {scope}"),
            )
        })
}

fn capability_field(arguments: Vec<Value>, kind: bool) -> Result<Value, Signal> {
    expect_arity("capability field", arguments.len(), 1)?;
    let Value::Capability(capability) = &arguments[0] else {
        return Err(condition("type", "expected a capability"));
    };
    Ok(Value::String(if kind {
        capability.kind().to_owned()
    } else {
        capability.scope().to_owned()
    }))
}

fn lookup(state: &State, env: &Env, module: Option<&str>, name: &str) -> Option<Value> {
    env.get(name)
        .cloned()
        .or_else(|| lookup_scoped(state, module, name))
}

fn lookup_scoped(state: &State, module: Option<&str>, name: &str) -> Option<Value> {
    module
        .and_then(|module| state.modules.get(module))
        .and_then(|module| module.bindings.get(name))
        .or_else(|| state.bindings.get(name))
        .cloned()
}

fn find_macro(state: &State, module: Option<&str>, head: &Expr) -> Option<MacroDef> {
    let (name, scope) = match head {
        Expr::Symbol(name) => (name.as_str(), module),
        Expr::ScopedSymbol { name, module } => (name.as_str(), module.as_deref()),
        _ => return None,
    };
    scope
        .and_then(|module| state.modules.get(module))
        .and_then(|module| module.macros.get(name))
        .or_else(|| state.macros.get(name))
        .cloned()
}

fn define_value(
    state: &mut State,
    module: Option<&str>,
    name: String,
    value: Value,
) -> Result<(), Signal> {
    if let Some(module) = module {
        state
            .modules
            .get_mut(module)
            .ok_or_else(|| {
                condition(
                    "module/not-found",
                    format!("module does not exist: {module}"),
                )
            })?
            .bindings
            .insert(name, value);
    } else {
        state.bindings.insert(name, value);
    }
    Ok(())
}

fn define_macro(
    state: &mut State,
    module: Option<&str>,
    name: String,
    definition: MacroDef,
) -> Result<(), Signal> {
    if let Some(module) = module {
        state
            .modules
            .get_mut(module)
            .ok_or_else(|| {
                condition(
                    "module/not-found",
                    format!("module does not exist: {module}"),
                )
            })?
            .macros
            .insert(name, definition);
    } else {
        state.macros.insert(name, definition);
    }
    Ok(())
}

fn parse_params(context: &str, expression: &Expr) -> Result<Vec<String>, Signal> {
    let Expr::List(params) = expression else {
        return Err(condition(
            "syntax",
            format!("{context} expects a parameter list"),
        ));
    };
    let mut names = Vec::with_capacity(params.len());
    for param in params {
        let name = expect_expr_symbol(context, param)?.to_owned();
        if names.contains(&name) {
            return Err(condition(
                "syntax/duplicate-parameter",
                format!("{context} repeats parameter {name}"),
            ));
        }
        names.push(name);
    }
    Ok(names)
}

fn expect_expr_symbol<'a>(context: &str, expression: &'a Expr) -> Result<&'a str, Signal> {
    match expression {
        Expr::Symbol(name) | Expr::ScopedSymbol { name, .. } => Ok(name),
        _ => Err(condition("syntax", format!("{context} expects a symbol"))),
    }
}

fn expr_name<'a>(expression: &'a Expr, context: &str) -> Result<&'a str, Signal> {
    match expression {
        Expr::Symbol(name) | Expr::String(name) | Expr::ScopedSymbol { name, .. } => Ok(name),
        _ => Err(condition(
            "syntax",
            format!("{context} expects a symbol or string name"),
        )),
    }
}

fn value_name<'a>(value: &'a Value, context: &str) -> Result<&'a str, Signal> {
    match value {
        Value::Symbol(name) | Value::String(name) => Ok(name),
        _ => Err(condition(
            "type",
            format!("{context} expects a symbol or string"),
        )),
    }
}

fn expect_int(name: &str, value: Value) -> Result<i64, Signal> {
    match value {
        Value::Int(value) => Ok(value),
        other => Err(condition(
            "type",
            format!("{name} expects integers, got {other}"),
        )),
    }
}

fn expect_arity(name: &str, actual: usize, expected: usize) -> Result<(), Signal> {
    if actual == expected {
        Ok(())
    } else {
        Err(condition(
            "arity",
            format!("{name} expects {expected} argument(s), got {actual}"),
        ))
    }
}

fn ensure_world_mutation_allowed(runtime: &Runtime<'_>, operation: &str) -> Result<(), Signal> {
    if runtime.current_agent.is_some() {
        Err(condition(
            "agent/isolation",
            format!("agent behavior cannot perform world mutation: {operation}"),
        ))
    } else {
        Ok(())
    }
}

fn ensure_not_in_agent(runtime: &Runtime<'_>, operation: &str) -> Result<(), Signal> {
    if runtime.current_agent.is_some() {
        Err(condition(
            "agent/isolation",
            format!("agent behavior cannot invoke scheduler operation: {operation}"),
        ))
    } else {
        Ok(())
    }
}

fn condition(kind: impl Into<String>, message: impl Into<String>) -> Signal {
    Condition::new(kind, message, Value::Nil).into()
}

fn expansion_condition(error: ExpansionError) -> Signal {
    match error {
        ExpansionError::Invalid(message) => condition("macro/expansion", message),
        ExpansionError::Limit(message) => condition("resource/macro-expansion-limit", message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn condition_has_language_visible_shape() {
        let condition = Condition::new("test", "failure", Value::Int(7));
        assert_eq!(
            condition.as_value(),
            Value::Map(vec![
                (Value::Symbol("kind".into()), Value::Symbol("test".into())),
                (
                    Value::Symbol("message".into()),
                    Value::String("failure".into())
                ),
                (Value::Symbol("data".into()), Value::Int(7)),
            ])
        );
    }
}
