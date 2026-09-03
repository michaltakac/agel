use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelRequest {
    pub id: u64,
    pub requester: u64,
    pub reply_to: u64,
    pub provider: String,
    pub prompt: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModelOutcome {
    Success(String),
    Failure { kind: String, message: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelCompletion {
    pub request_id: u64,
    pub outcome: ModelOutcome,
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
    Transaction(crate::TransactionError),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModelDispatchError {
    UnknownRequest(u64),
    NotPending(u64),
    Transaction(crate::TransactionError),
}

impl fmt::Display for ModelDispatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownRequest(id) => write!(f, "unknown model request: {id}"),
            Self::NotPending(id) => write!(f, "model request is not pending: {id}"),
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
            Self::Transaction(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for ModelCompletionError {}
