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
#[cfg(target_arch = "x86_64")]
use crate::workspace::{
    load_selector, read_slot_sector, save_selector, KernelSelector, KERNEL_SLOT_SECTORS,
    NO_CANDIDATE,
};
#[cfg(target_arch = "x86_64")]
use agel_integrity::{Sha512, Signature, VerifyingKey};

#[cfg(target_arch = "x86_64")]
include!(concat!(env!("OUT_DIR"), "/kernel-signing-key.rs"));

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

#[cfg(target_arch = "x86_64")]
/// Where the BIOS stage leaves the slot it loaded: a marker so a stage that
/// predates the selector is recognized, then the slot number.
const SELECTOR_MARKER: *const u32 = 0x6fe8 as *const u32;
#[cfg(target_arch = "x86_64")]
const SELECTOR_SLOT: *const u8 = 0x6fec as *const u8;
#[cfg(target_arch = "x86_64")]
const SELECTOR_MAGIC: u32 = 0xa6e1_5107;

#[cfg(target_arch = "x86_64")]
pub fn slot_name(slot: u8) -> &'static str {
    if slot == 0 {
        "A"
    } else {
        "B"
    }
}

#[cfg(target_arch = "x86_64")]
/// The kernel image's own A/B state. The boot stage charges an unverified
/// candidate's attempts and skips it after `ATTEMPT_BUDGET`; the running
/// kernel can only report what happened, verify itself by reaching a healthy
/// state, promote a verified candidate, or give the candidate up.
pub struct KernelRecovery {
    selector: KernelSelector,
    /// The slot the boot stage loaded, or `None` when the stage is older than
    /// the selector and loaded slot A without consulting it.
    booted: Option<u8>,
}

#[cfg(target_arch = "x86_64")]
impl KernelRecovery {
    pub fn load(storage: &mut ServiceDomain) -> Result<Self, &'static str> {
        // SAFETY: both addresses are inside the BIOS scratch area below the
        // kernel image and the frame pool; the boot stage wrote them before
        // entering long mode and nothing in the kernel maps or reuses them.
        let booted = unsafe {
            if SELECTOR_MARKER.read_volatile() == SELECTOR_MAGIC {
                Some(SELECTOR_SLOT.read_volatile() & 1)
            } else {
                None
            }
        };
        Ok(Self {
            selector: load_selector(storage)?,
            booted,
        })
    }

    pub fn selector(&self) -> KernelSelector {
        self.selector
    }

    pub fn booted(&self) -> Option<u8> {
        self.booted
    }

    /// Check a staged candidate's signature and either admit it, so the boot
    /// stage may load it, or clear it. Runs on every boot of any slot; a
    /// candidate is only ever staged from the host, and only a kernel built
    /// with the matching public key can let it run.
    pub fn admit(&mut self, storage: &mut ServiceDomain) -> Result<Admission, &'static str> {
        let selector = self.selector;
        if selector.candidate == NO_CANDIDATE || selector.admitted {
            return Ok(Admission::Nothing);
        }
        match verify_slot(
            storage,
            selector.candidate,
            selector.length,
            &selector.signature,
        ) {
            Ok(()) => {
                self.selector.admitted = true;
                save_selector(storage, &self.selector)?;
                Ok(Admission::Admitted(selector.candidate))
            }
            Err(reason) => {
                self.selector.clear_candidate();
                save_selector(storage, &self.selector)?;
                Ok(Admission::Refused(selector.candidate, reason))
            }
        }
    }

    /// The stage loaded the trusted slot because the candidate exhausted its
    /// budget without a healthy boot.
    pub fn rolled_back(&self) -> bool {
        self.booted == Some(self.selector.trusted)
            && self.selector.candidate != NO_CANDIDATE
            && !self.selector.verified
            && u32::from(self.selector.attempts) >= ATTEMPT_BUDGET
    }

    /// The running kernel reached a healthy state. Only a boot of the
    /// candidate slot itself is evidence for the candidate.
    pub fn healthy(&mut self, storage: &mut ServiceDomain) -> Result<Option<u8>, &'static str> {
        if self.selector.candidate == NO_CANDIDATE
            || self.selector.verified
            || self.booted != Some(self.selector.candidate)
        {
            return Ok(None);
        }
        self.selector.verified = true;
        self.selector.attempts = 0;
        save_selector(storage, &self.selector)?;
        Ok(Some(self.selector.candidate))
    }

    /// Make the verified candidate slot the one the stage loads by default.
    #[cfg(feature = "isolated-repl")]
    pub fn promote(&mut self, storage: &mut ServiceDomain) -> Result<(u8, u8), &'static str> {
        if self.selector.candidate == NO_CANDIDATE {
            return Err("denied: no candidate kernel slot");
        }
        if !self.selector.verified {
            return Err("denied: the candidate kernel has not completed a healthy boot");
        }
        let previous = self.selector.trusted;
        self.selector.trusted = self.selector.candidate;
        self.selector.clear_candidate();
        save_selector(storage, &self.selector)?;
        Ok((self.selector.trusted, previous))
    }

    /// Exhaust the candidate's budget so the next boot loads the trusted slot.
    /// The candidate stays recorded, so it cannot be booted again by accident.
    #[cfg(feature = "isolated-repl")]
    pub fn fault(&mut self, storage: &mut ServiceDomain) -> Result<u8, &'static str> {
        if self.selector.candidate == NO_CANDIDATE {
            return Err("denied: no candidate kernel slot to give up");
        }
        self.selector.attempts = ATTEMPT_BUDGET as u8;
        self.selector.verified = false;
        save_selector(storage, &self.selector)?;
        Ok(self.selector.trusted)
    }
}

#[cfg(target_arch = "x86_64")]
/// What checking a staged candidate decided.
pub enum Admission {
    /// No candidate, or one already admitted.
    Nothing,
    Admitted(u8),
    Refused(u8, &'static str),
}

#[cfg(target_arch = "x86_64")]
/// Hash the candidate slot sector by sector and verify the staged signature
/// over that digest against the key this kernel was built with.
fn verify_slot(
    storage: &mut ServiceDomain,
    slot: u8,
    length: u32,
    signature: &[u8; 64],
) -> Result<(), &'static str> {
    if length == 0 || length > KERNEL_SLOT_SECTORS * 512 {
        return Err("signed length is outside the slot");
    }
    let key = VerifyingKey::from_bytes(KERNEL_SIGNING_KEY)
        .map_err(|_| "the kernel's trust key is not a valid Ed25519 key")?;
    let mut hasher = Sha512::new();
    let mut sector = [0_u8; 512];
    let mut remaining = length as usize;
    let mut index = 0;
    while remaining > 0 {
        read_slot_sector(storage, slot, index, &mut sector)?;
        let take = remaining.min(512);
        hasher.update(&sector[..take]);
        remaining -= take;
        index += 1;
    }
    key.verify(&hasher.finish(), &Signature::from_bytes(*signature))
        .map_err(|_| "signature does not verify against the kernel's trust key")
}
