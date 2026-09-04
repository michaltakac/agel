//! A/B semantic-image staging outside the self-modifying Agel world.

use agel_core::{EvaluationOptions, Value};
use agel_image::Image;
use agel_integrity::Digest;
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HealthCheck {
    pub source: String,
    pub expected: Value,
}

impl HealthCheck {
    pub fn new(source: impl Into<String>, expected: Value) -> Self {
        Self {
            source: source.into(),
            expected,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PromotionEvidence {
    active_digest: Digest,
    candidate_digest: Digest,
    checks_passed: usize,
}

impl PromotionEvidence {
    pub fn active_digest(&self) -> Digest {
        self.active_digest
    }

    pub fn candidate_digest(&self) -> Digest {
        self.candidate_digest
    }

    pub fn checks_passed(&self) -> usize {
        self.checks_passed
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    A,
    B,
}

impl Slot {
    fn other(self) -> Self {
        match self {
            Self::A => Self::B,
            Self::B => Self::A,
        }
    }
}

#[derive(Clone, Debug)]
struct Staged {
    image: Image,
    evidence: PromotionEvidence,
}

#[derive(Clone, Debug)]
pub struct AbSupervisor {
    active_slot: Slot,
    active: Image,
    previous: Option<(Slot, Image)>,
    staged: Option<Staged>,
}

impl AbSupervisor {
    pub fn new(active: Image) -> Self {
        Self {
            active_slot: Slot::A,
            active,
            previous: None,
            staged: None,
        }
    }

    pub fn active_slot(&self) -> Slot {
        self.active_slot
    }

    pub fn active(&self) -> &Image {
        &self.active
    }

    pub fn staged_digest(&self) -> Option<Digest> {
        self.staged.as_ref().map(|staged| staged.image.digest())
    }

    pub fn stage(
        &mut self,
        candidate: Image,
        checks: &[HealthCheck],
    ) -> Result<PromotionEvidence, SupervisorError> {
        if checks.is_empty() {
            return Err(SupervisorError::NoHealthChecks);
        }
        if !candidate.extends(&self.active) {
            return Err(SupervisorError::DivergentCandidate);
        }
        let rebuilt = candidate
            .rebuild()
            .map_err(|error| SupervisorError::InvalidImage(error.to_string()))?;
        let options = EvaluationOptions {
            budget: candidate.budget().clone(),
            capabilities: Vec::new(),
        };
        for check in checks {
            let mut canary = rebuilt.world().fork_isolated();
            let values = canary
                .evaluate_with(&check.source, &options)
                .map_err(|error| SupervisorError::Canary {
                    source: check.source.clone(),
                    message: error.to_string(),
                })?
                .values;
            let actual =
                values
                    .into_iter()
                    .last()
                    .ok_or_else(|| SupervisorError::EmptyHealthCheck {
                        source: check.source.clone(),
                    })?;
            if actual != check.expected {
                return Err(SupervisorError::HealthCheck {
                    source: check.source.clone(),
                    expected: Box::new(check.expected.clone()),
                    actual: Box::new(actual),
                });
            }
        }
        let evidence = PromotionEvidence {
            active_digest: self.active.digest(),
            candidate_digest: candidate.digest(),
            checks_passed: checks.len(),
        };
        self.staged = Some(Staged {
            image: candidate,
            evidence: evidence.clone(),
        });
        Ok(evidence)
    }

    pub fn promote(&mut self, evidence: &PromotionEvidence) -> Result<Slot, SupervisorError> {
        let staged = self.staged.as_ref().ok_or(SupervisorError::NothingStaged)?;
        if &staged.evidence != evidence
            || evidence.active_digest != self.active.digest()
            || evidence.candidate_digest != staged.image.digest()
        {
            return Err(SupervisorError::EvidenceMismatch);
        }
        let staged = self.staged.take().expect("staged image was checked");
        let next_slot = self.active_slot.other();
        self.previous = Some((self.active_slot, self.active.clone()));
        self.active_slot = next_slot;
        self.active = staged.image;
        Ok(next_slot)
    }

    pub fn rollback(&mut self) -> Result<Slot, SupervisorError> {
        let (slot, image) = self
            .previous
            .take()
            .ok_or(SupervisorError::NoPreviousImage)?;
        let replaced = std::mem::replace(&mut self.active, image);
        let replaced_slot = std::mem::replace(&mut self.active_slot, slot);
        self.previous = Some((replaced_slot, replaced));
        self.staged = None;
        Ok(slot)
    }

    pub fn discard_staged(&mut self) {
        self.staged = None;
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SupervisorError {
    NoHealthChecks,
    DivergentCandidate,
    InvalidImage(String),
    Canary {
        source: String,
        message: String,
    },
    HealthCheck {
        source: String,
        expected: Box<Value>,
        actual: Box<Value>,
    },
    EmptyHealthCheck {
        source: String,
    },
    NothingStaged,
    EvidenceMismatch,
    NoPreviousImage,
}

impl fmt::Display for SupervisorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoHealthChecks => f.write_str("at least one health check is required"),
            Self::DivergentCandidate => {
                f.write_str("candidate is not an extension of the active image")
            }
            Self::InvalidImage(message) => write!(f, "candidate image is invalid: {message}"),
            Self::Canary { source, message } => {
                write!(f, "health check {source:?} failed to run: {message}")
            }
            Self::HealthCheck {
                source,
                expected,
                actual,
            } => write!(
                f,
                "health check {source:?} expected {expected}, got {actual}"
            ),
            Self::EmptyHealthCheck { source } => {
                write!(f, "health check {source:?} produced no value")
            }
            Self::NothingStaged => f.write_str("no candidate image is staged"),
            Self::EvidenceMismatch => {
                f.write_str("promotion evidence does not match current slots")
            }
            Self::NoPreviousImage => f.write_str("no previous image is available"),
        }
    }
}

impl std::error::Error for SupervisorError {}

#[cfg(test)]
mod tests {
    use super::*;
    use agel_core::Budget;
    use agel_image::ImageSession;

    fn active_and_candidate() -> (Image, Image) {
        let mut active = ImageSession::new(8, Budget::default());
        active.evaluate("(def transform (fn (x) (+ x 1)))").unwrap();
        let active_image = active.image().clone();
        let mut candidate = active_image.rebuild().unwrap();
        candidate
            .evaluate("(def transform (fn (x) (+ x 2)))")
            .unwrap();
        (active_image, candidate.image().clone())
    }

    #[test]
    fn canary_promote_and_rollback_switch_whole_images() {
        let (active, candidate) = active_and_candidate();
        let mut supervisor = AbSupervisor::new(active.clone());
        let evidence = supervisor
            .stage(
                candidate,
                &[
                    HealthCheck::new("(transform 40)", Value::Int(42)),
                    HealthCheck::new("(transform -2)", Value::Int(0)),
                ],
            )
            .unwrap();
        assert_eq!(evidence.checks_passed(), 2);
        assert_eq!(supervisor.promote(&evidence).unwrap(), Slot::B);
        let mut promoted = supervisor.active().rebuild().unwrap();
        assert_eq!(
            promoted.evaluate("(transform 40)").unwrap().values[0],
            Value::Int(42)
        );
        assert_eq!(supervisor.rollback().unwrap(), Slot::A);
        assert_eq!(supervisor.active().digest(), active.digest());
    }

    #[test]
    fn bad_canary_divergence_and_stale_evidence_fail_closed() {
        let (active, candidate) = active_and_candidate();
        let mut supervisor = AbSupervisor::new(active.clone());
        assert!(matches!(
            supervisor.stage(
                candidate.clone(),
                &[HealthCheck::new("(transform 40)", Value::Int(41))]
            ),
            Err(SupervisorError::HealthCheck { .. })
        ));
        assert!(matches!(
            supervisor.stage(
                candidate.clone(),
                &[HealthCheck::new("; no result", Value::Nil)]
            ),
            Err(SupervisorError::EmptyHealthCheck { .. })
        ));
        let passing = [HealthCheck::new("(transform 40)", Value::Int(42))];
        let evidence = supervisor.stage(candidate.clone(), &passing).unwrap();
        let mut newer = candidate.rebuild().unwrap();
        newer.evaluate("(def marker 'newer)").unwrap();
        supervisor.stage(newer.image().clone(), &passing).unwrap();
        assert_eq!(
            supervisor.promote(&evidence),
            Err(SupervisorError::EvidenceMismatch)
        );
        let unrelated = ImageSession::new(8, Budget::default());
        assert_eq!(
            supervisor.stage(unrelated.image().clone(), &passing),
            Err(SupervisorError::DivergentCandidate)
        );
    }
}
