//! Modality-neutral human interaction with a fast foreground lane and a
//! separately scheduled agent-work lane.
//!
//! This crate deliberately performs no speech recognition and invokes no
//! model. Adapters turn text or voice into [`Input`]; policy remains here.

use std::collections::{BTreeSet, VecDeque};
use std::fmt;
use std::sync::Arc;

/// The response budget exposed to an interactive adapter.
pub const FOREGROUND_DEADLINE_MS: u16 = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Modality {
    Text,
    VoiceTranscript,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Intent {
    Observe,
    Propose,
    Authorize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Input {
    pub modality: Modality,
    pub intent: Intent,
    pub content: String,
}

impl Input {
    pub fn text(content: impl Into<String>, intent: Intent) -> Self {
        Self {
            modality: Modality::Text,
            intent,
            content: content.into(),
        }
    }

    pub fn voice(content: impl Into<String>, intent: Intent) -> Self {
        Self {
            modality: Modality::VoiceTranscript,
            intent,
            content: content.into(),
        }
    }
}

/// Host-owned mint for presence proofs accepted by one or more hubs.
#[derive(Clone, Debug)]
pub struct PresenceAuthority(Arc<()>);

impl PresenceAuthority {
    pub fn new() -> Self {
        Self(Arc::new(()))
    }

    /// Called only after an adapter independently authenticates the human.
    pub fn attest(&self) -> PresenceProof {
        PresenceProof(self.0.clone())
    }
}

impl Default for PresenceAuthority {
    fn default() -> Self {
        Self::new()
    }
}

/// An opaque process-local proof; language/model text cannot manufacture one.
#[derive(Debug)]
pub struct PresenceProof(Arc<()>);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ForegroundEvent {
    Acknowledged { id: u64, deadline_ms: u16 },
    Completed { id: u64, response: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackgroundTask {
    pub id: u64,
    pub input: Input,
}

/// A bounded handoff between responsive interaction and potentially slow work.
///
/// Submission is atomic: either both queues receive their item or neither does.
#[derive(Debug)]
pub struct InteractionHub {
    next_id: u64,
    capacity: usize,
    presence_seal: Arc<()>,
    foreground: VecDeque<ForegroundEvent>,
    background: VecDeque<BackgroundTask>,
    outstanding: BTreeSet<u64>,
}

impl InteractionHub {
    pub fn new(
        capacity: usize,
        presence_authority: &PresenceAuthority,
    ) -> Result<Self, InteractionError> {
        if capacity == 0 {
            return Err(InteractionError::ZeroCapacity);
        }
        Ok(Self {
            next_id: 1,
            capacity,
            presence_seal: presence_authority.0.clone(),
            foreground: VecDeque::new(),
            background: VecDeque::new(),
            outstanding: BTreeSet::new(),
        })
    }

    pub fn submit(&mut self, input: Input) -> Result<u64, InteractionError> {
        self.submit_checked(input, None)
    }

    pub fn submit_authorized(
        &mut self,
        input: Input,
        proof: PresenceProof,
    ) -> Result<u64, InteractionError> {
        self.submit_checked(input, Some(&proof))
    }

    fn submit_checked(
        &mut self,
        input: Input,
        proof: Option<&PresenceProof>,
    ) -> Result<u64, InteractionError> {
        if input.content.trim().is_empty() {
            return Err(InteractionError::EmptyInput);
        }
        if input.intent == Intent::Authorize
            && !proof.is_some_and(|proof| Arc::ptr_eq(&proof.0, &self.presence_seal))
        {
            return Err(InteractionError::UnverifiedAuthorization);
        }
        if self.foreground.len() == self.capacity
            || self.background.len() == self.capacity
            || self.outstanding.len() == self.capacity
        {
            return Err(InteractionError::Backpressure);
        }
        let id = self.next_id;
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or(InteractionError::IdentifierExhausted)?;
        self.foreground.push_back(ForegroundEvent::Acknowledged {
            id,
            deadline_ms: FOREGROUND_DEADLINE_MS,
        });
        self.background.push_back(BackgroundTask { id, input });
        self.outstanding.insert(id);
        Ok(id)
    }

    pub fn complete(
        &mut self,
        id: u64,
        response: impl Into<String>,
    ) -> Result<(), InteractionError> {
        if !self.outstanding.contains(&id) {
            return Err(InteractionError::UnknownTask);
        }
        if self.foreground.len() == self.capacity {
            return Err(InteractionError::Backpressure);
        }
        self.outstanding.remove(&id);
        self.foreground.push_back(ForegroundEvent::Completed {
            id,
            response: response.into(),
        });
        Ok(())
    }

    pub fn next_foreground(&mut self) -> Option<ForegroundEvent> {
        self.foreground.pop_front()
    }

    pub fn next_background(&mut self) -> Option<BackgroundTask> {
        self.background.pop_front()
    }

    pub fn pending(&self) -> (usize, usize) {
        (self.foreground.len(), self.background.len())
    }

    pub fn outstanding(&self) -> usize {
        self.outstanding.len()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InteractionError {
    ZeroCapacity,
    EmptyInput,
    UnverifiedAuthorization,
    Backpressure,
    IdentifierExhausted,
    UnknownTask,
}

impl fmt::Display for InteractionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroCapacity => f.write_str("interaction capacity must be non-zero"),
            Self::EmptyInput => f.write_str("interaction input must not be empty"),
            Self::UnverifiedAuthorization => {
                f.write_str("authorization requires verified human presence")
            }
            Self::Backpressure => f.write_str("interaction lane is at capacity"),
            Self::IdentifierExhausted => f.write_str("interaction identifier space exhausted"),
            Self::UnknownTask => f.write_str("interaction task is unknown or already completed"),
        }
    }
}

impl std::error::Error for InteractionError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acknowledges_before_background_work() {
        let authority = PresenceAuthority::new();
        let mut hub = InteractionHub::new(2, &authority).unwrap();
        let id = hub
            .submit(Input::voice("explain the active world", Intent::Observe))
            .unwrap();
        assert_eq!(
            hub.next_foreground(),
            Some(ForegroundEvent::Acknowledged {
                id,
                deadline_ms: 200
            })
        );
        assert_eq!(hub.next_background().unwrap().id, id);
    }

    #[test]
    fn authority_requires_verified_presence_for_every_modality() {
        let authority = PresenceAuthority::new();
        let impostor = PresenceAuthority::new();
        let mut hub = InteractionHub::new(2, &authority).unwrap();
        assert_eq!(
            hub.submit(Input::voice("promote B", Intent::Authorize)),
            Err(InteractionError::UnverifiedAuthorization)
        );
        assert_eq!(
            hub.submit_authorized(
                Input::text("promote B", Intent::Authorize),
                impostor.attest()
            ),
            Err(InteractionError::UnverifiedAuthorization)
        );
        assert!(hub
            .submit_authorized(
                Input::text("promote B", Intent::Authorize),
                authority.attest()
            )
            .is_ok());
    }

    #[test]
    fn submission_is_atomic_under_backpressure() {
        let authority = PresenceAuthority::new();
        let mut hub = InteractionHub::new(1, &authority).unwrap();
        hub.submit(Input::text("one", Intent::Propose)).unwrap();
        assert_eq!(
            hub.submit(Input::text("two", Intent::Propose)),
            Err(InteractionError::Backpressure)
        );
        assert_eq!(hub.pending(), (1, 1));
        hub.next_foreground();
        hub.next_background();
        assert_eq!(
            hub.submit(Input::text("still bounded", Intent::Propose)),
            Err(InteractionError::Backpressure)
        );
    }

    #[test]
    fn completion_returns_to_foreground_lane() {
        let authority = PresenceAuthority::new();
        let mut hub = InteractionHub::new(2, &authority).unwrap();
        let id = hub.submit(Input::text("status", Intent::Observe)).unwrap();
        hub.next_foreground();
        hub.next_background();
        hub.complete(id, "world A is healthy").unwrap();
        assert_eq!(
            hub.next_foreground(),
            Some(ForegroundEvent::Completed {
                id,
                response: "world A is healthy".into()
            })
        );
        assert_eq!(
            hub.complete(id, "forged duplicate"),
            Err(InteractionError::UnknownTask)
        );
        assert_eq!(
            hub.complete(999, "forged result"),
            Err(InteractionError::UnknownTask)
        );
    }
}
