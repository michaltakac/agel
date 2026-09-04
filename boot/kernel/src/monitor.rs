//! The recovery plane: an A/B policy that no mutable world can reach.
//!
//! It is deliberately boring and deliberately small. Its only job is to be the
//! thing that is still working after everything else has gone wrong, which is
//! why it lives outside every protection domain, holds no language state, and
//! is exercised at the end of the isolation test on every architecture.

use crate::console;

#[derive(Clone, Copy)]
pub enum Slot {
    A,
    B,
}

pub struct RecoveryMonitor {
    active: Slot,
    previous: Slot,
    candidate_verified: bool,
}

impl RecoveryMonitor {
    /// A monitor with slot A active and no verified candidate.
    pub const fn new() -> Self {
        Self {
            active: Slot::A,
            previous: Slot::A,
            candidate_verified: false,
        }
    }

    /// Report which slot is active.
    pub fn status(&self) {
        console::write("active slot: ");
        console::write(match self.active {
            Slot::A => "A (stable)\n",
            Slot::B => "B (candidate)\n",
        });
    }

    /// Record isolated health evidence for candidate B.
    pub fn verify(&mut self) {
        self.candidate_verified = true;
        console::write("candidate B: isolated health evidence accepted\n");
    }

    /// Select a verified candidate, retaining the previous slot.
    pub fn promote(&mut self) {
        if matches!(self.active, Slot::B) {
            self.candidate_verified = false;
            console::write("denied: candidate B is already active; slot A remains rollback\n");
        } else if self.candidate_verified {
            self.previous = self.active;
            self.active = Slot::B;
            self.candidate_verified = false;
            console::write("selected slot B; slot A retained for rollback\n");
        } else {
            console::write("denied: verify candidate before promotion\n");
        }
    }

    /// Model a watchdog rollback to the retained slot.
    pub fn fault(&mut self) {
        self.active = self.previous;
        self.candidate_verified = false;
        console::write("watchdog fault: rolled back to slot ");
        console::write(match self.active {
            Slot::A => "A\n",
            Slot::B => "B\n",
        });
    }
}
