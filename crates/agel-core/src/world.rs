use crate::agent::{Agent, Event};
use crate::eval::{eval_all, EvalError};
use crate::macro_expander::MacroDef;
use crate::model::{
    EffectJournal, EffectJournalEntry, EffectJournalStatus, ModelCompletion, ModelCompletionError,
    ModelDispatchError, ModelRecord, ModelRequest, ModelRequestStatus,
};
use crate::reader::{read_all_with_limits, ReadError, ReadLimits};
use crate::value::Builtin;
use crate::{Capability, Value};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EvaluationOptions {
    pub budget: Budget,
    pub capabilities: Vec<Capability>,
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

impl Default for State {
    fn default() -> Self {
        let mut bindings = BTreeMap::new();
        for (name, builtin) in [
            ("+", Builtin::Add),
            ("-", Builtin::Subtract),
            ("*", Builtin::Multiply),
            ("/", Builtin::Divide),
            ("=", Builtin::Equal),
            ("list", Builtin::List),
            ("cons", Builtin::Cons),
            ("car", Builtin::Car),
            ("cdr", Builtin::Cdr),
            ("dict", Builtin::Dict),
            ("get", Builtin::Get),
            ("assoc", Builtin::Assoc),
            ("dissoc", Builtin::Dissoc),
            ("keys", Builtin::Keys),
            ("count", Builtin::Count),
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
        ] {
            bindings.insert(name.into(), Value::Builtin(builtin));
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
    effect_journal: Arc<Mutex<EffectJournal>>,
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
    effect_journal: Arc<Mutex<EffectJournal>>,
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
            effect_journal: Arc::new(Mutex::new(EffectJournal::default())),
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
            effect_journal: Arc::clone(&self.effect_journal),
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
            effect_journal: Arc::clone(&snapshot.effect_journal),
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
        world.effect_journal = Arc::new(Mutex::new(EffectJournal::default()));
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
            effect_journal: Arc::new(Mutex::new(EffectJournal::default())),
        }
    }
}

fn state_digest(state: &State) -> u64 {
    // Stable for a given Agel runtime version because all unordered state uses
    // BTree collections. This is a replay checksum, not a cryptographic proof.
    format!("{state:?}")
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
        })
}

fn state_content_digest(state: &State) -> agel_integrity::Digest {
    let mut bytes = b"agel-world-debug-v1\0".to_vec();
    bytes.extend_from_slice(format!("{state:?}").as_bytes());
    agel_integrity::sha256(&bytes)
}

fn next_world_id() -> u64 {
    NEXT_WORLD_ID.fetch_add(1, Ordering::Relaxed)
}

fn lock_journal(journal: &Arc<Mutex<EffectJournal>>) -> MutexGuard<'_, EffectJournal> {
    journal
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
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
