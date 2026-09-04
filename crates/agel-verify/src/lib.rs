//! Evidence-carrying, zero-authority staged upgrades for Agel worlds.

use agel_core::{read_all, Budget, Commit, Digest, EvaluationOptions, Expr, Value, World};
use agel_integrity::sha256;
use std::collections::BTreeSet;
use std::fmt;

pub const POLICY_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestCase {
    pub source: String,
    pub expected: Value,
}

impl TestCase {
    pub fn new(source: impl Into<String>, expected: Value) -> Self {
        Self {
            source: source.into(),
            expected,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proposal {
    pub base_revision: u64,
    pub base_digest: Digest,
    pub source: String,
    pub source_digest: Digest,
    pub declared_effects: BTreeSet<String>,
    pub tests: Vec<TestCase>,
    pub budget: Budget,
    pub policy_version: u32,
}

impl Proposal {
    pub fn new(world: &World, source: impl Into<String>) -> Self {
        let source = source.into();
        Self {
            base_revision: world.revision(),
            base_digest: world.content_digest(),
            source_digest: sha256(source.as_bytes()),
            source,
            declared_effects: BTreeSet::new(),
            tests: Vec::new(),
            budget: Budget::default(),
            policy_version: POLICY_VERSION,
        }
    }

    pub fn declares(mut self, effect: impl Into<String>) -> Self {
        self.declared_effects.insert(effect.into());
        self
    }

    pub fn tests(mut self, test: TestCase) -> Self {
        self.tests.push(test);
        self
    }

    pub fn digest(&self) -> Digest {
        let mut bytes = b"agel-proposal-v1\0".to_vec();
        push_u64(&mut bytes, self.base_revision);
        bytes.extend_from_slice(self.base_digest.as_bytes());
        bytes.extend_from_slice(self.source_digest.as_bytes());
        push_strings(&mut bytes, &self.declared_effects);
        push_u64(&mut bytes, self.tests.len() as u64);
        for test in &self.tests {
            push_string(&mut bytes, &test.source);
            push_string(&mut bytes, &test.expected.to_string());
        }
        push_u64(&mut bytes, self.budget.fuel);
        push_u64(&mut bytes, self.budget.max_call_depth as u64);
        push_u64(&mut bytes, self.budget.max_collection_len as u64);
        push_u64(&mut bytes, u64::from(self.policy_version));
        sha256(&bytes)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Evidence {
    pub proposal_digest: Digest,
    pub base_digest: Digest,
    pub candidate_digest: Digest,
    pub inferred_effects: BTreeSet<String>,
    pub tests_passed: usize,
    pub policy_version: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VerificationError {
    StaleBase,
    AlteredSource,
    UnsupportedPolicy(u32),
    Reader(String),
    ReservedDefinition(String),
    UndeclaredEffect(String),
    Canary(String),
    TestFailed {
        source: String,
        expected: Box<Value>,
        actual: Box<Value>,
    },
    EvidenceMismatch,
}

impl fmt::Display for VerificationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StaleBase => f.write_str("proposal base no longer matches the live world"),
            Self::AlteredSource => f.write_str("proposal source digest does not match its source"),
            Self::UnsupportedPolicy(version) => {
                write!(f, "unsupported verification policy: {version}")
            }
            Self::Reader(message) => write!(f, "proposal cannot be read: {message}"),
            Self::ReservedDefinition(name) => {
                write!(f, "proposal attempts to redefine trusted name: {name}")
            }
            Self::UndeclaredEffect(effect) => write!(f, "undeclared effect: {effect}"),
            Self::Canary(message) => write!(f, "canary rejected proposal: {message}"),
            Self::TestFailed {
                source,
                expected,
                actual,
            } => write!(f, "test {source:?} expected {expected}, produced {actual}"),
            Self::EvidenceMismatch => f.write_str("evidence does not match this proposal/world"),
        }
    }
}

impl std::error::Error for VerificationError {}

pub struct Verifier;

impl Verifier {
    pub fn verify(world: &World, proposal: &Proposal) -> Result<Evidence, VerificationError> {
        validate_identity(world, proposal)?;
        let expressions = read_all(&proposal.source)
            .map_err(|error| VerificationError::Reader(error.to_string()))?;
        reject_reserved_definitions(&expressions)?;
        let inferred_effects = infer_effects(&expressions);
        for effect in &inferred_effects {
            if !proposal.declared_effects.contains(effect) {
                return Err(VerificationError::UndeclaredEffect(effect.clone()));
            }
        }

        let options = EvaluationOptions {
            budget: proposal.budget.clone(),
            capabilities: Vec::new(),
        };
        let mut canary = world.fork_isolated();
        canary
            .evaluate_with(&proposal.source, &options)
            .map_err(|error| VerificationError::Canary(error.to_string()))?;
        let candidate_digest = canary.content_digest();
        for test in &proposal.tests {
            let mut test_world = canary.fork_isolated();
            let actual = test_world
                .evaluate_with(&test.source, &options)
                .map_err(|error| VerificationError::Canary(error.to_string()))?
                .values
                .pop()
                .unwrap_or(Value::Nil);
            if actual != test.expected {
                return Err(VerificationError::TestFailed {
                    source: test.source.clone(),
                    expected: Box::new(test.expected.clone()),
                    actual: Box::new(actual),
                });
            }
        }
        Ok(Evidence {
            proposal_digest: proposal.digest(),
            base_digest: proposal.base_digest,
            candidate_digest,
            inferred_effects,
            tests_passed: proposal.tests.len(),
            policy_version: POLICY_VERSION,
        })
    }

    pub fn promote(
        world: &mut World,
        proposal: &Proposal,
        evidence: &Evidence,
    ) -> Result<Commit, VerificationError> {
        validate_identity(world, proposal)?;
        if evidence.proposal_digest != proposal.digest()
            || evidence.base_digest != world.content_digest()
            || evidence.policy_version != POLICY_VERSION
        {
            return Err(VerificationError::EvidenceMismatch);
        }
        let options = EvaluationOptions {
            budget: proposal.budget.clone(),
            capabilities: Vec::new(),
        };
        world
            .evaluate_with(&proposal.source, &options)
            .map_err(|error| VerificationError::Canary(error.to_string()))
    }
}

fn validate_identity(world: &World, proposal: &Proposal) -> Result<(), VerificationError> {
    if proposal.policy_version != POLICY_VERSION {
        return Err(VerificationError::UnsupportedPolicy(
            proposal.policy_version,
        ));
    }
    if proposal.base_revision != world.revision() || proposal.base_digest != world.content_digest()
    {
        return Err(VerificationError::StaleBase);
    }
    if proposal.source_digest != sha256(proposal.source.as_bytes()) {
        return Err(VerificationError::AlteredSource);
    }
    Ok(())
}

fn infer_effects(expressions: &[Expr]) -> BTreeSet<String> {
    fn walk(expression: &Expr, effects: &mut BTreeSet<String>) {
        if let Expr::List(items) = expression {
            if let Some(Expr::Symbol(head)) = items.first() {
                if head == "model-request" {
                    effects.insert("model/infer".into());
                } else if head == "request-capability" {
                    effects.insert("authority/request".into());
                }
            }
            for item in items {
                walk(item, effects);
            }
        }
    }
    let mut effects = BTreeSet::new();
    for expression in expressions {
        walk(expression, &mut effects);
    }
    effects
}

fn reject_reserved_definitions(expressions: &[Expr]) -> Result<(), VerificationError> {
    fn walk(expression: &Expr) -> Result<(), VerificationError> {
        if let Expr::List(items) = expression {
            if matches!(items.first(), Some(Expr::Symbol(head)) if head == "def" || head == "defmacro")
            {
                if let Some(Expr::Symbol(name)) = items.get(1) {
                    if name.starts_with("agel/trusted-") {
                        return Err(VerificationError::ReservedDefinition(name.clone()));
                    }
                }
            }
            for item in items {
                walk(item)?;
            }
        }
        Ok(())
    }
    for expression in expressions {
        walk(expression)?;
    }
    Ok(())
}

fn push_u64(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

fn push_string(bytes: &mut Vec<u8>, value: &str) {
    push_u64(bytes, value.len() as u64);
    bytes.extend_from_slice(value.as_bytes());
}

fn push_strings<'a>(bytes: &mut Vec<u8>, values: impl IntoIterator<Item = &'a String>) {
    let values = values.into_iter().collect::<Vec<_>>();
    push_u64(bytes, values.len() as u64);
    for value in values {
        push_string(bytes, value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_upgrade_is_tested_and_atomically_promoted() {
        let mut world = World::default();
        let proposal = Proposal::new(&world, "(def square (fn (x) (* x x)))")
            .tests(TestCase::new("(square 9)", Value::Int(81)));
        let evidence = Verifier::verify(&world, &proposal).unwrap();
        assert_eq!(evidence.tests_passed, 1);
        Verifier::promote(&mut world, &proposal, &evidence).unwrap();
        assert_eq!(
            world.evaluate("(square 12)").unwrap().values[0],
            Value::Int(144)
        );
    }

    #[test]
    fn altered_stale_effectful_and_failing_proposals_are_rejected() {
        let mut world = World::default();
        let mut altered = Proposal::new(&world, "(def answer 42)");
        altered.source = "(def answer 0)".into();
        assert_eq!(
            Verifier::verify(&world, &altered),
            Err(VerificationError::AlteredSource)
        );

        let stale = Proposal::new(&world, "(def answer 42)");
        world.evaluate("(def intervening 1)").unwrap();
        assert_eq!(
            Verifier::verify(&world, &stale),
            Err(VerificationError::StaleBase)
        );

        let effectful = Proposal::new(&world, "(model-request 'claude \"x\" target)");
        assert_eq!(
            Verifier::verify(&world, &effectful),
            Err(VerificationError::UndeclaredEffect("model/infer".into()))
        );

        let failing = Proposal::new(&world, "(def add-one (fn (x) (+ x 1)))")
            .tests(TestCase::new("(add-one 4)", Value::Int(99)));
        assert!(matches!(
            Verifier::verify(&world, &failing),
            Err(VerificationError::TestFailed { .. })
        ));
    }

    #[test]
    fn trusted_checker_namespace_is_not_language_mutable() {
        let world = World::default();
        let proposal = Proposal::new(&world, "(def agel/trusted-verifier (fn (x) #t))");
        assert_eq!(
            Verifier::verify(&world, &proposal),
            Err(VerificationError::ReservedDefinition(
                "agel/trusted-verifier".into()
            ))
        );
    }
}
