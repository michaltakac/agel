//! The Agel language runtime: reader, hygienic expander, evaluator, agents
//! and transactional worlds.
//!
//! With the default `std` feature this is the hosted runtime. Without it the
//! crate is `no_std` over `alloc`, and the same runtime is what a process the
//! Agel supervisor loads links to bring the language into a protection
//! domain (`boot/posix/agel`). Nothing in the language differs between the
//! two builds; see `Cargo.toml` for what `std` adds.
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

mod agent;
mod eval;
mod macro_expander;
mod model;
mod reader;
mod value;
mod world;

pub use agel_integrity::Digest;
pub use agent::{AgentStatus, Event, EventKind, FailureAction, Protocol, TypeSpec};
pub use eval::{Condition, EvalError};
pub use model::{
    EffectJournal, EffectJournalEntry, EffectJournalStatus, EffectKey, ModelCompletion,
    ModelCompletionError, ModelDispatchError, ModelOutcome, ModelRequest,
};
pub use reader::{read_all, read_all_with_limits, ReadError, ReadLimits};
pub use value::{Capability, Expr, Value};
pub use world::{
    AuthorityError, Budget, Commit, EvaluationOptions, HostError, HostWord, Pulse, ReplayError,
    ReplayInput, ReplayReport, Snapshot, TransactionError, World,
};
