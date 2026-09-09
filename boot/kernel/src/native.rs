//! Fixed-memory Agel evaluator for the first native world.
//!
//! This is deliberately smaller than the hosted evaluator. It has no allocator,
//! and every parser, call, binding, and source buffer has a deterministic bound.

use core::mem;

const MAX_NODES: usize = 128;
const MAX_BINDINGS: usize = 24;
const MAX_NAME: usize = 24;
const MAX_PARAMS: usize = 4;
const MAX_LOCALS: usize = 8;
const MAX_BODY: usize = 192;
const MAX_ARGUMENTS: usize = 8;
const MAX_DEPTH: u8 = 24;
const INITIAL_FUEL: u16 = 2_000;
const MAX_AGENTS: usize = 8;
const MAX_MAILBOX: usize = 8;
const MAX_RUN_TURNS: usize = 32;
const MAX_SCENE_RECTS: usize = 12;
const NONE: u16 = u16::MAX;

/// Every fixed native resource bound, named and reported from the constants the
/// evaluator actually enforces. `:limits` renders this table, so the console can
/// never drift away from the implementation or the documentation.
#[cfg(not(feature = "native-selftest"))]
pub const LIMITS: &[(&str, u64)] = &[
    ("nodes", MAX_NODES as u64),
    ("globals", MAX_BINDINGS as u64),
    ("name", MAX_NAME as u64),
    ("params", MAX_PARAMS as u64),
    ("locals", MAX_LOCALS as u64),
    ("args", MAX_ARGUMENTS as u64),
    ("body", MAX_BODY as u64),
    ("depth", MAX_DEPTH as u64),
    ("fuel", INITIAL_FUEL as u64),
    ("agents", MAX_AGENTS as u64),
    ("mailbox", MAX_MAILBOX as u64),
    ("run-turns", MAX_RUN_TURNS as u64),
    ("scene-rects", MAX_SCENE_RECTS as u64),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Value {
    Int(i64),
    Bool(bool),
    Nil,
    Agent(u8),
    Code { start: u16, end: u16 },
    Function,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Error(pub &'static str);

#[derive(Clone, Copy)]
enum Scalar {
    Int(i64),
    Bool(bool),
    Nil,
    Agent(u8),
}

#[derive(Clone, Copy)]
struct Name {
    length: u8,
    bytes: [u8; MAX_NAME],
}

impl Name {
    const EMPTY: Self = Self {
        length: 0,
        bytes: [0; MAX_NAME],
    };

    fn new(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.is_empty() || bytes.len() > MAX_NAME {
            return Err(Error("symbol name exceeds native limit"));
        }
        let mut name = Self::EMPTY;
        name.bytes[..bytes.len()].copy_from_slice(bytes);
        name.length = bytes.len() as u8;
        Ok(name)
    }

    fn equals(self, bytes: &[u8]) -> bool {
        self.as_bytes() == bytes
    }

    fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.length as usize]
    }
}

#[derive(Clone, Copy)]
struct Function {
    parameter_count: u8,
    parameters: [Name; MAX_PARAMS],
    body_length: u16,
    body: [u8; MAX_BODY],
}

impl Function {
    const EMPTY: Self = Self {
        parameter_count: 0,
        parameters: [Name::EMPTY; MAX_PARAMS],
        body_length: 0,
        body: [0; MAX_BODY],
    };
}

#[allow(clippy::large_enum_variant)]
#[derive(Clone, Copy)]
enum StoredValue {
    Empty,
    Scalar(Scalar),
    Function(Function),
}

#[derive(Clone, Copy)]
struct Binding {
    name: Name,
    value: StoredValue,
}

impl Binding {
    const EMPTY: Self = Self {
        name: Name::EMPTY,
        value: StoredValue::Empty,
    };
}

/// One fixed-memory native actor. Behaviors and messages are deliberately
/// scalar at this bootstrap layer; richer protocols remain Agel libraries.
#[derive(Clone, Copy)]
struct Agent {
    used: bool,
    faulted: bool,
    behavior: Function,
    state: Scalar,
    mailbox: [Scalar; MAX_MAILBOX],
    mailbox_head: u8,
    mailbox_length: u8,
    turns: u64,
}

impl Agent {
    const EMPTY: Self = Self {
        used: false,
        faulted: false,
        behavior: Function::EMPTY,
        state: Scalar::Nil,
        mailbox: [Scalar::Nil; MAX_MAILBOX],
        mailbox_head: 0,
        mailbox_length: 0,
        turns: 0,
    };
}

#[derive(Clone, Copy)]
struct World {
    bindings: [Binding; MAX_BINDINGS],
    agents: [Agent; MAX_AGENTS],
    scheduler_cursor: u8,
    scheduler_active: bool,
    scene: [[u32; 7]; MAX_SCENE_RECTS],
    scene_count: u8,
    scene_ids: [i64; MAX_SCENE_RECTS],
    scene_owners: [u8; MAX_SCENE_RECTS],
}

impl World {
    const EMPTY: Self = Self {
        bindings: [Binding::EMPTY; MAX_BINDINGS],
        agents: [Agent::EMPTY; MAX_AGENTS],
        scheduler_cursor: 0,
        scheduler_active: false,
        scene: [[0; 7]; MAX_SCENE_RECTS],
        scene_count: 0,
        scene_ids: [0; MAX_SCENE_RECTS],
        scene_owners: [0; MAX_SCENE_RECTS],
    };

    fn find(&self, name: &[u8]) -> Option<usize> {
        self.bindings.iter().position(|binding| {
            !matches!(binding.value, StoredValue::Empty) && binding.name.equals(name)
        })
    }

    fn define(&mut self, name: &[u8], value: StoredValue) -> Result<(), Error> {
        let index = self
            .find(name)
            .or_else(|| {
                self.bindings
                    .iter()
                    .position(|binding| matches!(binding.value, StoredValue::Empty))
            })
            .ok_or(Error("native binding table is full"))?;
        self.bindings[index] = Binding {
            name: Name::new(name)?,
            value,
        };
        Ok(())
    }
}

/// A transactional native world with one committed rollback point.
pub struct Session {
    active: World,
    previous: World,
    scratch: World,
    has_previous: bool,
    revision: u64,
    candidate_revision: Option<u64>,
}

impl Session {
    #[cfg(any(feature = "isolation-selftest", test))]
    pub fn scene_count(&self) -> usize {
        self.active.scene_count as usize
    }

    #[cfg(any(feature = "isolation-selftest", test))]
    pub fn scene_record(&self, index: usize) -> Option<[u32; 7]> {
        if index < self.scene_count() {
            Some(self.active.scene[index])
        } else {
            None
        }
    }
    #[cfg(any(feature = "isolation-selftest", test))]
    pub fn candidate_scene(&self, index: usize) -> Option<(usize, Option<[u32; 7]>)> {
        if self.candidate_revision != Some(self.revision) {
            return None;
        }
        let count = self.scratch.scene_count as usize;
        Some((count, (index < count).then(|| self.scratch.scene[index])))
    }
    pub const fn new() -> Self {
        Self {
            active: World::EMPTY,
            previous: World::EMPTY,
            scratch: World::EMPTY,
            has_previous: false,
            revision: 0,
            candidate_revision: None,
        }
    }

    pub fn evaluate(&mut self, source: &[u8]) -> Result<Value, Error> {
        self.candidate_revision = None;
        let next_revision = self
            .revision
            .checked_add(1)
            .ok_or(Error("revision exhausted"))?;
        self.scratch = self.active;
        let result = evaluate_source(&mut self.scratch, source);
        match result {
            Ok(value) => {
                mem::swap(&mut self.previous, &mut self.active);
                mem::swap(&mut self.active, &mut self.scratch);
                self.has_previous = true;
                self.revision = next_revision;
                Ok(value)
            }
            Err(error) => Err(error),
        }
    }

    /// Candidate evaluation preserves both the live world and its rollback point.
    #[cfg(any(feature = "isolation-selftest", test))]
    pub fn preview(&mut self, source: &[u8]) -> Result<(), Error> {
        self.candidate_revision = None;
        self.scratch = self.active;
        evaluate_source(&mut self.scratch, source)?;
        if self
            .scratch
            .agents
            .iter()
            .zip(self.active.agents.iter())
            .any(|(after, before)| after.faulted && !before.faulted)
        {
            return Err(Error("candidate agent turn failed"));
        }
        self.candidate_revision = Some(self.revision);
        Ok(())
    }

    #[cfg(any(feature = "isolation-selftest", test))]
    pub fn promote(&mut self) -> Result<(), Error> {
        if self.candidate_revision != Some(self.revision) {
            return Err(Error("no current candidate; preview again"));
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(Error("revision exhausted"))?;
        mem::swap(&mut self.previous, &mut self.active);
        mem::swap(&mut self.active, &mut self.scratch);
        self.has_previous = true;
        self.revision = revision;
        self.candidate_revision = None;
        Ok(())
    }

    #[cfg(any(feature = "isolation-selftest", test))]
    pub fn discard(&mut self) {
        self.candidate_revision = None;
    }

    /// Reconstruct saved source without resetting the running world.
    #[cfg(any(feature = "isolation-selftest", test))]
    pub fn begin_rebuild(&mut self) {
        self.scratch = World::EMPTY;
        self.candidate_revision = Some(self.revision);
    }

    #[cfg(any(feature = "isolation-selftest", test))]
    pub fn stage(&mut self, source: &[u8]) -> Result<(), Error> {
        if self.candidate_revision != Some(self.revision) {
            return Err(Error("no current source candidate"));
        }
        self.candidate_revision = None;
        evaluate_source(&mut self.scratch, source)?;
        if self.scratch.agents.iter().any(|agent| agent.faulted) {
            return Err(Error("source candidate agent turn failed"));
        }
        self.candidate_revision = Some(self.revision);
        Ok(())
    }

    #[cfg(any(feature = "isolation-selftest", test))]
    pub fn agent_source(&self, id: u8) -> Result<&[u8], Error> {
        let agent = &self.active.agents[agent_index(&self.active, id)?];
        Ok(&agent.behavior.body[..agent.behavior.body_length as usize])
    }

    /// Clear language state without reusing a revision identifier.
    #[cfg(feature = "isolation-selftest")]
    pub fn reset(&mut self) {
        self.candidate_revision = None;
        self.active = World::EMPTY;
        self.previous = World::EMPTY;
        self.scratch = World::EMPTY;
        self.has_previous = false;
    }

    pub fn rollback(&mut self) -> Result<(), Error> {
        self.candidate_revision = None;
        if !self.has_previous {
            return Err(Error("no previous native revision"));
        }
        let next_revision = self
            .revision
            .checked_add(1)
            .ok_or(Error("revision exhausted"))?;
        mem::swap(&mut self.active, &mut self.previous);
        self.has_previous = false;
        self.revision = next_revision;
        Ok(())
    }

    #[cfg(not(feature = "native-selftest"))]
    pub fn revision(&self) -> u64 {
        self.revision
    }

    #[cfg(not(feature = "native-selftest"))]
    pub fn binding_count(&self) -> usize {
        self.active
            .bindings
            .iter()
            .filter(|binding| !matches!(binding.value, StoredValue::Empty))
            .count()
    }

    #[cfg(not(feature = "native-selftest"))]
    pub fn binding_name(&self, index: usize) -> Option<&[u8]> {
        self.active
            .bindings
            .iter()
            .filter(|binding| !matches!(binding.value, StoredValue::Empty))
            .nth(index)
            .map(|binding| binding.name.as_bytes())
    }

    #[cfg(feature = "native-selftest")]
    pub fn integer(&self, name: &[u8]) -> Option<i64> {
        let binding = &self.active.bindings[self.active.find(name)?];
        match binding.value {
            StoredValue::Scalar(Scalar::Int(value)) => Some(value),
            _ => None,
        }
    }
}

#[derive(Clone, Copy)]
enum NodeKind {
    Empty,
    List,
    Symbol,
    Int(i64),
    Bool(bool),
    Nil,
    Quote,
}

#[derive(Clone, Copy)]
struct Node {
    kind: NodeKind,
    first: u16,
    next: u16,
    start: u16,
    end: u16,
}

impl Node {
    const EMPTY: Self = Self {
        kind: NodeKind::Empty,
        first: NONE,
        next: NONE,
        start: 0,
        end: 0,
    };
}

struct Document {
    nodes: [Node; MAX_NODES],
    length: u16,
    root: u16,
}

impl Document {
    const fn new() -> Self {
        Self {
            nodes: [Node::EMPTY; MAX_NODES],
            length: 0,
            root: NONE,
        }
    }

    fn allocate(&mut self, node: Node) -> Result<u16, Error> {
        if self.length as usize == MAX_NODES {
            return Err(Error("native syntax arena is full"));
        }
        let index = self.length;
        self.nodes[index as usize] = node;
        self.length += 1;
        Ok(index)
    }
}

struct Parser<'a> {
    source: &'a [u8],
    position: usize,
    document: Document,
}

impl<'a> Parser<'a> {
    fn parse(source: &'a [u8]) -> Result<Document, Error> {
        let mut parser = Self {
            source,
            position: 0,
            document: Document::new(),
        };
        parser.skip_space();
        if parser.position == source.len() {
            return Err(Error("empty form"));
        }
        let root = parser.expression(0)?;
        parser.skip_space();
        if parser.position != source.len() {
            return Err(Error("expected one native form"));
        }
        parser.document.root = root;
        Ok(parser.document)
    }

    fn expression(&mut self, depth: u8) -> Result<u16, Error> {
        if depth >= MAX_DEPTH {
            return Err(Error("native reader depth exceeded"));
        }
        self.skip_space();
        let start = self.position;
        match self.source.get(self.position).copied() {
            Some(b'(') => self.list(depth + 1),
            Some(b')') => Err(Error("unexpected closing parenthesis")),
            Some(b'\'') => {
                self.position += 1;
                let child = self.expression(depth + 1)?;
                let end = self.document.nodes[child as usize].end;
                self.document.allocate(Node {
                    kind: NodeKind::Quote,
                    first: child,
                    next: NONE,
                    start: start as u16,
                    end,
                })
            }
            Some(_) => self.atom(),
            None => Err(Error("unexpected end of input")),
        }
    }

    fn list(&mut self, depth: u8) -> Result<u16, Error> {
        let start = self.position;
        self.position += 1;
        let list = self.document.allocate(Node {
            kind: NodeKind::List,
            first: NONE,
            next: NONE,
            start: start as u16,
            end: 0,
        })?;
        let mut last = NONE;
        loop {
            self.skip_space();
            match self.source.get(self.position).copied() {
                Some(b')') => {
                    self.position += 1;
                    self.document.nodes[list as usize].end = self.position as u16;
                    return Ok(list);
                }
                None => return Err(Error("unclosed list")),
                _ => {
                    let child = self.expression(depth)?;
                    if last == NONE {
                        self.document.nodes[list as usize].first = child;
                    } else {
                        self.document.nodes[last as usize].next = child;
                    }
                    last = child;
                }
            }
        }
    }

    fn atom(&mut self) -> Result<u16, Error> {
        let start = self.position;
        while let Some(byte) = self.source.get(self.position).copied() {
            if byte.is_ascii_whitespace() || matches!(byte, b'(' | b')' | b';') {
                break;
            }
            self.position += 1;
        }
        let atom = &self.source[start..self.position];
        let kind = match atom {
            b"nil" => NodeKind::Nil,
            b"#t" => NodeKind::Bool(true),
            b"#f" => NodeKind::Bool(false),
            _ => match parse_integer(atom) {
                Some(value) => NodeKind::Int(value),
                None => NodeKind::Symbol,
            },
        };
        self.document.allocate(Node {
            kind,
            first: NONE,
            next: NONE,
            start: start as u16,
            end: self.position as u16,
        })
    }

    fn skip_space(&mut self) {
        loop {
            while self
                .source
                .get(self.position)
                .is_some_and(u8::is_ascii_whitespace)
            {
                self.position += 1;
            }
            if self.source.get(self.position) == Some(&b';') {
                while self
                    .source
                    .get(self.position)
                    .is_some_and(|byte| *byte != b'\n')
                {
                    self.position += 1;
                }
            } else {
                return;
            }
        }
    }
}

fn parse_integer(bytes: &[u8]) -> Option<i64> {
    if bytes.is_empty() || bytes == b"-" {
        return None;
    }
    let (negative, digits) = if bytes[0] == b'-' {
        (true, &bytes[1..])
    } else {
        (false, bytes)
    };
    let mut value = 0_i64;
    for digit in digits {
        if !digit.is_ascii_digit() {
            return None;
        }
        value = if negative {
            value
                .checked_mul(10)?
                .checked_sub(i64::from(digit - b'0'))?
        } else {
            value
                .checked_mul(10)?
                .checked_add(i64::from(digit - b'0'))?
        };
    }
    Some(value)
}

#[allow(clippy::large_enum_variant)]
#[derive(Clone, Copy)]
enum RuntimeValue {
    Scalar(Scalar),
    Code(u16),
    Function(Function),
    Lambda {
        node: u16,
        captures: [CapturedLocal; MAX_LOCALS],
    },
    Builtin(Builtin),
}

#[derive(Clone, Copy)]
enum Builtin {
    SceneBind,
    SceneHit,
    SceneOwner,
    AgentBecome,
    SceneClear,
    SceneRect,
    SceneCount,
    Add,
    Subtract,
    Multiply,
    Divide,
    Equal,
    Less,
    Eval,
    Spawn,
    Send,
    Step,
    Run,
    AgentState,
    AgentPending,
    AgentTurns,
    AgentFaulted,
    RestartAgent,
    DropMessage,
    AgentCount,
}

#[derive(Clone, Copy)]
struct CapturedLocal {
    used: bool,
    name: Name,
    value: Scalar,
}

impl CapturedLocal {
    const EMPTY: Self = Self {
        used: false,
        name: Name::EMPTY,
        value: Scalar::Nil,
    };
}

#[derive(Clone, Copy)]
struct Local {
    used: bool,
    name: Name,
    value: RuntimeValue,
}

impl Local {
    const EMPTY: Self = Self {
        used: false,
        name: Name::EMPTY,
        value: RuntimeValue::Scalar(Scalar::Nil),
    };
}

fn capture_locals(locals: &[Local; MAX_LOCALS]) -> Result<[CapturedLocal; MAX_LOCALS], Error> {
    let mut captures = [CapturedLocal::EMPTY; MAX_LOCALS];
    for (index, local) in locals.iter().filter(|local| local.used).enumerate() {
        let value = match local.value {
            RuntimeValue::Scalar(value) => value,
            _ => {
                return Err(Error(
                    "native closures currently capture scalar values only",
                ))
            }
        };
        captures[index] = CapturedLocal {
            used: true,
            name: local.name,
            value,
        };
    }
    Ok(captures)
}

fn evaluate_source(world: &mut World, source: &[u8]) -> Result<Value, Error> {
    if source.len() > u16::MAX as usize {
        return Err(Error("native source is too long"));
    }
    let document = Parser::parse(source)?;
    let mut fuel = INITIAL_FUEL;
    let runtime = evaluate_node(
        &document,
        source,
        document.root,
        world,
        &[Local::EMPTY; MAX_LOCALS],
        0,
        &mut fuel,
    )?;
    public_value(runtime, &document)
}

fn public_value(value: RuntimeValue, document: &Document) -> Result<Value, Error> {
    match value {
        RuntimeValue::Scalar(Scalar::Int(value)) => Ok(Value::Int(value)),
        RuntimeValue::Scalar(Scalar::Bool(value)) => Ok(Value::Bool(value)),
        RuntimeValue::Scalar(Scalar::Nil) => Ok(Value::Nil),
        RuntimeValue::Scalar(Scalar::Agent(id)) => Ok(Value::Agent(id)),
        RuntimeValue::Code(node) => {
            let node = document.nodes[node as usize];
            Ok(Value::Code {
                start: node.start,
                end: node.end,
            })
        }
        RuntimeValue::Function(_) | RuntimeValue::Lambda { .. } | RuntimeValue::Builtin(_) => {
            Ok(Value::Function)
        }
    }
}

fn evaluate_node(
    document: &Document,
    source: &[u8],
    node_index: u16,
    world: &mut World,
    locals: &[Local; MAX_LOCALS],
    depth: u8,
    fuel: &mut u16,
) -> Result<RuntimeValue, Error> {
    if depth >= MAX_DEPTH {
        return Err(Error("native call depth exceeded"));
    }
    *fuel = fuel
        .checked_sub(1)
        .ok_or(Error("native evaluator fuel exhausted"))?;
    let node = document.nodes[node_index as usize];
    match node.kind {
        NodeKind::Int(value) => Ok(RuntimeValue::Scalar(Scalar::Int(value))),
        NodeKind::Bool(value) => Ok(RuntimeValue::Scalar(Scalar::Bool(value))),
        NodeKind::Nil => Ok(RuntimeValue::Scalar(Scalar::Nil)),
        NodeKind::Symbol => resolve_symbol(document, source, node_index, world, locals),
        NodeKind::Quote => Ok(RuntimeValue::Code(node.first)),
        NodeKind::List => evaluate_list(document, source, node_index, world, locals, depth, fuel),
        NodeKind::Empty => Err(Error("invalid native syntax node")),
    }
}

fn evaluate_list(
    document: &Document,
    source: &[u8],
    list_index: u16,
    world: &mut World,
    locals: &[Local; MAX_LOCALS],
    depth: u8,
    fuel: &mut u16,
) -> Result<RuntimeValue, Error> {
    let first = document.nodes[list_index as usize].first;
    if first == NONE {
        return Ok(RuntimeValue::Scalar(Scalar::Nil));
    }
    if symbol_is(document, source, first, b"quote") {
        let value = exactly_one_argument(document, first)?;
        return Ok(RuntimeValue::Code(value));
    }
    if symbol_is(document, source, first, b"if") {
        let condition = child_after(document, first)?;
        let consequent = child_after(document, condition)?;
        let alternative = child_after(document, consequent)?;
        if document.nodes[alternative as usize].next != NONE {
            return Err(Error("if expects three arguments"));
        }
        let condition = evaluate_node(document, source, condition, world, locals, depth + 1, fuel)?;
        return evaluate_node(
            document,
            source,
            if truthy(condition) {
                consequent
            } else {
                alternative
            },
            world,
            locals,
            depth + 1,
            fuel,
        );
    }
    if symbol_is(document, source, first, b"begin") {
        let mut child = document.nodes[first as usize].next;
        if child == NONE {
            return Ok(RuntimeValue::Scalar(Scalar::Nil));
        }
        let mut result = RuntimeValue::Scalar(Scalar::Nil);
        while child != NONE {
            result = evaluate_node(document, source, child, world, locals, depth + 1, fuel)?;
            child = document.nodes[child as usize].next;
        }
        return Ok(result);
    }
    if symbol_is(document, source, first, b"def") {
        return evaluate_def(document, source, first, world, locals, depth, fuel);
    }
    if symbol_is(document, source, first, b"let") {
        return evaluate_let(document, source, first, world, locals, depth, fuel);
    }
    if symbol_is(document, source, first, b"fn") {
        validate_lambda(document, source, first)?;
        return Ok(RuntimeValue::Lambda {
            node: first,
            captures: capture_locals(locals)?,
        });
    }

    let callable = evaluate_node(document, source, first, world, locals, depth + 1, fuel)?;
    let mut arguments = [RuntimeValue::Scalar(Scalar::Nil); MAX_ARGUMENTS];
    let mut count = 0;
    let mut child = document.nodes[first as usize].next;
    while child != NONE {
        if count == MAX_ARGUMENTS {
            return Err(Error("native argument limit exceeded"));
        }
        arguments[count] = evaluate_node(document, source, child, world, locals, depth + 1, fuel)?;
        count += 1;
        child = document.nodes[child as usize].next;
    }
    match callable {
        RuntimeValue::Builtin(builtin) => apply_builtin(
            builtin,
            &arguments[..count],
            document,
            source,
            world,
            locals,
            depth,
            fuel,
        ),
        RuntimeValue::Function(function) => {
            apply_stored_function(function, &arguments[..count], world, depth, fuel)
        }
        RuntimeValue::Lambda { node, captures } => apply_lambda(
            node,
            &captures,
            &arguments[..count],
            document,
            source,
            world,
            depth,
            fuel,
        ),
        _ => Err(Error("first list value is not callable")),
    }
}

fn evaluate_def(
    document: &Document,
    source: &[u8],
    def_node: u16,
    world: &mut World,
    locals: &[Local; MAX_LOCALS],
    depth: u8,
    fuel: &mut u16,
) -> Result<RuntimeValue, Error> {
    let name_node = child_after(document, def_node)?;
    if !matches!(document.nodes[name_node as usize].kind, NodeKind::Symbol) {
        return Err(Error("def name must be a symbol"));
    }
    let value_node = child_after(document, name_node)?;
    if document.nodes[value_node as usize].next != NONE {
        return Err(Error("def expects a name and value"));
    }
    let name = node_bytes(document, source, name_node);
    if matches!(
        name,
        b"quote"
            | b"if"
            | b"begin"
            | b"def"
            | b"let"
            | b"fn"
            | b"+"
            | b"-"
            | b"*"
            | b"/"
            | b"="
            | b"<"
            | b"eval"
            | b"spawn"
            | b"send"
            | b"step"
            | b"run"
            | b"agent-state"
            | b"agent-pending"
            | b"agent-turns"
            | b"agent-faulted?"
            | b"restart-agent"
            | b"drop-message"
            | b"agent-count"
            | b"scene-clear"
            | b"scene-bind"
            | b"scene-hit"
            | b"scene-owner"
            | b"agent-become"
            | b"scene-rect"
            | b"scene-count"
    ) {
        return Err(Error("native core names cannot be redefined"));
    }
    let value = evaluate_node(document, source, value_node, world, locals, depth + 1, fuel)?;
    let stored = match value {
        RuntimeValue::Scalar(scalar) => StoredValue::Scalar(scalar),
        RuntimeValue::Lambda { node, captures } => {
            if captures.iter().any(|capture| capture.used) {
                return Err(Error(
                    "persisted native closures cannot capture lexical state yet",
                ));
            }
            StoredValue::Function(capture_function(document, source, node)?)
        }
        RuntimeValue::Function(function) => StoredValue::Function(function),
        RuntimeValue::Code(_) => return Err(Error("quoted code is transaction-local in v0.1.1")),
        RuntimeValue::Builtin(_) => return Err(Error("native builtins cannot be rebound")),
    };
    world.define(name, stored)?;
    Ok(match stored {
        StoredValue::Scalar(scalar) => RuntimeValue::Scalar(scalar),
        StoredValue::Function(function) => RuntimeValue::Function(function),
        StoredValue::Empty => return Err(Error("cannot define empty value")),
    })
}

/// Parallel `let`: every initializer sees the enclosing scope, then the body
/// forms run in sequence with the new bindings. A repeated name takes its last
/// value, matching the hosted seed. Bindings occupy the same bounded local
/// slots as parameters, so `:limits` still describes the whole scope.
fn evaluate_let(
    document: &Document,
    source: &[u8],
    let_node: u16,
    world: &mut World,
    locals: &[Local; MAX_LOCALS],
    depth: u8,
    fuel: &mut u16,
) -> Result<RuntimeValue, Error> {
    let bindings = child_after(document, let_node)?;
    if !matches!(document.nodes[bindings as usize].kind, NodeKind::List) {
        return Err(Error("let expects a binding list"));
    }
    let body = child_after(document, bindings)?;
    let mut inner = *locals;
    let mut pair = document.nodes[bindings as usize].first;
    while pair != NONE {
        if !matches!(document.nodes[pair as usize].kind, NodeKind::List) {
            return Err(Error("let binding must be a name and value"));
        }
        let name_node = document.nodes[pair as usize].first;
        if name_node == NONE || !matches!(document.nodes[name_node as usize].kind, NodeKind::Symbol)
        {
            return Err(Error("let binding must be a name and value"));
        }
        let value_node = child_after(document, name_node)?;
        if document.nodes[value_node as usize].next != NONE {
            return Err(Error("let binding must be a name and value"));
        }
        let name_bytes = node_bytes(document, source, name_node);
        if is_special_form(name_bytes) {
            return Err(Error("native special forms cannot be rebound"));
        }
        let name = Name::new(name_bytes)?;
        let value = evaluate_node(document, source, value_node, world, locals, depth + 1, fuel)?;
        let slot = inner
            .iter()
            .position(|local| local.used && local.name.equals(name_bytes))
            .or_else(|| inner.iter().position(|local| !local.used))
            .ok_or(Error("native local binding limit exceeded"))?;
        inner[slot] = Local {
            used: true,
            name,
            value,
        };
        pair = document.nodes[pair as usize].next;
    }
    evaluate_sequence(document, source, body, world, &inner, depth, fuel)
}

fn evaluate_sequence(
    document: &Document,
    source: &[u8],
    mut child: u16,
    world: &mut World,
    locals: &[Local; MAX_LOCALS],
    depth: u8,
    fuel: &mut u16,
) -> Result<RuntimeValue, Error> {
    let mut result = RuntimeValue::Scalar(Scalar::Nil);
    while child != NONE {
        result = evaluate_node(document, source, child, world, locals, depth + 1, fuel)?;
        child = document.nodes[child as usize].next;
    }
    Ok(result)
}

fn is_special_form(name: &[u8]) -> bool {
    matches!(name, b"quote" | b"if" | b"begin" | b"def" | b"let" | b"fn")
}

fn validate_lambda(document: &Document, source: &[u8], fn_node: u16) -> Result<(), Error> {
    let parameters = child_after(document, fn_node)?;
    if !matches!(document.nodes[parameters as usize].kind, NodeKind::List) {
        return Err(Error("fn parameters must be a list"));
    }
    child_after(document, parameters)?;
    let mut parameter = document.nodes[parameters as usize].first;
    let mut count = 0;
    while parameter != NONE {
        if count == MAX_PARAMS {
            return Err(Error("native parameter limit exceeded"));
        }
        if !matches!(document.nodes[parameter as usize].kind, NodeKind::Symbol) {
            return Err(Error("fn parameter must be a symbol"));
        }
        let name = node_bytes(document, source, parameter);
        Name::new(name)?;
        let mut earlier = document.nodes[parameters as usize].first;
        while earlier != parameter {
            if node_bytes(document, source, earlier) == name {
                return Err(Error("native fn parameters must be unique"));
            }
            earlier = document.nodes[earlier as usize].next;
        }
        count += 1;
        parameter = document.nodes[parameter as usize].next;
    }
    Ok(())
}

fn capture_function(document: &Document, source: &[u8], fn_node: u16) -> Result<Function, Error> {
    validate_lambda(document, source, fn_node)?;
    let parameters = child_after(document, fn_node)?;
    let body_node = child_after(document, parameters)?;
    let mut function = Function::EMPTY;
    if document.nodes[body_node as usize].next == NONE {
        let body_source = node_bytes(document, source, body_node);
        if body_source.len() > MAX_BODY {
            return Err(Error("native function body is too large"));
        }
        function.body[..body_source.len()].copy_from_slice(body_source);
        function.body_length = body_source.len() as u16;
    } else {
        // Several body forms persist as one explicit sequence, so the stored
        // representation stays a single balanced form that `:source` can show.
        let mut last = body_node;
        while document.nodes[last as usize].next != NONE {
            last = document.nodes[last as usize].next;
        }
        let start = document.nodes[body_node as usize].start as usize;
        let end = document.nodes[last as usize].end as usize;
        let span = &source[start..end];
        let length = span.len() + b"(begin )".len();
        if length > MAX_BODY {
            return Err(Error("native function body is too large"));
        }
        function.body[..7].copy_from_slice(b"(begin ");
        function.body[7..7 + span.len()].copy_from_slice(span);
        function.body[7 + span.len()] = b')';
        function.body_length = length as u16;
    }
    let mut parameter = document.nodes[parameters as usize].first;
    while parameter != NONE {
        let index = function.parameter_count as usize;
        function.parameters[index] = Name::new(node_bytes(document, source, parameter))?;
        function.parameter_count += 1;
        parameter = document.nodes[parameter as usize].next;
    }
    Ok(function)
}

fn apply_stored_function(
    function: Function,
    arguments: &[RuntimeValue],
    world: &mut World,
    depth: u8,
    fuel: &mut u16,
) -> Result<RuntimeValue, Error> {
    if arguments.len() != function.parameter_count as usize {
        return Err(Error("native function arity mismatch"));
    }
    let mut locals = [Local::EMPTY; MAX_LOCALS];
    for (position, value) in arguments.iter().copied().enumerate() {
        if !matches!(value, RuntimeValue::Scalar(_)) {
            return Err(Error("stored functions currently accept scalar arguments"));
        }
        locals[position] = Local {
            used: true,
            name: function.parameters[position],
            value,
        };
    }
    let source = &function.body[..function.body_length as usize];
    let document = Parser::parse(source)?;
    let result = evaluate_node(
        &document,
        source,
        document.root,
        world,
        &locals,
        depth + 1,
        fuel,
    )?;
    if matches!(result, RuntimeValue::Code(_) | RuntimeValue::Lambda { .. }) {
        return Err(Error("ephemeral value escaped a stored function"));
    }
    Ok(result)
}

#[allow(clippy::too_many_arguments)]
fn apply_lambda(
    fn_node: u16,
    captures: &[CapturedLocal; MAX_LOCALS],
    arguments: &[RuntimeValue],
    document: &Document,
    source: &[u8],
    world: &mut World,
    depth: u8,
    fuel: &mut u16,
) -> Result<RuntimeValue, Error> {
    validate_lambda(document, source, fn_node)?;
    let parameters = child_after(document, fn_node)?;
    let body = child_after(document, parameters)?;
    let mut locals = [Local::EMPTY; MAX_LOCALS];
    let mut parameter = document.nodes[parameters as usize].first;
    let mut position = 0;
    while parameter != NONE {
        if position >= arguments.len() {
            return Err(Error("native lambda arity mismatch"));
        }
        let slot = locals
            .iter()
            .position(|local| !local.used)
            .ok_or(Error("native local binding limit exceeded"))?;
        locals[slot] = Local {
            used: true,
            name: Name::new(node_bytes(document, source, parameter))?,
            value: arguments[position],
        };
        position += 1;
        parameter = document.nodes[parameter as usize].next;
    }
    if position != arguments.len() {
        return Err(Error("native lambda arity mismatch"));
    }
    for captured in captures.iter().filter(|local| local.used) {
        if locals
            .iter()
            .any(|local| local.used && local.name.equals(captured.name.as_bytes()))
        {
            continue;
        }
        let slot = locals
            .iter()
            .position(|local| !local.used)
            .ok_or(Error("native local binding limit exceeded"))?;
        locals[slot] = Local {
            used: true,
            name: captured.name,
            value: RuntimeValue::Scalar(captured.value),
        };
    }
    evaluate_sequence(document, source, body, world, &locals, depth, fuel)
}

#[allow(clippy::too_many_arguments)]
fn apply_builtin(
    builtin: Builtin,
    arguments: &[RuntimeValue],
    document: &Document,
    source: &[u8],
    world: &mut World,
    locals: &[Local; MAX_LOCALS],
    depth: u8,
    fuel: &mut u16,
) -> Result<RuntimeValue, Error> {
    match builtin {
        Builtin::SceneClear | Builtin::SceneCount => {
            if !arguments.is_empty() {
                return Err(Error("scene-clear/count expect no arguments"));
            }
            if matches!(builtin, Builtin::SceneClear) {
                world.scene_count = 0;
                world.scene_ids.fill(0);
                world.scene_owners.fill(0);
            }
            return Ok(RuntimeValue::Scalar(Scalar::Int(i64::from(
                world.scene_count,
            ))));
        }
        Builtin::SceneRect => {
            if arguments.len() != 6 {
                return Err(Error("scene-rect expects x y width height radius rgb"));
            }
            let mut record = [0_u32; 7];
            record[0] = 2;
            for (index, value) in arguments.iter().enumerate() {
                let RuntimeValue::Scalar(Scalar::Int(value)) = value else {
                    return Err(Error("scene-rect expects integers"));
                };
                record[index + 1] = u32::try_from(*value)
                    .map_err(|_| Error("scene rectangle value out of range"))?;
            }
            let [_, x, y, width, height, radius, color] = record;
            if width == 0
                || height == 0
                || x > 1024
                || y > 684
                || width > 1024 - x
                || height > 684 - y
                || radius > width / 2
                || radius > height / 2
                || color > 0xffffff
            {
                return Err(Error("scene rectangle exceeds drawable bounds"));
            }
            if world.scene_count as usize == MAX_SCENE_RECTS {
                return Err(Error("native scene command limit exceeded"));
            }
            world.scene[world.scene_count as usize] = record;
            world.scene_count += 1;
            return Ok(RuntimeValue::Scalar(Scalar::Int(i64::from(
                world.scene_count,
            ))));
        }
        Builtin::SceneBind => {
            let [RuntimeValue::Scalar(Scalar::Int(id)), RuntimeValue::Scalar(Scalar::Agent(owner))] =
                arguments
            else {
                return Err(Error(
                    "scene-bind expects positive identity and owner agent",
                ));
            };
            agent_index(world, *owner)?;
            let count = world.scene_count as usize;
            if *id <= 0 || count == 0 || world.scene_ids[..count].contains(id) {
                return Err(Error("scene identity must be positive and unique"));
            }
            world.scene_ids[count - 1] = *id;
            world.scene_owners[count - 1] = *owner;
            return Ok(RuntimeValue::Scalar(Scalar::Int(*id)));
        }
        Builtin::SceneOwner => {
            let [RuntimeValue::Scalar(Scalar::Int(id))] = arguments else {
                return Err(Error("scene-owner expects an identity"));
            };
            let index = world.scene_ids[..world.scene_count as usize]
                .iter()
                .position(|value| *value == *id && *id > 0)
                .ok_or(Error("no such scene identity"))?;
            return Ok(RuntimeValue::Scalar(Scalar::Agent(
                world.scene_owners[index],
            )));
        }
        Builtin::SceneHit => {
            let [RuntimeValue::Scalar(Scalar::Int(px)), RuntimeValue::Scalar(Scalar::Int(py))] =
                arguments
            else {
                return Err(Error("scene-hit expects x y"));
            };
            for index in (0..world.scene_count as usize).rev() {
                let [_, x, y, w, h, r, _] = world.scene[index].map(i64::from);
                if *px < x || *py < y || *px >= x + w || *py >= y + h {
                    continue;
                }
                // Match the compositor's discrete rounded-corner convention.
                let (lx, ly) = (*px - x, *py - y);
                let (dx, dy) = if lx < r && ly < r {
                    (r - lx, r - ly)
                } else if lx >= w - r && ly < r {
                    (lx - (w - r - 1), r - ly)
                } else if lx < r && ly >= h - r {
                    (r - lx, ly - (h - r - 1))
                } else if lx >= w - r && ly >= h - r {
                    (lx - (w - r - 1), ly - (h - r - 1))
                } else {
                    (0, 0)
                };
                if dx * dx + dy * dy > r * r {
                    continue;
                }
                return Ok(RuntimeValue::Scalar(Scalar::Int(world.scene_ids[index])));
            }
            return Ok(RuntimeValue::Scalar(Scalar::Int(0)));
        }
        Builtin::AgentBecome => {
            let [RuntimeValue::Scalar(Scalar::Agent(id)), RuntimeValue::Function(behavior)] =
                arguments
            else {
                return Err(Error("agent-become expects agent and stored function"));
            };
            if world.scheduler_active || behavior.parameter_count != 3 {
                return Err(Error(
                    "behavior replacement requires operator and three parameters",
                ));
            }
            let index = agent_index(world, *id)?;
            if world.agents[index].faulted {
                return Err(Error("restart agent before replacing behavior"));
            }
            world.agents[index].behavior = *behavior;
            return Ok(RuntimeValue::Scalar(Scalar::Agent(*id)));
        }
        Builtin::Spawn => return spawn_agent(arguments, world),
        Builtin::Send => return send_agent(arguments, world),
        Builtin::Step => {
            if !arguments.is_empty() {
                return Err(Error("step expects no arguments"));
            }
            return Ok(RuntimeValue::Scalar(Scalar::Bool(!matches!(
                schedule_one(world, depth, fuel)?,
                Schedule::Idle
            ))));
        }
        Builtin::Run => return run_agents(arguments, world, depth, fuel),
        Builtin::AgentState => return inspect_agent(arguments, world, AgentField::State),
        Builtin::AgentPending => return inspect_agent(arguments, world, AgentField::Pending),
        Builtin::AgentTurns => return inspect_agent(arguments, world, AgentField::Turns),
        Builtin::AgentFaulted => return inspect_agent(arguments, world, AgentField::Faulted),
        Builtin::RestartAgent => return restart_agent(arguments, world),
        Builtin::DropMessage => return drop_message(arguments, world),
        Builtin::AgentCount => {
            if !arguments.is_empty() {
                return Err(Error("agent-count expects no arguments"));
            }
            let count = world.agents.iter().filter(|agent| agent.used).count() as i64;
            return Ok(RuntimeValue::Scalar(Scalar::Int(count)));
        }
        _ => {}
    }
    if matches!(builtin, Builtin::Eval) {
        if let [RuntimeValue::Code(node)] = arguments {
            return evaluate_node(document, source, *node, world, locals, depth + 1, fuel);
        }
        return Err(Error("eval expects one quoted form"));
    }
    let (values, count) = integer_arguments(arguments)?;
    let values = &values[..count];
    if matches!(builtin, Builtin::Equal | Builtin::Less) {
        let [left, right] = values else {
            return Err(Error("= and < expect two integers"));
        };
        return Ok(RuntimeValue::Scalar(Scalar::Bool(match builtin {
            Builtin::Equal => left == right,
            Builtin::Less => left < right,
            _ => false,
        })));
    }
    // Variadic checked arithmetic with the hosted seed's identities and arities.
    let result = match builtin {
        Builtin::Add => values
            .iter()
            .try_fold(0_i64, |total, value| total.checked_add(*value)),
        Builtin::Multiply => values
            .iter()
            .try_fold(1_i64, |total, value| total.checked_mul(*value)),
        Builtin::Subtract => match values {
            [] => return Err(Error("- expects at least one integer")),
            [only] => only.checked_neg(),
            [first, rest @ ..] => rest
                .iter()
                .try_fold(*first, |total, value| total.checked_sub(*value)),
        },
        Builtin::Divide => match values {
            [first, rest @ ..] if !rest.is_empty() => {
                let mut total = *first;
                for value in rest {
                    if *value == 0 {
                        return Err(Error("division by zero"));
                    }
                    total = total.checked_div(*value).ok_or(Error("integer overflow"))?;
                }
                Some(total)
            }
            _ => return Err(Error("/ expects at least two integers")),
        },
        _ => None,
    }
    .ok_or(Error("integer overflow"))?;
    Ok(RuntimeValue::Scalar(Scalar::Int(result)))
}

fn spawn_agent(arguments: &[RuntimeValue], world: &mut World) -> Result<RuntimeValue, Error> {
    let [RuntimeValue::Function(behavior), RuntimeValue::Scalar(state)] = arguments else {
        return Err(Error("spawn expects a stored function and scalar state"));
    };
    if behavior.parameter_count != 3 {
        return Err(Error("native agent behavior expects self, state, message"));
    }
    let index = world
        .agents
        .iter()
        .position(|agent| !agent.used)
        .ok_or(Error("native agent table is full"))?;
    world.agents[index] = Agent {
        used: true,
        behavior: *behavior,
        state: *state,
        ..Agent::EMPTY
    };
    Ok(RuntimeValue::Scalar(Scalar::Agent((index + 1) as u8)))
}

fn send_agent(arguments: &[RuntimeValue], world: &mut World) -> Result<RuntimeValue, Error> {
    let [RuntimeValue::Scalar(Scalar::Agent(id)), RuntimeValue::Scalar(message)] = arguments else {
        return Err(Error("send expects an agent and scalar message"));
    };
    let index = agent_index(world, *id)?;
    let agent = &mut world.agents[index];
    if agent.faulted {
        return Err(Error("cannot send to faulted native agent"));
    }
    if agent.mailbox_length as usize == MAX_MAILBOX {
        return Err(Error("native agent mailbox is full"));
    }
    let tail = (agent.mailbox_head as usize + agent.mailbox_length as usize) % MAX_MAILBOX;
    agent.mailbox[tail] = *message;
    agent.mailbox_length += 1;
    Ok(RuntimeValue::Scalar(Scalar::Int(i64::from(
        agent.mailbox_length,
    ))))
}

#[derive(Clone, Copy)]
enum Schedule {
    Idle,
    Committed,
    Faulted,
}

fn schedule_one(world: &mut World, depth: u8, fuel: &mut u16) -> Result<Schedule, Error> {
    if world.scheduler_active {
        return Err(Error("agent behavior cannot invoke the native scheduler"));
    }
    let mut scanned = 0;
    let mut selected = None;
    while scanned < MAX_AGENTS {
        let index = world.scheduler_cursor as usize;
        world.scheduler_cursor = ((index + 1) % MAX_AGENTS) as u8;
        let agent = world.agents[index];
        if agent.used && !agent.faulted && agent.mailbox_length > 0 {
            selected = Some(index);
            break;
        }
        scanned += 1;
    }
    let Some(index) = selected else {
        return Ok(Schedule::Idle);
    };

    // A complete world checkpoint makes the behavior's state writes and sends
    // atomic. On failure only the fault marker survives; the input remains in
    // the mailbox for inspection and an explicit restart.
    let checkpoint = *world;
    let actor = world.agents[index];
    let message = actor.mailbox[actor.mailbox_head as usize];
    world.agents[index].mailbox_head = ((actor.mailbox_head as usize + 1) % MAX_MAILBOX) as u8;
    world.agents[index].mailbox_length -= 1;
    world.scheduler_active = true;
    let arguments = [
        RuntimeValue::Scalar(Scalar::Agent((index + 1) as u8)),
        RuntimeValue::Scalar(actor.state),
        RuntimeValue::Scalar(message),
    ];
    let result = apply_stored_function(actor.behavior, &arguments, world, depth + 1, fuel);
    if *fuel == 0 {
        return Err(Error("native evaluator fuel exhausted"));
    }
    match result {
        Ok(RuntimeValue::Scalar(state)) => {
            world.scheduler_active = false;
            world.agents[index].state = state;
            world.agents[index].turns = world.agents[index]
                .turns
                .checked_add(1)
                .ok_or(Error("native agent turn counter exhausted"))?;
            Ok(Schedule::Committed)
        }
        Ok(_) | Err(_) => {
            *world = checkpoint;
            world.agents[index].faulted = true;
            Ok(Schedule::Faulted)
        }
    }
}

fn run_agents(
    arguments: &[RuntimeValue],
    world: &mut World,
    depth: u8,
    fuel: &mut u16,
) -> Result<RuntimeValue, Error> {
    if world.scheduler_active {
        return Err(Error("agent behavior cannot invoke the native scheduler"));
    }
    let [RuntimeValue::Scalar(Scalar::Int(requested))] = arguments else {
        return Err(Error("run expects one non-negative turn limit"));
    };
    let turns = usize::try_from(*requested)
        .ok()
        .filter(|turns| *turns <= MAX_RUN_TURNS)
        .ok_or(Error("native scheduler turn limit exceeded"))?;
    let mut performed = 0;
    while performed < turns {
        match schedule_one(world, depth, fuel)? {
            Schedule::Idle => break,
            Schedule::Committed | Schedule::Faulted => performed += 1,
        }
    }
    Ok(RuntimeValue::Scalar(Scalar::Int(performed as i64)))
}

#[derive(Clone, Copy)]
enum AgentField {
    State,
    Pending,
    Turns,
    Faulted,
}

fn inspect_agent(
    arguments: &[RuntimeValue],
    world: &World,
    field: AgentField,
) -> Result<RuntimeValue, Error> {
    let [RuntimeValue::Scalar(Scalar::Agent(id))] = arguments else {
        return Err(Error("agent inspector expects one agent"));
    };
    let agent = world.agents[agent_index(world, *id)?];
    Ok(RuntimeValue::Scalar(match field {
        AgentField::State => agent.state,
        AgentField::Pending => Scalar::Int(i64::from(agent.mailbox_length)),
        AgentField::Turns => Scalar::Int(i64::try_from(agent.turns).unwrap_or(i64::MAX)),
        AgentField::Faulted => Scalar::Bool(agent.faulted),
    }))
}

fn restart_agent(arguments: &[RuntimeValue], world: &mut World) -> Result<RuntimeValue, Error> {
    if world.scheduler_active {
        return Err(Error("native agent recovery requires the operator"));
    }
    let [RuntimeValue::Scalar(Scalar::Agent(id))] = arguments else {
        return Err(Error("restart-agent expects one agent"));
    };
    let index = agent_index(world, *id)?;
    world.agents[index].faulted = false;
    Ok(RuntimeValue::Scalar(Scalar::Agent(*id)))
}

fn drop_message(arguments: &[RuntimeValue], world: &mut World) -> Result<RuntimeValue, Error> {
    if world.scheduler_active {
        return Err(Error("native agent recovery requires the operator"));
    }
    let [RuntimeValue::Scalar(Scalar::Agent(id))] = arguments else {
        return Err(Error("drop-message expects one agent"));
    };
    let index = agent_index(world, *id)?;
    let agent = &mut world.agents[index];
    if agent.mailbox_length == 0 {
        return Err(Error("native agent mailbox is empty"));
    }
    let message = agent.mailbox[agent.mailbox_head as usize];
    agent.mailbox_head = ((agent.mailbox_head as usize + 1) % MAX_MAILBOX) as u8;
    agent.mailbox_length -= 1;
    Ok(RuntimeValue::Scalar(message))
}

fn agent_index(world: &World, id: u8) -> Result<usize, Error> {
    let index = usize::from(id)
        .checked_sub(1)
        .ok_or(Error("invalid native agent"))?;
    if index >= MAX_AGENTS || !world.agents[index].used {
        return Err(Error("invalid native agent"));
    }
    Ok(index)
}

fn integer_arguments(arguments: &[RuntimeValue]) -> Result<([i64; MAX_ARGUMENTS], usize), Error> {
    let mut values = [0; MAX_ARGUMENTS];
    for (index, value) in arguments.iter().enumerate() {
        values[index] = match value {
            RuntimeValue::Scalar(Scalar::Int(value)) => *value,
            _ => return Err(Error("expected integer")),
        };
    }
    Ok((values, arguments.len()))
}

fn resolve_symbol(
    document: &Document,
    source: &[u8],
    node: u16,
    world: &World,
    locals: &[Local; MAX_LOCALS],
) -> Result<RuntimeValue, Error> {
    let name = node_bytes(document, source, node);
    if let Some(local) = locals
        .iter()
        .find(|local| local.used && local.name.equals(name))
    {
        return Ok(local.value);
    }
    let builtin = match name {
        b"scene-clear" => Some(Builtin::SceneClear),
        b"scene-bind" => Some(Builtin::SceneBind),
        b"scene-hit" => Some(Builtin::SceneHit),
        b"scene-owner" => Some(Builtin::SceneOwner),
        b"agent-become" => Some(Builtin::AgentBecome),
        b"scene-rect" => Some(Builtin::SceneRect),
        b"scene-count" => Some(Builtin::SceneCount),
        b"+" => Some(Builtin::Add),
        b"-" => Some(Builtin::Subtract),
        b"*" => Some(Builtin::Multiply),
        b"/" => Some(Builtin::Divide),
        b"=" => Some(Builtin::Equal),
        b"<" => Some(Builtin::Less),
        b"eval" => Some(Builtin::Eval),
        b"spawn" => Some(Builtin::Spawn),
        b"send" => Some(Builtin::Send),
        b"step" => Some(Builtin::Step),
        b"run" => Some(Builtin::Run),
        b"agent-state" => Some(Builtin::AgentState),
        b"agent-pending" => Some(Builtin::AgentPending),
        b"agent-turns" => Some(Builtin::AgentTurns),
        b"agent-faulted?" => Some(Builtin::AgentFaulted),
        b"restart-agent" => Some(Builtin::RestartAgent),
        b"drop-message" => Some(Builtin::DropMessage),
        b"agent-count" => Some(Builtin::AgentCount),
        _ => None,
    };
    if let Some(builtin) = builtin {
        return Ok(RuntimeValue::Builtin(builtin));
    }
    let index = world.find(name).ok_or(Error("unbound native symbol"))?;
    Ok(match world.bindings[index].value {
        StoredValue::Scalar(scalar) => RuntimeValue::Scalar(scalar),
        StoredValue::Function(function) => RuntimeValue::Function(function),
        StoredValue::Empty => return Err(Error("unbound native symbol")),
    })
}

fn truthy(value: RuntimeValue) -> bool {
    !matches!(
        value,
        RuntimeValue::Scalar(Scalar::Bool(false) | Scalar::Nil)
    )
}

fn exactly_one_argument(document: &Document, head: u16) -> Result<u16, Error> {
    let argument = child_after(document, head)?;
    if document.nodes[argument as usize].next != NONE {
        return Err(Error("form expects exactly one argument"));
    }
    Ok(argument)
}

fn child_after(document: &Document, node: u16) -> Result<u16, Error> {
    let child = document.nodes[node as usize].next;
    if child == NONE {
        Err(Error("missing native form argument"))
    } else {
        Ok(child)
    }
}

fn symbol_is(document: &Document, source: &[u8], node: u16, expected: &[u8]) -> bool {
    matches!(document.nodes[node as usize].kind, NodeKind::Symbol)
        && node_bytes(document, source, node) == expected
}

fn node_bytes<'a>(document: &Document, source: &'a [u8], node: u16) -> &'a [u8] {
    let node = document.nodes[node as usize];
    &source[node.start as usize..node.end as usize]
}

#[cfg(test)]
mod tests {
    #[test]
    fn source_rebuild_failure_preserves_live_state_and_rollback() {
        let mut session = Session::new();
        eval(&mut session, "(def live 41)");
        eval(&mut session, "(def live 42)");
        let revision = session.revision();
        session.begin_rebuild();
        session.stage(b"(def live 99)").unwrap();
        assert!(session.stage(b"(/ 1 0)").is_err());
        assert!(session.promote().is_err());
        assert_eq!(session.revision(), revision);
        session.rollback().unwrap();
        assert_eq!(eval(&mut session, "live"), Value::Int(41));
        session.begin_rebuild();
        session.stage(b"(def live 99)").unwrap();
        session.promote().unwrap();
        assert_eq!(eval(&mut session, "live"), Value::Int(99));
    }
    #[test]
    fn workbench_preview_promote_reject_stale_and_rollback() {
        let mut session = Session::new();
        for source in include_str!("../../desktop/workbench.agel")
            .lines()
            .filter(|s| s.starts_with('('))
        {
            eval(&mut session, source);
        }
        assert_eq!(eval(&mut session, "(scene-hit 360 640)"), Value::Int(1));
        assert_eq!(eval(&mut session, "(scene-hit 340 622)"), Value::Int(0));
        assert_eq!(
            eval(&mut session, "(scene-hit -9223372036854775808 640)"),
            Value::Int(0)
        );
        let revision = session.revision();
        let old = session.scene_record(1);
        session.preview(b"(point 360 640)").unwrap();
        assert_eq!(session.revision(), revision);
        assert_eq!(session.scene_record(1), old);
        assert_ne!(session.candidate_scene(1).unwrap().1, old);
        session.promote().unwrap();
        assert_ne!(session.scene_record(1), old);
        session.rollback().unwrap();
        assert_eq!(session.scene_record(1), old);
        session.preview(b"(point 360 640)").unwrap();
        eval(&mut session, "(inspect-agent)");
        assert!(session.promote().is_err());
        eval(
            &mut session,
            "(def broken (fn (self state message) (/ 1 0)))",
        );
        assert!(session
            .preview(b"(begin (agent-become dock broken) (activate))")
            .is_err());
        assert_eq!(
            eval(&mut session, "(agent-faulted? dock)"),
            Value::Bool(false)
        );
        assert_eq!(eval(&mut session, "(activate)"), Value::Int(1));
        session.preview(b"(point 360 640)").unwrap();
        session.discard();
        assert!(session.promote().is_err());
        assert!(session.agent_source(1).unwrap().starts_with(b"(begin"));
    }

    #[test]
    fn bindings_are_transactional_unique_and_occluded() {
        let mut session = Session::new();
        eval(&mut session, "(def b (fn (self state msg) state))");
        eval(&mut session, "(def a (spawn b 0))");
        eval(
            &mut session,
            "(begin (scene-rect 0 0 10 10 5 0) (scene-bind 77 a))",
        );
        assert_eq!(eval(&mut session, "(scene-hit 5 5)"), Value::Int(77));
        assert!(session
            .evaluate(b"(begin (scene-rect 0 0 10 10 0 0) (scene-bind 77 a))")
            .is_err());
        assert_eq!(session.scene_count(), 1);
        eval(&mut session, "(scene-rect 0 0 10 10 0 0)");
        assert_eq!(eval(&mut session, "(scene-hit 5 5)"), Value::Int(0));
    }
    use super::*;

    #[test]
    fn scene_is_part_of_the_transactional_actor_world() {
        let mut session = Session::new();
        eval(&mut session, "(scene-rect 318 610 372 72 20 3423048)");
        let original = session.scene_record(0);
        assert!(session
            .evaluate(b"(begin (scene-clear) (scene-rect 0 0 9999 80 10 123))")
            .is_err());
        assert_eq!(session.scene_record(0), original);
        eval(&mut session, "(def paint (fn (self state message) (begin (scene-clear) (scene-rect 318 610 372 72 20 message) message)))");
        eval(&mut session, "(def a (spawn paint 0))");
        eval(&mut session, "(send a 4609905)");
        eval(&mut session, "(step)");
        assert_eq!(session.scene_record(0).unwrap()[6], 4609905);
        session.rollback().unwrap();
        assert_eq!(session.scene_record(0), original);
        assert_eq!(eval(&mut session, "(agent-state a)"), Value::Int(0));
        eval(
            &mut session,
            "(def bad (fn (self state message) (begin (scene-clear) (/ 1 0))))",
        );
        eval(&mut session, "(def b (spawn bad 0))");
        eval(&mut session, "(send b 1)");
        eval(&mut session, "(run 2)");
        assert_eq!(session.scene_count(), 1);
        assert_eq!(eval(&mut session, "(agent-faulted? b)"), Value::Bool(true));
    }

    #[test]
    fn scene_bounds_and_capacity_are_enforced() {
        let mut session = Session::new();
        for source in [
            "(scene-rect -1 0 1 1 0 0)",
            "(scene-rect 0 0 0 1 0 0)",
            "(scene-rect 0 680 1 8 0 0)",
            "(scene-rect 0 0 8 8 5 0)",
            "(scene-rect 0 0 8 8 0 16777216)",
        ] {
            assert!(session.evaluate(source.as_bytes()).is_err());
        }
        for _ in 0..12 {
            eval(&mut session, "(scene-rect 0 0 10 10 0 0)");
        }
        assert!(session.evaluate(b"(scene-rect 0 0 10 10 0 0)").is_err());
        assert_eq!(session.scene_count(), 12);
    }

    fn eval(session: &mut Session, source: &str) -> Value {
        session.evaluate(source.as_bytes()).unwrap()
    }

    #[test]
    fn let_variadic_arithmetic_and_multi_body_functions_match_the_seed() {
        let mut session = Session::new();
        for (source, expected) in [
            ("(let ((x 20) (y 22)) (+ x y))", 42),
            ("(let ((x 40)) (let ((x 1) (y x)) (+ x y)))", 41),
            ("(let ((x 1) (x 42)) x)", 42),
            ("(let ((x 40)) (+ x 1) (+ x 2))", 42),
            (
                "(let ((x 40)) ((fn (f) (let ((x 99)) (f 2))) (fn (y) (+ x y))))",
                42,
            ),
            ("(+)", 0),
            ("(*)", 1),
            ("(* 2 3 7)", 42),
            ("(- 5)", -5),
            ("(- 50 5 3)", 42),
            ("(/ 336 2 2 2)", 42),
            ("((fn () 1 42))", 42),
            ("((fn (x) (+ x 1) (+ x 2)) 40)", 40 + 2),
        ] {
            assert_eq!(eval(&mut session, source), Value::Int(expected), "{source}");
        }
        eval(
            &mut session,
            "(def f (fn (x) (def seen x) (let ((twice (* x 2))) (- twice 38))))",
        );
        assert_eq!(eval(&mut session, "(f 40)"), Value::Int(42));
        assert_eq!(eval(&mut session, "seen"), Value::Int(40));
        eval(
            &mut session,
            "(def tick (fn (self state message) (send self 1) (+ state message)))",
        );
        eval(&mut session, "(def counter (spawn tick 0))");
        eval(&mut session, "(send counter 41)");
        eval(&mut session, "(run 2)");
        assert_eq!(eval(&mut session, "(agent-state counter)"), Value::Int(42));
        let revision = session.revision();
        for bad in [
            "(let ((x 1 2)) x)",
            "(let (x) 1)",
            "(let ((1 2)) 3)",
            "(let ())",
            "(let ((if 1)) 1)",
            "(let ((a 1) (b 2) (c 3) (d 4) (e 5) (f 6) (g 7) (h 8) (i 9)) i)",
            "(/ 1)",
            "(/ 1 0 2)",
            "(-)",
            "(= 1)",
            "(< 1 2 3)",
            "(+ 1 #t)",
            "(- -9223372036854775808)",
            "(fn (x))",
            "(def let 1)",
        ] {
            assert!(session.evaluate(bad.as_bytes()).is_err(), "accepted {bad}");
        }
        assert_eq!(session.revision(), revision);
    }

    #[test]
    fn continuations_yield_fairly_and_world_rollback_restores_mailboxes() {
        let mut session = Session::new();
        eval(
            &mut session,
            "(def tick (fn (self state message) (begin (send self message) (+ state 1))))",
        );
        eval(&mut session, "(def a (spawn tick 0))");
        eval(&mut session, "(def b (spawn tick 0))");
        eval(&mut session, "(send a 1)");
        eval(&mut session, "(send b 1)");
        assert_eq!(eval(&mut session, "(run 4)"), Value::Int(4));
        session.rollback().unwrap();
        assert_eq!(eval(&mut session, "(agent-state a)"), Value::Int(0));
        assert_eq!(eval(&mut session, "(agent-pending a)"), Value::Int(1));
        eval(&mut session, "(run 4)");
        assert_eq!(eval(&mut session, "(agent-state a)"), Value::Int(2));
        assert_eq!(eval(&mut session, "(agent-state b)"), Value::Int(2));
    }

    #[test]
    fn faults_restore_sends_and_globals_and_preserve_the_poison_message() {
        let mut session = Session::new();
        eval(&mut session, "(def x 1)");
        eval(
            &mut session,
            "(def bad (fn (self state message) (begin (def x 99) (send self 9) (/ 1 0))))",
        );
        eval(&mut session, "(def a (spawn bad 7))");
        eval(&mut session, "(send a 42)");
        eval(&mut session, "(step)");
        assert_eq!(eval(&mut session, "x"), Value::Int(1));
        assert_eq!(eval(&mut session, "(agent-state a)"), Value::Int(7));
        assert_eq!(eval(&mut session, "(agent-pending a)"), Value::Int(1));
        assert_eq!(eval(&mut session, "(agent-faulted? a)"), Value::Bool(true));
        assert_eq!(eval(&mut session, "(drop-message a)"), Value::Int(42));
        eval(&mut session, "(restart-agent a)");
        assert_eq!(eval(&mut session, "(agent-faulted? a)"), Value::Bool(false));
    }

    #[test]
    fn nested_scheduling_and_recovery_are_rejected_even_with_zero_turns() {
        for body in [
            "(run 0)",
            "(step)",
            "(restart-agent self)",
            "(drop-message self)",
        ] {
            let mut session = Session::new();
            eval(
                &mut session,
                &format!("(def bad (fn (self state message) {body}))"),
            );
            eval(&mut session, "(def a (spawn bad 0))");
            eval(&mut session, "(send a 1)");
            eval(&mut session, "(step)");
            assert_eq!(eval(&mut session, "(agent-faulted? a)"), Value::Bool(true));
            assert_eq!(eval(&mut session, "(agent-pending a)"), Value::Int(1));
        }
    }

    #[test]
    fn capacity_rejection_preserves_committed_world() {
        let mut session = Session::new();
        eval(&mut session, "(def tick (fn (self state message) message))");
        eval(&mut session, "(def a (spawn tick 0))");
        for _ in 0..MAX_MAILBOX {
            eval(&mut session, "(send a 1)");
        }
        let revision = session.revision();
        assert!(session.evaluate(b"(send a 2)").is_err());
        assert_eq!(session.revision(), revision);
        for _ in 1..MAX_AGENTS {
            eval(&mut session, "(spawn tick 0)");
        }
        assert!(session.evaluate(b"(spawn tick 0)").is_err());
        assert!(session.evaluate(b"(run 33)").is_err());
        assert!(session.evaluate(b"(run -1)").is_err());
        assert_eq!(eval(&mut session, "(agent-pending a)"), Value::Int(8));
    }

    #[test]
    fn shared_fuel_exhaustion_aborts_all_turns() {
        let mut session = Session::new();
        let work = "(+ 1 1) ".repeat(20);
        eval(
            &mut session,
            &format!("(def tick (fn (self state message) (begin {work}(send self 1) state)))"),
        );
        eval(&mut session, "(def a (spawn tick 0))");
        eval(&mut session, "(send a 1)");
        assert_eq!(
            session.evaluate(b"(run 32)"),
            Err(Error("native evaluator fuel exhausted"))
        );
        assert_eq!(eval(&mut session, "(agent-turns a)"), Value::Int(0));
        assert_eq!(eval(&mut session, "(agent-faulted? a)"), Value::Bool(false));
        assert_eq!(eval(&mut session, "(agent-pending a)"), Value::Int(1));
    }
}
