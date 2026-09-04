use agel_integrity::Digest;
use std::collections::BTreeMap;
use std::fmt;

pub type EffectKey = Digest;

pub(crate) fn model_effect_key(
    world_id: u64,
    request_id: u64,
    provider: &str,
    prompt_digest: Digest,
) -> EffectKey {
    let mut bytes = b"agel:model-effect:v1".to_vec();
    bytes.extend_from_slice(&world_id.to_be_bytes());
    bytes.extend_from_slice(&request_id.to_be_bytes());
    bytes.extend_from_slice(&(provider.len() as u64).to_be_bytes());
    bytes.extend_from_slice(provider.as_bytes());
    bytes.extend_from_slice(prompt_digest.as_bytes());
    agel_integrity::sha256(&bytes)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelRequest {
    pub id: u64,
    pub requester: u64,
    pub reply_to: u64,
    pub provider: String,
    pub prompt: String,
    pub prompt_digest: Digest,
    pub effect_key: EffectKey,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModelOutcome {
    Success(String),
    Failure { kind: String, message: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelCompletion {
    pub request_id: u64,
    pub effect_key: EffectKey,
    pub outcome: ModelOutcome,
}

impl ModelCompletion {
    pub fn success(request: &ModelRequest, text: impl Into<String>) -> Self {
        Self {
            request_id: request.id,
            effect_key: request.effect_key,
            outcome: ModelOutcome::Success(text.into()),
        }
    }

    pub fn failure(
        request: &ModelRequest,
        kind: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            request_id: request.id,
            effect_key: request.effect_key,
            outcome: ModelOutcome::Failure {
                kind: kind.into(),
                message: message.into(),
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EffectJournalStatus {
    Claimed,
    Completed(ModelOutcome),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EffectJournalEntry {
    pub request: ModelRequest,
    pub status: EffectJournalStatus,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EffectJournal {
    pub(crate) entries: BTreeMap<EffectKey, EffectJournalEntry>,
}

impl EffectJournal {
    pub fn entries(&self) -> &BTreeMap<EffectKey, EffectJournalEntry> {
        &self.entries
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ModelRequestStatus {
    Pending,
    Dispatching,
    Completed(ModelOutcome),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ModelRecord {
    pub request: ModelRequest,
    pub status: ModelRequestStatus,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModelCompletionError {
    UnknownRequest(u64),
    AlreadyCompleted(u64),
    MismatchedEffect(u64),
    Transaction(crate::TransactionError),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModelDispatchError {
    UnknownRequest(u64),
    NotPending(u64),
    AlreadyClaimed(EffectKey),
    Transaction(crate::TransactionError),
}

impl fmt::Display for ModelDispatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownRequest(id) => write!(f, "unknown model request: {id}"),
            Self::NotPending(id) => write!(f, "model request is not pending: {id}"),
            Self::AlreadyClaimed(key) => write!(f, "external effect was already claimed: {key}"),
            Self::Transaction(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for ModelDispatchError {}

impl fmt::Display for ModelCompletionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownRequest(id) => write!(f, "unknown model request: {id}"),
            Self::AlreadyCompleted(id) => write!(f, "model request already completed: {id}"),
            Self::MismatchedEffect(id) => {
                write!(f, "completion does not match model request: {id}")
            }
            Self::Transaction(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for ModelCompletionError {}
