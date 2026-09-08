//! Validated, effect-free integer IR and its hosted native-code backend.

use agel_core::Value as AgelValue;
use cranelift_codegen::ir::{condcodes::IntCC, types, AbiParam, InstBuilder, MemFlags, Value};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{default_libcall_names, Linkage, Module};
use std::fmt;

pub mod managed;
pub mod state;

const MAX_NODES: usize = 256;
const MAX_DEPTH: usize = 32;
const MAX_ARGUMENTS: usize = 8;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Invalid(&'static str),
    Backend(String),
    Arity,
    Fuel,
    Overflow,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Int,
    Bool,
}

#[derive(Clone, Copy, Debug)]
enum Op {
    Add,
    Sub,
    Mul,
    Eq,
    Lt,
}

#[derive(Debug)]
enum Node {
    Int(i64),
    Bool(bool),
    Arg(usize),
    Binary(Op, usize, usize),
    If(usize, usize, usize),
}

struct Program {
    arity: usize,
    nodes: Vec<(Node, Kind)>,
    root: usize,
}

fn list(value: &AgelValue, length: usize) -> Result<&[AgelValue], Error> {
    match value {
        AgelValue::List(values) if values.len() == length => Ok(values),
        _ => Err(Error::Invalid("wrong IR list shape")),
    }
}
fn symbol(value: &AgelValue) -> Result<&str, Error> {
    match value {
        AgelValue::Symbol(name) => Ok(name),
        _ => Err(Error::Invalid("IR opcode must be a symbol")),
    }
}

impl Program {
    fn parse(ir: &AgelValue) -> Result<Self, Error> {
        let fields = list(ir, 3)?;
        if symbol(&fields[0])? != "agel/jit-v1" {
            return Err(Error::Invalid("unknown IR version"));
        }
        let AgelValue::Int(arity) = fields[1] else {
            return Err(Error::Invalid("invalid arity"));
        };
        let arity = usize::try_from(arity).map_err(|_| Error::Invalid("negative arity"))?;
        if arity > MAX_ARGUMENTS {
            return Err(Error::Invalid("too many arguments"));
        }
        let mut program = Self {
            arity,
            nodes: Vec::new(),
            root: 0,
        };
        program.root = program.node(&fields[2], 0)?;
        Ok(program)
    }

    fn node(&mut self, value: &AgelValue, depth: usize) -> Result<usize, Error> {
        if depth > MAX_DEPTH || self.nodes.len() >= MAX_NODES {
            return Err(Error::Invalid("IR resource limit"));
        }
        let AgelValue::List(fields) = value else {
            return Err(Error::Invalid("node must be a list"));
        };
        let op = symbol(fields.first().ok_or(Error::Invalid("empty node"))?)?;
        let (node, kind) = match op {
            "i64" => {
                let fields = list(value, 2)?;
                let AgelValue::Int(n) = fields[1] else {
                    return Err(Error::Invalid("invalid integer"));
                };
                (Node::Int(n), Kind::Int)
            }
            "bool" => {
                let fields = list(value, 2)?;
                let AgelValue::Bool(b) = fields[1] else {
                    return Err(Error::Invalid("invalid boolean"));
                };
                (Node::Bool(b), Kind::Bool)
            }
            "arg" => {
                let fields = list(value, 2)?;
                let AgelValue::Int(n) = fields[1] else {
                    return Err(Error::Invalid("invalid argument index"));
                };
                let index =
                    usize::try_from(n).map_err(|_| Error::Invalid("negative argument index"))?;
                if index >= self.arity {
                    return Err(Error::Invalid("argument out of bounds"));
                }
                (Node::Arg(index), Kind::Int)
            }
            "if" => {
                let fields = list(value, 4)?;
                let predicate = self.node(&fields[1], depth + 1)?;
                let yes = self.node(&fields[2], depth + 1)?;
                let no = self.node(&fields[3], depth + 1)?;
                let kind = self.nodes[yes].1;
                if kind != self.nodes[no].1 {
                    return Err(Error::Invalid("branch types differ"));
                }
                (Node::If(predicate, yes, no), kind)
            }
            "add" | "sub" | "mul" | "eq" | "lt" => {
                let fields = list(value, 3)?;
                let left = self.node(&fields[1], depth + 1)?;
                let right = self.node(&fields[2], depth + 1)?;
                if self.nodes[left].1 != Kind::Int || self.nodes[right].1 != Kind::Int {
                    return Err(Error::Invalid("binary operands must be integers"));
                }
                let (op, kind) = match op {
                    "add" => (Op::Add, Kind::Int),
                    "sub" => (Op::Sub, Kind::Int),
                    "mul" => (Op::Mul, Kind::Int),
                    "eq" => (Op::Eq, Kind::Bool),
                    _ => (Op::Lt, Kind::Bool),
                };
                (Node::Binary(op, left, right), kind)
            }
            _ => return Err(Error::Invalid("unknown opcode")),
        };
        if self.nodes.len() >= MAX_NODES {
            return Err(Error::Invalid("IR resource limit"));
        }
        let index = self.nodes.len();
        self.nodes.push((node, kind));
        Ok(index)
    }
}

// JITModule does not automatically release executable memory. This owner also
// cleans up partially compiled modules on errors. No code pointer is public.
struct ExecutableMemory(Option<JITModule>);
impl Drop for ExecutableMemory {
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        if let Some(module) = self.0.take() {
            // SAFETY: invocations borrow Compiled; no invocation can outlive this
            // owner. Both backends keep entry pointers private; any managed
            // callbacks have returned before the invocation borrow can end.
            unsafe {
                module.free_memory();
            }
        }
    }
}

/// Owns finalized code and its lifetime. Compilation is an explicit host action,
/// not an automatic capability available to Agel actors or the native kernel.
pub struct Compiled {
    _memory: ExecutableMemory,
    entry: *const u8,
    arity: usize,
    kind: Kind,
    fuel: u64,
    clif: String,
}

impl Compiled {
    pub fn compile(ir: &AgelValue) -> Result<Self, Error> {
        let program = Program::parse(ir)?;
        let builder =
            JITBuilder::new(default_libcall_names()).map_err(|e| Error::Backend(e.to_string()))?;
        let mut memory = ExecutableMemory(Some(JITModule::new(builder)));
        let module = memory.0.as_mut().expect("new module");
        let pointer = module.target_config().pointer_type();
        let mut context = module.make_context();
        context
            .func
            .signature
            .params
            .extend([AbiParam::new(pointer), AbiParam::new(pointer)]);
        context
            .func
            .signature
            .returns
            .push(AbiParam::new(types::I32));
        let id = module
            .declare_function("agel_entry", Linkage::Export, &context.func.signature)
            .map_err(|e| Error::Backend(e.to_string()))?;
        let mut builder_context = FunctionBuilderContext::new();
        {
            let mut b = FunctionBuilder::new(&mut context.func, &mut builder_context);
            let entry = b.create_block();
            let overflow = b.create_block();
            b.append_block_params_for_function_params(entry);
            b.switch_to_block(entry);
            let args = b.block_params(entry)[0];
            let output = b.block_params(entry)[1];
            let value = emit(&program, program.root, args, overflow, &mut b);
            b.ins().store(MemFlags::new(), value, output, 0);
            let success = b.ins().iconst(types::I32, 0);
            b.ins().return_(&[success]);
            b.switch_to_block(overflow);
            let failure = b.ins().iconst(types::I32, 1);
            b.ins().return_(&[failure]);
            b.seal_all_blocks();
            b.finalize();
        }
        let clif = context.func.display().to_string();
        module
            .define_function(id, &mut context)
            .map_err(|e| Error::Backend(e.to_string()))?;
        module
            .finalize_definitions()
            .map_err(|e| Error::Backend(e.to_string()))?;
        let entry = module.get_finalized_function(id);
        Ok(Self {
            _memory: memory,
            entry,
            arity: program.arity,
            kind: program.nodes[program.root].1,
            fuel: program.nodes.len() as u64,
            clif,
        })
    }

    /// Conservative per-invocation cost: every IR node, including untaken branches.
    /// This finite, nonrecursive tier has no loops, calls or external effects.
    pub fn required_fuel(&self) -> u64 {
        self.fuel
    }
    pub fn clif(&self) -> &str {
        &self.clif
    }

    pub fn invoke(&self, arguments: &[i64], fuel: u64) -> Result<AgelValue, Error> {
        if arguments.len() != self.arity {
            return Err(Error::Arity);
        }
        if fuel < self.fuel {
            return Err(Error::Fuel);
        }
        let value = self.enter(arguments)?;
        Ok(match self.kind {
            Kind::Int => AgelValue::Int(value),
            Kind::Bool => AgelValue::Bool(value != 0),
        })
    }

    #[allow(unsafe_code)]
    fn enter(&self, arguments: &[i64]) -> Result<i64, Error> {
        let mut result = 0_i64;
        // SAFETY: only the validator/emitter constructs entry. The declared
        // native C ABI takes two aligned i64 pointers and returns i32 status.
        // invoke checked arity; validated arg offsets stay inside this slice.
        // Generated code writes only this result slot, calls nothing, and the
        // executable owner remains borrowed for the duration of this invocation.
        let status = unsafe {
            let function: unsafe extern "C" fn(*const i64, *mut i64) -> i32 =
                std::mem::transmute(self.entry);
            function(arguments.as_ptr(), &mut result)
        };
        if status == 0 {
            Ok(result)
        } else {
            Err(Error::Overflow)
        }
    }
}

fn emit(
    program: &Program,
    index: usize,
    args: Value,
    overflow: cranelift_codegen::ir::Block,
    b: &mut FunctionBuilder<'_>,
) -> Value {
    match program.nodes[index].0 {
        Node::Int(n) => b.ins().iconst(types::I64, n),
        Node::Bool(value) => b.ins().iconst(types::I64, i64::from(value)),
        Node::Arg(index) => b
            .ins()
            .load(types::I64, MemFlags::new(), args, (index * 8) as i32),
        Node::Binary(op, left, right) => {
            let left = emit(program, left, args, overflow, b);
            let right = emit(program, right, args, overflow, b);
            if matches!(op, Op::Eq | Op::Lt) {
                let cc = if matches!(op, Op::Eq) {
                    IntCC::Equal
                } else {
                    IntCC::SignedLessThan
                };
                let flag = b.ins().icmp(cc, left, right);
                return b.ins().uextend(types::I64, flag);
            }
            let (value, flag) = match op {
                Op::Add => b.ins().sadd_overflow(left, right),
                Op::Sub => b.ins().ssub_overflow(left, right),
                Op::Mul => b.ins().smul_overflow(left, right),
                _ => unreachable!(),
            };
            let next = b.create_block();
            b.ins().brif(flag, overflow, &[], next, &[]);
            b.switch_to_block(next);
            value
        }
        Node::If(predicate, yes, no) => {
            let test = emit(program, predicate, args, overflow, b);
            // In Agel integer zero is truthy. Only Boolean false is false here.
            let test = if program.nodes[predicate].1 == Kind::Int {
                b.ins().iconst(types::I8, 1)
            } else {
                b.ins().icmp_imm(IntCC::NotEqual, test, 0)
            };
            let yes_block = b.create_block();
            let no_block = b.create_block();
            let join = b.create_block();
            b.append_block_param(join, types::I64);
            b.ins().brif(test, yes_block, &[], no_block, &[]);
            b.switch_to_block(yes_block);
            let yes = emit(program, yes, args, overflow, b);
            b.ins().jump(join, &[yes.into()]);
            b.switch_to_block(no_block);
            let no = emit(program, no, args, overflow, b);
            b.ins().jump(join, &[no.into()]);
            b.switch_to_block(join);
            b.block_params(join)[0]
        }
    }
}
