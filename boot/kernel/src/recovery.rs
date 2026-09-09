//! The disk-backed recovery plane for machines that have a disk.
//!
//! `monitor.rs` is the executable A/B policy model every backend exercises.
//! This module is that policy made durable and bound to real workspace
//! generations: a *trusted* generation is the rollback point, a *candidate* is
//! a newer generation still earning trust, and a boot counter turns a
//! candidate that keeps failing to reach an interactive state into an
//! automatic rollback. Promotion is an explicit operator decision; the health
//! oracle is a source cell named `health` evaluated in an isolated world.
//!
//! None of this lives in a language world. The record is supervisor policy on
//! bytes the storage driver domain carried.

use crate::service::ServiceDomain;
use crate::workspace::{load_record, save_record, RecoveryRecord};

/// Boots a candidate may take without reaching a healthy state before the
/// trusted generation is booted instead.
pub const ATTEMPT_BUDGET: u32 = 3;

/// Which generation the boot should replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BootPlan {
    /// Replay the newest valid generation, as before.
    Newest,
    /// The candidate exhausted its attempts; replay the trusted generation.
    Rollback {
        trusted: u64,
        candidate: u64,
        attempts: u32,
    },
}

pub struct LiveRecovery {
    record: RecoveryRecord,
    /// This boot replayed the trusted generation because the candidate
    /// exhausted its attempts. Not persisted: it describes this boot.
    rolled_back: bool,
}

impl LiveRecovery {
    pub fn load(storage: &mut ServiceDomain) -> Result<Self, &'static str> {
        Ok(Self {
            record: load_record(storage)?,
            rolled_back: false,
        })
    }

    pub fn record(&self) -> RecoveryRecord {
        self.record
    }

    pub fn rolled_back(&self) -> bool {
        self.rolled_back
    }

    /// The caller booted the trusted generation instead of the candidate.
    pub fn note_rollback(&mut self) {
        self.rolled_back = true;
    }

    /// Decide what to boot given the newest generation on disk, and charge
    /// this boot to the candidate before anything of it runs.
    pub fn plan_boot(
        &mut self,
        storage: &mut ServiceDomain,
        newest: u64,
    ) -> Result<BootPlan, &'static str> {
        if newest == 0 || newest == self.record.trusted {
            return Ok(BootPlan::Newest);
        }
        if newest != self.record.candidate {
            // A generation this record has never seen, or the recorded
            // candidate is no longer on disk: the newest becomes the candidate
            // with a fresh budget.
            self.record.candidate = newest;
            self.record.attempts = 0;
            self.record.verified = false;
        }
        if self.record.verified {
            return Ok(BootPlan::Newest);
        }
        if self.record.attempts >= ATTEMPT_BUDGET && self.record.trusted != 0 {
            return Ok(BootPlan::Rollback {
                trusted: self.record.trusted,
                candidate: self.record.candidate,
                attempts: self.record.attempts,
            });
        }
        self.record.attempts += 1;
        save_record(storage, &self.record)?;
        Ok(BootPlan::Newest)
    }

    /// The running generation reached an interactive, working state. Only the
    /// candidate itself can be verified this way: a healthy boot of the
    /// trusted generation after a rollback says nothing about the candidate.
    /// Returns the generation newly marked healthy, if this call changed anything.
    pub fn healthy(
        &mut self,
        storage: &mut ServiceDomain,
        running: u64,
    ) -> Result<Option<u64>, &'static str> {
        if self.record.candidate == 0 || self.record.verified || running != self.record.candidate {
            return Ok(None);
        }
        self.record.verified = true;
        self.record.attempts = 0;
        save_record(storage, &self.record)?;
        Ok(Some(self.record.candidate))
    }

    /// A new generation was published; it is the candidate until promoted.
    pub fn on_saved(
        &mut self,
        storage: &mut ServiceDomain,
        generation: u64,
    ) -> Result<(), &'static str> {
        self.record.candidate = generation;
        self.record.attempts = 0;
        self.record.verified = false;
        save_record(storage, &self.record)
    }

    /// Explicit health evidence for the candidate.
    #[cfg(feature = "isolated-repl")]
    pub fn verify(&mut self, storage: &mut ServiceDomain) -> Result<u64, &'static str> {
        if self.record.candidate == 0 {
            return Err("denied: no candidate generation to verify");
        }
        self.record.verified = true;
        self.record.attempts = 0;
        save_record(storage, &self.record)?;
        Ok(self.record.candidate)
    }

    /// Make the verified candidate the trusted generation; the previous
    /// trusted generation is what a later fault rolls back to only until the
    /// next save reuses its slot, which is why promotion is deliberate.
    #[cfg(feature = "isolated-repl")]
    pub fn promote(&mut self, storage: &mut ServiceDomain) -> Result<(u64, u64), &'static str> {
        if self.record.candidate == 0 {
            return Err("denied: no candidate generation");
        }
        if !self.record.verified {
            return Err("denied: verify candidate before promotion");
        }
        let previous = self.record.trusted;
        self.record.trusted = self.record.candidate;
        self.record.candidate = 0;
        self.record.attempts = 0;
        self.record.verified = false;
        save_record(storage, &self.record)?;
        Ok((self.record.trusted, previous))
    }

    /// Roll back to the trusted generation now. The candidate stays recorded
    /// with its budget exhausted, so later boots keep choosing the trusted
    /// generation until an operator verifies the candidate or saves a new one;
    /// clearing it would let the same generation boot again by being newest.
    #[cfg(feature = "isolated-repl")]
    pub fn fault(&mut self, storage: &mut ServiceDomain) -> Result<u64, &'static str> {
        if self.record.trusted == 0 {
            return Err("denied: no trusted generation to roll back to");
        }
        if self.record.candidate != 0 {
            self.record.attempts = ATTEMPT_BUDGET;
            self.record.verified = false;
        }
        save_record(storage, &self.record)?;
        self.rolled_back = true;
        Ok(self.record.trusted)
    }
}
