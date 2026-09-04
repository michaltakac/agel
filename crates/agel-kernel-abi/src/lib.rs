//! The Agel kernel contract: one small, versioned semantic boundary that every
//! native backend must implement identically.
//!
//! This crate deliberately contains no policy, no allocator, and no language
//! concepts. The contract transports bounded words, notifications, and opaque
//! capability handles. S-expressions, garbage collection, strings, filesystem
//! paths, model tokens, and network policy live above it, never in it.
//!
//! The crate is `no_std` so that the freestanding research kernel and a future
//! seL4/Microkit backend can link exactly the same definitions and the same
//! conformance corpus that the hosted reference model runs. A backend is
//! conformant when [`conformance::compare`] finds no divergence against the
//! reference model and its [`conformance::transcribe`] output is byte-identical
//! to the frozen transcript in `bootstrap/kernel-contract.trace`.
//!
//! Nothing here is a security claim. The contract states what a backend must
//! *answer*; whether the backend enforces those answers with hardware
//! protection domains is a property of the backend, recorded in
//! `docs/kernel-contract.md`.

#![no_std]
#![forbid(unsafe_code)]

pub mod conformance;
pub mod model;

use core::fmt;

/// Contract version. A backend reports this through [`Operation::BootInfo`].
///
/// Major changes remove or redefine an operation, a status value, or the
/// meaning of an argument. Minor changes add operations or add fields to a
/// reserved position. Patch changes clarify wording without changing bytes.
pub const VERSION_MAJOR: u16 = 1;
/// See [`VERSION_MAJOR`].
pub const VERSION_MINOR: u16 = 0;
/// See [`VERSION_MAJOR`].
pub const VERSION_PATCH: u16 = 0;

/// The packed version word reported by [`Operation::BootInfo`].
pub const fn version_word() -> u64 {
    ((VERSION_MAJOR as u64) << 32) | ((VERSION_MINOR as u64) << 16) | VERSION_PATCH as u64
}

/// Number of capability slots in a conformance domain's capability space.
///
/// This is a property of the *conformance harness*, not a limit of the
/// contract. Real domains are sized by their system manifest.
pub const CONFORMANCE_SLOTS: u32 = 32;

/// Capacity of the conformance endpoint's bounded message queue.
///
/// Every queue in the contract is bounded and every backend must report
/// [`Status::QueueFull`] rather than growing. Four is small enough that a
/// conformance trace can actually reach the backpressure edge.
pub const CONFORMANCE_ENDPOINT_CAPACITY: u64 = 4;

/// Fixed on-wire size of a [`Request`] or a [`Response`], in bytes.
pub const FRAME_BYTES: usize = 40;

/// Number of argument or result words carried by one frame.
pub const WORDS: usize = 4;

// ---------------------------------------------------------------------------
// Object types
// ---------------------------------------------------------------------------

/// The kind of kernel object a capability slot designates.
///
/// A slot's type is not advisory. Invoking an operation defined for a different
/// object type is [`Status::WrongObjectType`], never a coincidentally similar
/// action on the wrong object.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectType {
    /// An empty slot. Never designates authority.
    Null,
    /// The capability space itself; required to derive, move, or revoke.
    CNode,
    /// A protection domain: an address space, a capability space, and threads.
    ProtectionDomain,
    /// A virtual address space that frames are mapped into.
    AddressSpace,
    /// A schedulable execution context inside a protection domain.
    Thread,
    /// A bounded synchronous call/reply and asynchronous send/receive port.
    Endpoint,
    /// A coalescing binary signal carrying a badge word.
    Notification,
    /// A page of physical memory that can be mapped, shared, and reclaimed.
    Frame,
    /// A hardware interrupt source that can be bound, acknowledged, and masked.
    Interrupt,
    /// An MCS-style CPU reservation: budget, period, and priority.
    ScheduleContext,
    /// A monotonic time source and deadline service.
    Clock,
}

impl ObjectType {
    /// The canonical name used in rendered conformance transcripts.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Null => "null",
            Self::CNode => "cnode",
            Self::ProtectionDomain => "protection-domain",
            Self::AddressSpace => "address-space",
            Self::Thread => "thread",
            Self::Endpoint => "endpoint",
            Self::Notification => "notification",
            Self::Frame => "frame",
            Self::Interrupt => "interrupt",
            Self::ScheduleContext => "schedule-context",
            Self::Clock => "clock",
        }
    }

    /// The stable wire code for this object type.
    pub const fn code(self) -> u16 {
        match self {
            Self::Null => 0,
            Self::CNode => 1,
            Self::ProtectionDomain => 2,
            Self::AddressSpace => 3,
            Self::Thread => 4,
            Self::Endpoint => 5,
            Self::Notification => 6,
            Self::Frame => 7,
            Self::Interrupt => 8,
            Self::ScheduleContext => 9,
            Self::Clock => 10,
        }
    }
}

// ---------------------------------------------------------------------------
// Rights
// ---------------------------------------------------------------------------

/// Rights carried by a capability.
///
/// Rights are the only thing that makes a handle authority. A derived
/// capability may never hold a right its parent lacked; see
/// [`Rights::is_attenuation_of`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Rights(pub u32);

impl Rights {
    /// No authority at all.
    pub const NONE: Self = Self(0);
    /// Read the object's contents.
    pub const READ: Self = Self(1 << 0);
    /// Modify the object's contents.
    pub const WRITE: Self = Self(1 << 1);
    /// Execute from a mapped frame.
    pub const EXECUTE: Self = Self(1 << 2);
    /// Send a message or signal.
    pub const SEND: Self = Self(1 << 3);
    /// Receive a message or wait for a signal.
    pub const RECEIVE: Self = Self(1 << 4);
    /// Transfer another capability inside a message.
    pub const GRANT: Self = Self(1 << 5);
    /// Administer the object: derive, revoke, configure, reap.
    pub const CONTROL: Self = Self(1 << 6);

    /// Every right this contract version defines.
    pub const ALL: Self = Self(0b111_1111);

    /// True when `self` holds every right in `other`.
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Rights held by both.
    pub const fn intersect(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    /// True when `self` is a legal derivation of `parent`: equal or weaker.
    ///
    /// This is the monotonicity rule that makes "no agent, model, compiler, or
    /// macro can mint its own authority" mechanical rather than aspirational.
    pub const fn is_attenuation_of(self, parent: Self) -> bool {
        parent.contains(self)
    }
}

// ---------------------------------------------------------------------------
// Operations
// ---------------------------------------------------------------------------

/// Every operation in the contract.
///
/// The numeric codes are stable. A backend that has not implemented an
/// operation must answer [`Status::InvalidOperation`]; silence, a hang, or a
/// plausible-looking wrong answer is a conformance failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    /// Probe: succeeds on every conformant backend and returns the version word.
    Nop,

    /// Allocate a protection domain from untyped authority.
    PdCreate,
    /// Make a protection domain's threads eligible to run.
    PdStart,
    /// Make a protection domain's threads ineligible to run.
    PdStop,
    /// Read the fault record that stopped a protection domain.
    PdFault,
    /// Destroy a protection domain and reclaim everything charged to it.
    PdReap,

    /// Map a frame into an address space with explicit rights.
    AsMap,
    /// Remove a mapping.
    AsUnmap,
    /// Change an existing mapping's rights without remapping it.
    AsProtect,
    /// Report what is mapped at an address.
    AsQuery,

    /// Set a thread's entry point, stack, and scheduling context.
    ThreadConfigure,
    /// Make a configured thread runnable.
    ThreadResume,
    /// Make a runnable thread not runnable.
    ThreadSuspend,

    /// Bounded synchronous call: send and block for the reply.
    EndpointCall,
    /// Answer the outstanding call.
    EndpointReply,
    /// Enqueue a message without blocking; fails with `QueueFull`.
    EndpointSend,
    /// Dequeue a message without blocking; fails with `WouldBlock`.
    EndpointReceive,

    /// Raise the notification and OR in a badge.
    NotificationSignal,
    /// Block until the notification is raised, then consume it.
    NotificationWait,
    /// Consume the notification if raised; never blocks.
    NotificationPoll,

    /// Duplicate a capability into an empty slot with the same rights.
    CapCopy,
    /// Duplicate a capability with a badge and equal-or-weaker rights.
    CapMint,
    /// Reduce a capability's rights in place.
    CapAttenuate,
    /// Transfer a capability to another slot, emptying the source.
    CapMove,
    /// Invalidate a capability and every capability derived from it.
    CapRevoke,

    /// Obtain a frame of memory charged to the caller.
    FrameAllocate,
    /// Map a frame; the address-space-relative form of [`Operation::AsMap`].
    FrameMap,
    /// Derive a frame capability for another domain.
    FrameShare,
    /// Return a frame's memory to the allocator.
    FrameReclaim,

    /// Route a hardware interrupt to a notification.
    IrqBind,
    /// Acknowledge a delivered interrupt so the source can fire again.
    IrqAck,
    /// Mask or unmask an interrupt source.
    IrqMask,

    /// Set a scheduling context's execution budget.
    SchedBudget,
    /// Set a scheduling context's replenishment period.
    SchedPeriod,
    /// Set a scheduling context's priority.
    SchedPriority,
    /// Attach a scheduling context to a thread.
    SchedBind,
    /// Detach a scheduling context from a thread.
    SchedUnbind,

    /// Read the monotonic clock.
    ClockMonotonicNow,
    /// Arm a deadline against the monotonic clock.
    ClockDeadline,

    /// Describe the domain's initial capabilities, bounds, and platform.
    BootInfo,
}

impl Operation {
    /// The stable wire code for this operation.
    pub const fn code(self) -> u16 {
        match self {
            Self::Nop => 0x0000,

            Self::PdCreate => 0x0100,
            Self::PdStart => 0x0101,
            Self::PdStop => 0x0102,
            Self::PdFault => 0x0103,
            Self::PdReap => 0x0104,

            Self::AsMap => 0x0200,
            Self::AsUnmap => 0x0201,
            Self::AsProtect => 0x0202,
            Self::AsQuery => 0x0203,

            Self::ThreadConfigure => 0x0300,
            Self::ThreadResume => 0x0301,
            Self::ThreadSuspend => 0x0302,

            Self::EndpointCall => 0x0400,
            Self::EndpointReply => 0x0401,
            Self::EndpointSend => 0x0402,
            Self::EndpointReceive => 0x0403,

            Self::NotificationSignal => 0x0500,
            Self::NotificationWait => 0x0501,
            Self::NotificationPoll => 0x0502,

            Self::CapCopy => 0x0600,
            Self::CapMint => 0x0601,
            Self::CapAttenuate => 0x0602,
            Self::CapMove => 0x0603,
            Self::CapRevoke => 0x0604,

            Self::FrameAllocate => 0x0700,
            Self::FrameMap => 0x0701,
            Self::FrameShare => 0x0702,
            Self::FrameReclaim => 0x0703,

            Self::IrqBind => 0x0800,
            Self::IrqAck => 0x0801,
            Self::IrqMask => 0x0802,

            Self::SchedBudget => 0x0900,
            Self::SchedPeriod => 0x0901,
            Self::SchedPriority => 0x0902,
            Self::SchedBind => 0x0903,
            Self::SchedUnbind => 0x0904,

            Self::ClockMonotonicNow => 0x0a00,
            Self::ClockDeadline => 0x0a01,

            Self::BootInfo => 0x0b00,
        }
    }

    /// The canonical name used in rendered conformance transcripts.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Nop => "nop",

            Self::PdCreate => "pd.create",
            Self::PdStart => "pd.start",
            Self::PdStop => "pd.stop",
            Self::PdFault => "pd.fault",
            Self::PdReap => "pd.reap",

            Self::AsMap => "as.map",
            Self::AsUnmap => "as.unmap",
            Self::AsProtect => "as.protect",
            Self::AsQuery => "as.query",

            Self::ThreadConfigure => "thread.configure",
            Self::ThreadResume => "thread.resume",
            Self::ThreadSuspend => "thread.suspend",

            Self::EndpointCall => "endpoint.call",
            Self::EndpointReply => "endpoint.reply",
            Self::EndpointSend => "endpoint.send",
            Self::EndpointReceive => "endpoint.receive",

            Self::NotificationSignal => "notification.signal",
            Self::NotificationWait => "notification.wait",
            Self::NotificationPoll => "notification.poll",

            Self::CapCopy => "cap.copy",
            Self::CapMint => "cap.mint",
            Self::CapAttenuate => "cap.attenuate",
            Self::CapMove => "cap.move",
            Self::CapRevoke => "cap.revoke",

            Self::FrameAllocate => "frame.allocate",
            Self::FrameMap => "frame.map",
            Self::FrameShare => "frame.share",
            Self::FrameReclaim => "frame.reclaim",

            Self::IrqBind => "irq.bind",
            Self::IrqAck => "irq.ack",
            Self::IrqMask => "irq.mask",

            Self::SchedBudget => "sched.budget",
            Self::SchedPeriod => "sched.period",
            Self::SchedPriority => "sched.priority",
            Self::SchedBind => "sched.bind",
            Self::SchedUnbind => "sched.unbind",

            Self::ClockMonotonicNow => "clock.monotonic-now",
            Self::ClockDeadline => "clock.deadline",

            Self::BootInfo => "boot.info",
        }
    }

    /// Every operation, in wire-code order. Used to check code uniqueness and
    /// to let a backend enumerate what it must answer for.
    pub const ALL: &'static [Self] = &[
        Self::Nop,
        Self::PdCreate,
        Self::PdStart,
        Self::PdStop,
        Self::PdFault,
        Self::PdReap,
        Self::AsMap,
        Self::AsUnmap,
        Self::AsProtect,
        Self::AsQuery,
        Self::ThreadConfigure,
        Self::ThreadResume,
        Self::ThreadSuspend,
        Self::EndpointCall,
        Self::EndpointReply,
        Self::EndpointSend,
        Self::EndpointReceive,
        Self::NotificationSignal,
        Self::NotificationWait,
        Self::NotificationPoll,
        Self::CapCopy,
        Self::CapMint,
        Self::CapAttenuate,
        Self::CapMove,
        Self::CapRevoke,
        Self::FrameAllocate,
        Self::FrameMap,
        Self::FrameShare,
        Self::FrameReclaim,
        Self::IrqBind,
        Self::IrqAck,
        Self::IrqMask,
        Self::SchedBudget,
        Self::SchedPeriod,
        Self::SchedPriority,
        Self::SchedBind,
        Self::SchedUnbind,
        Self::ClockMonotonicNow,
        Self::ClockDeadline,
        Self::BootInfo,
    ];

    /// Resolve a wire code back to an operation.
    pub fn from_code(code: u16) -> Option<Self> {
        Self::ALL.iter().copied().find(|op| op.code() == code)
    }
}

// ---------------------------------------------------------------------------
// Status
// ---------------------------------------------------------------------------

/// The canonical reply status of every invocation.
///
/// The set is closed. A backend must not invent a status, and must not collapse
/// distinct failures into one: the difference between "you do not hold that
/// right" and "policy forbids this" is exactly what an audit needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// The operation completed. Result words are defined per operation.
    Ok,
    /// The operation code is not defined, or not implemented by this backend.
    InvalidOperation,
    /// The slot is out of range or empty. Naming an object is never authority.
    InvalidCapability,
    /// The slot holds an object of a type this operation is not defined for.
    WrongObjectType,
    /// The capability lacks a right this operation requires.
    InsufficientRights,
    /// An argument word is malformed, reserved, or out of range.
    InvalidArgument,
    /// A non-blocking operation found nothing to do.
    WouldBlock,
    /// A bounded queue is at capacity. This is backpressure, not an error path
    /// a caller may retry indefinitely without policy.
    QueueFull,
    /// The named subject does not exist.
    NotFound,
    /// The destination slot is already occupied.
    AlreadyExists,
    /// The capability was invalidated by a revocation of an ancestor.
    Revoked,
    /// The server restarted; this handle's generation is stale and fails closed.
    StaleGeneration,
    /// The scheduling context has no budget remaining this period.
    BudgetExhausted,
    /// A fixed resource bound of this backend is exhausted.
    ResourceExhausted,
    /// The target protection domain is stopped by an unhandled fault.
    FaultedDomain,
    /// Rights were sufficient but policy denied the operation.
    NotPermitted,
}

impl Status {
    /// The stable wire code for this status.
    pub const fn code(self) -> u16 {
        match self {
            Self::Ok => 0,
            Self::InvalidOperation => 1,
            Self::InvalidCapability => 2,
            Self::WrongObjectType => 3,
            Self::InsufficientRights => 4,
            Self::InvalidArgument => 5,
            Self::WouldBlock => 6,
            Self::QueueFull => 7,
            Self::NotFound => 8,
            Self::AlreadyExists => 9,
            Self::Revoked => 10,
            Self::StaleGeneration => 11,
            Self::BudgetExhausted => 12,
            Self::ResourceExhausted => 13,
            Self::FaultedDomain => 14,
            Self::NotPermitted => 15,
        }
    }

    /// The canonical name used in rendered conformance transcripts.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::InvalidOperation => "invalid-operation",
            Self::InvalidCapability => "invalid-capability",
            Self::WrongObjectType => "wrong-object-type",
            Self::InsufficientRights => "insufficient-rights",
            Self::InvalidArgument => "invalid-argument",
            Self::WouldBlock => "would-block",
            Self::QueueFull => "queue-full",
            Self::NotFound => "not-found",
            Self::AlreadyExists => "already-exists",
            Self::Revoked => "revoked",
            Self::StaleGeneration => "stale-generation",
            Self::BudgetExhausted => "budget-exhausted",
            Self::ResourceExhausted => "resource-exhausted",
            Self::FaultedDomain => "faulted-domain",
            Self::NotPermitted => "not-permitted",
        }
    }

    /// Every status, in wire-code order.
    pub const ALL: &'static [Self] = &[
        Self::Ok,
        Self::InvalidOperation,
        Self::InvalidCapability,
        Self::WrongObjectType,
        Self::InsufficientRights,
        Self::InvalidArgument,
        Self::WouldBlock,
        Self::QueueFull,
        Self::NotFound,
        Self::AlreadyExists,
        Self::Revoked,
        Self::StaleGeneration,
        Self::BudgetExhausted,
        Self::ResourceExhausted,
        Self::FaultedDomain,
        Self::NotPermitted,
    ];

    /// Resolve a wire code back to a status.
    pub fn from_code(code: u16) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|status| status.code() == code)
    }
}

// ---------------------------------------------------------------------------
// Frames
// ---------------------------------------------------------------------------

/// One invocation: an operation, the capability slot it acts on, and four
/// bounded argument words.
///
/// There is no pointer, no length-prefixed buffer, and no variable-size payload
/// anywhere in this structure. Bulk data moves through shared frames signalled
/// by notifications, never through the control path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Request {
    /// The operation to perform.
    pub operation: Operation,
    /// The capability slot the operation acts on.
    pub capability: u32,
    /// Bounded argument words, defined per operation.
    pub arguments: [u64; WORDS],
}

impl Request {
    /// An invocation with no arguments.
    pub const fn new(operation: Operation, capability: u32) -> Self {
        Self {
            operation,
            capability,
            arguments: [0; WORDS],
        }
    }

    /// An invocation with explicit argument words.
    pub const fn with(operation: Operation, capability: u32, arguments: [u64; WORDS]) -> Self {
        Self {
            operation,
            capability,
            arguments,
        }
    }

    /// Encode to the canonical fixed-size big-endian frame.
    pub fn encode(&self) -> [u8; FRAME_BYTES] {
        let mut frame = [0_u8; FRAME_BYTES];
        frame[0..2].copy_from_slice(&self.operation.code().to_be_bytes());
        frame[2..4].copy_from_slice(&0_u16.to_be_bytes());
        frame[4..8].copy_from_slice(&self.capability.to_be_bytes());
        for (index, word) in self.arguments.iter().enumerate() {
            let start = 8 + index * 8;
            frame[start..start + 8].copy_from_slice(&word.to_be_bytes());
        }
        frame
    }

    /// Decode a canonical frame. Unknown operation codes and non-zero reserved
    /// bytes are rejected rather than ignored.
    pub fn decode(frame: &[u8; FRAME_BYTES]) -> Option<Self> {
        let operation = Operation::from_code(u16::from_be_bytes([frame[0], frame[1]]))?;
        if u16::from_be_bytes([frame[2], frame[3]]) != 0 {
            return None;
        }
        let capability = u32::from_be_bytes([frame[4], frame[5], frame[6], frame[7]]);
        let mut arguments = [0_u64; WORDS];
        for (index, word) in arguments.iter_mut().enumerate() {
            let start = 8 + index * 8;
            let mut bytes = [0_u8; 8];
            bytes.copy_from_slice(&frame[start..start + 8]);
            *word = u64::from_be_bytes(bytes);
        }
        Some(Self {
            operation,
            capability,
            arguments,
        })
    }
}

/// One reply: a canonical status and four bounded result words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Response {
    /// The canonical outcome.
    pub status: Status,
    /// Result words, defined per operation. Always zero unless `status` is
    /// [`Status::Ok`], so a failing reply can never smuggle data.
    pub values: [u64; WORDS],
}

impl Response {
    /// A successful reply with no result words.
    pub const OK: Self = Self {
        status: Status::Ok,
        values: [0; WORDS],
    };

    /// A successful reply carrying result words.
    pub const fn ok(values: [u64; WORDS]) -> Self {
        Self {
            status: Status::Ok,
            values,
        }
    }

    /// A successful reply carrying one result word.
    pub const fn ok1(value: u64) -> Self {
        Self::ok([value, 0, 0, 0])
    }

    /// A failing reply. Result words are forced to zero.
    pub const fn fail(status: Status) -> Self {
        Self {
            status,
            values: [0; WORDS],
        }
    }

    /// Encode to the canonical fixed-size big-endian frame.
    pub fn encode(&self) -> [u8; FRAME_BYTES] {
        let mut frame = [0_u8; FRAME_BYTES];
        frame[0..2].copy_from_slice(&self.status.code().to_be_bytes());
        for (index, word) in self.values.iter().enumerate() {
            let start = 8 + index * 8;
            frame[start..start + 8].copy_from_slice(&word.to_be_bytes());
        }
        frame
    }
}

// ---------------------------------------------------------------------------
// Backend trait
// ---------------------------------------------------------------------------

/// A backend that answers the Agel kernel contract.
///
/// Both the hosted reference model and the freestanding research kernel
/// implement this trait, and the same [`conformance::CORPUS`] runs against
/// both. A future seL4/Microkit backend implements it by translating to seL4
/// invocations.
pub trait Kernel {
    /// Answer one invocation. Implementations must be total: every request
    /// produces a [`Response`], including [`Status::InvalidOperation`] for
    /// operations the backend has not implemented.
    fn invoke(&mut self, request: &Request) -> Response;

    /// Reset to the conformance domain described in `docs/kernel-contract.md`,
    /// so a corpus run starts from a specified state on every backend.
    fn reset_to_conformance_domain(&mut self);
}

// ---------------------------------------------------------------------------
// Canonical rendering
// ---------------------------------------------------------------------------

/// Render one request/response pair as one canonical transcript line.
///
/// The rendering is the cross-implementation comparison surface, exactly as the
/// canonical value printer is for the Common Lisp bootstrap comparison. Two
/// backends agree when their transcripts are byte-identical.
pub fn write_step(
    out: &mut impl fmt::Write,
    label: &str,
    request: &Request,
    response: &Response,
) -> fmt::Result {
    write!(
        out,
        "{label}: {}(cap={}",
        request.operation.name(),
        request.capability
    )?;
    for word in &request.arguments {
        write!(out, " {word:#x}")?;
    }
    write!(out, ") -> {}", response.status.name())?;
    for value in &response.values {
        write!(out, " {value:#x}")?;
    }
    writeln!(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operation_codes_are_unique_and_round_trip() {
        for (index, operation) in Operation::ALL.iter().enumerate() {
            assert_eq!(Operation::from_code(operation.code()), Some(*operation));
            for other in &Operation::ALL[index + 1..] {
                assert_ne!(operation.code(), other.code(), "duplicate operation code");
                assert_ne!(operation.name(), other.name(), "duplicate operation name");
            }
        }
    }

    #[test]
    fn status_codes_are_unique_and_round_trip() {
        for (index, status) in Status::ALL.iter().enumerate() {
            assert_eq!(Status::from_code(status.code()), Some(*status));
            for other in &Status::ALL[index + 1..] {
                assert_ne!(status.code(), other.code(), "duplicate status code");
                assert_ne!(status.name(), other.name(), "duplicate status name");
            }
        }
    }

    #[test]
    fn requests_round_trip_through_the_canonical_frame() {
        let request = Request::with(Operation::CapMint, 7, [1, 2, 3, 4]);
        assert_eq!(Request::decode(&request.encode()), Some(request));
    }

    #[test]
    fn unknown_operations_and_reserved_bytes_are_rejected() {
        let mut frame = Request::new(Operation::Nop, 0).encode();
        frame[0] = 0xff;
        frame[1] = 0xff;
        assert_eq!(Request::decode(&frame), None);

        let mut frame = Request::new(Operation::Nop, 0).encode();
        frame[3] = 1;
        assert_eq!(Request::decode(&frame), None);
    }

    #[test]
    fn a_failing_response_carries_no_data() {
        let response = Response::fail(Status::InsufficientRights);
        assert_eq!(response.values, [0; WORDS]);
        assert_eq!(response.encode()[8..], [0; FRAME_BYTES - 8]);
    }

    #[test]
    fn rights_derivation_is_monotonic() {
        let parent = Rights(Rights::SEND.0 | Rights::RECEIVE.0);
        assert!(Rights::SEND.is_attenuation_of(parent));
        assert!(parent.is_attenuation_of(parent));
        assert!(!Rights::GRANT.is_attenuation_of(parent));
        assert!(!Rights::ALL.is_attenuation_of(parent));
    }

    #[test]
    fn version_word_is_packed_as_documented() {
        assert_eq!(version_word(), (1_u64 << 32));
    }
}
