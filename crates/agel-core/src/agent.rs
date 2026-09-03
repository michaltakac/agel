use crate::value::{Capability, Closure};
use crate::Value;
use std::collections::{BTreeMap, VecDeque};
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypeSpec {
    Any,
    Nil,
    Bool,
    Int,
    String,
    Symbol,
    List,
    Map,
    Agent,
    Module,
    Capability,
    Callable,
    Protocol,
}

impl TypeSpec {
    pub(crate) fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "any" => Self::Any,
            "nil" => Self::Nil,
            "bool" => Self::Bool,
            "int" => Self::Int,
            "string" => Self::String,
            "symbol" => Self::Symbol,
            "list" => Self::List,
            "map" => Self::Map,
            "agent" => Self::Agent,
            "module" => Self::Module,
            "capability" => Self::Capability,
            "callable" => Self::Callable,
            "protocol" => Self::Protocol,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Any => "any",
            Self::Nil => "nil",
            Self::Bool => "bool",
            Self::Int => "int",
            Self::String => "string",
            Self::Symbol => "symbol",
            Self::List => "list",
            Self::Map => "map",
            Self::Agent => "agent",
            Self::Module => "module",
            Self::Capability => "capability",
            Self::Callable => "callable",
            Self::Protocol => "protocol",
        }
    }

    pub(crate) fn accepts(self, value: &Value) -> bool {
        match self {
            Self::Any => true,
            Self::Nil => matches!(value, Value::Nil),
            Self::Bool => matches!(value, Value::Bool(_)),
            Self::Int => matches!(value, Value::Int(_)),
            Self::String => matches!(value, Value::String(_)),
            Self::Symbol => matches!(value, Value::Symbol(_)),
            Self::List => matches!(value, Value::Nil | Value::List(_)),
            Self::Map => matches!(value, Value::Map(_)),
            Self::Agent => matches!(value, Value::Agent(_)),
            Self::Module => matches!(value, Value::Module(_)),
            Self::Capability => matches!(value, Value::Capability(_)),
            Self::Callable => matches!(value, Value::Closure(_) | Value::Builtin(_)),
            Self::Protocol => matches!(value, Value::Protocol(_)),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Protocol {
    name: String,
    messages: BTreeMap<String, Vec<TypeSpec>>,
}

impl Protocol {
    pub(crate) fn new(name: String, messages: BTreeMap<String, Vec<TypeSpec>>) -> Self {
        Self { name, messages }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn messages(&self) -> &BTreeMap<String, Vec<TypeSpec>> {
        &self.messages
    }

    pub(crate) fn validate(&self, message: &Value) -> Result<(), String> {
        let Value::List(parts) = message else {
            return Err(format!(
                "protocol {} requires a tagged list message",
                self.name
            ));
        };
        let Some(tag_value) = parts.first() else {
            return Err(format!("protocol {} rejects an empty message", self.name));
        };
        let tag = match tag_value {
            Value::Symbol(tag) | Value::String(tag) => tag,
            _ => {
                return Err(format!(
                    "protocol {} requires a symbol or string message tag",
                    self.name
                ));
            }
        };
        let Some(types) = self.messages.get(tag) else {
            return Err(format!("protocol {} has no message named {tag}", self.name));
        };
        let payload = &parts[1..];
        if payload.len() != types.len() {
            return Err(format!(
                "protocol {} message {tag} expects {} payload value(s), got {}",
                self.name,
                types.len(),
                payload.len()
            ));
        }
        for (index, (expected, actual)) in types.iter().zip(payload).enumerate() {
            if !expected.accepts(actual) {
                return Err(format!(
                    "protocol {} message {tag} payload {} expects {}, got {actual}",
                    self.name,
                    index + 1,
                    expected.name()
                ));
            }
        }
        Ok(())
    }
}

impl fmt::Display for Protocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#<protocol:{}>", self.name)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentStatus {
    Running,
    Stopped,
}

impl AgentStatus {
    pub fn name(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Stopped => "stopped",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureAction {
    Restart,
    Stop,
    Escalate,
}

impl FailureAction {
    pub(crate) fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "restart" => Self::Restart,
            "stop" => Self::Stop,
            "escalate" => Self::Escalate,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Restart => "restart",
            Self::Stop => "stop",
            Self::Escalate => "escalate",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventKind {
    Spawned,
    MessageQueued,
    TurnStarted,
    TurnCommitted,
    TurnFailed,
    Restarted,
    Stopped,
    Escalated,
    ModelRequested,
    ModelDispatchStarted,
    ModelCompleted,
    ModelFailed,
    ModelDeliveryDropped,
}

impl EventKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Spawned => "spawned",
            Self::MessageQueued => "message-queued",
            Self::TurnStarted => "turn-started",
            Self::TurnCommitted => "turn-committed",
            Self::TurnFailed => "turn-failed",
            Self::Restarted => "restarted",
            Self::Stopped => "stopped",
            Self::Escalated => "escalated",
            Self::ModelRequested => "model-requested",
            Self::ModelDispatchStarted => "model-dispatch-started",
            Self::ModelCompleted => "model-completed",
            Self::ModelFailed => "model-failed",
            Self::ModelDeliveryDropped => "model-delivery-dropped",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    pub sequence: u64,
    pub kind: EventKind,
    pub agent: u64,
    pub detail: Value,
}

impl Event {
    pub fn as_value(&self) -> Value {
        Value::Map(vec![
            (
                Value::Symbol("sequence".into()),
                Value::Int(i64::try_from(self.sequence).unwrap_or(i64::MAX)),
            ),
            (
                Value::Symbol("kind".into()),
                Value::Symbol(self.kind.name().into()),
            ),
            (Value::Symbol("agent".into()), Value::Agent(self.agent)),
            (Value::Symbol("detail".into()), self.detail.clone()),
        ])
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Agent {
    pub name: String,
    pub mailbox: VecDeque<Value>,
    pub behavior: Option<Closure>,
    pub heap: Value,
    pub initial_heap: Value,
    pub protocol: Option<Protocol>,
    pub supervisor: Option<u64>,
    pub failure_action: FailureAction,
    pub max_restarts: u32,
    pub restart_count: u32,
    pub status: AgentStatus,
    pub capabilities: Vec<Capability>,
}
