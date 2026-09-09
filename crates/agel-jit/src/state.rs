//! Explicit host-side commit point for a pure compiled (state, input) function.
//! Scheduling and authority policy belong to the supplied Agel program, not here.

use crate::managed::{Fault, Limits, Native};
use agel_core::Value;

#[derive(Debug, PartialEq, Eq)]
pub enum CommitError {
    StaleRevision,
    RevisionOverflow,
    Execution(Fault),
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
}
impl NativeState {
    /// The host chooses the initial state and trusted program. No execution occurs.
    pub fn new(program: Native, initial: Value) -> Self {
        Self {
            program,
            state: initial,
            revision: 0,
        }
    }
    pub fn state(&self) -> &Value {
        &self.state
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// A failed call (including output export) changes neither state nor revision.
    /// This is in-memory atomicity, not durable storage or the hosted World API.
    pub fn transact(
        &mut self,
        expected: u64,
        input: &Value,
        limits: Limits,
    ) -> Result<Receipt, CommitError> {
        if expected != self.revision {
            return Err(CommitError::StaleRevision);
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(CommitError::RevisionOverflow)?;
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
