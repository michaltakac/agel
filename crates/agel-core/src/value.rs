use crate::agent::Protocol;
use crate::canon::{Canon, CanonError, Decoder, Encoder};
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::{format, string::String, vec, vec::Vec};
use core::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expr {
    Nil,
    Bool(bool),
    Int(i64),
    String(String),
    Symbol(String),
    List(Vec<Expr>),
    #[doc(hidden)]
    ScopedSymbol {
        name: String,
        module: Option<String>,
    },
}

#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Builtin {
    Add,
    Subtract,
    Multiply,
    Divide,
    Equal,
    LessThan,
    List,
    Cons,
    Car,
    Cdr,
    Dict,
    Get,
    HasKey,
    Assoc,
    Dissoc,
    Keys,
    Count,
    TypeOf,
    TextBytes,
    TextByte,
    TextSlice,
    TextConcat,
    TextSymbol,
    Apply,
    Spawn,
    Send,
    Receive,
    Run,
    Step,
    AgentInfo,
    EventLog,
    PendingTurns,
    ModelRequest,
    PendingModelRequests,
    Signal,
    RequestCapability,
    CapabilityKind,
    CapabilityScope,
    /// A word the embedding supplied: an index into the host table of the
    /// options an evaluation runs with (`EvaluationOptions::host`), bound
    /// by `World::install_host`.
    Host(u16),
}

#[doc(hidden)]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Env {
    bindings: Arc<BTreeMap<String, Value>>,
    parent: Option<Arc<Env>>,
}

impl Env {
    pub(crate) fn child(&self) -> Self {
        Self {
            bindings: Arc::default(),
            parent: Some(Arc::new(self.clone())),
        }
    }

    pub(crate) fn get(&self, name: &str) -> Option<&Value> {
        self.bindings
            .get(name)
            .or_else(|| self.parent.as_deref().and_then(|parent| parent.get(name)))
    }

    pub(crate) fn insert(&mut self, name: String, value: Value) {
        Arc::make_mut(&mut self.bindings).insert(name, value);
    }
}

#[doc(hidden)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Closure {
    pub params: Vec<String>,
    pub body: Vec<Expr>,
    pub env: Env,
    pub module: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Capability {
    id: u64,
    kind: String,
    scope: String,
    issuer_world: u64,
    epoch: u64,
}

impl Capability {
    pub(crate) fn new(id: u64, kind: String, scope: String, issuer_world: u64, epoch: u64) -> Self {
        Self {
            id,
            kind,
            scope,
            issuer_world,
            epoch,
        }
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn kind(&self) -> &str {
        &self.kind
    }

    pub fn scope(&self) -> &str {
        &self.scope
    }

    pub fn issuer_world(&self) -> u64 {
        self.issuer_world
    }

    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    pub(crate) fn permits(&self, kind: &str, scope: &str, world: u64, epoch: u64) -> bool {
        self.issuer_world == world
            && self.epoch == epoch
            && self.kind == kind
            && (self.scope == "*"
                || self.scope == scope
                || scope
                    .strip_prefix(&self.scope)
                    .is_some_and(|suffix| suffix.starts_with('/')))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Nil,
    Bool(bool),
    Int(i64),
    String(String),
    Symbol(String),
    List(Vec<Value>),
    Map(Vec<(Value, Value)>),
    Agent(u64),
    Protocol(Protocol),
    Module(String),
    Capability(Capability),
    #[doc(hidden)]
    Closure(Arc<Closure>),
    #[doc(hidden)]
    Builtin(Builtin),
}

impl Value {
    pub fn is_truthy(&self) -> bool {
        !matches!(self, Self::Nil | Self::Bool(false))
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Nil => "nil",
            Self::Bool(_) => "bool",
            Self::Int(_) => "int",
            Self::String(_) => "string",
            Self::Symbol(_) => "symbol",
            Self::List(_) => "list",
            Self::Map(_) => "map",
            Self::Agent(_) => "agent",
            Self::Protocol(_) => "protocol",
            Self::Module(_) => "module",
            Self::Capability(_) => "capability",
            Self::Closure(_) | Self::Builtin(_) => "callable",
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Nil => write!(f, "nil"),
            Self::Bool(true) => write!(f, "#t"),
            Self::Bool(false) => write!(f, "#f"),
            Self::Int(value) => write!(f, "{value}"),
            Self::String(value) => write!(f, "\"{}\"", escape_string(value)),
            Self::Symbol(value) => write!(f, "{value}"),
            Self::List(values) => display_sequence(f, "(", ")", values.iter()),
            Self::Map(entries) => {
                write!(f, "{{")?;
                for (index, (key, value)) in entries.iter().enumerate() {
                    if index > 0 {
                        write!(f, " ")?;
                    }
                    write!(f, "{key} {value}")?;
                }
                write!(f, "}}")
            }
            Self::Agent(id) => write!(f, "#<agent:{id}>"),
            Self::Protocol(protocol) => protocol.fmt(f),
            Self::Module(name) => write!(f, "#<module:{name}>"),
            Self::Capability(capability) => {
                write!(f, "#<capability:{}:{}>", capability.kind, capability.scope)
            }
            Self::Closure(_) => write!(f, "#<closure>"),
            Self::Builtin(_) => write!(f, "#<builtin>"),
        }
    }
}

fn display_sequence<'a>(
    f: &mut fmt::Formatter<'_>,
    open: &str,
    close: &str,
    values: impl Iterator<Item = &'a Value>,
) -> fmt::Result {
    write!(f, "{open}")?;
    for (index, value) in values.enumerate() {
        if index > 0 {
            write!(f, " ")?;
        }
        write!(f, "{value}")?;
    }
    write!(f, "{close}")
}

fn escape_string(value: &str) -> String {
    value
        .chars()
        .flat_map(|character| match character {
            '\\' => "\\\\".chars().collect::<Vec<_>>(),
            '"' => "\\\"".chars().collect(),
            '\n' => "\\n".chars().collect(),
            '\r' => "\\r".chars().collect(),
            '\t' => "\\t".chars().collect(),
            other => vec![other],
        })
        .collect()
}

impl Builtin {
    /// The name a builtin is bound to in a fresh world, or `host/N` for
    /// the N-th host word: how an encoding names it.
    fn canonical_name(self) -> String {
        if let Self::Host(index) = self {
            return format!("host/{index}");
        }
        crate::world::SEED_BUILTINS
            .iter()
            .find(|(_, builtin)| *builtin == self)
            .map(|(name, _)| String::from(*name))
            .unwrap_or_else(|| format!("{self:?}"))
    }

    fn from_canonical_name(name: &str) -> Option<Self> {
        if let Some(index) = name.strip_prefix("host/") {
            return index.parse().ok().map(Self::Host);
        }
        crate::world::SEED_BUILTINS
            .iter()
            .find(|(seed, _)| *seed == name)
            .map(|(_, builtin)| *builtin)
    }
}

impl Canon for Expr {
    fn canon(&self, out: &mut Encoder) {
        match self {
            Self::Nil => out.tag("nil"),
            Self::Bool(value) => {
                out.tag("bool");
                out.bool(*value);
            }
            Self::Int(value) => {
                out.tag("int");
                out.i64(*value);
            }
            Self::String(value) => {
                out.tag("string");
                out.text(value);
            }
            Self::Symbol(value) => {
                out.tag("symbol");
                out.text(value);
            }
            Self::List(items) => {
                out.tag("list");
                out.items(items.iter());
            }
            Self::ScopedSymbol { name, module } => {
                out.tag("scoped");
                out.text(name);
                out.option(module.as_ref());
            }
        }
    }

    fn decode_inner(input: &mut Decoder<'_>) -> Result<Self, CanonError> {
        Ok(match input.tag()? {
            "nil" => Self::Nil,
            "bool" => Self::Bool(input.bool()?),
            "int" => Self::Int(input.i64()?),
            "string" => Self::String(input.text()?),
            "symbol" => Self::Symbol(input.text()?),
            "list" => Self::List(input.items()?),
            "scoped" => Self::ScopedSymbol {
                name: input.text()?,
                module: input.option()?,
            },
            other => return input.fail(format!("not a syntax tag: {other}")),
        })
    }
}

impl Canon for Value {
    fn canon(&self, out: &mut Encoder) {
        match self {
            Self::Nil => out.tag("nil"),
            Self::Bool(value) => {
                out.tag("bool");
                out.bool(*value);
            }
            Self::Int(value) => {
                out.tag("int");
                out.i64(*value);
            }
            Self::String(value) => {
                out.tag("string");
                out.text(value);
            }
            Self::Symbol(value) => {
                out.tag("symbol");
                out.text(value);
            }
            Self::List(items) => {
                out.tag("list");
                out.items(items.iter());
            }
            Self::Map(entries) => {
                out.tag("map");
                out.seq(entries.len());
                for (key, value) in entries {
                    key.canon(out);
                    value.canon(out);
                }
            }
            Self::Agent(id) => {
                out.tag("agent");
                out.u64(*id);
            }
            Self::Protocol(protocol) => protocol.canon(out),
            Self::Module(name) => {
                out.tag("module");
                out.text(name);
            }
            Self::Capability(capability) => capability.canon(out),
            Self::Closure(closure) => closure.canon(out),
            Self::Builtin(builtin) => {
                out.tag("builtin");
                out.text(&builtin.canonical_name());
            }
        }
    }

    fn decode_inner(input: &mut Decoder<'_>) -> Result<Self, CanonError> {
        Ok(match input.peek_tag()? {
            "protocol" => Self::Protocol(Protocol::decode(input)?),
            "capability" => Self::Capability(Capability::decode(input)?),
            "fn" => Self::Closure(Arc::new(Closure::decode(input)?)),
            _ => match input.tag()? {
                "nil" => Self::Nil,
                "bool" => Self::Bool(input.bool()?),
                "int" => Self::Int(input.i64()?),
                "string" => Self::String(input.text()?),
                "symbol" => Self::Symbol(input.text()?),
                "list" => Self::List(input.items()?),
                "map" => {
                    let count = input.seq()?;
                    let mut entries = Vec::with_capacity(count.min(4096));
                    for _ in 0..count {
                        let key = Self::decode(input)?;
                        let value = Self::decode(input)?;
                        entries.push((key, value));
                    }
                    Self::Map(entries)
                }
                "agent" => Self::Agent(input.u64()?),
                "module" => Self::Module(input.text()?),
                "builtin" => {
                    let name = input.text()?;
                    match Builtin::from_canonical_name(&name) {
                        Some(builtin) => Self::Builtin(builtin),
                        None => return input.fail(format!("not a builtin: {name}")),
                    }
                }
                other => return input.fail(format!("not a value tag: {other}")),
            },
        })
    }
}

impl Canon for Capability {
    fn canon(&self, out: &mut Encoder) {
        out.tag("capability");
        out.u64(self.id);
        out.text(&self.kind);
        out.text(&self.scope);
        out.u64(self.issuer_world);
        out.u64(self.epoch);
    }

    fn decode_inner(input: &mut Decoder<'_>) -> Result<Self, CanonError> {
        input.expect("capability")?;
        let id = input.u64()?;
        let kind = input.text()?;
        let scope = input.text()?;
        let issuer_world = input.u64()?;
        let epoch = input.u64()?;
        Ok(Self::new(id, kind, scope, issuer_world, epoch))
    }
}

impl Canon for Closure {
    fn canon(&self, out: &mut Encoder) {
        out.tag("fn");
        out.items(self.params.iter());
        out.items(self.body.iter());
        self.env.canon(out);
        out.option(self.module.as_ref());
    }

    fn decode_inner(input: &mut Decoder<'_>) -> Result<Self, CanonError> {
        input.expect("fn")?;
        Ok(Self {
            params: input.items()?,
            body: input.items()?,
            env: Env::decode(input)?,
            module: input.option()?,
        })
    }
}

impl Canon for Env {
    /// Frames innermost first, each its bindings in order; shared frames are
    /// written wherever they are reached.
    fn canon(&self, out: &mut Encoder) {
        out.tag("env");
        let mut frames = 0;
        let mut frame = Some(self);
        while let Some(current) = frame {
            frames += 1;
            frame = current.parent.as_deref();
        }
        out.seq(frames);
        let mut frame = Some(self);
        while let Some(current) = frame {
            out.entries(current.bindings.iter());
            frame = current.parent.as_deref();
        }
    }

    fn decode_inner(input: &mut Decoder<'_>) -> Result<Self, CanonError> {
        input.expect("env")?;
        let count = input.seq()?;
        if count == 0 || count > crate::canon::MAX_DECODE_DEPTH {
            return input.fail("environment frame count exceeds the depth limit or is zero");
        }
        let mut frames = Vec::with_capacity(count);
        for _ in 0..count {
            frames.push(input.entries::<Value>()?);
        }
        // Innermost first in the bytes; the chain is built from the outside.
        let mut env = None;
        for bindings in frames.into_iter().rev() {
            env = Some(Self {
                bindings: Arc::new(bindings),
                parent: env.map(Arc::new),
            });
        }
        Ok(env.unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexical_frames_share_storage_but_writes_do_not_alias() {
        let mut original = Env::default();
        original.insert("x".into(), Value::Int(1));
        let captured = original.clone();
        assert!(Arc::ptr_eq(&original.bindings, &captured.bindings));
        original.insert("x".into(), Value::Int(2));
        assert!(!Arc::ptr_eq(&original.bindings, &captured.bindings));
        let mut child = captured.child();
        child.insert("x".into(), Value::Int(3));
        assert_eq!(original.get("x"), Some(&Value::Int(2)));
        assert_eq!(captured.get("x"), Some(&Value::Int(1)));
        assert_eq!(child.get("x"), Some(&Value::Int(3)));
    }
}
