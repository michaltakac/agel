//! Managed native tier. Agel owns source lowering; this module validates IR,
//! emits native control flow and provides bounded immutable-value mechanisms.

use agel_core::Value;
use cranelift_codegen::ir::{condcodes::IntCC, types, AbiParam, InstBuilder};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{default_libcall_names, Linkage, Module};

use crate::ExecutableMemory;

const PRIMITIVES: &[&str] = &[
    "+", "-", "*", "/", "=", "<", "list", "cons", "car", "cdr", "dict", "get", "assoc", "dissoc",
    "keys", "count", "has-key?", "type-of", "apply", "signal",
];
const MAX_DEPTH: usize = 128;
const MAX_IR: usize = 16_384;
const MAX_ARITY: usize = 64;
const MAX_FUNCTIONS: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fault {
    Invalid(&'static str),
    Backend(String),
    Arity,
    Type,
    Overflow,
    DivisionByZero,
    Fuel,
    Heap,
    Depth,
    NonData,
    Signaled,
    Internal,
}
impl std::fmt::Display for Fault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Fault {}

/// Invocation-local limits. Heap counters are cumulative logical allocations,
/// not an exact byte count of the system allocator. All storage dies on return.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub fuel: u64,
    pub values: usize,
    pub edges: usize,
    pub text_bytes: usize,
    pub call_depth: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            fuel: 4_000_000,
            values: 1_000_000,
            edges: 4_000_000,
            text_bytes: 16_000_000,
            call_depth: 256,
        }
    }
}

#[derive(Debug)]
pub struct Outcome {
    pub value: Value,
    pub fuel_used: u64,
    pub allocated_values: usize,
    pub allocated_edges: usize,
    pub tail_calls: u64,
    pub peak_call_depth: usize,
}

/// Diagnostic switches for paired performance/conformance comparisons.
#[derive(Clone, Copy, Debug)]
pub struct NativeOptions {
    pub tail_calls: bool,
    pub cache_builtins: bool,
}
impl Default for NativeOptions {
    fn default() -> Self {
        Self {
            tail_calls: true,
            cache_builtins: true,
        }
    }
}

enum Node {
    Constant(usize),
    Local(usize, usize),
    Builtin(usize),
    Closure(usize),
    If(Box<Node>, Box<Node>, Box<Node>),
    Begin(Vec<Node>),
    Call(Box<Node>, Vec<Node>, bool),
}
struct Function {
    arity: usize,
    body: Node,
}
struct Program {
    functions: Vec<Function>,
    constants: Vec<Value>,
    nodes: usize,
    text: usize,
    tail_ir: bool,
}
fn sequence(value: &Value) -> Result<&[Value], Fault> {
    match value {
        Value::Nil => Ok(&[]),
        Value::List(xs) => Ok(xs),
        _ => Err(Fault::Invalid("expected IR list")),
    }
}
fn name(value: &Value) -> Result<&str, Fault> {
    if let Value::Symbol(s) = value {
        Ok(s)
    } else {
        Err(Fault::Invalid("expected IR symbol"))
    }
}
fn index(value: &Value) -> Result<usize, Fault> {
    if let Value::Int(n) = value {
        usize::try_from(*n).map_err(|_| Fault::Invalid("negative index"))
    } else {
        Err(Fault::Invalid("expected integer index"))
    }
}
impl Program {
    fn count(&mut self, depth: usize) -> Result<(), Fault> {
        self.nodes += 1;
        if depth > MAX_DEPTH || self.nodes > MAX_IR {
            Err(Fault::Invalid("IR size/depth limit"))
        } else {
            Ok(())
        }
    }
    fn data(&mut self, value: &Value, depth: usize) -> Result<(), Fault> {
        self.count(depth)?;
        match value {
            Value::Nil | Value::Bool(_) | Value::Int(_) => (),
            Value::String(s) | Value::Symbol(s) => {
                self.text = self.text.checked_add(s.len()).ok_or(Fault::Heap)?;
                if self.text > 1_000_000 {
                    return Err(Fault::Invalid("IR text limit"));
                }
            }
            Value::List(xs) => {
                for x in xs {
                    self.data(x, depth + 1)?;
                }
            }
            Value::Map(xs) => {
                for (k, v) in xs {
                    self.data(k, depth + 1)?;
                    self.data(v, depth + 1)?;
                }
            }
            _ => return Err(Fault::Invalid("constant is not inert data")),
        }
        Ok(())
    }
    fn node(
        &mut self,
        value: &Value,
        scopes: &[usize],
        depth: usize,
        tail: bool,
    ) -> Result<Node, Fault> {
        self.count(depth)?;
        let xs = sequence(value)?;
        let Some(head) = xs.first() else {
            return Err(Fault::Invalid("empty node"));
        };
        Ok(match (name(head)?, &xs[1..]) {
            ("const", [value]) => {
                self.data(value, depth + 1)?;
                let id = self.constants.len();
                self.constants.push(value.clone());
                Node::Constant(id)
            }
            ("local", [d, s]) => {
                let (d, s) = (index(d)?, index(s)?);
                if d >= scopes.len() || s >= scopes[scopes.len() - d - 1] {
                    return Err(Fault::Invalid("lexical address out of bounds"));
                }
                Node::Local(d, s)
            }
            ("builtin", [value]) => Node::Builtin(
                PRIMITIVES
                    .iter()
                    .position(|p| *p == name(value).unwrap_or(""))
                    .ok_or(Fault::Invalid("unknown primitive"))?,
            ),
            ("fn", [arity, body]) => {
                let arity = index(arity)?;
                if arity > MAX_ARITY || self.functions.len() >= MAX_FUNCTIONS {
                    return Err(Fault::Invalid("function/arity limit"));
                }
                let id = self.functions.len();
                self.functions.push(Function {
                    arity,
                    body: Node::Constant(0),
                });
                let mut local = scopes.to_vec();
                local.push(arity);
                let body = self.node(body, &local, depth + 1, true)?;
                self.functions[id].body = body;
                Node::Closure(id)
            }
            ("if", [p, y, n]) => Node::If(
                Box::new(self.node(p, scopes, depth + 1, false)?),
                Box::new(self.node(y, scopes, depth + 1, tail)?),
                Box::new(self.node(n, scopes, depth + 1, tail)?),
            ),
            ("begin", forms) => Node::Begin(
                forms
                    .iter()
                    .enumerate()
                    .map(|(i, v)| self.node(v, scopes, depth + 1, tail && i + 1 == forms.len()))
                    .collect::<Result<_, _>>()?,
            ),
            (op @ ("call" | "tail-call"), [function, arguments]) => {
                let is_tail = op == "tail-call";
                if is_tail && (!self.tail_ir || !tail) {
                    return Err(Fault::Invalid("tail call outside tail position or v2 IR"));
                }
                let args = sequence(arguments)?;
                if args.len() > MAX_ARITY {
                    return Err(Fault::Invalid("call arity limit"));
                }
                Node::Call(
                    Box::new(self.node(function, scopes, depth + 1, false)?),
                    args.iter()
                        .map(|v| self.node(v, scopes, depth + 1, false))
                        .collect::<Result<_, _>>()?,
                    is_tail,
                )
            }
            _ => return Err(Fault::Invalid("unknown or malformed node")),
        })
    }
    fn parse(ir: &Value) -> Result<Self, Fault> {
        let xs = sequence(ir)?;
        if xs.len() != 2 || !matches!(name(&xs[0])?, "agel/native-v1" | "agel/native-v2") {
            return Err(Fault::Invalid("unknown IR version"));
        }
        let mut program = Self {
            functions: vec![],
            constants: vec![Value::Nil],
            nodes: 0,
            text: 0,
            tail_ir: name(&xs[0])? == "agel/native-v2",
        };
        if !matches!(program.node(&xs[1], &[], 0, false)?, Node::Closure(0)) {
            return Err(Fault::Invalid("entry must be a function"));
        }
        Ok(program)
    }
}

/// Native code plus immutable constant pool. No raw code or heap handle escapes.
pub struct Native {
    _memory: ExecutableMemory,
    code: Vec<*const u8>,
    arities: Vec<usize>,
    constants: Vec<Value>,
    options: NativeOptions,
}

impl Native {
    pub fn compile(ir: &Value) -> Result<Self, Fault> {
        Self::compile_with(ir, NativeOptions::default())
    }

    pub fn compile_with(ir: &Value, options: NativeOptions) -> Result<Self, Fault> {
        stacker::maybe_grow(128 * 1024, 2 * 1024 * 1024, || {
            Self::compile_inner(ir, options)
        })
    }

    fn compile_inner(ir: &Value, options: NativeOptions) -> Result<Self, Fault> {
        let program = Program::parse(ir)?;
        let mut builder =
            JITBuilder::new(default_libcall_names()).map_err(|e| Fault::Backend(e.to_string()))?;
        builder.symbol("agel_managed_step", step as *const u8);
        let mut memory = ExecutableMemory(Some(JITModule::new(builder)));
        let module = memory.0.as_mut().expect("new module");
        let pointer = module.target_config().pointer_type();
        let mut helper_sig = module.make_signature();
        helper_sig.params.push(AbiParam::new(pointer));
        helper_sig.params.extend([AbiParam::new(types::I64); 4]);
        helper_sig.returns.push(AbiParam::new(types::I64));
        let helper = module
            .declare_function("agel_managed_step", Linkage::Import, &helper_sig)
            .map_err(|e| Fault::Backend(e.to_string()))?;
        let mut ids = Vec::new();
        for (i, function) in program.functions.iter().enumerate() {
            let mut context = module.make_context();
            context
                .func
                .signature
                .params
                .extend([AbiParam::new(pointer), AbiParam::new(types::I64)]);
            context
                .func
                .signature
                .returns
                .push(AbiParam::new(types::I64));
            let id = module
                .declare_function(
                    &format!("agel_managed_{i}"),
                    Linkage::Local,
                    &context.func.signature,
                )
                .map_err(|e| Fault::Backend(e.to_string()))?;
            let helper_ref = module.declare_func_in_func(helper, &mut context.func);
            let mut builder_context = FunctionBuilderContext::new();
            {
                let mut b = FunctionBuilder::new(&mut context.func, &mut builder_context);
                let entry = b.create_block();
                let failure = b.create_block();
                b.append_block_params_for_function_params(entry);
                b.switch_to_block(entry);
                let runtime = b.block_params(entry)[0];
                let env = b.block_params(entry)[1];
                let mut emitter = Emitter {
                    b: &mut b,
                    helper: helper_ref,
                    runtime,
                    failure,
                    tail_calls: options.tail_calls,
                };
                let result = emitter.node(&function.body, env);
                b.ins().return_(&[result]);
                b.switch_to_block(failure);
                let zero = b.ins().iconst(types::I64, 0);
                b.ins().return_(&[zero]);
                b.seal_all_blocks();
                b.finalize();
            }
            module
                .define_function(id, &mut context)
                .map_err(|e| Fault::Backend(e.to_string()))?;
            ids.push(id);
        }
        module
            .finalize_definitions()
            .map_err(|e| Fault::Backend(e.to_string()))?;
        let code = ids
            .into_iter()
            .map(|id| module.get_finalized_function(id))
            .collect();
        Ok(Self {
            _memory: memory,
            code,
            arities: program.functions.iter().map(|f| f.arity).collect(),
            constants: program.constants,
            options,
        })
    }

    pub fn invoke(&self, arguments: &[Value], limits: Limits) -> Result<Outcome, Fault> {
        if arguments.len() != self.arities[0] {
            return Err(Fault::Arity);
        }
        self.invoke_refs(&arguments.iter().collect::<Vec<_>>(), limits)
    }

    /// Import borrowed values without cloning an entire host state beforehand.
    pub fn invoke_refs(&self, arguments: &[&Value], limits: Limits) -> Result<Outcome, Fault> {
        stacker::maybe_grow(128 * 1024, 2 * 1024 * 1024, || {
            self.invoke_inner(arguments, limits)
        })
    }

    fn invoke_inner(&self, arguments: &[&Value], limits: Limits) -> Result<Outcome, Fault> {
        if arguments.len() != self.arities[0] {
            return Err(Fault::Arity);
        }
        let mut run = Run {
            native: self,
            limits,
            fuel: limits.fuel,
            values: vec![],
            frames: vec![],
            exported: 0,
            edges: 0,
            text: 0,
            depth: 0,
            peak_depth: 0,
            tail_calls: 0,
            pending_tail: None,
            builtins: vec![None; PRIMITIVES.len()],
            constants: vec![None; self.constants.len()],
            error: None,
        };
        // Zero is exclusively the error sentinel; nil is handle one.
        run.alloc(Datum::Nil)?;
        let args = arguments
            .iter()
            .map(|v| run.import(v, 0))
            .collect::<Result<Vec<_>, _>>()?;
        let env = run.frame(None, args)?;
        let result = run.enter(0, env)?;
        let value = run.export(result, 0)?;
        Ok(Outcome {
            value,
            fuel_used: limits.fuel - run.fuel,
            allocated_values: run.values.len() + run.frames.len() + run.exported,
            allocated_edges: run.edges,
            tail_calls: run.tail_calls,
            peak_call_depth: run.peak_depth,
        })
    }
}

struct Emitter<'a, 'b> {
    b: &'a mut FunctionBuilder<'b>,
    helper: cranelift_codegen::ir::FuncRef,
    runtime: cranelift_codegen::ir::Value,
    failure: cranelift_codegen::ir::Block,
    tail_calls: bool,
}

impl Emitter<'_, '_> {
    fn int(&mut self, n: usize) -> cranelift_codegen::ir::Value {
        self.b.ins().iconst(types::I64, n as i64)
    }
    fn helper(
        &mut self,
        op: usize,
        a: cranelift_codegen::ir::Value,
        b: cranelift_codegen::ir::Value,
        c: cranelift_codegen::ir::Value,
    ) -> cranelift_codegen::ir::Value {
        let op = self.int(op);
        let call = self.b.ins().call(self.helper, &[self.runtime, op, a, b, c]);
        let result = self.b.inst_results(call)[0];
        let next = self.b.create_block();
        self.b.ins().brif(result, next, &[], self.failure, &[]);
        self.b.switch_to_block(next);
        result
    }
    fn node(
        &mut self,
        node: &Node,
        env: cranelift_codegen::ir::Value,
    ) -> cranelift_codegen::ir::Value {
        let zero = self.int(0);
        self.helper(0, zero, zero, zero); // every executed expression spends fuel
        match node {
            Node::Constant(id) => {
                let id = self.int(*id);
                self.helper(1, id, zero, zero)
            }
            Node::Local(depth, slot) => {
                let depth = self.int(*depth);
                let slot = self.int(*slot);
                self.helper(2, env, depth, slot)
            }
            Node::Closure(id) => {
                let id = self.int(*id);
                self.helper(3, id, env, zero)
            }
            Node::Builtin(id) => {
                let id = self.int(*id);
                self.helper(4, id, zero, zero)
            }
            Node::Begin(forms) => {
                let mut result = self.int(1); // nil
                for form in forms {
                    result = self.node(form, env);
                }
                result
            }
            Node::Call(function, arguments, tail) => {
                let function = self.node(function, env);
                let mut args = self.int(1);
                for argument in arguments {
                    let argument = self.node(argument, env);
                    args = self.helper(7, args, argument, zero);
                }
                self.helper(
                    if *tail && self.tail_calls { 9 } else { 8 },
                    function,
                    args,
                    zero,
                )
            }
            Node::If(predicate, yes, no) => {
                let test = self.node(predicate, env);
                let truth = self.helper(6, test, zero, zero);
                let truth = self.b.ins().icmp_imm(IntCC::Equal, truth, 2);
                let y = self.b.create_block();
                let n = self.b.create_block();
                let join = self.b.create_block();
                self.b.append_block_param(join, types::I64);
                self.b.ins().brif(truth, y, &[], n, &[]);
                self.b.switch_to_block(y);
                let yes = self.node(yes, env);
                self.b.ins().jump(join, &[yes.into()]);
                self.b.switch_to_block(n);
                let no = self.node(no, env);
                self.b.ins().jump(join, &[no.into()]);
                self.b.switch_to_block(join);
                self.b.block_params(join)[0]
            }
        }
    }
}

type Handle = usize;
#[derive(Clone)]
enum Datum {
    Nil,
    Bool(bool),
    Int(i64),
    String(String),
    Symbol(String),
    List(Vec<Handle>),
    Map(Vec<(Handle, Handle)>),
    Closure { function: usize, environment: usize },
    Builtin(usize),
}
struct Frame {
    parent: Option<usize>,
    slots: Vec<Handle>,
}
struct Run<'a> {
    native: &'a Native,
    limits: Limits,
    fuel: u64,
    values: Vec<Datum>,
    frames: Vec<Frame>,
    exported: usize,
    edges: usize,
    text: usize,
    depth: usize,
    peak_depth: usize,
    tail_calls: u64,
    pending_tail: Option<(Handle, Vec<Handle>)>,
    builtins: Vec<Option<Handle>>,
    constants: Vec<Option<Handle>>,
    error: Option<Fault>,
}

impl Run<'_> {
    fn spend(&mut self, amount: usize) -> Result<(), Fault> {
        self.fuel = self.fuel.checked_sub(amount as u64).ok_or(Fault::Fuel)?;
        Ok(())
    }
    fn reserve(&mut self, edges: usize, text: usize) -> Result<(), Fault> {
        if self
            .values
            .len()
            .saturating_add(self.frames.len())
            .saturating_add(self.exported)
            >= self.limits.values
        {
            return Err(Fault::Heap);
        }
        let next_edges = self.edges.checked_add(edges).ok_or(Fault::Heap)?;
        let next_text = self.text.checked_add(text).ok_or(Fault::Heap)?;
        if next_edges > self.limits.edges || next_text > self.limits.text_bytes {
            return Err(Fault::Heap);
        }
        self.spend(edges.saturating_add(text).saturating_add(1))?;
        self.edges = next_edges;
        self.text = next_text;
        Ok(())
    }
    fn alloc(&mut self, value: Datum) -> Result<Handle, Fault> {
        let (edges, text) = match &value {
            Datum::List(xs) => (xs.len(), 0),
            Datum::Map(xs) => (xs.len() * 2, 0),
            Datum::String(s) | Datum::Symbol(s) => (0, s.len()),
            Datum::Closure { .. } => (1, 0),
            _ => (0, 0),
        };
        self.reserve(edges, text)?;
        self.values.push(value);
        Ok(self.values.len())
    }
    fn frame(&mut self, parent: Option<usize>, slots: Vec<Handle>) -> Result<usize, Fault> {
        self.reserve(slots.len() + usize::from(parent.is_some()), 0)?;
        let id = self.frames.len();
        self.frames.push(Frame { parent, slots });
        Ok(id)
    }
    fn datum(&self, id: Handle) -> Result<&Datum, Fault> {
        self.values.get(id.wrapping_sub(1)).ok_or(Fault::Internal)
    }
    fn list(&self, id: Handle) -> Result<&[Handle], Fault> {
        match self.datum(id)? {
            Datum::Nil => Ok(&[]),
            Datum::List(xs) => Ok(xs),
            _ => Err(Fault::Type),
        }
    }
    fn make_list(&mut self, xs: Vec<Handle>) -> Result<Handle, Fault> {
        if xs.is_empty() {
            Ok(1)
        } else {
            self.alloc(Datum::List(xs))
        }
    }
    fn copy_list(&mut self, id: Handle) -> Result<Vec<Handle>, Fault> {
        self.spend(self.list(id)?.len())?;
        Ok(self.list(id)?.to_vec())
    }
    fn copy_map(&mut self, id: Handle) -> Result<Vec<(Handle, Handle)>, Fault> {
        self.spend(self.map(id)?.len().saturating_mul(2))?;
        Ok(self.map(id)?.to_vec())
    }
    fn snapshot(&mut self, id: Handle) -> Result<Datum, Fault> {
        let cost = match self.datum(id)? {
            Datum::List(xs) => xs.len(),
            Datum::Map(xs) => xs.len().saturating_mul(2),
            Datum::String(s) | Datum::Symbol(s) => s.len(),
            _ => 0,
        };
        self.spend(cost)?;
        Ok(self.datum(id)?.clone())
    }
    fn import(&mut self, value: &Value, depth: usize) -> Result<Handle, Fault> {
        if depth > MAX_DEPTH {
            return Err(Fault::Depth);
        }
        self.spend(1)?;
        let datum = match value {
            Value::Nil => return Ok(1),
            Value::Bool(b) => Datum::Bool(*b),
            Value::Int(n) => Datum::Int(*n),
            Value::String(s) | Value::Symbol(s) => {
                if s.len() > self.limits.text_bytes.saturating_sub(self.text) {
                    return Err(Fault::Heap);
                }
                self.spend(s.len())?;
                if matches!(value, Value::String(_)) {
                    Datum::String(s.clone())
                } else {
                    Datum::Symbol(s.clone())
                }
            }
            Value::List(xs) => {
                if xs.len() > self.limits.edges.saturating_sub(self.edges) {
                    return Err(Fault::Heap);
                }
                Datum::List(
                    xs.iter()
                        .map(|v| self.import(v, depth + 1))
                        .collect::<Result<_, _>>()?,
                )
            }
            Value::Map(xs) => {
                if xs.len() > self.limits.edges.saturating_sub(self.edges) / 2 {
                    return Err(Fault::Heap);
                }
                Datum::Map(
                    xs.iter()
                        .map(|(k, v)| Ok((self.import(k, depth + 1)?, self.import(v, depth + 1)?)))
                        .collect::<Result<_, Fault>>()?,
                )
            }
            _ => return Err(Fault::NonData),
        };
        self.alloc(datum)
    }
    fn export(&mut self, id: Handle, depth: usize) -> Result<Value, Fault> {
        if depth > MAX_DEPTH {
            return Err(Fault::Depth);
        }
        self.spend(1)?;
        let (edges, text) = match self.datum(id)? {
            Datum::List(xs) => (xs.len(), 0),
            Datum::Map(xs) => (xs.len() * 2, 0),
            Datum::String(s) | Datum::Symbol(s) => (0, s.len()),
            Datum::Closure { .. } | Datum::Builtin(_) => return Err(Fault::NonData),
            _ => (0, 0),
        };
        // Output is a tree, while the arena can share subgraphs. Charge every
        // exported occurrence before expanding it, not just each unique handle.
        self.reserve(edges, text)?;
        self.exported += 1;
        Ok(match self.datum(id)?.clone() {
            Datum::Nil => Value::Nil,
            Datum::Bool(b) => Value::Bool(b),
            Datum::Int(n) => Value::Int(n),
            Datum::String(s) => {
                self.spend(s.len())?;
                Value::String(s)
            }
            Datum::Symbol(s) => {
                self.spend(s.len())?;
                Value::Symbol(s)
            }
            Datum::List(xs) => Value::List(
                xs.into_iter()
                    .map(|v| self.export(v, depth + 1))
                    .collect::<Result<_, _>>()?,
            ),
            Datum::Map(xs) => Value::Map(
                xs.into_iter()
                    .map(|(k, v)| Ok((self.export(k, depth + 1)?, self.export(v, depth + 1)?)))
                    .collect::<Result<_, Fault>>()?,
            ),
            _ => return Err(Fault::NonData),
        })
    }
    fn enter(&mut self, function: usize, env: usize) -> Result<Handle, Fault> {
        if self.depth >= self.limits.call_depth.min(256) {
            return Err(Fault::Depth);
        }
        self.depth += 1;
        self.peak_depth = self.peak_depth.max(self.depth);
        // Check the semantic depth limit first; segment growth prevents a small
        // host thread stack from failing before that deterministic limit.
        let result = stacker::maybe_grow(128 * 1024, 2 * 1024 * 1024, || {
            self.trampoline(function, env)
        });
        self.depth -= 1;
        result
    }
    fn trampoline(&mut self, mut function: usize, mut env: usize) -> Result<Handle, Fault> {
        'entry: loop {
            self.spend(1)?;
            let entry = *self.native.code.get(function).ok_or(Fault::Internal)?;
            let result = self.enter_native(entry, env);
            if let Some(error) = self.error.take() {
                return Err(error);
            }
            if result == 0 {
                return Err(Fault::Internal);
            }
            let Some((mut target, mut args)) = self.pending_tail.take() else {
                return Ok(result);
            };
            loop {
                self.spend(1)?;
                match self.snapshot(target)? {
                    Datum::Closure {
                        function: next,
                        environment,
                    } => {
                        if args.len() != self.native.arities[next] {
                            return Err(Fault::Arity);
                        }
                        env = self.frame(Some(environment), args)?;
                        function = next;
                        continue 'entry;
                    }
                    Datum::Builtin(id) if PRIMITIVES[id] == "apply" => {
                        if args.len() != 2 {
                            return Err(Fault::Arity);
                        }
                        target = args[0];
                        args = self.copy_list(args[1])?;
                    }
                    Datum::Builtin(id) => return self.primitive(id, &args),
                    _ => return Err(Fault::Type),
                }
            }
        }
    }
    #[allow(unsafe_code)]
    fn enter_native(&mut self, entry: *const u8, env: usize) -> usize {
        // SAFETY: only finalized private entries reach here. Native owns them
        // throughout this borrow; signature is exactly (Run*, u64) -> u64.
        // Generated code never dereferences Run, only passes it to step. There
        // are no live references into Run's reallocatable arenas across entry.
        unsafe {
            let f: unsafe extern "C" fn(*mut Run<'_>, u64) -> u64 = std::mem::transmute(entry);
            f(self, env as u64) as usize
        }
    }
    fn call(&mut self, function: Handle, args: &[Handle]) -> Result<Handle, Fault> {
        match self.snapshot(function)? {
            Datum::Closure {
                function,
                environment,
            } => {
                if args.len() != self.native.arities[function] {
                    return Err(Fault::Arity);
                }
                let env = self.frame(Some(environment), args.to_vec())?;
                self.enter(function, env)
            }
            Datum::Builtin(id) => {
                if self.depth >= self.limits.call_depth.min(256) {
                    return Err(Fault::Depth);
                }
                self.spend(1)?;
                self.depth += 1;
                self.peak_depth = self.peak_depth.max(self.depth);
                let result =
                    stacker::maybe_grow(128 * 1024, 2 * 1024 * 1024, || self.primitive(id, args));
                self.depth -= 1;
                result
            }
            _ => Err(Fault::Type),
        }
    }
    fn dispatch(&mut self, op: usize, a: usize, b: usize, c: usize) -> Result<usize, Fault> {
        match op {
            0 => {
                self.spend(1)?;
                Ok(1)
            }
            1 => {
                if let Some(id) = self.constants[a] {
                    return Ok(id);
                }
                let id = self.import(&self.native.constants[a], 0)?;
                self.constants[a] = Some(id);
                Ok(id)
            }
            2 => {
                let mut frame = a;
                for _ in 0..b {
                    self.spend(1)?;
                    frame = self
                        .frames
                        .get(frame)
                        .and_then(|f| f.parent)
                        .ok_or(Fault::Internal)?;
                }
                self.frames
                    .get(frame)
                    .and_then(|f| f.slots.get(c))
                    .copied()
                    .ok_or(Fault::Internal)
            }
            3 => self.alloc(Datum::Closure {
                function: a,
                environment: b,
            }),
            4 => {
                if self.native.options.cache_builtins {
                    if let Some(value) = self.builtins[a] {
                        return Ok(value);
                    }
                }
                let value = self.alloc(Datum::Builtin(a))?;
                if self.native.options.cache_builtins {
                    self.builtins[a] = Some(value);
                }
                Ok(value)
            }
            6 => Ok(
                if matches!(self.datum(a)?, Datum::Nil | Datum::Bool(false)) {
                    1
                } else {
                    2
                },
            ),
            7 => {
                let mut xs = self.copy_list(a)?;
                xs.push(b);
                self.make_list(xs)
            }
            8 => {
                let args = self.copy_list(b)?;
                self.call(a, &args)
            }
            9 => {
                self.spend(1)?;
                if self.pending_tail.is_some() {
                    return Err(Fault::Internal);
                }
                self.pending_tail = Some((a, self.copy_list(b)?));
                self.tail_calls += 1;
                Ok(1)
            }
            _ => Err(Fault::Internal),
        }
    }
}

impl Run<'_> {
    fn integer(&self, id: Handle) -> Result<i64, Fault> {
        if let Datum::Int(n) = self.datum(id)? {
            Ok(*n)
        } else {
            Err(Fault::Type)
        }
    }
    fn equal(&mut self, a: Handle, b: Handle, depth: usize) -> Result<bool, Fault> {
        if depth > MAX_DEPTH {
            return Err(Fault::Depth);
        }
        self.spend(1)?;
        Ok(match (self.snapshot(a)?, self.snapshot(b)?) {
            (Datum::Closure { .. }, _) | (_, Datum::Closure { .. }) => return Err(Fault::Type),
            (Datum::Nil, Datum::Nil) => true,
            (Datum::Bool(a), Datum::Bool(b)) => a == b,
            (Datum::Int(a), Datum::Int(b)) => a == b,
            (Datum::Builtin(a), Datum::Builtin(b)) => a == b,
            (Datum::String(a), Datum::String(b)) | (Datum::Symbol(a), Datum::Symbol(b)) => {
                self.spend(a.len().max(b.len()))?;
                a == b
            }
            (Datum::List(a), Datum::List(b)) => {
                if a.len() != b.len() {
                    return Ok(false);
                }
                for (a, b) in a.into_iter().zip(b) {
                    if !self.equal(a, b, depth + 1)? {
                        return Ok(false);
                    }
                }
                true
            }
            (Datum::Map(a), Datum::Map(b)) => {
                if a.len() != b.len() {
                    return Ok(false);
                }
                // Agel maps preserve insertion order, including in equality.
                for ((ak, av), (bk, bv)) in a.into_iter().zip(b) {
                    if !self.equal(ak, bk, depth + 1)? || !self.equal(av, bv, depth + 1)? {
                        return Ok(false);
                    }
                }
                true
            }
            _ => false,
        })
    }
    fn map(&self, id: Handle) -> Result<&[(Handle, Handle)], Fault> {
        if let Datum::Map(xs) = self.datum(id)? {
            Ok(xs)
        } else {
            Err(Fault::Type)
        }
    }
    fn find(&mut self, xs: &[(Handle, Handle)], key: Handle) -> Result<Option<usize>, Fault> {
        for (i, (k, _)) in xs.iter().enumerate() {
            if self.equal(*k, key, 0)? {
                return Ok(Some(i));
            }
        }
        Ok(None)
    }
    fn data_key(&mut self, key: Handle, depth: usize) -> Result<(), Fault> {
        if depth > MAX_DEPTH {
            return Err(Fault::Depth);
        }
        self.spend(1)?;
        match self.snapshot(key)? {
            Datum::Closure { .. } | Datum::Builtin(_) => return Err(Fault::Type),
            Datum::List(xs) => {
                for x in xs {
                    self.data_key(x, depth + 1)?;
                }
            }
            Datum::Map(xs) => {
                for (k, v) in xs {
                    self.data_key(k, depth + 1)?;
                    self.data_key(v, depth + 1)?;
                }
            }
            _ => (),
        }
        Ok(())
    }
    fn primitive(&mut self, id: usize, args: &[Handle]) -> Result<Handle, Fault> {
        let op = *PRIMITIVES.get(id).ok_or(Fault::Internal)?;
        let arity = match op {
            "=" | "<" | "cons" | "get" | "has-key?" | "dissoc" | "apply" => Some(2),
            "car" | "cdr" | "keys" | "count" | "type-of" => Some(1),
            "assoc" => Some(3),
            _ => None,
        };
        if arity.is_some_and(|n| args.len() != n) {
            return Err(Fault::Arity);
        }
        self.spend(args.len())?;
        match op {
            "+" | "-" | "*" | "/" => {
                if (op == "-" && args.is_empty()) || (op == "/" && args.len() < 2) {
                    return Err(Fault::Arity);
                }
                let mut values = args.iter();
                let mut result = match op {
                    "+" => 0,
                    "*" => 1,
                    _ => self.integer(*values.next().ok_or(Fault::Arity)?)?,
                };
                if op == "-" && args.len() == 1 {
                    result = result.checked_neg().ok_or(Fault::Overflow)?;
                } else {
                    for value in values {
                        let n = self.integer(*value)?;
                        result = match op {
                            "+" => result.checked_add(n),
                            "-" => result.checked_sub(n),
                            "*" => result.checked_mul(n),
                            _ => {
                                if n == 0 {
                                    return Err(Fault::DivisionByZero);
                                }
                                result.checked_div(n)
                            }
                        }
                        .ok_or(Fault::Overflow)?;
                    }
                }
                self.alloc(Datum::Int(result))
            }
            "=" => {
                let same = self.equal(args[0], args[1], 0)?;
                self.alloc(Datum::Bool(same))
            }
            "<" => self.alloc(Datum::Bool(self.integer(args[0])? < self.integer(args[1])?)),
            "list" => self.make_list(args.to_vec()),
            "cons" => {
                let mut xs = self.copy_list(args[1])?;
                xs.insert(0, args[0]);
                self.make_list(xs)
            }
            "car" => Ok(self.list(args[0])?.first().copied().unwrap_or(1)),
            "cdr" => {
                self.spend(self.list(args[0])?.len())?;
                let xs = self.list(args[0])?.iter().skip(1).copied().collect();
                self.make_list(xs)
            }
            "dict" => {
                if args.len() % 2 != 0 {
                    return Err(Fault::Arity);
                }
                let mut xs = Vec::new();
                for pair in args.chunks_exact(2) {
                    self.data_key(pair[0], 0)?;
                    if let Some(i) = self.find(&xs, pair[0])? {
                        xs[i].1 = pair[1];
                    } else {
                        xs.push((pair[0], pair[1]));
                    }
                }
                self.alloc(Datum::Map(xs))
            }
            "get" | "has-key?" | "assoc" | "dissoc" => {
                self.data_key(args[1], 0)?;
                let mut xs = self.copy_map(args[0])?;
                let found = self.find(&xs, args[1])?;
                match op {
                    "get" => Ok(found.map(|i| xs[i].1).unwrap_or(1)),
                    "has-key?" => self.alloc(Datum::Bool(found.is_some())),
                    "assoc" => {
                        if let Some(i) = found {
                            xs[i].1 = args[2];
                        } else {
                            xs.push((args[1], args[2]));
                        }
                        self.alloc(Datum::Map(xs))
                    }
                    _ => {
                        if let Some(i) = found {
                            xs.remove(i);
                        }
                        self.alloc(Datum::Map(xs))
                    }
                }
            }
            "keys" => {
                self.spend(self.map(args[0])?.len())?;
                let keys = self.map(args[0])?.iter().map(|(k, _)| *k).collect();
                self.alloc(Datum::List(keys))
            }
            "count" => {
                if let Datum::String(s) = self.datum(args[0])? {
                    self.spend(s.len())?;
                }
                let n = match self.datum(args[0])? {
                    Datum::Nil => 0,
                    Datum::List(xs) => xs.len(),
                    Datum::Map(xs) => xs.len(),
                    Datum::String(s) => {
                        let n = s.chars().count();
                        let bytes = s.len();
                        self.spend(bytes)?;
                        n
                    }
                    _ => return Err(Fault::Type),
                };
                self.alloc(Datum::Int(i64::try_from(n).map_err(|_| Fault::Overflow)?))
            }
            "type-of" => {
                let kind = match self.datum(args[0])? {
                    Datum::Nil => "nil",
                    Datum::Bool(_) => "bool",
                    Datum::Int(_) => "int",
                    Datum::String(_) => "string",
                    Datum::Symbol(_) => "symbol",
                    Datum::List(_) => "list",
                    Datum::Map(_) => "map",
                    Datum::Closure { .. } | Datum::Builtin(_) => "callable",
                };
                self.alloc(Datum::Symbol(kind.into()))
            }
            "apply" => {
                let values = self.copy_list(args[1])?;
                self.call(args[0], &values)
            }
            "signal" => Err(Fault::Signaled),
            _ => Err(Fault::Internal),
        }
    }
}

#[allow(unsafe_code)]
unsafe extern "C" fn step(runtime: *mut Run<'_>, op: u64, a: u64, b: u64, c: u64) -> u64 {
    // SAFETY: generated code passes the live, uniquely reborrowed Run pointer
    // supplied by enter_native. No pointer is sourced from an Agel value.
    let run = unsafe { &mut *runtime };
    if run.error.is_some() {
        return 0;
    }
    // Never unwind a Rust panic through a generated-code frame.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run.dispatch(op as usize, a as usize, b as usize, c as usize)
    }));
    match result {
        Ok(Ok(value)) => value as u64,
        Ok(Err(error)) => {
            run.error = Some(error);
            0
        }
        Err(_) => {
            run.error = Some(Fault::Internal);
            0
        }
    }
}
