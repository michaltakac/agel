//! A/B semantic-image staging outside the self-modifying Agel world.

use agel_core::{EvaluationOptions, Value};
use agel_image::Image;
use agel_integrity::{Digest, Signature, SigningKey, VerifyingKey};
use std::fmt;

/// Domain separation for evidence signatures.
pub const EVIDENCE_SIGNATURE_DOMAIN: &[u8] = b"agel/promotion-evidence/v1\0";

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

    /// The canonical bytes a signature commits to.
    pub fn message(&self) -> Vec<u8> {
        let mut message = EVIDENCE_SIGNATURE_DOMAIN.to_vec();
        message.extend_from_slice(self.active_digest.as_bytes());
        message.extend_from_slice(self.candidate_digest.as_bytes());
        message.extend_from_slice(&(self.checks_passed as u64).to_be_bytes());
        message
    }

    /// Sign this evidence. The signer is whoever ran the health checks; a
    /// supervisor configured with a trusted key accepts only their evidence.
    pub fn sign(&self, key: &SigningKey) -> SignedEvidence {
        SignedEvidence {
            evidence: self.clone(),
            signer: key.verifying_key(),
            signature: key.sign(&self.message()),
        }
    }
}

/// Promotion evidence bound to the key that produced it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedEvidence {
    evidence: PromotionEvidence,
    signer: VerifyingKey,
    signature: Signature,
}

impl SignedEvidence {
    pub fn evidence(&self) -> &PromotionEvidence {
        &self.evidence
    }

    pub fn signer(&self) -> VerifyingKey {
        self.signer
    }

    pub fn signature(&self) -> Signature {
        self.signature
    }

    /// Check the signature against the signer it names. This does not decide
    /// whether that signer is trusted; the supervisor does.
    pub fn verify(&self) -> Result<(), SupervisorError> {
        self.signer
            .verify(&self.evidence.message(), &self.signature)
            .map_err(|_| SupervisorError::InvalidSignature)
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
    trusted: Option<VerifyingKey>,
}

impl AbSupervisor {
    pub fn new(active: Image) -> Self {
        Self {
            active_slot: Slot::A,
            active,
            previous: None,
            staged: None,
            trusted: None,
        }
    }

    /// Require promotion evidence to be signed by `key`. Once set, unsigned
    /// promotion is refused; the key is the supervisor's policy, not the
    /// candidate's, so a candidate image cannot change it.
    pub fn trust(mut self, key: VerifyingKey) -> Self {
        self.trusted = Some(key);
        self
    }

    pub fn trusted_signer(&self) -> Option<VerifyingKey> {
        self.trusted
    }

    /// Promote with signed evidence. The signer must be the trusted key and
    /// the signature must cover exactly this evidence; then the ordinary
    /// evidence binding applies.
    pub fn promote_signed(&mut self, signed: &SignedEvidence) -> Result<Slot, SupervisorError> {
        let trusted = self.trusted.ok_or(SupervisorError::NoTrustedSigner)?;
        if signed.signer != trusted {
            return Err(SupervisorError::UntrustedSigner);
        }
        signed.verify()?;
        self.promote_checked(&signed.evidence)
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

    /// Promote with unsigned evidence. Refused when a trusted signer is
    /// configured, so configuring one cannot be bypassed by the older API.
    pub fn promote(&mut self, evidence: &PromotionEvidence) -> Result<Slot, SupervisorError> {
        if self.trusted.is_some() {
            return Err(SupervisorError::SignatureRequired);
        }
        self.promote_checked(evidence)
    }

    fn promote_checked(&mut self, evidence: &PromotionEvidence) -> Result<Slot, SupervisorError> {
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
    SignatureRequired,
    NoTrustedSigner,
    UntrustedSigner,
    InvalidSignature,
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
            Self::SignatureRequired => f.write_str("this supervisor promotes only signed evidence"),
            Self::NoTrustedSigner => f.write_str("no trusted evidence signer is configured"),
            Self::UntrustedSigner => f.write_str("evidence is signed by an untrusted key"),
            Self::InvalidSignature => f.write_str("evidence signature does not verify"),
        }
    }
}

impl std::error::Error for SupervisorError {}

#[cfg(test)]
mod tests {
    use super::*;
    use agel_core::Budget;
    use agel_image::ImageSession;
    use agel_integrity::SigningKey;

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
    fn trusted_supervisor_promotes_only_evidence_it_can_verify() {
        let (active, candidate) = active_and_candidate();
        let verifier = SigningKey::from_seed([3; 32]);
        let impostor = SigningKey::from_seed([4; 32]);
        let mut supervisor = AbSupervisor::new(active).trust(verifier.verifying_key());
        let checks = [HealthCheck::new("(transform 40)", Value::Int(42))];
        let evidence = supervisor.stage(candidate.clone(), &checks).unwrap();
        assert_eq!(
            supervisor.promote(&evidence),
            Err(SupervisorError::SignatureRequired)
        );
        assert_eq!(
            supervisor.promote_signed(&evidence.sign(&impostor)),
            Err(SupervisorError::UntrustedSigner)
        );
        let mut tampered = evidence.sign(&verifier);
        tampered.evidence.checks_passed = 99;
        assert_eq!(
            supervisor.promote_signed(&tampered),
            Err(SupervisorError::InvalidSignature)
        );
        let mut swapped = evidence.sign(&verifier);
        swapped.signature = evidence.sign(&impostor).signature;
        assert_eq!(
            supervisor.promote_signed(&swapped),
            Err(SupervisorError::InvalidSignature)
        );
        assert_eq!(
            supervisor
                .promote_signed(&evidence.sign(&verifier))
                .unwrap(),
            Slot::B
        );
        // Evidence for the old active image no longer binds, even when signed.
        supervisor.stage(candidate, &checks).unwrap();
        assert_eq!(
            supervisor.promote_signed(&evidence.sign(&verifier)),
            Err(SupervisorError::EvidenceMismatch)
        );
        let mut untrusted = AbSupervisor::new(supervisor.active().clone());
        assert_eq!(
            untrusted.promote_signed(&evidence.sign(&verifier)),
            Err(SupervisorError::NoTrustedSigner)
        );
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
