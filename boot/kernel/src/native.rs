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
}

impl World {
    const EMPTY: Self = Self {
        bindings: [Binding::EMPTY; MAX_BINDINGS],
        agents: [Agent::EMPTY; MAX_AGENTS],
        scheduler_cursor: 0,
        scheduler_active: false,
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
}

impl Session {
    pub const fn new() -> Self {
        Self {
            active: World::EMPTY,
            previous: World::EMPTY,
            scratch: World::EMPTY,
            has_previous: false,
            revision: 0,
        }
    }

    pub fn evaluate(&mut self, source: &[u8]) -> Result<Value, Error> {
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

    /// Clear language state without reusing a revision identifier.
    #[cfg(feature = "isolation-selftest")]
    pub fn reset(&mut self) {
        self.active = World::EMPTY;
        self.previous = World::EMPTY;
        self.scratch = World::EMPTY;
        self.has_previous = false;
    }

    pub fn rollback(&mut self) -> Result<(), Error> {
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

fn validate_lambda(document: &Document, source: &[u8], fn_node: u16) -> Result<(), Error> {
    let parameters = child_after(document, fn_node)?;
    if !matches!(document.nodes[parameters as usize].kind, NodeKind::List) {
        return Err(Error("fn parameters must be a list"));
    }
    let body = child_after(document, parameters)?;
    if document.nodes[body as usize].next != NONE {
        return Err(Error("native fn accepts one body form"));
    }
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
    let body_source = node_bytes(document, source, body_node);
    if body_source.len() > MAX_BODY {
        return Err(Error("native function body is too large"));
    }
    let mut function = Function::EMPTY;
    function.body[..body_source.len()].copy_from_slice(body_source);
    function.body_length = body_source.len() as u16;
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
    evaluate_node(document, source, body, world, &locals, depth + 1, fuel)
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
    if matches!(builtin, Builtin::Equal | Builtin::Less) {
        let [left, right] = scalar_integers(arguments)?;
        return Ok(RuntimeValue::Scalar(Scalar::Bool(match builtin {
            Builtin::Equal => left == right,
            Builtin::Less => left < right,
            _ => false,
        })));
    }
    let [left, right] = scalar_integers(arguments)?;
    let result = match builtin {
        Builtin::Add => left.checked_add(right),
        Builtin::Subtract => left.checked_sub(right),
        Builtin::Multiply => left.checked_mul(right),
        Builtin::Divide if right != 0 => left.checked_div(right),
        Builtin::Divide => return Err(Error("division by zero")),
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

fn scalar_integers(arguments: &[RuntimeValue]) -> Result<[i64; 2], Error> {
    if arguments.len() != 2 {
        return Err(Error("native arithmetic currently expects two integers"));
    }
    let mut values = [0; 2];
    for (index, value) in arguments.iter().enumerate() {
        values[index] = match value {
            RuntimeValue::Scalar(Scalar::Int(value)) => *value,
            _ => return Err(Error("expected integer")),
        };
    }
    Ok(values)
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
    use super::*;

    fn eval(session: &mut Session, source: &str) -> Value {
        session.evaluate(source.as_bytes()).unwrap()
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
