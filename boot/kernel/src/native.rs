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
const MAX_CELLS: usize = 384;
const MAX_TEXT: usize = 2048;
/// Rendered result bytes retained for the frontends; one shared-page payload.
const RESULT_BYTES: usize = 256;
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
    ("cells", MAX_CELLS as u64),
    ("text", MAX_TEXT as u64),
];

/// A result as the frontends see it. Data values (strings, symbols, lists,
/// maps) live in the world heap and are reported through the session's
/// rendered result text rather than as a handle that could outlive a commit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Value {
    Int(i64),
    Bool(bool),
    Nil,
    /// An agent handle: slot number in the low byte, the slot's generation
    /// in the high byte. See [`agent_label`].
    Agent(u16),
    Data,
    Function,
}

/// Split an agent handle into the number the operator sees (slot + 1) and
/// the generation of the slot it was issued against. A handle from before a
/// slot was reaped carries an older generation and is refused; printers show
/// the generation only when it is not the first, so `#<native-agent:2>`
/// stays `#<native-agent:2>` until slot 2 has been reaped and reused.
pub fn agent_label(id: u16) -> (u8, u8) {
    ((id & 0xff) as u8, (id >> 8) as u8)
}

fn agent_handle(index: usize, generation: u8) -> u16 {
    (index as u16 + 1) | (u16::from(generation) << 8)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Error(pub &'static str);

/// Every value an agent state, mailbox slot, binding or heap cell may hold.
/// Heap handles are indices into the owning world's bounded heap, so copying a
/// world copies the values with it and rollback restores them together.
/// `Nil` is declared first so that an empty cell, slot, binding and heap is
/// all-zero bytes. The image then zero-fills the three world banks at boot
/// instead of carrying three non-zero copies of them in its read-only data,
/// which is what keeps the kernel inside the 254-sector BIOS load.
#[derive(Clone, Copy)]
enum Scalar {
    Nil,
    Int(i64),
    Bool(bool),
    Agent(u16),
    Text {
        start: u16,
        len: u16,
    },
    Symbol {
        start: u16,
        len: u16,
    },
    List(u16),
    /// Alternating key/value chain of cells; `NONE` is the empty map.
    Map(u16),
}

#[derive(Clone, Copy)]
struct Cell {
    car: Scalar,
    cdr: Scalar,
}

impl Cell {
    const EMPTY: Self = Self {
        car: Scalar::Nil,
        cdr: Scalar::Nil,
    };
}

/// The fixed-memory data heap: a cons-cell arena and an immutable text arena.
/// Allocation only appends; a copying collection at each commit boundary keeps
/// exactly the reachable cells and bytes.
#[derive(Clone, Copy)]
struct Heap {
    cells: [Cell; MAX_CELLS],
    cell_count: u16,
    text: [u8; MAX_TEXT],
    text_len: u16,
}

impl Heap {
    const EMPTY: Self = Self {
        cells: [Cell::EMPTY; MAX_CELLS],
        cell_count: 0,
        text: [0; MAX_TEXT],
        text_len: 0,
    };

    fn cons(&mut self, car: Scalar, cdr: Scalar) -> Result<u16, Error> {
        if self.cell_count as usize == MAX_CELLS {
            return Err(Error("native heap cells exhausted"));
        }
        let index = self.cell_count;
        self.cells[index as usize] = Cell { car, cdr };
        self.cell_count += 1;
        Ok(index)
    }

    fn cell(&self, index: u16) -> Cell {
        self.cells[index as usize]
    }

    fn bytes(&self, start: u16, len: u16) -> &[u8] {
        &self.text[start as usize..start as usize + len as usize]
    }

    fn reserve_text(&mut self, len: usize) -> Result<u16, Error> {
        if len > MAX_TEXT - self.text_len as usize {
            return Err(Error("native text arena exhausted"));
        }
        let start = self.text_len;
        self.text_len += len as u16;
        Ok(start)
    }

    fn alloc_text(&mut self, bytes: &[u8]) -> Result<(u16, u16), Error> {
        let start = self.reserve_text(bytes.len())?;
        self.text[start as usize..start as usize + bytes.len()].copy_from_slice(bytes);
        Ok((start, bytes.len() as u16))
    }

    /// Symbols are interned by content so repeated quoting does not consume
    /// the arena; any existing equal byte run is a valid home for a symbol.
    fn intern(&mut self, bytes: &[u8]) -> Result<Scalar, Error> {
        let len = self.text_len as usize;
        if !bytes.is_empty() && bytes.len() <= len {
            if let Some(start) = self.text[..len]
                .windows(bytes.len())
                .position(|window| window == bytes)
            {
                return Ok(Scalar::Symbol {
                    start: start as u16,
                    len: bytes.len() as u16,
                });
            }
        }
        let (start, len) = self.alloc_text(bytes)?;
        Ok(Scalar::Symbol { start, len })
    }
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
    /// Scalars captured from the lexical context the function was made in,
    /// bound before the body runs; a parameter of the same name shadows one.
    captures: [CapturedLocal; MAX_LOCALS],
}

impl Function {
    const EMPTY: Self = Self {
        parameter_count: 0,
        parameters: [Name::EMPTY; MAX_PARAMS],
        body_length: 0,
        body: [0; MAX_BODY],
        captures: [CapturedLocal::EMPTY; MAX_LOCALS],
    };
}

// An explicit tag with `Empty` as zero: with a niche available inside
// `Function` (a captured local's `used` flag), Rust would otherwise encode
// `Empty` as a non-zero byte there, and an empty binding, hence the empty
// world, would no longer be all-zero bytes. The three world banks are
// zero-filled at boot because of exactly that property; see `Scalar`.
#[repr(u8)]
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Copy)]
enum StoredValue {
    Empty = 0,
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
    /// Bumped each time the slot is reaped, so a handle to the previous
    /// occupant is refused rather than reaching the next one.
    generation: u8,
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
        generation: 0,
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
    heap: Heap,
    bindings: [Binding; MAX_BINDINGS],
    agents: [Agent; MAX_AGENTS],
    scheduler_cursor: u8,
    scheduler_active: bool,
    scene: [[u32; 7]; MAX_SCENE_RECTS],
    scene_count: u8,
    scene_ids: [i64; MAX_SCENE_RECTS],
    scene_owners: [u16; MAX_SCENE_RECTS],
}

impl World {
    const EMPTY: Self = Self {
        heap: Heap::EMPTY,
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
    result: [u8; RESULT_BYTES],
    result_length: u16,
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
            result: [0; RESULT_BYTES],
            result_length: 0,
        }
    }

    /// The last successful evaluation's value, rendered in Agel syntax. Data
    /// values are rendered before the commit-time collection so that a result
    /// need not be a heap root to be reported.
    #[cfg(not(feature = "native-selftest"))]
    pub fn result(&self) -> &[u8] {
        &self.result[..self.result_length as usize]
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
            Ok((value, scalar)) => {
                self.result_length =
                    render_result(scalar, &self.scratch.heap, &mut self.result) as u16;
                collect(&mut self.scratch)?;
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
        collect(&mut self.scratch)?;
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
        collect(&mut self.scratch)?;
        if self.scratch.agents.iter().any(|agent| agent.faulted) {
            return Err(Error("source candidate agent turn failed"));
        }
        self.candidate_revision = Some(self.revision);
        Ok(())
    }

    /// The behavior source of the agent currently in slot `number` (as the
    /// operator sees it, from 1), whatever generation that occupant is: the
    /// inspector names slots, not handles.
    #[cfg(any(feature = "isolation-selftest", test))]
    pub fn agent_source(&self, number: u8) -> Result<&[u8], Error> {
        let index = usize::from(number)
            .checked_sub(1)
            .filter(|index| *index < MAX_AGENTS)
            .ok_or(Error("invalid native agent"))?;
        let id = agent_handle(index, self.active.agents[index].generation);
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
    /// A string literal; `start..end` spans the quotes and escapes are decoded
    /// when the literal is evaluated into the heap.
    Text,
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
            Some(b'"') => self.text(),
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

    fn text(&mut self) -> Result<u16, Error> {
        let start = self.position;
        self.position += 1;
        loop {
            match self.source.get(self.position).copied() {
                None => return Err(Error("unterminated string")),
                Some(b'"') => {
                    self.position += 1;
                    break;
                }
                Some(b'\\') => {
                    if !matches!(
                        self.source.get(self.position + 1).copied(),
                        Some(b'n' | b'r' | b't' | b'\\' | b'"')
                    ) {
                        return Err(Error("invalid string escape"));
                    }
                    self.position += 2;
                }
                Some(_) => self.position += 1,
            }
        }
        self.document.allocate(Node {
            kind: NodeKind::Text,
            first: NONE,
            next: NONE,
            start: start as u16,
            end: self.position as u16,
        })
    }

    fn atom(&mut self) -> Result<u16, Error> {
        let start = self.position;
        while let Some(byte) = self.source.get(self.position).copied() {
            if byte.is_ascii_whitespace() || matches!(byte, b'(' | b')' | b';' | b'"') {
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

// Explicitly tagged for the same reason as `StoredValue`: the empty local
// slot must stay all-zero bytes.
#[repr(u8)]
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Copy)]
enum RuntimeValue {
    Scalar(Scalar) = 0,
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
    ListOf,
    Cons,
    Car,
    Cdr,
    Count,
    Dict,
    Get,
    HasKey,
    Assoc,
    Dissoc,
    Keys,
    TypeOf,
    TextBytes,
    TextByte,
    TextSlice,
    TextConcat,
    TextSymbol,
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
    ReapAgent,
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

fn evaluate_source(world: &mut World, source: &[u8]) -> Result<(Value, Option<Scalar>), Error> {
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
    Ok(public_value(runtime))
}

fn public_value(value: RuntimeValue) -> (Value, Option<Scalar>) {
    match value {
        RuntimeValue::Scalar(Scalar::Int(value)) => (Value::Int(value), None),
        RuntimeValue::Scalar(Scalar::Bool(value)) => (Value::Bool(value), None),
        RuntimeValue::Scalar(Scalar::Nil) => (Value::Nil, None),
        RuntimeValue::Scalar(Scalar::Agent(id)) => (Value::Agent(id), None),
        RuntimeValue::Scalar(data) => (Value::Data, Some(data)),
        RuntimeValue::Function(_) | RuntimeValue::Lambda { .. } | RuntimeValue::Builtin(_) => {
            (Value::Function, None)
        }
    }
}

/// Render a result into `out` in Agel syntax. `None` means the public
/// `Value` already describes it (scalars and functions); the rendering still
/// happens so every frontend prints one way. Overflow truncates at a character
/// boundary with a marker; it never rejects the transaction.
fn render_result(scalar: Option<Scalar>, heap: &Heap, out: &mut [u8]) -> usize {
    let mut length = 0;
    let complete = match scalar {
        Some(value) => render(value, heap, out, &mut length, 0).is_ok(),
        None => true,
    };
    if !complete {
        while length > 0 && out[length - 1] & 0xC0 == 0x80 {
            length -= 1;
        }
        length = length.saturating_sub(1).min(out.len() - 3);
        out[length..length + 3].copy_from_slice(b"...");
        length += 3;
    }
    length
}

struct Overflow;

fn emit(out: &mut [u8], length: &mut usize, bytes: &[u8]) -> Result<(), Overflow> {
    if bytes.len() > out.len() - *length {
        return Err(Overflow);
    }
    out[*length..*length + bytes.len()].copy_from_slice(bytes);
    *length += bytes.len();
    Ok(())
}

fn emit_i64(out: &mut [u8], length: &mut usize, value: i64) -> Result<(), Overflow> {
    let mut digits = [0_u8; 20];
    let mut count = 0;
    let mut magnitude = value.unsigned_abs();
    loop {
        digits[count] = b'0' + (magnitude % 10) as u8;
        count += 1;
        magnitude /= 10;
        if magnitude == 0 {
            break;
        }
    }
    if value < 0 {
        emit(out, length, b"-")?;
    }
    while count > 0 {
        count -= 1;
        emit(out, length, &digits[count..count + 1])?;
    }
    Ok(())
}

fn render(
    value: Scalar,
    heap: &Heap,
    out: &mut [u8],
    length: &mut usize,
    depth: u8,
) -> Result<(), Overflow> {
    if depth >= MAX_DEPTH {
        return emit(out, length, b"#<deep>");
    }
    match value {
        Scalar::Int(value) => emit_i64(out, length, value),
        Scalar::Bool(true) => emit(out, length, b"#t"),
        Scalar::Bool(false) => emit(out, length, b"#f"),
        Scalar::Nil => emit(out, length, b"nil"),
        Scalar::Agent(id) => {
            let (number, generation) = agent_label(id);
            emit(out, length, b"#<native-agent:")?;
            emit_i64(out, length, i64::from(number))?;
            if generation != 0 {
                emit(out, length, b".")?;
                emit_i64(out, length, i64::from(generation))?;
            }
            emit(out, length, b">")
        }
        Scalar::Symbol { start, len } => emit(out, length, heap.bytes(start, len)),
        Scalar::Text { start, len } => {
            emit(out, length, b"\"")?;
            for byte in heap.bytes(start, len) {
                let escaped: &[u8] = match byte {
                    b'\\' => b"\\\\",
                    b'"' => b"\\\"",
                    b'\n' => b"\\n",
                    b'\r' => b"\\r",
                    b'\t' => b"\\t",
                    other => core::slice::from_ref(other),
                };
                emit(out, length, escaped)?;
            }
            emit(out, length, b"\"")
        }
        Scalar::List(mut cell) => {
            emit(out, length, b"(")?;
            let mut first = true;
            loop {
                if !first {
                    emit(out, length, b" ")?;
                }
                first = false;
                let Cell { car, cdr } = heap.cell(cell);
                render(car, heap, out, length, depth + 1)?;
                match cdr {
                    Scalar::List(next) => cell = next,
                    _ => break,
                }
            }
            emit(out, length, b")")
        }
        Scalar::Map(mut cell) => {
            emit(out, length, b"{")?;
            let mut first = true;
            while cell != NONE {
                if !first {
                    emit(out, length, b" ")?;
                }
                first = false;
                let Cell { car, cdr } = heap.cell(cell);
                render(car, heap, out, length, depth + 1)?;
                match cdr {
                    Scalar::List(next) => cell = next,
                    _ => break,
                }
            }
            emit(out, length, b"}")
        }
    }
}

/// Copying collection of the world heap. Roots are every global binding, every
/// agent state and every queued message. Handles are rewritten in place, so
/// the world after collection is observationally identical with a compact heap.
fn collect(world: &mut World) -> Result<(), Error> {
    let mut collector = Collector {
        source: &world.heap,
        target: Heap::EMPTY,
        forward: [NONE; MAX_CELLS],
    };
    for binding in world.bindings.iter_mut() {
        if let StoredValue::Scalar(value) = &mut binding.value {
            *value = collector.forward(*value)?;
        }
    }
    for agent in world.agents.iter_mut().filter(|agent| agent.used) {
        agent.state = collector.forward(agent.state)?;
        for slot in 0..agent.mailbox_length as usize {
            let index = (agent.mailbox_head as usize + slot) % MAX_MAILBOX;
            agent.mailbox[index] = collector.forward(agent.mailbox[index])?;
        }
    }
    // Cheney scan: cells copied so far may reference cells not yet copied.
    let mut scan = 0;
    while scan < collector.target.cell_count as usize {
        let cell = collector.target.cells[scan];
        let car = collector.forward(cell.car)?;
        let cdr = collector.forward(cell.cdr)?;
        collector.target.cells[scan] = Cell { car, cdr };
        scan += 1;
    }
    world.heap = collector.target;
    Ok(())
}

struct Collector<'a> {
    source: &'a Heap,
    target: Heap,
    forward: [u16; MAX_CELLS],
}

impl Collector<'_> {
    fn forward(&mut self, value: Scalar) -> Result<Scalar, Error> {
        Ok(match value {
            Scalar::List(cell) => Scalar::List(self.forward_cell(cell)?),
            Scalar::Map(cell) if cell != NONE => Scalar::Map(self.forward_cell(cell)?),
            Scalar::Text { start, len } => {
                let (start, len) = self.forward_text(start, len)?;
                Scalar::Text { start, len }
            }
            Scalar::Symbol { start, len } => {
                let (start, len) = self.forward_text(start, len)?;
                Scalar::Symbol { start, len }
            }
            other => other,
        })
    }

    fn forward_cell(&mut self, cell: u16) -> Result<u16, Error> {
        if self.forward[cell as usize] != NONE {
            return Ok(self.forward[cell as usize]);
        }
        // Contents are forwarded by the scan loop; copy the raw cell now so
        // the forwarding address exists before any cycle-free descendant.
        let copied = self.source.cell(cell);
        let index = self.target.cons(copied.car, copied.cdr)?;
        self.forward[cell as usize] = index;
        Ok(index)
    }

    fn forward_text(&mut self, start: u16, len: u16) -> Result<(u16, u16), Error> {
        let bytes = self.source.bytes(start, len);
        let copied = self.target.text_len as usize;
        if let Some(existing) = self.target.text[..copied]
            .windows(bytes.len())
            .position(|window| window == bytes)
        {
            return Ok((existing as u16, len));
        }
        self.target.alloc_text(bytes)
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
        NodeKind::Quote => Ok(RuntimeValue::Scalar(quote_data(
            document,
            source,
            node.first,
            &mut world.heap,
            depth,
            fuel,
        )?)),
        NodeKind::Text => Ok(RuntimeValue::Scalar(decode_text(
            node_bytes(document, source, node_index),
            &mut world.heap,
        )?)),
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
        return Ok(RuntimeValue::Scalar(quote_data(
            document,
            source,
            value,
            &mut world.heap,
            depth,
            fuel,
        )?));
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
        RuntimeValue::Builtin(builtin) => {
            apply_builtin(builtin, &arguments[..count], world, locals, depth, fuel)
        }
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
    if is_special_form(name) || builtin_for_name(name).is_some() {
        return Err(Error("native core names cannot be redefined"));
    }
    let value = evaluate_node(document, source, value_node, world, locals, depth + 1, fuel)?;
    let stored = match value {
        RuntimeValue::Scalar(scalar) => StoredValue::Scalar(scalar),
        RuntimeValue::Lambda { node, captures } => {
            StoredValue::Function(capture_function(document, source, node, &captures)?)
        }
        RuntimeValue::Function(function) => StoredValue::Function(function),
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

#[inline(never)]
fn capture_function(
    document: &Document,
    source: &[u8],
    fn_node: u16,
    captures: &[CapturedLocal; MAX_LOCALS],
) -> Result<Function, Error> {
    validate_lambda(document, source, fn_node)?;
    let parameters = child_after(document, fn_node)?;
    let body_node = child_after(document, parameters)?;
    let mut function = Function::EMPTY;
    function.captures = *captures;
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
    bind_captures(&mut locals, &function.captures)?;
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
    // A lambda leaving a stored function keeps its captures by becoming a
    // stored function itself; its syntax lives in this function's body, so
    // it is captured here, while that body is still in scope.
    if let RuntimeValue::Lambda { node, captures } = result {
        return Ok(RuntimeValue::Function(capture_function(
            &document, source, node, &captures,
        )?));
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
    bind_captures(&mut locals, captures)?;
    evaluate_sequence(document, source, body, world, &locals, depth, fuel)
}

/// Bind captured scalars after the parameters, so a parameter shadows a
/// capture of the same name. Shared by lambdas applied in place and stored
/// functions, and kept out of line so the image carries it once.
#[inline(never)]
fn bind_captures(
    locals: &mut [Local; MAX_LOCALS],
    captures: &[CapturedLocal; MAX_LOCALS],
) -> Result<(), Error> {
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
    Ok(())
}

fn apply_builtin(
    builtin: Builtin,
    arguments: &[RuntimeValue],
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
        Builtin::ReapAgent => return reap_agent(arguments, world),
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
        // Quoted data is re-read from its rendering: the printer and reader
        // are inverses for every datum that fits one payload, and this keeps
        // a single evaluator over syntax nodes.
        let [RuntimeValue::Scalar(datum)] = arguments else {
            return Err(Error("eval expects one quoted form"));
        };
        let mut text = [0_u8; RESULT_BYTES];
        let mut length = 0;
        if render(*datum, &world.heap, &mut text, &mut length, 0).is_err() {
            return Err(Error("quoted form exceeds the native eval payload"));
        }
        let inner = Parser::parse(&text[..length])?;
        return evaluate_node(
            &inner,
            &text[..length],
            inner.root,
            world,
            locals,
            depth + 1,
            fuel,
        );
    }
    if matches!(builtin, Builtin::Equal) {
        let [RuntimeValue::Scalar(left), RuntimeValue::Scalar(right)] = arguments else {
            return Err(Error("= expects two values"));
        };
        return Ok(RuntimeValue::Scalar(Scalar::Bool(structurally_equal(
            *left,
            *right,
            &world.heap,
            0,
        )?)));
    }
    if let Some(result) = data_builtin(builtin, arguments, world, fuel)? {
        return Ok(RuntimeValue::Scalar(result));
    }
    let (values, count) = integer_arguments(arguments)?;
    let values = &values[..count];
    if matches!(builtin, Builtin::Less) {
        let [left, right] = values else {
            return Err(Error("< expects two integers"));
        };
        return Ok(RuntimeValue::Scalar(Scalar::Bool(left < right)));
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
    let generation = world.agents[index].generation;
    world.agents[index] = Agent {
        used: true,
        generation,
        behavior: *behavior,
        state: *state,
        ..Agent::EMPTY
    };
    Ok(RuntimeValue::Scalar(Scalar::Agent(agent_handle(
        index, generation,
    ))))
}

/// Free an agent's slot. The slot's generation moves on, so every handle to
/// the reaped agent is refused from now on, and scene rectangles it owned
/// become unowned. A reaped slot is the one `spawn` fills next.
fn reap_agent(arguments: &[RuntimeValue], world: &mut World) -> Result<RuntimeValue, Error> {
    if world.scheduler_active {
        return Err(Error("native agent recovery requires the operator"));
    }
    let [RuntimeValue::Scalar(Scalar::Agent(id))] = arguments else {
        return Err(Error("reap-agent expects one agent"));
    };
    let index = agent_index(world, *id)?;
    let generation = world.agents[index].generation.wrapping_add(1);
    world.agents[index] = Agent {
        generation,
        ..Agent::EMPTY
    };
    for owner in world.scene_owners.iter_mut() {
        if *owner == *id {
            *owner = 0;
        }
    }
    Ok(RuntimeValue::Scalar(Scalar::Bool(true)))
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
        RuntimeValue::Scalar(Scalar::Agent(agent_handle(index, actor.generation))),
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

fn agent_index(world: &World, id: u16) -> Result<usize, Error> {
    let (number, generation) = agent_label(id);
    let index = usize::from(number)
        .checked_sub(1)
        .ok_or(Error("invalid native agent"))?;
    if index >= MAX_AGENTS {
        return Err(Error("invalid native agent"));
    }
    let agent = &world.agents[index];
    if agent.generation != generation {
        // The slot has been reaped since this handle was issued, whether or
        // not something else lives there now.
        return Err(Error("stale native agent"));
    }
    if !agent.used {
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
    if let Some(builtin) = builtin_for_name(name) {
        return Ok(RuntimeValue::Builtin(builtin));
    }

    let index = world.find(name).ok_or(Error("unbound native symbol"))?;
    Ok(match world.bindings[index].value {
        StoredValue::Scalar(scalar) => RuntimeValue::Scalar(scalar),
        StoredValue::Function(function) => RuntimeValue::Function(function),
        StoredValue::Empty => return Err(Error("unbound native symbol")),
    })
}

fn builtin_for_name(name: &[u8]) -> Option<Builtin> {
    Some(match name {
        b"scene-clear" => Builtin::SceneClear,
        b"scene-bind" => Builtin::SceneBind,
        b"scene-hit" => Builtin::SceneHit,
        b"scene-owner" => Builtin::SceneOwner,
        b"agent-become" => Builtin::AgentBecome,
        b"scene-rect" => Builtin::SceneRect,
        b"scene-count" => Builtin::SceneCount,
        b"+" => Builtin::Add,
        b"-" => Builtin::Subtract,
        b"*" => Builtin::Multiply,
        b"/" => Builtin::Divide,
        b"=" => Builtin::Equal,
        b"<" => Builtin::Less,
        b"eval" => Builtin::Eval,
        b"list" => Builtin::ListOf,
        b"cons" => Builtin::Cons,
        b"car" => Builtin::Car,
        b"cdr" => Builtin::Cdr,
        b"count" => Builtin::Count,
        b"dict" => Builtin::Dict,
        b"get" => Builtin::Get,
        b"has-key?" => Builtin::HasKey,
        b"assoc" => Builtin::Assoc,
        b"dissoc" => Builtin::Dissoc,
        b"keys" => Builtin::Keys,
        b"type-of" => Builtin::TypeOf,
        b"text-bytes" => Builtin::TextBytes,
        b"text-byte" => Builtin::TextByte,
        b"text-slice" => Builtin::TextSlice,
        b"text-concat" => Builtin::TextConcat,
        b"text-symbol" => Builtin::TextSymbol,
        b"spawn" => Builtin::Spawn,
        b"send" => Builtin::Send,
        b"step" => Builtin::Step,
        b"run" => Builtin::Run,
        b"agent-state" => Builtin::AgentState,
        b"agent-pending" => Builtin::AgentPending,
        b"agent-turns" => Builtin::AgentTurns,
        b"agent-faulted?" => Builtin::AgentFaulted,
        b"restart-agent" => Builtin::RestartAgent,
        b"drop-message" => Builtin::DropMessage,
        b"reap-agent" => Builtin::ReapAgent,
        b"agent-count" => Builtin::AgentCount,
        _ => return None,
    })
}

/// Build inert data from quoted syntax. Symbols intern, strings decode, lists
/// become cell chains and the empty list is nil, matching the hosted seed.
fn quote_data(
    document: &Document,
    source: &[u8],
    node_index: u16,
    heap: &mut Heap,
    depth: u8,
    fuel: &mut u16,
) -> Result<Scalar, Error> {
    if depth >= MAX_DEPTH {
        return Err(Error("native call depth exceeded"));
    }
    *fuel = fuel
        .checked_sub(1)
        .ok_or(Error("native evaluator fuel exhausted"))?;
    let node = document.nodes[node_index as usize];
    Ok(match node.kind {
        NodeKind::Int(value) => Scalar::Int(value),
        NodeKind::Bool(value) => Scalar::Bool(value),
        NodeKind::Nil => Scalar::Nil,
        NodeKind::Symbol => heap.intern(node_bytes(document, source, node_index))?,
        NodeKind::Text => decode_text(node_bytes(document, source, node_index), heap)?,
        NodeKind::Quote => {
            let inner = quote_data(document, source, node.first, heap, depth + 1, fuel)?;
            let tail = heap.cons(inner, Scalar::Nil)?;
            let quote = heap.intern(b"quote")?;
            Scalar::List(heap.cons(quote, Scalar::List(tail))?)
        }
        NodeKind::List => {
            let mut child = node.first;
            let mut head = Scalar::Nil;
            let mut last = NONE;
            while child != NONE {
                let item = quote_data(document, source, child, heap, depth + 1, fuel)?;
                let cell = heap.cons(item, Scalar::Nil)?;
                if last == NONE {
                    head = Scalar::List(cell);
                } else {
                    heap.cells[last as usize].cdr = Scalar::List(cell);
                }
                last = cell;
                child = document.nodes[child as usize].next;
            }
            head
        }
        NodeKind::Empty => return Err(Error("invalid native syntax node")),
    })
}

fn decode_text(literal: &[u8], heap: &mut Heap) -> Result<Scalar, Error> {
    let body = &literal[1..literal.len() - 1];
    let mut decoded = [0_u8; RESULT_BYTES];
    let mut length = 0;
    let mut index = 0;
    while index < body.len() {
        let byte = if body[index] == b'\\' {
            index += 1;
            match body[index] {
                b'n' => b'\n',
                b'r' => b'\r',
                b't' => b'\t',
                other => other,
            }
        } else {
            body[index]
        };
        if length == decoded.len() {
            return Err(Error("string literal exceeds native text limit"));
        }
        decoded[length] = byte;
        length += 1;
        index += 1;
    }
    let (start, len) = heap.alloc_text(&decoded[..length])?;
    Ok(Scalar::Text { start, len })
}

fn structurally_equal(left: Scalar, right: Scalar, heap: &Heap, depth: u8) -> Result<bool, Error> {
    if depth >= MAX_DEPTH {
        return Err(Error("native call depth exceeded"));
    }
    Ok(match (left, right) {
        (Scalar::Int(a), Scalar::Int(b)) => a == b,
        (Scalar::Bool(a), Scalar::Bool(b)) => a == b,
        (Scalar::Nil, Scalar::Nil) => true,
        (Scalar::Agent(a), Scalar::Agent(b)) => a == b,
        (Scalar::Text { start: a, len: la }, Scalar::Text { start: b, len: lb })
        | (Scalar::Symbol { start: a, len: la }, Scalar::Symbol { start: b, len: lb }) => {
            heap.bytes(a, la) == heap.bytes(b, lb)
        }
        (Scalar::List(a), Scalar::List(b)) => chains_equal(a, b, heap, depth)?,
        (Scalar::Map(a), Scalar::Map(b)) => {
            (a == NONE && b == NONE) || (a != NONE && b != NONE && chains_equal(a, b, heap, depth)?)
        }
        _ => false,
    })
}

fn chains_equal(mut a: u16, mut b: u16, heap: &Heap, depth: u8) -> Result<bool, Error> {
    loop {
        let (left, right) = (heap.cell(a), heap.cell(b));
        if !structurally_equal(left.car, right.car, heap, depth + 1)? {
            return Ok(false);
        }
        match (left.cdr, right.cdr) {
            (Scalar::List(next_a), Scalar::List(next_b)) => {
                a = next_a;
                b = next_b;
            }
            (Scalar::List(_), _) | (_, Scalar::List(_)) => return Ok(false),
            (tail_a, tail_b) => return structurally_equal(tail_a, tail_b, heap, depth + 1),
        }
    }
}

fn text_of(value: &RuntimeValue) -> Result<(u16, u16), Error> {
    match value {
        RuntimeValue::Scalar(Scalar::Text { start, len }) => Ok((*start, *len)),
        _ => Err(Error("expected text")),
    }
}

fn offset_of(value: &RuntimeValue) -> Result<usize, Error> {
    match value {
        RuntimeValue::Scalar(Scalar::Int(offset)) => {
            usize::try_from(*offset).map_err(|_| Error("negative text offset"))
        }
        _ => Err(Error("expected byte offset")),
    }
}

fn scalars(arguments: &[RuntimeValue]) -> Result<[Scalar; MAX_ARGUMENTS], Error> {
    let mut values = [Scalar::Nil; MAX_ARGUMENTS];
    for (index, value) in arguments.iter().enumerate() {
        values[index] = match value {
            RuntimeValue::Scalar(value) => *value,
            _ => return Err(Error("expected a data value")),
        };
    }
    Ok(values)
}

fn list_chain(value: Scalar) -> Result<u16, Error> {
    match value {
        Scalar::List(cell) => Ok(cell),
        Scalar::Nil => Ok(NONE),
        _ => Err(Error("expected a list")),
    }
}

fn map_chain(value: Scalar) -> Result<u16, Error> {
    match value {
        Scalar::Map(cell) => Ok(cell),
        _ => Err(Error("expected a map")),
    }
}

/// Copy a key/value chain up to (excluding) `stop`, appending each copied cell
/// after `last`; returns the new head and the last copied cell.
fn copy_chain(
    heap: &mut Heap,
    mut cell: u16,
    stop: u16,
    fuel: &mut u16,
) -> Result<(Scalar, u16), Error> {
    let mut head = Scalar::Nil;
    let mut last = NONE;
    while cell != NONE && cell != stop {
        *fuel = fuel
            .checked_sub(1)
            .ok_or(Error("native evaluator fuel exhausted"))?;
        let copied = heap.cell(cell);
        let index = heap.cons(copied.car, Scalar::Nil)?;
        if last == NONE {
            head = Scalar::List(index);
        } else {
            heap.cells[last as usize].cdr = Scalar::List(index);
        }
        last = index;
        cell = list_chain(copied.cdr)?;
    }
    Ok((head, last))
}

fn map_find(heap: &Heap, mut cell: u16, key: Scalar) -> Result<Option<u16>, Error> {
    while cell != NONE {
        let entry = heap.cell(cell);
        if structurally_equal(entry.car, key, heap, 0)? {
            return Ok(Some(cell));
        }
        let value_cell = list_chain(entry.cdr)?;
        cell = list_chain(heap.cell(value_cell).cdr)?;
    }
    Ok(None)
}

/// Persistent insertion: the existing key keeps its position with a new
/// value; a new key is appended. Untouched cells are shared, never mutated.
fn map_insert(
    heap: &mut Heap,
    map: u16,
    key: Scalar,
    value: Scalar,
    fuel: &mut u16,
) -> Result<u16, Error> {
    match map_find(heap, map, key)? {
        Some(found) => {
            let (head, last) = copy_chain(heap, map, found, fuel)?;
            let rest = list_chain(heap.cell(list_chain(heap.cell(found).cdr)?).cdr)?;
            let tail = if rest == NONE {
                Scalar::Nil
            } else {
                Scalar::List(rest)
            };
            let value_cell = heap.cons(value, tail)?;
            let key_cell = heap.cons(key, Scalar::List(value_cell))?;
            if last == NONE {
                Ok(key_cell)
            } else {
                heap.cells[last as usize].cdr = Scalar::List(key_cell);
                Ok(list_chain(head)?)
            }
        }
        None => {
            let (head, last) = copy_chain(heap, map, NONE, fuel)?;
            let value_cell = heap.cons(value, Scalar::Nil)?;
            let key_cell = heap.cons(key, Scalar::List(value_cell))?;
            if last == NONE {
                Ok(key_cell)
            } else {
                heap.cells[last as usize].cdr = Scalar::List(key_cell);
                Ok(list_chain(head)?)
            }
        }
    }
}

fn map_len(heap: &Heap, mut cell: u16) -> Result<i64, Error> {
    let mut count = 0;
    while cell != NONE {
        count += 1;
        let value_cell = list_chain(heap.cell(cell).cdr)?;
        cell = list_chain(heap.cell(value_cell).cdr)?;
    }
    Ok(count)
}

fn utf8_boundary(bytes: &[u8], index: usize) -> bool {
    index == bytes.len() || bytes[index] & 0xC0 != 0x80
}

/// List, map and text builtins. `Ok(None)` means the builtin is not one of
/// them and the caller continues with arithmetic.
fn data_builtin(
    builtin: Builtin,
    arguments: &[RuntimeValue],
    world: &mut World,
    fuel: &mut u16,
) -> Result<Option<Scalar>, Error> {
    let heap = &mut world.heap;
    Ok(Some(match builtin {
        Builtin::ListOf => {
            let values = scalars(arguments)?;
            let mut head = Scalar::Nil;
            for value in values[..arguments.len()].iter().rev() {
                head = Scalar::List(heap.cons(*value, head)?);
            }
            head
        }
        Builtin::Cons => {
            let [RuntimeValue::Scalar(head), RuntimeValue::Scalar(tail)] = arguments else {
                return Err(Error("cons expects a value and a list"));
            };
            let tail = match tail {
                Scalar::Nil | Scalar::List(_) => *tail,
                _ => return Err(Error("cons expects a list")),
            };
            Scalar::List(heap.cons(*head, tail)?)
        }
        Builtin::Car | Builtin::Cdr => {
            let [RuntimeValue::Scalar(value)] = arguments else {
                return Err(Error("car/cdr expect one list"));
            };
            match value {
                Scalar::Nil => Scalar::Nil,
                Scalar::List(cell) => {
                    let cell = heap.cell(*cell);
                    if matches!(builtin, Builtin::Car) {
                        cell.car
                    } else {
                        cell.cdr
                    }
                }
                _ => return Err(Error("car/cdr expect a list")),
            }
        }
        Builtin::Count => {
            let [RuntimeValue::Scalar(value)] = arguments else {
                return Err(Error("count expects one collection"));
            };
            Scalar::Int(match value {
                Scalar::Nil => 0,
                Scalar::List(mut cell) => {
                    let mut count = 1;
                    while let Scalar::List(next) = heap.cell(cell).cdr {
                        count += 1;
                        cell = next;
                    }
                    count
                }
                Scalar::Map(cell) => map_len(heap, *cell)?,
                Scalar::Text { start, len } => heap
                    .bytes(*start, *len)
                    .iter()
                    .filter(|byte| **byte & 0xC0 != 0x80)
                    .count() as i64,
                _ => return Err(Error("count cannot inspect this value")),
            })
        }
        Builtin::Dict => {
            if arguments.len() & 1 != 0 {
                return Err(Error("dict expects key/value pairs"));
            }
            let values = scalars(arguments)?;
            let mut map = NONE;
            let mut index = 0;
            // Pairs are complete: the length was checked to be even above.
            while index < arguments.len() {
                map = map_insert(heap, map, values[index], values[index + 1], fuel)?;
                index += 2;
            }
            Scalar::Map(map)
        }
        Builtin::Get | Builtin::HasKey => {
            let [RuntimeValue::Scalar(map), RuntimeValue::Scalar(key)] = arguments else {
                return Err(Error("get/has-key? expect a map and a key"));
            };
            let found = map_find(heap, map_chain(*map)?, *key)?;
            if matches!(builtin, Builtin::HasKey) {
                Scalar::Bool(found.is_some())
            } else {
                match found {
                    Some(cell) => heap.cell(list_chain(heap.cell(cell).cdr)?).car,
                    None => Scalar::Nil,
                }
            }
        }
        Builtin::Assoc => {
            let [RuntimeValue::Scalar(map), RuntimeValue::Scalar(key), RuntimeValue::Scalar(value)] =
                arguments
            else {
                return Err(Error("assoc expects a map, a key and a value"));
            };
            Scalar::Map(map_insert(heap, map_chain(*map)?, *key, *value, fuel)?)
        }
        Builtin::Dissoc => {
            let [RuntimeValue::Scalar(map), RuntimeValue::Scalar(key)] = arguments else {
                return Err(Error("dissoc expects a map and a key"));
            };
            let map = map_chain(*map)?;
            match map_find(heap, map, *key)? {
                None => Scalar::Map(map),
                Some(found) => {
                    let (head, last) = copy_chain(heap, map, found, fuel)?;
                    let rest = list_chain(heap.cell(list_chain(heap.cell(found).cdr)?).cdr)?;
                    if last == NONE {
                        Scalar::Map(rest)
                    } else {
                        heap.cells[last as usize].cdr = if rest == NONE {
                            Scalar::Nil
                        } else {
                            Scalar::List(rest)
                        };
                        Scalar::Map(list_chain(head)?)
                    }
                }
            }
        }
        Builtin::Keys => {
            let [RuntimeValue::Scalar(map)] = arguments else {
                return Err(Error("keys expects one map"));
            };
            let mut cell = map_chain(*map)?;
            let mut head = Scalar::Nil;
            let mut last = NONE;
            while cell != NONE {
                let entry = heap.cell(cell);
                let index = heap.cons(entry.car, Scalar::Nil)?;
                if last == NONE {
                    head = Scalar::List(index);
                } else {
                    heap.cells[last as usize].cdr = Scalar::List(index);
                }
                last = index;
                let value_cell = list_chain(entry.cdr)?;
                cell = list_chain(heap.cell(value_cell).cdr)?;
            }
            head
        }
        Builtin::TypeOf => {
            let [value] = arguments else {
                return Err(Error("type-of expects one value"));
            };
            let name: &[u8] = match value {
                RuntimeValue::Scalar(Scalar::Int(_)) => b"int",
                RuntimeValue::Scalar(Scalar::Bool(_)) => b"bool",
                RuntimeValue::Scalar(Scalar::Nil) => b"nil",
                RuntimeValue::Scalar(Scalar::Agent(_)) => b"agent",
                RuntimeValue::Scalar(Scalar::Text { .. }) => b"string",
                RuntimeValue::Scalar(Scalar::Symbol { .. }) => b"symbol",
                RuntimeValue::Scalar(Scalar::List(_)) => b"list",
                RuntimeValue::Scalar(Scalar::Map(_)) => b"map",
                _ => b"callable",
            };
            heap.intern(name)?
        }
        Builtin::TextBytes => {
            let [text] = arguments else {
                return Err(Error("text-bytes expects one string"));
            };
            Scalar::Int(i64::from(text_of(text)?.1))
        }
        Builtin::TextByte => {
            let [text, offset] = arguments else {
                return Err(Error("text-byte expects a string and an offset"));
            };
            let (start, len) = text_of(text)?;
            let offset = offset_of(offset)?;
            if offset >= len as usize {
                return Err(Error("text byte out of range"));
            }
            Scalar::Int(i64::from(heap.text[start as usize + offset]))
        }
        Builtin::TextSlice => {
            let [text, from, to] = arguments else {
                return Err(Error("text-slice expects a string and two offsets"));
            };
            let (start, len) = text_of(text)?;
            let (from, to) = (offset_of(from)?, offset_of(to)?);
            let bytes = heap.bytes(start, len);
            if from > to
                || to > bytes.len()
                || !utf8_boundary(bytes, from)
                || !utf8_boundary(bytes, to)
            {
                return Err(Error("invalid UTF-8 slice"));
            }
            Scalar::Text {
                start: start + from as u16,
                len: (to - from) as u16,
            }
        }
        Builtin::TextConcat => {
            let [left, right] = arguments else {
                return Err(Error("text-concat expects two strings"));
            };
            let (a, la) = text_of(left)?;
            let (b, lb) = text_of(right)?;
            let total = la as usize + lb as usize;
            *fuel = fuel
                .checked_sub((total / 16) as u16 + 1)
                .ok_or(Error("native evaluator fuel exhausted"))?;
            let start = heap.reserve_text(total)?;
            for offset in 0..la as usize {
                heap.text[start as usize + offset] = heap.text[a as usize + offset];
            }
            for offset in 0..lb as usize {
                heap.text[start as usize + la as usize + offset] = heap.text[b as usize + offset];
            }
            Scalar::Text {
                start,
                len: total as u16,
            }
        }
        Builtin::TextSymbol => {
            let [text] = arguments else {
                return Err(Error("text-symbol expects one string"));
            };
            let (start, len) = text_of(text)?;
            Scalar::Symbol { start, len }
        }
        _ => return Ok(None),
    }))
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

    fn text(session: &mut Session, source: &str) -> String {
        let value = session.evaluate(source.as_bytes()).unwrap();
        let rendered = core::str::from_utf8(session.result()).unwrap().to_owned();
        if value != Value::Data {
            assert!(rendered.is_empty(), "{source}: {rendered}");
        }
        rendered
    }

    #[test]
    fn data_values_match_the_hosted_seed_and_survive_commit_and_rollback() {
        let mut session = Session::new();
        for (source, expected) in [
            ("'(1 2 (3 nil) #t)", "(1 2 (3 nil) #t)"),
            ("'()", ""),
            ("'answer", "answer"),
            ("'(quote x)", "(quote x)"),
            ("''x", "(quote x)"),
            ("\"Ahoj \\\"svet\\\"\\n\"", "\"Ahoj \\\"svet\\\"\\n\""),
            ("(list 1 (+ 20 22) 'x)", "(1 42 x)"),
            ("(cons 0 '(1 2))", "(0 1 2)"),
            ("(cons 0 nil)", "(0)"),
            ("(car '(20 22))", ""),
            ("(cdr '(0 42))", "(42)"),
            ("(cdr '(42))", ""),
            ("(car nil)", ""),
            ("(count '(1 2 3))", ""),
            ("(count \"Ahoj 👋\")", ""),
            ("(dict 'a 1 'b 2)", "{a 1 b 2}"),
            ("(dict)", "{}"),
            ("(assoc (dict 'b 1 'a 2) 'b 3)", "{b 3 a 2}"),
            ("(assoc (dict 'a 1) 'b 2)", "{a 1 b 2}"),
            ("(dissoc (dict 'a 1 'b 2 'c 3) 'b)", "{a 1 c 3}"),
            ("(dissoc (dict 'a 1) 'a)", "{}"),
            ("(keys (dict 'a 1 'b 2 'a 3))", "(a b)"),
            ("(get (dict 'a 1 (list 1 2) 42) (list 1 2))", ""),
            ("(get (dict 'a 1) 'z)", ""),
            ("(has-key? (dict 'x nil) 'x)", ""),
            ("(type-of (dict))", "map"),
            ("(type-of \"s\")", "string"),
            ("(type-of 'x)", "symbol"),
            ("(type-of car)", "callable"),
            ("(= '(1 (2)) (list 1 (list 2)))", ""),
            ("(= (dict 'a 1 'b 2) (dict 'b 2 'a 1))", ""),
            ("(= \"a\" \"a\")", ""),
            ("(= 'a \"a\")", ""),
            ("(text-bytes \"Ahoj 👋\")", ""),
            ("(text-byte \"A\" 0)", ""),
            ("(text-slice \"Ahoj svet\" 0 4)", "\"Ahoj\""),
            ("(text-concat \"Ag\" \"el\")", "\"Agel\""),
            ("(text-symbol \"agel\")", "agel"),
            ("(eval '(+ 20 22))", ""),
            ("(eval (list '+ 20 22))", ""),
            ("(eval (cons 'list '(1 2)))", "(1 2)"),
        ] {
            assert_eq!(text(&mut session, source), expected, "{source}");
        }
        assert_eq!(eval(&mut session, "(car '(20 22))"), Value::Int(20));
        assert_eq!(eval(&mut session, "(count \"Ahoj 👋\")"), Value::Int(6));
        assert_eq!(
            eval(&mut session, "(text-bytes \"Ahoj 👋\")"),
            Value::Int(9)
        );
        assert_eq!(
            eval(&mut session, "(= (dict 'a 1 'b 2) (dict 'b 2 'a 1))"),
            Value::Bool(false)
        );
        assert_eq!(eval(&mut session, "(= 'a \"a\")"), Value::Bool(false));
        assert_eq!(
            eval(&mut session, "(has-key? (dict 'x nil) 'x)"),
            Value::Bool(true)
        );

        // Quoted graphs persist in globals and reconstruct after rollback.
        eval(&mut session, "(def plan '(compile (core) \"v1\"))");
        eval(&mut session, "(def table (assoc (dict 'plan plan) 'n 1))");
        assert_eq!(
            text(&mut session, "(get table 'plan)"),
            "(compile (core) \"v1\")"
        );
        // The dissoc is the last commit before rollback, since every read also
        // commits a revision and rollback only restores the preceding one.
        eval(&mut session, "(def table (dissoc table 'plan))");
        session.rollback().unwrap();
        assert_eq!(
            text(&mut session, "table"),
            "{plan (compile (core) \"v1\") n 1}"
        );
        assert_eq!(
            text(&mut session, "(car (cdr (get table 'plan)))"),
            "(core)"
        );

        // Agents exchange data, and a faulted turn keeps the list message.
        eval(
            &mut session,
            "(def collect (fn (self state message) (cons message state)))",
        );
        eval(&mut session, "(def log (spawn collect nil))");
        eval(&mut session, "(send log '(open \"a\"))");
        eval(&mut session, "(send log (dict 'close 1))");
        eval(&mut session, "(run 2)");
        assert_eq!(
            text(&mut session, "(agent-state log)"),
            "({close 1} (open \"a\"))"
        );
        eval(
            &mut session,
            "(def strict (fn (self state message) (if (= message 'stop) (/ 1 0) message)))",
        );
        eval(&mut session, "(def s (spawn strict nil))");
        eval(&mut session, "(send s 'stop)");
        eval(&mut session, "(step)");
        assert_eq!(eval(&mut session, "(agent-faulted? s)"), Value::Bool(true));
        assert_eq!(text(&mut session, "(drop-message s)"), "stop");

        for bad in [
            "(cons 1 2)",
            "(car 1)",
            "(dict 'a)",
            "(get 1 'a)",
            "(assoc nil 'a 1)",
            "(text-byte \"A\" 1)",
            "(text-byte \"A\" -1)",
            "(text-slice \"👋\" 0 1)",
            "(text-concat \"a\" 1)",
            "(text-symbol 'agel)",
            "\"unterminated",
            "\"bad \\q escape\"",
            "(def list 1)",
            "(eval 42 1)",
        ] {
            assert!(session.evaluate(bad.as_bytes()).is_err(), "accepted {bad}");
        }
    }

    #[test]
    fn heap_is_collected_at_commit_and_exhaustion_is_transactional() {
        let mut session = Session::new();
        eval(&mut session, "(def keep '(a b c))");
        // Far more garbage than the heap holds, across many transactions.
        for _ in 0..40 {
            eval(&mut session, "(list 1 2 3 4 5 6 7 8)");
            eval(
                &mut session,
                "(text-concat \"0123456789abcdef\" \"0123456789abcdef\")",
            );
        }
        assert!(
            session.active.heap.cell_count < 16,
            "cells {}",
            session.active.heap.cell_count
        );
        assert!(
            session.active.heap.text_len < 64,
            "text {}",
            session.active.heap.text_len
        );
        assert_eq!(text(&mut session, "keep"), "(a b c)");
        // A transaction that would overrun the heap is rejected whole, leaving
        // the committed world and its revision untouched.
        eval(
            &mut session,
            "(def deep (fn (n acc) (if (= n 0) acc (deep (- n 1) (cons n acc)))))",
        );
        let revision = session.revision();
        let cells_before = session.active.heap.cell_count;
        assert!(session.evaluate(b"(def big (deep 500 nil))").is_err());
        assert_eq!(session.revision(), revision);
        assert_eq!(session.active.heap.cell_count, cells_before);
        assert_eq!(text(&mut session, "keep"), "(a b c)");
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
    fn stored_functions_keep_their_captures() {
        let mut session = Session::new();
        assert_eq!(
            eval(&mut session, "(def add40 ((fn (x) (fn (y) (+ x y))) 40))"),
            Value::Function
        );
        assert_eq!(eval(&mut session, "(add40 2)"), Value::Int(42));
        // A parameter shadows a capture of the same name.
        assert_eq!(
            eval(&mut session, "(def shadow ((fn (x) (fn (x) (+ x 1))) 40))"),
            Value::Function
        );
        assert_eq!(eval(&mut session, "(shadow 1)"), Value::Int(2));
        // A lambda escaping a stored function is stored with its captures.
        eval(&mut session, "(def make-adder (fn (n) (fn (m) (+ n m))))");
        assert_eq!(eval(&mut session, "((make-adder 5) 6)"), Value::Int(11));
        assert_eq!(
            eval(&mut session, "(def add5 (make-adder 5))"),
            Value::Function
        );
        assert_eq!(eval(&mut session, "(add5 7)"), Value::Int(12));
        // Captures are values fixed when the closure was made.
        eval(&mut session, "(def base 1)");
        eval(
            &mut session,
            "(def from-base (let ((b base)) (fn (m) (+ b m))))",
        );
        eval(&mut session, "(def base 100)");
        assert_eq!(eval(&mut session, "(from-base 1)"), Value::Int(2));
        // Function-valued captures are still refused, not dropped.
        assert_eq!(
            session.evaluate(b"(def compose ((fn (f) (fn (x) (f x))) add40))"),
            Err(Error(
                "native closures currently capture scalar values only"
            ))
        );
    }

    #[test]
    fn reaped_slots_are_reused_and_old_handles_refused() {
        let mut session = Session::new();
        eval(&mut session, "(def tick (fn (self state message) message))");
        eval(&mut session, "(def a (spawn tick 0))");
        for _ in 1..MAX_AGENTS {
            eval(&mut session, "(spawn tick 0)");
        }
        assert!(session.evaluate(b"(spawn tick 0)").is_err());
        assert_eq!(eval(&mut session, "(agent-count)"), Value::Int(8));
        eval(&mut session, "(scene-rect 1 1 4 4 0 0)");
        eval(&mut session, "(scene-bind 7 a)");
        assert_eq!(eval(&mut session, "(scene-owner 7)"), Value::Agent(1));
        assert_eq!(eval(&mut session, "(reap-agent a)"), Value::Bool(true));
        assert_eq!(eval(&mut session, "(agent-count)"), Value::Int(7));
        assert_eq!(
            session.evaluate(b"(send a 1)"),
            Err(Error("stale native agent"))
        );
        assert_eq!(
            session.evaluate(b"(reap-agent a)"),
            Err(Error("stale native agent"))
        );
        assert_eq!(eval(&mut session, "(scene-owner 7)"), Value::Agent(0));
        // The freed slot is the next one spawned into, at a new generation.
        assert_eq!(
            eval(&mut session, "(def b (spawn tick 5))"),
            Value::Agent(1 | (1 << 8))
        );
        assert_eq!(eval(&mut session, "(agent-count)"), Value::Int(8));
        assert_eq!(
            session.evaluate(b"(agent-state a)"),
            Err(Error("stale native agent"))
        );
        assert_eq!(eval(&mut session, "(agent-state b)"), Value::Int(5));
        // Reaping is transactional like everything else: a failing form
        // that reaps leaves the agent in place.
        assert!(session.evaluate(b"(begin (reap-agent b) (/ 1 0))").is_err());
        assert_eq!(eval(&mut session, "(agent-state b)"), Value::Int(5));
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
