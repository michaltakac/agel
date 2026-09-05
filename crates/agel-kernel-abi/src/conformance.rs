//! The backend-independent conformance corpus.
//!
//! Phase 0 of the native roadmap is "freeze the boundary". This corpus is that
//! freeze in executable form: a fixed sequence of invocations, against a fixed
//! initial capability space, whose rendered transcript must be byte-identical
//! on every backend that claims to implement contract v1.0.
//!
//! The corpus deliberately spends most of its steps on *refusals*. An interface
//! is defined by what it declines, in what words, and in what order it checks;
//! two kernels that agree on the happy path and disagree on which of
//! `invalid-capability`, `wrong-object-type`, and `insufficient-rights` a bad
//! call earns do not have the same contract.

use crate::model::{group, slot};
use crate::{write_step, Kernel, Operation, Request, Response, Rights, Status};
use core::fmt;

/// One corpus entry: a stable label and the invocation to make.
///
/// Expected responses are not stored here. The reference model in
/// [`crate::model`] is the authority, the checked-in transcript is its frozen
/// output, and an independent backend proves itself by reproducing that output
/// rather than by being handed the answers.
#[derive(Clone, Copy, Debug)]
pub struct Step {
    /// Stable identifier of this step, used in transcripts and divergences.
    pub label: &'static str,
    /// The invocation to perform.
    pub request: Request,
}

impl Step {
    const fn new(label: &'static str, request: Request) -> Self {
        Self { label, request }
    }
}

/// Where two backends stopped agreeing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Divergence {
    /// Index into [`CORPUS`].
    pub index: usize,
    /// The diverging step's label.
    pub label: &'static str,
    /// What the first backend answered.
    pub left: Response,
    /// What the second backend answered.
    pub right: Response,
}

const CNODE: u32 = slot::CNODE;
const EP: u32 = slot::ENDPOINT;
const NOTE: u32 = slot::NOTIFICATION;
const FRAME: u32 = slot::FRAME;
const CLOCK: u32 = slot::CLOCK;

// Scratch slots the corpus derives into. Keeping them named makes the
// derivation and revocation steps readable.
const FRAME_READ: u32 = 6;
const EP_COPY: u32 = 7;
const EP_RECEIVE_ONLY: u32 = 8;
const EP_BADGED: u32 = 9;
const NOTE_BADGED: u32 = 10;
const EP_CHAIN_PARENT: u32 = 11;
const EP_CHAIN_CHILD: u32 = 12;
const FRAME_MOVED: u32 = 13;
const CLOCK_POWERLESS: u32 = 14;

const READ: u64 = Rights::READ.0 as u64;
const SEND: u64 = Rights::SEND.0 as u64;
const RECEIVE: u64 = Rights::RECEIVE.0 as u64;
const CONTROL: u64 = Rights::CONTROL.0 as u64;
const ALL: u64 = Rights::ALL.0 as u64;
const NO_RIGHTS: u64 = 0;
/// A bit pattern outside [`Rights::ALL`]; reserved rights must be rejected.
const UNDEFINED_RIGHTS: u64 = 0x8000_0000;

const fn cnode(operation: Operation, arguments: [u64; 4]) -> Request {
    Request::with(operation, CNODE, arguments)
}

/// The frozen v1.0 conformance corpus.
///
/// Steps are ordered and stateful: each one runs against the capability space
/// the previous ones left behind. Inserting a step in the middle changes every
/// later derivation identifier and is therefore a contract change that must
/// bump [`crate::VERSION_MINOR`] and regenerate the checked-in transcript.
pub const CORPUS: &[Step] = &[
    // -- Probe and published profile ---------------------------------------
    Step::new("probe/nop", Request::new(Operation::Nop, slot::NULL)),
    Step::new(
        "probe/nop-rejects-a-capability",
        Request::new(Operation::Nop, CNODE),
    ),
    Step::new("boot/info", Request::new(Operation::BootInfo, slot::NULL)),
    Step::new(
        "boot/info-rejects-a-capability",
        Request::new(Operation::BootInfo, CNODE),
    ),
    // -- Operations outside the v1.0 profile answer, they do not guess ------
    Step::new(
        "profile/pd-create",
        Request::new(Operation::PdCreate, slot::NULL),
    ),
    Step::new(
        "profile/pd-reap",
        Request::new(Operation::PdReap, slot::NULL),
    ),
    Step::new("profile/as-map", Request::new(Operation::AsMap, slot::NULL)),
    Step::new(
        "profile/thread-configure",
        Request::new(Operation::ThreadConfigure, slot::NULL),
    ),
    Step::new(
        "profile/frame-allocate",
        Request::new(Operation::FrameAllocate, FRAME),
    ),
    Step::new(
        "profile/irq-bind",
        Request::new(Operation::IrqBind, slot::NULL),
    ),
    Step::new(
        "profile/sched-budget",
        Request::new(Operation::SchedBudget, slot::NULL),
    ),
    Step::new(
        "profile/clock-deadline",
        Request::new(Operation::ClockDeadline, CLOCK),
    ),
    // -- Naming an object is never authority --------------------------------
    Step::new(
        "capability/empty-slot",
        Request::new(Operation::EndpointSend, FRAME_READ),
    ),
    Step::new(
        "capability/out-of-range",
        Request::new(Operation::EndpointSend, 4096),
    ),
    Step::new(
        "capability/null-slot",
        Request::new(Operation::EndpointSend, slot::NULL),
    ),
    // -- A slot's type is checked before its rights -------------------------
    Step::new(
        "type/notification-op-on-endpoint",
        Request::new(Operation::NotificationSignal, EP),
    ),
    Step::new(
        "type/endpoint-op-on-clock",
        Request::with(Operation::EndpointSend, CLOCK, [0, 1, 0, 0]),
    ),
    Step::new(
        "type/derivation-requires-the-cnode",
        Request::with(Operation::CapCopy, EP, [EP as u64, EP_COPY as u64, 0, 0]),
    ),
    // -- Reserved argument words must be zero -------------------------------
    Step::new(
        "argument/reserved-receive-word",
        Request::with(Operation::EndpointReceive, EP, [0, 0, 0, 1]),
    ),
    Step::new(
        "argument/reserved-send-flags",
        Request::with(Operation::EndpointSend, EP, [1, 0, 0, 0]),
    ),
    Step::new(
        "argument/undefined-rights-bit",
        cnode(
            Operation::CapMint,
            [FRAME as u64, FRAME_READ as u64, UNDEFINED_RIGHTS, 0],
        ),
    ),
    // -- Derivation is monotonic: no component mints its own authority ------
    Step::new(
        "derive/mint-cannot-widen",
        cnode(
            Operation::CapMint,
            [FRAME as u64, FRAME_READ as u64, ALL, 0],
        ),
    ),
    Step::new(
        "derive/mint-narrows",
        cnode(
            Operation::CapMint,
            [FRAME as u64, FRAME_READ as u64, READ, 0],
        ),
    ),
    Step::new(
        "derive/copy-preserves-rights",
        cnode(Operation::CapCopy, [EP as u64, EP_COPY as u64, 0, 0]),
    ),
    Step::new(
        "derive/copy-rejects-an-occupied-slot",
        cnode(Operation::CapCopy, [EP as u64, EP_COPY as u64, 0, 0]),
    ),
    Step::new(
        "derive/mint-receive-only",
        cnode(
            Operation::CapMint,
            [EP as u64, EP_RECEIVE_ONLY as u64, RECEIVE, 0],
        ),
    ),
    Step::new(
        "rights/send-without-send-right",
        Request::with(Operation::EndpointSend, EP_RECEIVE_ONLY, [0, 1, 0, 0]),
    ),
    Step::new(
        "rights/receive-with-receive-right",
        Request::new(Operation::EndpointReceive, EP_RECEIVE_ONLY),
    ),
    // -- Moving transfers authority and empties the source ------------------
    Step::new(
        "move/frame-to-a-free-slot",
        cnode(
            Operation::CapMove,
            [FRAME_READ as u64, FRAME_MOVED as u64, 0, 0],
        ),
    ),
    Step::new(
        "move/source-is-now-empty",
        cnode(Operation::CapAttenuate, [FRAME_READ as u64, READ, 0, 0]),
    ),
    Step::new(
        "move/destination-is-live",
        cnode(Operation::CapAttenuate, [FRAME_MOVED as u64, READ, 0, 0]),
    ),
    Step::new(
        "move/onto-itself-is-refused",
        cnode(
            Operation::CapMove,
            [FRAME_MOVED as u64, FRAME_MOVED as u64, 0, 0],
        ),
    ),
    Step::new(
        "move/onto-an-occupied-slot-is-refused",
        cnode(Operation::CapMove, [FRAME_MOVED as u64, EP as u64, 0, 0]),
    ),
    // -- Attenuation is one-way ---------------------------------------------
    Step::new(
        "attenuate/cannot-widen",
        cnode(Operation::CapAttenuate, [FRAME_MOVED as u64, ALL, 0, 0]),
    ),
    Step::new(
        "attenuate/to-powerless",
        cnode(
            Operation::CapAttenuate,
            [FRAME_MOVED as u64, NO_RIGHTS, 0, 0],
        ),
    ),
    Step::new(
        "attenuate/powerless-stays-powerless",
        cnode(Operation::CapAttenuate, [FRAME_MOVED as u64, READ, 0, 0]),
    ),
    // -- Every queue is bounded and pushes back -----------------------------
    Step::new(
        "endpoint/receive-when-empty",
        Request::new(Operation::EndpointReceive, EP),
    ),
    Step::new(
        "endpoint/send-1",
        Request::with(Operation::EndpointSend, EP, [0, 1, 0, 0]),
    ),
    Step::new(
        "endpoint/send-2",
        Request::with(Operation::EndpointSend, EP, [0, 2, 0, 0]),
    ),
    Step::new(
        "endpoint/send-3",
        Request::with(Operation::EndpointSend, EP, [0, 3, 0, 0]),
    ),
    Step::new(
        "endpoint/send-4",
        Request::with(Operation::EndpointSend, EP, [0, 4, 0, 0]),
    ),
    Step::new(
        "endpoint/send-5-is-backpressure",
        Request::with(Operation::EndpointSend, EP, [0, 5, 0, 0]),
    ),
    Step::new(
        "endpoint/receive-1",
        Request::new(Operation::EndpointReceive, EP),
    ),
    Step::new(
        "endpoint/receive-2",
        Request::new(Operation::EndpointReceive, EP),
    ),
    Step::new(
        "endpoint/receive-3",
        Request::new(Operation::EndpointReceive, EP),
    ),
    Step::new(
        "endpoint/receive-4",
        Request::new(Operation::EndpointReceive, EP),
    ),
    Step::new(
        "endpoint/receive-when-drained",
        Request::new(Operation::EndpointReceive, EP),
    ),
    // -- A badge identifies the sending capability, not the sender ----------
    Step::new(
        "endpoint/mint-badged-sender",
        cnode(
            Operation::CapMint,
            [EP as u64, EP_BADGED as u64, SEND, 0x5eed],
        ),
    ),
    Step::new(
        "endpoint/send-through-badge",
        Request::with(Operation::EndpointSend, EP_BADGED, [0, 7, 8, 9]),
    ),
    Step::new(
        "endpoint/receive-carries-the-badge",
        Request::new(Operation::EndpointReceive, EP),
    ),
    Step::new(
        "endpoint/attenuate-the-badged-sender",
        cnode(Operation::CapAttenuate, [EP_BADGED as u64, NO_RIGHTS, 0, 0]),
    ),
    Step::new(
        "endpoint/badged-sender-is-now-powerless",
        Request::with(Operation::EndpointSend, EP_BADGED, [0, 7, 0, 0]),
    ),
    // -- Synchronous paths never hang; they answer ---------------------------
    Step::new(
        "endpoint/call-without-a-server",
        Request::new(Operation::EndpointCall, EP),
    ),
    Step::new(
        "endpoint/reply-without-a-call",
        Request::new(Operation::EndpointReply, EP),
    ),
    // -- Notifications coalesce; badges accumulate ---------------------------
    Step::new(
        "notification/poll-when-idle",
        Request::new(Operation::NotificationPoll, NOTE),
    ),
    Step::new(
        "notification/wait-when-idle",
        Request::new(Operation::NotificationWait, NOTE),
    ),
    Step::new(
        "notification/mint-badged-signaller",
        cnode(
            Operation::CapMint,
            [NOTE as u64, NOTE_BADGED as u64, SEND, 0x8],
        ),
    ),
    Step::new(
        "notification/signal-base",
        Request::new(Operation::NotificationSignal, NOTE),
    ),
    Step::new(
        "notification/signal-badged",
        Request::new(Operation::NotificationSignal, NOTE_BADGED),
    ),
    Step::new(
        "notification/signal-base-again",
        Request::new(Operation::NotificationSignal, NOTE),
    ),
    Step::new(
        "notification/three-signals-one-delivery",
        Request::new(Operation::NotificationPoll, NOTE),
    ),
    Step::new(
        "notification/poll-after-delivery",
        Request::new(Operation::NotificationPoll, NOTE),
    ),
    // -- Revocation removes descendants and fails stale holders closed ------
    Step::new(
        "revoke/notification-descendants",
        cnode(Operation::CapRevoke, [NOTE as u64, 0, 0, 0]),
    ),
    Step::new(
        "revoke/descendant-fails-closed",
        Request::new(Operation::NotificationSignal, NOTE_BADGED),
    ),
    Step::new(
        "revoke/the-revoked-capability-survives",
        Request::new(Operation::NotificationSignal, NOTE),
    ),
    Step::new(
        "revoke/build-a-chain-parent",
        cnode(
            Operation::CapMint,
            [EP as u64, EP_CHAIN_PARENT as u64, SEND, 0x1],
        ),
    ),
    Step::new(
        "revoke/build-a-chain-child",
        cnode(
            Operation::CapMint,
            [EP_CHAIN_PARENT as u64, EP_CHAIN_CHILD as u64, SEND, 0x1],
        ),
    ),
    Step::new(
        "revoke/endpoint-descendants-transitively",
        cnode(Operation::CapRevoke, [EP as u64, 0, 0, 0]),
    ),
    Step::new(
        "revoke/grandchild-fails-closed",
        Request::with(Operation::EndpointSend, EP_CHAIN_CHILD, [0, 1, 0, 0]),
    ),
    Step::new(
        "revoke/plain-copy-fails-closed",
        Request::with(Operation::EndpointSend, EP_COPY, [0, 1, 0, 0]),
    ),
    Step::new(
        "revoke/the-revoked-endpoint-survives",
        Request::with(Operation::EndpointSend, EP, [0, 1, 0, 0]),
    ),
    Step::new(
        "revoke/a-tombstoned-slot-can-be-refilled",
        cnode(Operation::CapCopy, [EP as u64, EP_COPY as u64, 0, 0]),
    ),
    // -- The clock is monotonic and rights-checked --------------------------
    Step::new(
        "clock/first-read",
        Request::new(Operation::ClockMonotonicNow, CLOCK),
    ),
    Step::new(
        "clock/second-read",
        Request::new(Operation::ClockMonotonicNow, CLOCK),
    ),
    Step::new(
        "clock/third-read",
        Request::new(Operation::ClockMonotonicNow, CLOCK),
    ),
    Step::new(
        "clock/mint-without-read",
        cnode(
            Operation::CapMint,
            [CLOCK as u64, CLOCK_POWERLESS as u64, NO_RIGHTS, 0],
        ),
    ),
    Step::new(
        "clock/powerless-handle-cannot-read",
        Request::new(Operation::ClockMonotonicNow, CLOCK_POWERLESS),
    ),
    // -- The capability space can weaken itself, and cannot restore itself --
    Step::new(
        "cnode/attenuate-itself",
        cnode(Operation::CapAttenuate, [CNODE as u64, NO_RIGHTS, 0, 0]),
    ),
    Step::new(
        "cnode/derivation-is-now-denied",
        cnode(Operation::CapCopy, [EP as u64, 20, 0, 0]),
    ),
    Step::new(
        "cnode/there-is-no-way-back",
        cnode(Operation::CapAttenuate, [CNODE as u64, CONTROL, 0, 0]),
    ),
    Step::new(
        "cnode/revocation-is-now-denied",
        cnode(Operation::CapRevoke, [EP as u64, 0, 0, 0]),
    ),
];

/// Reset `kernel` to the conformance domain and invoke every corpus step,
/// handing each request and its answer to `visit`.
pub fn walk<K: Kernel>(kernel: &mut K, mut visit: impl FnMut(usize, &Step, &Response)) {
    kernel.reset_to_conformance_domain();
    for (index, step) in CORPUS.iter().enumerate() {
        let response = kernel.invoke(&step.request);
        visit(index, step, &response);
    }
}

/// Render `kernel`'s answers as the canonical transcript.
///
/// Two backends are contract-equivalent when their transcripts are identical
/// byte for byte. This is the same comparison discipline the Common Lisp
/// bootstrap reference uses for the language kernel.
pub fn transcribe<K: Kernel>(kernel: &mut K, out: &mut impl fmt::Write) -> fmt::Result {
    let mut result = Ok(());
    walk(kernel, |_, step, response| {
        if result.is_ok() {
            result = write_step(out, step.label, &step.request, response);
        }
    });
    result
}

/// Run the corpus against two backends and report the first divergence.
///
/// On success, returns the number of steps both backends agreed on.
pub fn compare<A: Kernel, B: Kernel>(left: &mut A, right: &mut B) -> Result<usize, Divergence> {
    left.reset_to_conformance_domain();
    right.reset_to_conformance_domain();
    for (index, step) in CORPUS.iter().enumerate() {
        let left_response = left.invoke(&step.request);
        let right_response = right.invoke(&step.request);
        if left_response != right_response {
            return Err(Divergence {
                index,
                label: step.label,
                left: left_response,
                right: right_response,
            });
        }
    }
    Ok(CORPUS.len())
}

/// Assert the properties the corpus exists to pin down, independently of the
/// transcript's exact bytes. A golden file catches drift; these catch a
/// backend that drifted in the same direction as the model.
pub fn check_invariants<K: Kernel>(kernel: &mut K) -> Result<(), &'static str> {
    let mut failure = None;
    walk(kernel, |_, step, response| {
        if failure.is_some() {
            return;
        }
        failure = invariant_failure(step, response);
    });
    match failure {
        Some(message) => Err(message),
        None => Ok(()),
    }
}

fn invariant_failure(step: &Step, response: &Response) -> Option<&'static str> {
    if response.status != Status::Ok && response.values != [0; crate::WORDS] {
        return Some("a failing response carried result words");
    }
    match step.label {
        "boot/info" => {
            if response.values[3] & group::V1_PROFILE != group::V1_PROFILE {
                return Some("backend does not publish the v1.0 profile");
            }
            if response.values[0] != crate::version_word() {
                return Some("backend reports a different contract version");
            }
        }
        "derive/mint-cannot-widen" | "attenuate/cannot-widen" => {
            if response.status != Status::InsufficientRights {
                return Some("authority was widened by derivation");
            }
        }
        "endpoint/send-5-is-backpressure" => {
            if response.status != Status::QueueFull {
                return Some("a bounded queue did not push back at capacity");
            }
        }
        "revoke/descendant-fails-closed"
        | "revoke/grandchild-fails-closed"
        | "revoke/plain-copy-fails-closed" => {
            if response.status != Status::Revoked {
                return Some("a revoked handle did not fail closed");
            }
        }
        "cnode/derivation-is-now-denied" | "cnode/there-is-no-way-back" => {
            if response.status != Status::InsufficientRights {
                return Some("a domain restored authority it had given up");
            }
        }
        "capability/empty-slot" | "capability/out-of-range" | "capability/null-slot" => {
            match response.status {
                Status::InvalidCapability => {}
                _ => return Some("naming a slot granted authority"),
            }
        }
        _ => {}
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ModelKernel;

    #[test]
    fn the_reference_model_satisfies_its_own_corpus() {
        check_invariants(&mut ModelKernel::new()).expect("reference model is conformant");
    }

    #[test]
    fn the_model_is_deterministic_across_resets() {
        let compared = compare(&mut ModelKernel::new(), &mut ModelKernel::new())
            .expect("two fresh model kernels agree");
        assert_eq!(compared, CORPUS.len());
    }

    #[test]
    fn step_labels_are_unique() {
        for (index, step) in CORPUS.iter().enumerate() {
            for other in &CORPUS[index + 1..] {
                assert_ne!(step.label, other.label, "duplicate corpus label");
            }
        }
    }

    #[test]
    fn the_corpus_reaches_every_v1_profile_operation() {
        for operation in [
            Operation::Nop,
            Operation::BootInfo,
            Operation::CapCopy,
            Operation::CapMint,
            Operation::CapAttenuate,
            Operation::CapMove,
            Operation::CapRevoke,
            Operation::EndpointSend,
            Operation::EndpointReceive,
            Operation::EndpointCall,
            Operation::EndpointReply,
            Operation::NotificationSignal,
            Operation::NotificationWait,
            Operation::NotificationPoll,
            Operation::ClockMonotonicNow,
        ] {
            assert!(
                CORPUS
                    .iter()
                    .any(|step| step.request.operation == operation),
                "corpus never exercises {}",
                operation.name()
            );
        }
    }
}
