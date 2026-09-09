//! Shared fixed-memory session boundary for native Agel workshops.
//!
//! Frontends own interaction policy. This module owns the one bounded protocol
//! for talking to an evaluator domain and reconstructing it from source cells.

use crate::arch;
use crate::service::ServiceDomain;
use crate::workspace::Workspace;
use crate::world::{shared, Stop, PAYLOAD_BYTES};

#[derive(Clone, Copy)]
pub struct EvaluatorReply {
    pub bytes: [u8; PAYLOAD_BYTES],
    pub length: usize,
    pub revision: u64,
    pub error: bool,
}

pub fn request(
    evaluator: &mut arch::Domain,
    command: u64,
    source: &[u8],
) -> Result<EvaluatorReply, &'static str> {
    if source.len() > PAYLOAD_BYTES {
        return Err("request exceeds shared evaluator payload");
    }
    for (offset, byte) in source.iter().enumerate() {
        evaluator.core().write_payload(offset, *byte);
    }
    evaluator
        .core()
        .write_shared(shared::ARGUMENTS, source.len() as u64);
    evaluator.core().stage_command(command);
    match evaluator.run() {
        Stop::Replied => {}
        Stop::Faulted(fault) => {
            crate::kprint!(
                "native evaluator contained: {} at {:#x}; restart required\n",
                fault.name(),
                fault.pc
            );
            return Err("domain fault; restart required");
        }
        Stop::BudgetExhausted => {
            crate::kprint!("native evaluator contained: tick budget exhausted; restart required\n");
            return Err("tick budget exhausted; restart required");
        }
    }

    let error = evaluator.core().read_shared(shared::STATUS) != 0;
    let length = (evaluator.core().read_shared(shared::VALUES) as usize).min(PAYLOAD_BYTES);
    let revision = evaluator.core().read_shared(shared::VALUES + 1);
    let mut bytes = [0_u8; PAYLOAD_BYTES];
    for (offset, byte) in bytes.iter_mut().take(length).enumerate() {
        *byte = evaluator.core().read_payload(offset);
    }
    Ok(EvaluatorReply {
        bytes,
        length,
        revision,
        error,
    })
}

#[cfg(feature = "isolated-repl")]
pub fn reset(evaluator: &mut arch::Domain) -> Result<(), &'static str> {
    request(evaluator, shared::COMMAND_EVALUATOR_RESET, b"").map(|_| ())
}

#[derive(Clone, Copy)]
pub enum ReplayFailure {
    Language(usize),
    Transport {
        ordinal: usize,
        reason: &'static str,
    },
}

impl ReplayFailure {
    #[cfg(feature = "native-graphics")]
    pub fn message(self) -> &'static str {
        match self {
            Self::Language(ordinal) => {
                crate::kprint!("workspace replay rejected at cell {}\n", ordinal);
                "workspace replay rejected"
            }
            Self::Transport { ordinal, reason } => {
                crate::kprint!("workspace transport failed at cell {}\n", ordinal);
                reason
            }
        }
    }
}

/// Validate every source cell in an empty candidate, publish disk, then adopt
/// the candidate. No failed validation or disk write resets the live evaluator.
pub fn save(
    evaluator: &mut arch::Domain,
    storage: Option<&mut ServiceDomain>,
    workspace: &Workspace,
    generation: u64,
    protect: u64,
) -> Result<(u64, u64), &'static str> {
    let Some(storage) = storage else {
        return Err("no storage device on this machine");
    };
    save_to_disk(evaluator, storage, workspace, generation, protect)
}

fn save_to_disk(
    evaluator: &mut arch::Domain,
    storage: &mut ServiceDomain,
    workspace: &Workspace,
    generation: u64,
    protect: u64,
) -> Result<(u64, u64), &'static str> {
    request(evaluator, shared::COMMAND_EVALUATOR_REBUILD, b"")?;
    for ordinal in 0..workspace.count() {
        let cell = workspace.cell(ordinal).ok_or("missing source cell")?;
        let reply = request(evaluator, shared::COMMAND_EVALUATOR_STAGE, cell.source())?;
        if reply.error {
            return Err("source candidate rejected; live world retained");
        }
    }
    let next = match crate::workspace::save(storage, workspace, generation, protect) {
        Ok(next) => next,
        Err(reason) => {
            let _ = request(evaluator, shared::COMMAND_EVALUATOR_DISCARD, b"");
            return Err(reason);
        }
    };
    let reply = request(evaluator, shared::COMMAND_EVALUATOR_PROMOTE, b"")?;
    if reply.error {
        return Err("source saved; live promotion failed; reload required");
    }
    Ok((next, reply.revision))
}

pub fn replay(evaluator: &mut arch::Domain, workspace: &Workspace) -> Result<u64, ReplayFailure> {
    let reset = request(evaluator, shared::COMMAND_EVALUATOR_RESET, b"").map_err(|reason| {
        ReplayFailure::Transport {
            ordinal: usize::MAX,
            reason,
        }
    })?;
    let mut revision = reset.revision;
    for ordinal in 0..workspace.count() {
        let cell = workspace
            .cell(ordinal)
            .ok_or(ReplayFailure::Language(ordinal))?;
        let reply = request(evaluator, shared::COMMAND_EVALUATE, cell.source())
            .map_err(|reason| ReplayFailure::Transport { ordinal, reason })?;
        if reply.error {
            return Err(ReplayFailure::Language(ordinal));
        }
        revision = reply.revision;
    }
    Ok(revision)
}
