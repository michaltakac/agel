use crate::agent::Protocol;
use std::collections::BTreeMap;
use std::fmt;

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
    List,
    Cons,
    Car,
    Cdr,
    Dict,
    Get,
    Assoc,
    Dissoc,
    Keys,
    Count,
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
}

#[doc(hidden)]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Env {
    bindings: BTreeMap<String, Value>,
    parent: Option<Box<Env>>,
}

impl Env {
    pub(crate) fn child(&self) -> Self {
        Self {
            bindings: BTreeMap::new(),
            parent: Some(Box::new(self.clone())),
        }
    }

    pub(crate) fn get(&self, name: &str) -> Option<&Value> {
        self.bindings
            .get(name)
            .or_else(|| self.parent.as_deref().and_then(|parent| parent.get(name)))
    }

    pub(crate) fn insert(&mut self, name: String, value: Value) {
        self.bindings.insert(name, value);
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
}

impl Capability {
    pub(crate) fn new(id: u64, kind: String, scope: String) -> Self {
        Self { id, kind, scope }
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

    pub(crate) fn permits(&self, kind: &str, scope: &str) -> bool {
        self.kind == kind
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
    Closure(Closure),
    #[doc(hidden)]
    Builtin(Builtin),
}

impl Value {
    pub fn is_truthy(&self) -> bool {
        !matches!(self, Self::Nil | Self::Bool(false))
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
