//! Explicit host-side commit point for a pure compiled (state, input) function.
//! Scheduling and authority policy belong to the supplied Agel program, not here.

use crate::managed::{Fault, Limits, Native, Outcome};
use agel_core::Value;
use std::sync::Arc;

#[derive(Debug, PartialEq, Eq)]
pub enum CommitError {
    StaleRevision,
    RevisionOverflow,
    Execution(Fault),
    WrongOwner,
    NoPreviousProgram,
}
impl std::fmt::Display for CommitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for CommitError {}

#[derive(Debug)]
pub struct Receipt {
    pub revision: u64,
    pub fuel_used: u64,
    pub allocated_values: usize,
    pub tail_calls: u64,
    pub peak_call_depth: usize,
    pub collections: usize,
    pub reclaimed_slots: usize,
    pub peak_arena_slots: usize,
}

pub struct NativeState {
    program: Native,
    state: Value,
    revision: u64,
    owner: Arc<()>,
    previous: Option<Native>,
}

/// Opaque proposal bound to the exact machine/revision that was probed.
/// A successful probe is evidence for that input, not proof of all future behavior.
pub struct Candidate {
    owner: Arc<()>,
    revision: u64,
    program: Native,
    probe: Outcome,
}
impl Candidate {
    pub fn base_revision(&self) -> u64 {
        self.revision
    }
    pub fn ir(&self) -> &Value {
        self.program.ir()
    }
    pub fn probe(&self) -> &Outcome {
        &self.probe
    }
}

#[derive(Debug)]
pub struct UpgradeReceipt {
    pub revision: u64,
    pub probe_fuel_used: u64,
}
impl NativeState {
    /// The host chooses the initial state and trusted program. No execution occurs.
    pub fn new(program: Native, initial: Value) -> Self {
        Self {
            program,
            state: initial,
            revision: 0,
            owner: Arc::new(()),
            previous: None,
        }
    }
    pub fn state(&self) -> &Value {
        &self.state
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn program_ir(&self) -> &Value {
        self.program.ir()
    }

    fn next_revision(&self, expected: u64) -> Result<u64, CommitError> {
        if expected != self.revision {
            return Err(CommitError::StaleRevision);
        }
        self.revision
            .checked_add(1)
            .ok_or(CommitError::RevisionOverflow)
    }

    /// Runs an explicit probe without changing active code, state, or revision.
    pub fn preview(
        &self,
        expected: u64,
        program: Native,
        input: &Value,
        limits: Limits,
    ) -> Result<Candidate, CommitError> {
        self.next_revision(expected)?;
        let probe = program
            .invoke_refs(&[&self.state, input], limits)
            .map_err(CommitError::Execution)?;
        Ok(Candidate {
            owner: self.owner.clone(),
            revision: self.revision,
            program,
            probe,
        })
    }

    /// Switches code only; preview output is deliberately NOT committed as state.
    pub fn promote(
        &mut self,
        expected: u64,
        candidate: Candidate,
    ) -> Result<UpgradeReceipt, CommitError> {
        if !Arc::ptr_eq(&self.owner, &candidate.owner) {
            return Err(CommitError::WrongOwner);
        }
        let revision = self.next_revision(expected)?;
        if candidate.revision != self.revision {
            return Err(CommitError::StaleRevision);
        }
        self.previous = Some(std::mem::replace(&mut self.program, candidate.program));
        self.revision = revision;
        Ok(UpgradeReceipt {
            revision,
            probe_fuel_used: candidate.probe.fuel_used,
        })
    }

    /// Probes previous code on CURRENT state, then swaps code only. No state rewind.
    /// Repeating this operation toggles the two retained programs (undo/redo).
    pub fn rollback_program(
        &mut self,
        expected: u64,
        input: &Value,
        limits: Limits,
    ) -> Result<UpgradeReceipt, CommitError> {
        let revision = self.next_revision(expected)?;
        let previous = self
            .previous
            .as_mut()
            .ok_or(CommitError::NoPreviousProgram)?;
        let probe = previous
            .invoke_refs(&[&self.state, input], limits)
            .map_err(CommitError::Execution)?;
        std::mem::swap(&mut self.program, previous);
        self.revision = revision;
        Ok(UpgradeReceipt {
            revision,
            probe_fuel_used: probe.fuel_used,
        })
    }

    /// A failed call (including output export) changes neither state nor revision.
    /// This is in-memory atomicity, not durable storage or the hosted World API.
    pub fn transact(
        &mut self,
        expected: u64,
        input: &Value,
        limits: Limits,
    ) -> Result<Receipt, CommitError> {
        let revision = self.next_revision(expected)?;
        let result = self
            .program
            .invoke_refs(&[&self.state, input], limits)
            .map_err(CommitError::Execution)?;
        self.state = result.value;
        self.revision = revision;
        Ok(Receipt {
            revision,
            fuel_used: result.fuel_used,
            allocated_values: result.allocated_values,
            tail_calls: result.tail_calls,
            peak_call_depth: result.peak_call_depth,
            collections: result.collections,
            reclaimed_slots: result.reclaimed_slots,
            peak_arena_slots: result.peak_arena_slots,
        })
    }
}
