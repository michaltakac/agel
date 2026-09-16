use crate::agent::{Agent, Event};
use crate::canon::{Canon, CanonError, Decoder, Encoder};
use crate::eval::{eval_all, EvalError};
use crate::macro_expander::MacroDef;
use crate::model::{
    EffectJournal, EffectJournalEntry, EffectJournalStatus, ModelCompletion, ModelCompletionError,
    ModelDispatchError, ModelRecord, ModelRequest, ModelRequestStatus,
};
use crate::reader::{read_all_with_limits, ReadError, ReadLimits};
use crate::value::Builtin;
use crate::{Capability, Value};
use alloc::collections::{BTreeMap, BTreeSet, VecDeque};
#[cfg(not(feature = "std"))]
use alloc::rc::Rc;
#[cfg(feature = "std")]
use alloc::sync::Arc;
use alloc::{boxed::Box, format, string::String, vec::Vec};
#[cfg(not(feature = "std"))]
use core::cell::{RefCell, RefMut};
use core::fmt;
use core::sync::atomic::{AtomicU64, Ordering};
#[cfg(feature = "std")]
use std::sync::{Mutex, MutexGuard};

/// The effect journal a world shares with its snapshots: behind a mutex in
/// an `Arc` where the hosted runtime may cross threads, behind a `RefCell`
/// in an `Rc` in the `no_std` build, which a loaded process links and
/// never threads.
#[cfg(feature = "std")]
type Journal = Arc<Mutex<EffectJournal>>;
#[cfg(not(feature = "std"))]
type Journal = Rc<RefCell<EffectJournal>>;

const DEFAULT_HISTORY_LIMIT: usize = 64;
static NEXT_WORLD_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Budget {
    pub fuel: u64,
    pub max_call_depth: usize,
    pub max_collection_len: usize,
    pub max_source_bytes: usize,
    pub max_parse_depth: usize,
    pub max_model_prompt_bytes: usize,
    pub max_pending_model_requests: usize,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            fuel: 100_000,
            max_call_depth: 256,
            max_collection_len: 65_536,
            max_source_bytes: 1_048_576,
            max_parse_depth: 256,
            max_model_prompt_bytes: 65_536,
            max_pending_model_requests: 1_024,
        }
    }
}

/// A hook the evaluator calls every `every` fuel ticks, so an embedding
/// whose supervisor stops a domain that computes too long between requests
/// can make one: the process the Agel supervisor loads asks for the clock,
/// which is an entry boundary. A plain function pointer, so options stay
/// `Copy`-cheap to clone; the hook sees no evaluator state. Options with a
/// hook are not comparable: two function pointers are not.
#[derive(Clone, Copy, Debug)]
pub struct Pulse {
    pub every: u64,
    pub hook: fn(),
}

/// A word the embedding supplies to the language: its name, the capability
/// kind a caller must hold to apply it (`""` for none), and the function.
/// The evaluator binds each to `Builtin::Host(index)` when a world installs
/// a table, and applies it through the table in the options an evaluation
/// runs with, so a world's bindings never hold a pointer. A word that needs
/// a capability is refused with `capability/denied` when the caller — the
/// agent whose turn it is, or the evaluation's own set — holds none of that
/// kind, whatever the word would do: the check is the evaluator's.
#[derive(Clone, Copy, Debug)]
pub struct HostWord {
    pub name: &'static str,
    pub capability: &'static str,
    pub call: fn(&[Value]) -> Result<Value, HostError>,
}

/// What a host word signals: a condition kind and message, raised in the
/// caller's transaction like any other.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostError {
    pub kind: String,
    pub message: String,
}

#[derive(Clone, Debug, Default)]
pub struct EvaluationOptions {
    pub budget: Budget,
    pub capabilities: Vec<Capability>,
    pub pulse: Option<Pulse>,
    /// The host words an evaluation may apply; a world's bindings made by
    /// `install_host` index this table, and an evaluation without it
    /// answers `host/unavailable` for them.
    pub host: &'static [HostWord],
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Module {
    pub bindings: BTreeMap<String, Value>,
    pub macros: BTreeMap<String, MacroDef>,
    pub exports: BTreeSet<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct State {
    pub bindings: BTreeMap<String, Value>,
    pub macros: BTreeMap<String, MacroDef>,
    pub modules: BTreeMap<String, Module>,
    pub agents: BTreeMap<u64, Agent>,
    pub ready_queue: VecDeque<u64>,
    pub events: Vec<Event>,
    pub next_event_sequence: u64,
    pub next_agent_id: u64,
    pub next_syntax_id: u64,
    pub model_requests: BTreeMap<u64, ModelRecord>,
    pub next_model_request_id: u64,
}

/// The seed builtins by the names a fresh world binds them to: the
/// evaluator's own words, which an encoding names by these.
pub(crate) const SEED_BUILTINS: &[(&str, Builtin)] = &[
    ("+", Builtin::Add),
    ("-", Builtin::Subtract),
    ("*", Builtin::Multiply),
    ("/", Builtin::Divide),
    ("=", Builtin::Equal),
    ("<", Builtin::LessThan),
    ("list", Builtin::List),
    ("cons", Builtin::Cons),
    ("car", Builtin::Car),
    ("cdr", Builtin::Cdr),
    ("dict", Builtin::Dict),
    ("get", Builtin::Get),
    ("has-key?", Builtin::HasKey),
    ("assoc", Builtin::Assoc),
    ("dissoc", Builtin::Dissoc),
    ("keys", Builtin::Keys),
    ("count", Builtin::Count),
    ("type-of", Builtin::TypeOf),
    ("text-bytes", Builtin::TextBytes),
    ("text-byte", Builtin::TextByte),
    ("text-slice", Builtin::TextSlice),
    ("text-concat", Builtin::TextConcat),
    ("text-symbol", Builtin::TextSymbol),
    ("apply", Builtin::Apply),
    ("spawn", Builtin::Spawn),
    ("send", Builtin::Send),
    ("recv", Builtin::Receive),
    ("run", Builtin::Run),
    ("step", Builtin::Step),
    ("agent-info", Builtin::AgentInfo),
    ("event-log", Builtin::EventLog),
    ("pending-turns", Builtin::PendingTurns),
    ("model-request", Builtin::ModelRequest),
    ("pending-model-requests", Builtin::PendingModelRequests),
    ("signal", Builtin::Signal),
    ("request-capability", Builtin::RequestCapability),
    ("capability-kind", Builtin::CapabilityKind),
    ("capability-scope", Builtin::CapabilityScope),
];

impl Default for State {
    fn default() -> Self {
        let mut bindings = BTreeMap::new();
        for (name, builtin) in SEED_BUILTINS {
            bindings.insert((*name).into(), Value::Builtin(*builtin));
        }
        Self {
            bindings,
            macros: BTreeMap::new(),
            modules: BTreeMap::new(),
            agents: BTreeMap::new(),
            ready_queue: VecDeque::new(),
            events: Vec::new(),
            next_event_sequence: 1,
            next_agent_id: 1,
            next_syntax_id: 1,
            model_requests: BTreeMap::new(),
            next_model_request_id: 1,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Commit {
    pub revision: u64,
    pub values: Vec<Value>,
    pub steps_used: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransactionError {
    Read(ReadError),
    Eval(EvalError),
    RevisionExhausted,
}

impl fmt::Display for TransactionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(error) => error.fmt(f),
            Self::Eval(error) => write!(f, "evaluation error: {error}"),
            Self::RevisionExhausted => f.write_str("world revision space exhausted"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for TransactionError {}

impl From<ReadError> for TransactionError {
    fn from(value: ReadError) -> Self {
        Self::Read(value)
    }
}

impl From<EvalError> for TransactionError {
    fn from(value: EvalError) -> Self {
        Self::Eval(value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorityError;

impl fmt::Display for AuthorityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("capability identifier space exhausted")
    }
}

#[cfg(feature = "std")]
impl std::error::Error for AuthorityError {}

#[derive(Clone, Debug)]
pub struct Snapshot {
    state: State,
    revision: u64,
    next_revision: u64,
    next_capability_id: u64,
    digest: u64,
    world_id: u64,
    authority_epoch: u64,
    effect_journal: Journal,
}

impl Snapshot {
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn digest(&self) -> u64 {
        self.digest
    }

    pub fn content_digest(&self) -> agel_integrity::Digest {
        state_content_digest(&self.state)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayReport {
    pub final_revision: u64,
    pub final_digest: u64,
    pub events: Vec<Event>,
    pub values: Vec<Vec<Value>>,
    pub steps_used: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReplayInput {
    Evaluate(String),
    ClaimModel(u64),
    CompleteModel(ModelCompletion),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReplayError {
    InvalidSnapshot,
    Transaction(TransactionError),
    StepCountOverflow,
}

impl fmt::Display for ReplayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSnapshot => f.write_str("snapshot digest does not match its state"),
            Self::Transaction(error) => error.fmt(f),
            Self::StepCountOverflow => f.write_str("replay step count overflow"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for ReplayError {}

#[derive(Clone, Debug)]
pub struct World {
    state: State,
    revision: u64,
    next_revision: u64,
    next_capability_id: u64,
    history: VecDeque<(u64, State)>,
    history_limit: usize,
    world_id: u64,
    authority_epoch: u64,
    effect_journal: Journal,
}

impl Default for World {
    fn default() -> Self {
        Self::new(DEFAULT_HISTORY_LIMIT)
    }
}

impl World {
    pub fn new(history_limit: usize) -> Self {
        Self {
            state: State::default(),
            revision: 0,
            next_revision: 1,
            next_capability_id: 1,
            history: VecDeque::new(),
            history_limit,
            world_id: next_world_id(),
            authority_epoch: 1,
            effect_journal: new_journal(),
        }
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn events(&self) -> &[Event] {
        &self.state.events
    }

    pub fn state_digest(&self) -> u64 {
        state_digest(&self.state)
    }

    pub fn content_digest(&self) -> agel_integrity::Digest {
        state_content_digest(&self.state)
    }

    pub fn world_id(&self) -> u64 {
        self.world_id
    }

    pub fn authority_epoch(&self) -> u64 {
        self.authority_epoch
    }

    pub fn effect_journal(&self) -> EffectJournal {
        lock_journal(&self.effect_journal).clone()
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            state: self.state.clone(),
            revision: self.revision,
            next_revision: self.next_revision,
            next_capability_id: self.next_capability_id,
            digest: self.state_digest(),
            world_id: self.world_id,
            authority_epoch: self.authority_epoch,
            effect_journal: self.effect_journal.clone(),
        }
    }

    pub fn from_snapshot(snapshot: &Snapshot) -> Result<Self, ReplayError> {
        if snapshot.digest != state_digest(&snapshot.state) {
            return Err(ReplayError::InvalidSnapshot);
        }
        Ok(Self {
            state: snapshot.state.clone(),
            revision: snapshot.revision,
            next_revision: snapshot.next_revision,
            next_capability_id: snapshot.next_capability_id,
            history: VecDeque::new(),
            history_limit: DEFAULT_HISTORY_LIMIT,
            world_id: snapshot.world_id,
            authority_epoch: snapshot.authority_epoch,
            effect_journal: snapshot.effect_journal.clone(),
        })
    }

    pub fn restore_snapshot(&mut self, snapshot: &Snapshot) -> Result<u64, ReplayError> {
        if snapshot.digest != state_digest(&snapshot.state) {
            return Err(ReplayError::InvalidSnapshot);
        }
        let revision = self.next_revision;
        let following_revision =
            self.next_revision
                .checked_add(1)
                .ok_or(ReplayError::Transaction(
                    TransactionError::RevisionExhausted,
                ))?;
        if self.history_limit > 0 {
            if self.history.len() == self.history_limit {
                self.history.pop_front();
            }
            self.history.push_back((self.revision, self.state.clone()));
        }
        self.state = snapshot.state.clone();
        self.revision = revision;
        self.next_revision = following_revision;
        self.next_capability_id = self.next_capability_id.max(snapshot.next_capability_id);
        self.authority_epoch =
            self.authority_epoch
                .checked_add(1)
                .ok_or(ReplayError::Transaction(
                    TransactionError::RevisionExhausted,
                ))?;
        Ok(revision)
    }

    pub fn replay(
        snapshot: &Snapshot,
        transactions: &[String],
        options: &EvaluationOptions,
    ) -> Result<ReplayReport, ReplayError> {
        let mut world = Self::from_snapshot(snapshot)?;
        let initial_events = world.events().len();
        let mut values = Vec::with_capacity(transactions.len());
        let mut steps_used = 0_u64;
        for source in transactions {
            let commit = world
                .evaluate_with(source, options)
                .map_err(ReplayError::Transaction)?;
            steps_used = steps_used
                .checked_add(commit.steps_used)
                .ok_or(ReplayError::StepCountOverflow)?;
            values.push(commit.values);
        }
        Ok(ReplayReport {
            final_revision: world.revision(),
            final_digest: world.state_digest(),
            events: world.events()[initial_events..].to_vec(),
            values,
            steps_used,
        })
    }

    pub fn replay_inputs(
        snapshot: &Snapshot,
        inputs: &[ReplayInput],
        options: &EvaluationOptions,
    ) -> Result<ReplayReport, ReplayError> {
        let mut world = Self::from_snapshot(snapshot)?;
        world.effect_journal = new_journal();
        let initial_events = world.events().len();
        let mut values = Vec::with_capacity(inputs.len());
        let mut steps_used = 0_u64;
        for input in inputs {
            let commit = match input {
                ReplayInput::Evaluate(source) => world
                    .evaluate_with(source, options)
                    .map_err(ReplayError::Transaction)?,
                ReplayInput::ClaimModel(id) => world
                    .claim_model_request(*id, options)
                    .map(|(commit, _)| commit)
                    .map_err(|error| match error {
                        ModelDispatchError::Transaction(error) => ReplayError::Transaction(error),
                        ModelDispatchError::UnknownRequest(id) => model_replay_error(
                            "model/unknown-request",
                            format!("unknown model request: {id}"),
                            id,
                        ),
                        ModelDispatchError::NotPending(id) => model_replay_error(
                            "model/not-pending",
                            format!("model request is not pending: {id}"),
                            id,
                        ),
                        ModelDispatchError::AlreadyClaimed(key) => model_replay_error(
                            "model/already-claimed",
                            format!("external effect was already claimed: {key}"),
                            *id,
                        ),
                    })?,
                ReplayInput::CompleteModel(completion) => world
                    .complete_model_request(completion.clone(), options)
                    .map_err(|error| match error {
                        ModelCompletionError::Transaction(error) => ReplayError::Transaction(error),
                        ModelCompletionError::UnknownRequest(id) => {
                            ReplayError::Transaction(TransactionError::Eval(crate::EvalError {
                                condition: Box::new(crate::Condition {
                                    kind: "model/unknown-request".into(),
                                    message: format!("unknown model request: {id}"),
                                    data: Value::Int(i64::try_from(id).unwrap_or(i64::MAX)),
                                }),
                            }))
                        }
                        ModelCompletionError::AlreadyCompleted(id) => {
                            ReplayError::Transaction(TransactionError::Eval(crate::EvalError {
                                condition: Box::new(crate::Condition {
                                    kind: "model/already-completed".into(),
                                    message: format!("model request already completed: {id}"),
                                    data: Value::Int(i64::try_from(id).unwrap_or(i64::MAX)),
                                }),
                            }))
                        }
                        ModelCompletionError::MismatchedEffect(id) => model_replay_error(
                            "model/mismatched-effect",
                            format!("completion does not match model request: {id}"),
                            id,
                        ),
                    })?,
            };
            steps_used = steps_used
                .checked_add(commit.steps_used)
                .ok_or(ReplayError::StepCountOverflow)?;
            values.push(commit.values);
        }
        Ok(ReplayReport {
            final_revision: world.revision(),
            final_digest: world.state_digest(),
            events: world.events()[initial_events..].to_vec(),
            values,
            steps_used,
        })
    }

    /// Bind every word of `host` in the global environment, as
    /// `Builtin::Host(index)`. The same table must be in the options of the
    /// evaluations that apply them. Meant for a world's setup, before its
    /// first transaction: the bindings are state like the seed builtins,
    /// not a revision.
    pub fn install_host(&mut self, host: &'static [HostWord]) {
        for (index, word) in host.iter().enumerate() {
            let index = u16::try_from(index).expect("a host table holds fewer than 65,536 words");
            self.state
                .bindings
                .insert(word.name.into(), Value::Builtin(Builtin::Host(index)));
        }
    }

    pub fn issue_capability(
        &mut self,
        kind: impl Into<String>,
        scope: impl Into<String>,
    ) -> Result<Capability, AuthorityError> {
        let id = self.next_capability_id;
        self.next_capability_id = self
            .next_capability_id
            .checked_add(1)
            .ok_or(AuthorityError)?;
        Ok(Capability::new(
            id,
            kind.into(),
            scope.into(),
            self.world_id,
            self.authority_epoch,
        ))
    }

    pub fn evaluate(&mut self, source: &str) -> Result<Commit, TransactionError> {
        self.evaluate_with(source, &EvaluationOptions::default())
    }

    pub fn evaluate_with(
        &mut self,
        source: &str,
        options: &EvaluationOptions,
    ) -> Result<Commit, TransactionError> {
        let expressions = read_all_with_limits(
            source,
            ReadLimits {
                max_source_bytes: options.budget.max_source_bytes,
                max_depth: options.budget.max_parse_depth,
            },
        )?;
        if expressions.is_empty() {
            return Ok(Commit {
                revision: self.revision,
                values: Vec::new(),
                steps_used: 0,
            });
        }

        let revision = self.next_revision;
        let following_revision = self
            .next_revision
            .checked_add(1)
            .ok_or(TransactionError::RevisionExhausted)?;
        let mut candidate = self.state.clone();
        let (values, steps_used) = eval_all(
            &expressions,
            &mut candidate,
            options,
            self.world_id,
            self.authority_epoch,
        )?;

        if self.history_limit > 0 {
            if self.history.len() == self.history_limit {
                self.history.pop_front();
            }
            self.history.push_back((self.revision, self.state.clone()));
        }
        self.state = candidate;
        self.revision = revision;
        self.next_revision = following_revision;
        Ok(Commit {
            revision: self.revision,
            values,
            steps_used,
        })
    }

    pub fn pending_model_requests(&self) -> Vec<ModelRequest> {
        self.state
            .model_requests
            .values()
            .filter(|record| matches!(record.status, ModelRequestStatus::Pending))
            .map(|record| record.request.clone())
            .collect()
    }

    pub fn dispatching_model_requests(&self) -> Vec<ModelRequest> {
        self.state
            .model_requests
            .values()
            .filter(|record| matches!(record.status, ModelRequestStatus::Dispatching))
            .map(|record| record.request.clone())
            .collect()
    }

    pub fn claim_model_request(
        &mut self,
        request_id: u64,
        options: &EvaluationOptions,
    ) -> Result<(Commit, ModelRequest), ModelDispatchError> {
        let record = self
            .state
            .model_requests
            .get(&request_id)
            .ok_or(ModelDispatchError::UnknownRequest(request_id))?;
        if !matches!(record.status, ModelRequestStatus::Pending) {
            return Err(ModelDispatchError::NotPending(request_id));
        }
        let request = record.request.clone();
        {
            let mut journal = lock_journal(&self.effect_journal);
            if journal.entries.contains_key(&request.effect_key) {
                return Err(ModelDispatchError::AlreadyClaimed(request.effect_key));
            }
            journal.entries.insert(
                request.effect_key,
                EffectJournalEntry {
                    request: request.clone(),
                    status: EffectJournalStatus::Claimed,
                },
            );
        }
        let revision = self.next_revision;
        let following_revision =
            self.next_revision
                .checked_add(1)
                .ok_or(ModelDispatchError::Transaction(
                    TransactionError::RevisionExhausted,
                ))?;
        let mut candidate = self.state.clone();
        let steps_used = crate::eval::claim_model_request(
            &mut candidate,
            request_id,
            options,
            self.world_id,
            self.authority_epoch,
        )
        .map_err(|error| ModelDispatchError::Transaction(TransactionError::Eval(error)))?;
        if self.history_limit > 0 {
            if self.history.len() == self.history_limit {
                self.history.pop_front();
            }
            self.history.push_back((self.revision, self.state.clone()));
        }
        self.state = candidate;
        self.revision = revision;
        self.next_revision = following_revision;
        Ok((
            Commit {
                revision,
                values: Vec::new(),
                steps_used,
            },
            request,
        ))
    }

    pub fn complete_model_request(
        &mut self,
        completion: ModelCompletion,
        options: &EvaluationOptions,
    ) -> Result<Commit, ModelCompletionError> {
        let status = self
            .state
            .model_requests
            .get(&completion.request_id)
            .map(|record| &record.status)
            .ok_or(ModelCompletionError::UnknownRequest(completion.request_id))?;
        let request = &self
            .state
            .model_requests
            .get(&completion.request_id)
            .expect("request was just found")
            .request;
        if completion.effect_key != request.effect_key {
            return Err(ModelCompletionError::MismatchedEffect(
                completion.request_id,
            ));
        }
        if matches!(status, ModelRequestStatus::Completed(_)) {
            return Err(ModelCompletionError::AlreadyCompleted(
                completion.request_id,
            ));
        }
        {
            let mut journal = lock_journal(&self.effect_journal);
            let Some(entry) = journal.entries.get_mut(&completion.effect_key) else {
                return Err(ModelCompletionError::MismatchedEffect(
                    completion.request_id,
                ));
            };
            entry.status = EffectJournalStatus::Completed(completion.outcome.clone());
        }
        let revision = self.next_revision;
        let following_revision =
            self.next_revision
                .checked_add(1)
                .ok_or(ModelCompletionError::Transaction(
                    TransactionError::RevisionExhausted,
                ))?;
        let mut candidate = self.state.clone();
        let steps_used = crate::eval::complete_model_request(
            &mut candidate,
            &completion,
            options,
            self.world_id,
            self.authority_epoch,
        )
        .map_err(|error| ModelCompletionError::Transaction(TransactionError::Eval(error)))?;
        if self.history_limit > 0 {
            if self.history.len() == self.history_limit {
                self.history.pop_front();
            }
            self.history.push_back((self.revision, self.state.clone()));
        }
        self.state = candidate;
        self.revision = revision;
        self.next_revision = following_revision;
        Ok(Commit {
            revision,
            values: Vec::new(),
            steps_used,
        })
    }

    pub fn rollback(&mut self) -> Option<u64> {
        let (revision, state) = self.history.pop_back()?;
        self.revision = revision;
        self.state = state;
        Some(revision)
    }

    pub fn binding(&self, name: &str) -> Option<&Value> {
        self.state.bindings.get(name)
    }

    pub fn agent_name(&self, id: u64) -> Option<&str> {
        self.state.agents.get(&id).map(|agent| agent.name.as_str())
    }

    pub fn fork_isolated(&self) -> Self {
        Self {
            state: self.state.clone(),
            revision: self.revision,
            next_revision: self.next_revision,
            next_capability_id: 1,
            history: VecDeque::new(),
            history_limit: self.history_limit,
            world_id: next_world_id(),
            authority_epoch: 1,
            effect_journal: new_journal(),
        }
    }
}

impl Canon for Module {
    fn canon(&self, out: &mut Encoder) {
        out.tag("module");
        out.entries(self.bindings.iter());
        out.entries(self.macros.iter());
        out.seq(self.exports.len());
        for name in &self.exports {
            out.text(name);
        }
    }

    fn decode(input: &mut Decoder<'_>) -> Result<Self, CanonError> {
        input.expect("module")?;
        let bindings = input.entries()?;
        let macros = input.entries()?;
        let count = input.seq()?;
        let mut exports = BTreeSet::new();
        for _ in 0..count {
            exports.insert(input.text()?);
        }
        Ok(Self {
            bindings,
            macros,
            exports,
        })
    }
}

impl Canon for State {
    fn canon(&self, out: &mut Encoder) {
        out.tag("state");
        out.entries(self.bindings.iter());
        out.entries(self.macros.iter());
        out.entries(self.modules.iter());
        self.canon_tail(out);
    }

    fn decode(input: &mut Decoder<'_>) -> Result<Self, CanonError> {
        input.expect("state")?;
        let mut state = Self {
            bindings: input.entries()?,
            macros: input.entries()?,
            modules: input.entries()?,
            ..Self::default()
        };
        state.decode_tail(input)?;
        Ok(state)
    }
}

impl State {
    /// Everything after the three tables, whole in either form.
    fn canon_tail(&self, out: &mut Encoder) {
        out.seq(self.agents.len());
        for (id, agent) in &self.agents {
            out.u64(*id);
            agent.canon(out);
        }
        out.items(self.ready_queue.iter());
        out.items(self.events.iter());
        out.u64(self.next_event_sequence);
        out.u64(self.next_agent_id);
        out.u64(self.next_syntax_id);
        out.seq(self.model_requests.len());
        for (id, record) in &self.model_requests {
            out.u64(*id);
            record.canon(out);
        }
        out.u64(self.next_model_request_id);
    }

    fn decode_tail(&mut self, input: &mut Decoder<'_>) -> Result<(), CanonError> {
        let count = input.seq()?;
        self.agents = BTreeMap::new();
        for _ in 0..count {
            let id = input.u64()?;
            self.agents.insert(id, Agent::decode(input)?);
        }
        self.ready_queue = VecDeque::from(input.items::<u64>()?);
        self.events = input.items()?;
        self.next_event_sequence = input.u64()?;
        self.next_agent_id = input.u64()?;
        self.next_syntax_id = input.u64()?;
        let count = input.seq()?;
        self.model_requests = BTreeMap::new();
        for _ in 0..count {
            let id = input.u64()?;
            self.model_requests.insert(id, ModelRecord::decode(input)?);
        }
        self.next_model_request_id = input.u64()?;
        Ok(())
    }

    /// The state as a delta over `base`: of the three tables only the
    /// entries `base` lacks or holds differently, the rest whole. What a
    /// session added to a freshly built world, small where the whole is
    /// not; an entry removed from `base` is not expressible and stays.
    fn canon_over(&self, base: &Self, out: &mut Encoder) {
        out.tag("delta");
        let bindings: Vec<_> = self
            .bindings
            .iter()
            .filter(|(name, value)| base.bindings.get(*name) != Some(value))
            .collect();
        out.entries(bindings.into_iter());
        let macros: Vec<_> = self
            .macros
            .iter()
            .filter(|(name, value)| base.macros.get(*name) != Some(value))
            .collect();
        out.entries(macros.into_iter());
        let modules: Vec<_> = self
            .modules
            .iter()
            .filter(|(name, value)| base.modules.get(*name) != Some(value))
            .collect();
        out.entries(modules.into_iter());
        self.canon_tail(out);
    }

    /// `base` with a delta applied.
    fn decode_over(mut base: Self, input: &mut Decoder<'_>) -> Result<Self, CanonError> {
        input.expect("delta")?;
        base.bindings.extend(input.entries::<Value>()?);
        base.macros.extend(input.entries::<MacroDef>()?);
        base.modules.extend(input.entries::<Module>()?);
        base.decode_tail(input)?;
        Ok(base)
    }
}

/// The version a world file's header names: bumped with the encoding.
const WORLD_FILE_VERSION: u64 = 2;

impl World {
    fn canonical_header(&self, out: &mut Encoder) {
        out.tag("agel-world");
        out.u64(WORLD_FILE_VERSION);
        out.u64(self.revision);
        out.u64(self.next_revision);
        out.u64(self.next_capability_id);
        out.u64(self.world_id);
        out.u64(self.authority_epoch);
    }

    fn decode_header(input: &mut Decoder<'_>) -> Result<[u64; 5], CanonError> {
        input.expect("agel-world")?;
        let version = input.u64()?;
        if version != WORLD_FILE_VERSION {
            return input.fail(format!(
                "world file version {version}; this runtime reads {WORLD_FILE_VERSION}"
            ));
        }
        Ok([
            input.u64()?,
            input.u64()?,
            input.u64()?,
            input.u64()?,
            input.u64()?,
        ])
    }

    fn with_header(mut self, header: [u64; 5], state: State) -> Self {
        let [revision, next_revision, next_capability_id, world_id, authority_epoch] = header;
        self.state = state;
        self.revision = revision;
        self.next_revision = next_revision;
        self.next_capability_id = next_capability_id;
        self.world_id = world_id;
        self.authority_epoch = authority_epoch;
        self.history.clear();
        self
    }

    /// The world as a file: a versioned header (revision, capability and
    /// authority counters, identity) and the canonical encoding of the
    /// whole state, which `from_canonical` reads back. The identity is
    /// kept so the capabilities it issued still permit.
    pub fn to_canonical(&self) -> Vec<u8> {
        let mut out = Encoder::new();
        self.canonical_header(&mut out);
        self.state.canon(&mut out);
        out.finish()
    }

    /// A world from `to_canonical`'s bytes: no history, a fresh effect
    /// journal, and a refusal, not a guess, at anything the encoder never
    /// wrote or a version this runtime does not read.
    pub fn from_canonical(bytes: &[u8]) -> Result<Self, CanonError> {
        let mut input = Decoder::new(bytes);
        let header = Self::decode_header(&mut input)?;
        let state = State::decode(&mut input)?;
        if !input.finished() {
            return input.fail("bytes after the state");
        }
        Ok(Self::new(0).with_header(header, state))
    }

    /// The world as a delta over `base`, a world built the same way (the
    /// same host words, the same library): of the bindings, macros and
    /// modules only what differs, the rest whole. Small where the whole
    /// world is not; `from_canonical_over` applies it to such a base.
    pub fn to_canonical_over(&self, base: &Self) -> Vec<u8> {
        let mut out = Encoder::new();
        self.canonical_header(&mut out);
        self.state.canon_over(&base.state, &mut out);
        out.finish()
    }

    /// `base` with a delta from `to_canonical_over` applied, and the
    /// delta's header: the world that was saved, over the library the base
    /// carries.
    pub fn from_canonical_over(base: Self, bytes: &[u8]) -> Result<Self, CanonError> {
        let mut input = Decoder::new(bytes);
        let header = Self::decode_header(&mut input)?;
        let state = State::decode_over(base.state.clone(), &mut input)?;
        if !input.finished() {
            return input.fail("bytes after the state");
        }
        Ok(base.with_header(header, state))
    }
}

/// The state's canonical encoding (`canon.rs`): the same bytes for the
/// same state on any build.
fn canonical_bytes(state: &State) -> Vec<u8> {
    let mut encoder = Encoder::new();
    state.canon(&mut encoder);
    encoder.finish()
}

/// A replay checksum over the canonical encoding: not a cryptographic
/// proof, and the same state on every build.
fn state_digest(state: &State) -> u64 {
    canonical_bytes(state)
        .iter()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
        })
}

/// The digest that binds evidence to a world state: SHA-256 over the
/// canonical encoding, versioned by its prefix.
fn state_content_digest(state: &State) -> agel_integrity::Digest {
    let mut bytes = b"agel-world-canonical-v2\0".to_vec();
    bytes.extend_from_slice(&canonical_bytes(state));
    agel_integrity::sha256(&bytes)
}

fn next_world_id() -> u64 {
    NEXT_WORLD_ID.fetch_add(1, Ordering::Relaxed)
}

#[cfg(feature = "std")]
fn new_journal() -> Journal {
    Arc::new(Mutex::new(EffectJournal::default()))
}

#[cfg(not(feature = "std"))]
fn new_journal() -> Journal {
    Rc::new(RefCell::new(EffectJournal::default()))
}

#[cfg(feature = "std")]
fn lock_journal(journal: &Journal) -> MutexGuard<'_, EffectJournal> {
    journal
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(not(feature = "std"))]
fn lock_journal(journal: &Journal) -> RefMut<'_, EffectJournal> {
    journal.borrow_mut()
}

fn model_replay_error(kind: &str, message: String, id: u64) -> ReplayError {
    ReplayError::Transaction(TransactionError::Eval(crate::EvalError {
        condition: Box::new(crate::Condition {
            kind: kind.into(),
            message,
            data: Value::Int(i64::try_from(id).unwrap_or(i64::MAX)),
        }),
    }))
}
