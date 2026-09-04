//! The executable reference model of the Agel kernel contract.
//!
//! This is the specification in runnable form, not a kernel. It has no
//! protection domains, no hardware, and no isolation: it exists so that the
//! conformance corpus has an authoritative answer to compare a real backend
//! against, and so that a contract change is a change to code that fails tests
//! rather than a change to prose that nobody re-reads.
//!
//! It is `no_std` and allocation-free for the same reason the corpus is: the
//! freestanding research kernel links it to check itself against the model
//! inside QEMU, on the same fixed memory budget as the rest of that image.

use crate::{
    version_word, Kernel, ObjectType, Operation, Request, Response, Rights, Status,
    CONFORMANCE_ENDPOINT_CAPACITY, CONFORMANCE_SLOTS, WORDS,
};

/// Slots in the modelled capability space.
const SLOTS: usize = CONFORMANCE_SLOTS as usize;
/// Bounded endpoint queue capacity.
const QUEUE: usize = CONFORMANCE_ENDPOINT_CAPACITY as usize;
/// Words carried by one endpoint message. One request word is reserved for
/// flags, so a message is three words plus the sending capability's badge.
const MESSAGE_WORDS: usize = WORDS - 1;

/// Operation groups a backend may implement, reported by
/// [`Operation::BootInfo`] so that "not implemented" is a published fact rather
/// than something a caller discovers by being refused.
pub mod group {
    /// `nop` and `boot.info`.
    pub const CORE: u64 = 1 << 0;
    /// `cap.copy`, `cap.mint`, `cap.attenuate`, `cap.move`, `cap.revoke`.
    pub const CAPABILITY: u64 = 1 << 1;
    /// `endpoint.*`.
    pub const ENDPOINT: u64 = 1 << 2;
    /// `notification.*`.
    pub const NOTIFICATION: u64 = 1 << 3;
    /// `clock.*`.
    pub const CLOCK: u64 = 1 << 4;
    /// `frame.*` and `as.*`.
    pub const MEMORY: u64 = 1 << 5;
    /// `pd.*`, `thread.*`, `sched.*`.
    pub const DOMAIN: u64 = 1 << 6;
    /// `irq.*`.
    pub const INTERRUPT: u64 = 1 << 7;

    /// The groups every v1.0-conformant backend must implement.
    pub const V1_PROFILE: u64 = CORE | CAPABILITY | ENDPOINT | NOTIFICATION | CLOCK;
}

/// Well-known slots of the conformance domain. A backend constructs exactly
/// this capability space in [`Kernel::reset_to_conformance_domain`].
pub mod slot {
    /// Always empty. Naming it is never authority.
    pub const NULL: u32 = 0;
    /// The domain's own capability space; required to derive, move, or revoke.
    pub const CNODE: u32 = 1;
    /// A bounded endpoint with `send`, `receive`, and `grant`.
    pub const ENDPOINT: u32 = 2;
    /// A notification with `send` and `receive`, badge `0x1`.
    pub const NOTIFICATION: u32 = 3;
    /// A frame with `read` and `write` but deliberately no `execute`.
    pub const FRAME: u32 = 4;
    /// A monotonic clock with `read`.
    pub const CLOCK: u32 = 5;
    /// The first slot left empty for the corpus to derive into.
    pub const FIRST_FREE: u32 = 6;
}

#[derive(Clone, Copy)]
struct Slot {
    kind: ObjectType,
    target: u8,
    rights: Rights,
    badge: u64,
    /// Derivation identity. Zero means the slot is empty.
    id: u32,
    /// Identity of the capability this one was derived from; zero when root.
    parent: u32,
    /// Set when an ancestor was revoked, so a stale handle fails closed with
    /// [`Status::Revoked`] instead of looking like a caller mistake.
    revoked: bool,
}

impl Slot {
    const EMPTY: Self = Self {
        kind: ObjectType::Null,
        target: 0,
        rights: Rights::NONE,
        badge: 0,
        id: 0,
        parent: 0,
        revoked: false,
    };

    const fn is_empty(&self) -> bool {
        self.id == 0
    }
}

#[derive(Clone, Copy)]
struct Message {
    badge: u64,
    words: [u64; MESSAGE_WORDS],
}

impl Message {
    const EMPTY: Self = Self {
        badge: 0,
        words: [0; MESSAGE_WORDS],
    };
}

/// The reference implementation of the contract.
///
/// Every bound is fixed and every failure is one of the canonical
/// [`Status`] values.
pub struct ModelKernel {
    slots: [Slot; SLOTS],
    next_id: u32,
    queue: [Message; QUEUE],
    queued: usize,
    notification_pending: bool,
    notification_badge: u64,
    /// A logical tick, not wall time. The conformance domain specifies a
    /// counter so that a corpus run is reproducible on every backend; a real
    /// deployment binds this capability to a hardware time source.
    clock: u64,
}

impl Default for ModelKernel {
    fn default() -> Self {
        let mut kernel = Self {
            slots: [Slot::EMPTY; SLOTS],
            next_id: 1,
            queue: [Message::EMPTY; QUEUE],
            queued: 0,
            notification_pending: false,
            notification_badge: 0,
            clock: 0,
        };
        kernel.reset_to_conformance_domain();
        kernel
    }
}

impl ModelKernel {
    /// A model kernel holding the conformance domain's capability space.
    pub fn new() -> Self {
        Self::default()
    }

    fn allocate_id(&mut self) -> Option<u32> {
        let id = self.next_id;
        self.next_id = self.next_id.checked_add(1)?;
        Some(id)
    }

    fn install(&mut self, index: u32, kind: ObjectType, rights: Rights, badge: u64) {
        let id = self
            .allocate_id()
            .expect("conformance domain fits in u32 ids");
        self.slots[index as usize] = Slot {
            kind,
            target: index as u8,
            rights,
            badge,
            id,
            parent: 0,
            revoked: false,
        };
    }

    /// Look a slot up for an operation that requires a live capability.
    fn resolve(&self, index: u32) -> Result<Slot, Status> {
        let slot = self
            .slots
            .get(index as usize)
            .copied()
            .ok_or(Status::InvalidCapability)?;
        if slot.revoked {
            return Err(Status::Revoked);
        }
        if slot.is_empty() {
            return Err(Status::InvalidCapability);
        }
        Ok(slot)
    }

    fn resolve_typed(
        &self,
        index: u32,
        kind: ObjectType,
        required: Rights,
    ) -> Result<Slot, Status> {
        let slot = self.resolve(index)?;
        if slot.kind != kind {
            return Err(Status::WrongObjectType);
        }
        if !slot.rights.contains(required) {
            return Err(Status::InsufficientRights);
        }
        Ok(slot)
    }

    /// Reject reserved argument words that are not zero, so that adding a
    /// meaning to one later cannot silently change an old caller's behavior.
    fn reserved(arguments: &[u64], from: usize) -> Result<(), Status> {
        if arguments[from..].iter().any(|word| *word != 0) {
            return Err(Status::InvalidArgument);
        }
        Ok(())
    }

    fn destination(&self, index: u64) -> Result<usize, Status> {
        let index = usize::try_from(index).map_err(|_| Status::InvalidArgument)?;
        let slot = self.slots.get(index).ok_or(Status::InvalidCapability)?;
        if slot.revoked || slot.is_empty() {
            Ok(index)
        } else {
            Err(Status::AlreadyExists)
        }
    }

    fn boot_info(&self, request: &Request) -> Response {
        if request.capability != slot::NULL {
            return Response::fail(Status::InvalidArgument);
        }
        if let Err(status) = Self::reserved(&request.arguments, 0) {
            return Response::fail(status);
        }
        Response::ok([
            version_word(),
            u64::from(CONFORMANCE_SLOTS),
            CONFORMANCE_ENDPOINT_CAPACITY,
            group::V1_PROFILE,
        ])
    }

    fn cap_copy(&mut self, request: &Request) -> Response {
        self.derive(request, DeriveMode::Copy)
    }

    fn cap_mint(&mut self, request: &Request) -> Response {
        self.derive(request, DeriveMode::Mint)
    }

    fn derive(&mut self, request: &Request, mode: DeriveMode) -> Response {
        if let Err(status) =
            self.resolve_typed(request.capability, ObjectType::CNode, Rights::CONTROL)
        {
            return Response::fail(status);
        }
        let reserved_from = match mode {
            DeriveMode::Copy => 2,
            DeriveMode::Mint => 4,
        };
        if let Err(status) = Self::reserved(&request.arguments, reserved_from) {
            return Response::fail(status);
        }
        let source_index = match u32::try_from(request.arguments[0]) {
            Ok(index) => index,
            Err(_) => return Response::fail(Status::InvalidArgument),
        };
        let source = match self.resolve(source_index) {
            Ok(slot) => slot,
            Err(status) => return Response::fail(status),
        };
        let destination = match self.destination(request.arguments[1]) {
            Ok(index) => index,
            Err(status) => return Response::fail(status),
        };
        let (rights, badge) = match mode {
            DeriveMode::Copy => (source.rights, source.badge),
            DeriveMode::Mint => {
                let requested = match u32::try_from(request.arguments[2]) {
                    Ok(bits) if Rights(bits).is_attenuation_of(Rights::ALL) => Rights(bits),
                    _ => return Response::fail(Status::InvalidArgument),
                };
                if !requested.is_attenuation_of(source.rights) {
                    return Response::fail(Status::InsufficientRights);
                }
                (requested, request.arguments[3])
            }
        };
        let Some(id) = self.allocate_id() else {
            return Response::fail(Status::ResourceExhausted);
        };
        self.slots[destination] = Slot {
            kind: source.kind,
            target: source.target,
            rights,
            badge,
            id,
            parent: source.id,
            revoked: false,
        };
        Response::ok1(u64::from(id))
    }

    fn cap_attenuate(&mut self, request: &Request) -> Response {
        if let Err(status) =
            self.resolve_typed(request.capability, ObjectType::CNode, Rights::CONTROL)
        {
            return Response::fail(status);
        }
        if let Err(status) = Self::reserved(&request.arguments, 2) {
            return Response::fail(status);
        }
        let index = match u32::try_from(request.arguments[0]) {
            Ok(index) => index,
            Err(_) => return Response::fail(Status::InvalidArgument),
        };
        let slot = match self.resolve(index) {
            Ok(slot) => slot,
            Err(status) => return Response::fail(status),
        };
        let requested = match u32::try_from(request.arguments[1]) {
            Ok(bits) if Rights(bits).is_attenuation_of(Rights::ALL) => Rights(bits),
            _ => return Response::fail(Status::InvalidArgument),
        };
        if !requested.is_attenuation_of(slot.rights) {
            return Response::fail(Status::InsufficientRights);
        }
        self.slots[index as usize].rights = requested;
        Response::ok1(u64::from(requested.0))
    }

    fn cap_move(&mut self, request: &Request) -> Response {
        if let Err(status) =
            self.resolve_typed(request.capability, ObjectType::CNode, Rights::CONTROL)
        {
            return Response::fail(status);
        }
        if let Err(status) = Self::reserved(&request.arguments, 2) {
            return Response::fail(status);
        }
        let source_index = match u32::try_from(request.arguments[0]) {
            Ok(index) => index,
            Err(_) => return Response::fail(Status::InvalidArgument),
        };
        let source = match self.resolve(source_index) {
            Ok(slot) => slot,
            Err(status) => return Response::fail(status),
        };
        let destination = match self.destination(request.arguments[1]) {
            Ok(index) => index,
            Err(status) => return Response::fail(status),
        };
        if destination == source_index as usize {
            return Response::fail(Status::AlreadyExists);
        }
        self.slots[destination] = source;
        self.slots[source_index as usize] = Slot::EMPTY;
        Response::ok1(u64::from(source.id))
    }

    /// Delete every capability derived from the named one, leaving the named
    /// capability itself intact. Descendants are tombstoned rather than merely
    /// emptied, so a stale holder is told [`Status::Revoked`].
    fn cap_revoke(&mut self, request: &Request) -> Response {
        if let Err(status) =
            self.resolve_typed(request.capability, ObjectType::CNode, Rights::CONTROL)
        {
            return Response::fail(status);
        }
        if let Err(status) = Self::reserved(&request.arguments, 1) {
            return Response::fail(status);
        }
        let index = match u32::try_from(request.arguments[0]) {
            Ok(index) => index,
            Err(_) => return Response::fail(Status::InvalidArgument),
        };
        let root = match self.resolve(index) {
            Ok(slot) => slot,
            Err(status) => return Response::fail(status),
        };
        let mut revoked = 0_u64;
        // Fixed point over a bounded slot table: a capability dies when its
        // parent has died. No recursion, no allocation, no unbounded work.
        loop {
            let mut changed = false;
            for position in 0..SLOTS {
                let slot = self.slots[position];
                if slot.is_empty() || slot.parent == 0 {
                    continue;
                }
                let parent_alive = self
                    .slots
                    .iter()
                    .any(|candidate| !candidate.is_empty() && candidate.id == slot.parent);
                if slot.parent == root.id || !parent_alive {
                    self.slots[position] = Slot {
                        revoked: true,
                        ..Slot::EMPTY
                    };
                    revoked += 1;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        Response::ok1(revoked)
    }

    fn endpoint_send(&mut self, request: &Request) -> Response {
        let slot = match self.resolve_typed(request.capability, ObjectType::Endpoint, Rights::SEND)
        {
            Ok(slot) => slot,
            Err(status) => return Response::fail(status),
        };
        if request.arguments[0] != 0 {
            // Word 0 is a reserved flags word in contract v1.0.
            return Response::fail(Status::InvalidArgument);
        }
        if self.queued == QUEUE {
            return Response::fail(Status::QueueFull);
        }
        let mut words = [0_u64; MESSAGE_WORDS];
        words.copy_from_slice(&request.arguments[1..]);
        self.queue[self.queued] = Message {
            badge: slot.badge,
            words,
        };
        self.queued += 1;
        Response::ok1(self.queued as u64)
    }

    fn endpoint_receive(&mut self, request: &Request) -> Response {
        if let Err(status) =
            self.resolve_typed(request.capability, ObjectType::Endpoint, Rights::RECEIVE)
        {
            return Response::fail(status);
        }
        if let Err(status) = Self::reserved(&request.arguments, 0) {
            return Response::fail(status);
        }
        if self.queued == 0 {
            return Response::fail(Status::WouldBlock);
        }
        let message = self.queue[0];
        self.queue.copy_within(1..self.queued, 0);
        self.queued -= 1;
        Response::ok([
            message.badge,
            message.words[0],
            message.words[1],
            message.words[2],
        ])
    }

    /// A synchronous call needs a receiver in another protection domain. The
    /// single-domain conformance harness has none, so the specified answer is
    /// [`Status::WouldBlock`] — never a hang.
    fn endpoint_call(&mut self, request: &Request) -> Response {
        match self.resolve_typed(request.capability, ObjectType::Endpoint, Rights::SEND) {
            Ok(_) => Response::fail(Status::WouldBlock),
            Err(status) => Response::fail(status),
        }
    }

    /// There is no outstanding call to answer in the conformance harness.
    fn endpoint_reply(&mut self, request: &Request) -> Response {
        match self.resolve_typed(request.capability, ObjectType::Endpoint, Rights::SEND) {
            Ok(_) => Response::fail(Status::NotFound),
            Err(status) => Response::fail(status),
        }
    }

    fn notification_signal(&mut self, request: &Request) -> Response {
        let slot =
            match self.resolve_typed(request.capability, ObjectType::Notification, Rights::SEND) {
                Ok(slot) => slot,
                Err(status) => return Response::fail(status),
            };
        if let Err(status) = Self::reserved(&request.arguments, 0) {
            return Response::fail(status);
        }
        // Signals coalesce: badges accumulate, deliveries do not queue.
        self.notification_pending = true;
        self.notification_badge |= slot.badge;
        Response::OK
    }

    fn notification_poll(&mut self, request: &Request) -> Response {
        if let Err(status) = self.resolve_typed(
            request.capability,
            ObjectType::Notification,
            Rights::RECEIVE,
        ) {
            return Response::fail(status);
        }
        if let Err(status) = Self::reserved(&request.arguments, 0) {
            return Response::fail(status);
        }
        if !self.notification_pending {
            return Response::ok([0, 0, 0, 0]);
        }
        let badge = self.notification_badge;
        self.notification_pending = false;
        self.notification_badge = 0;
        Response::ok([1, badge, 0, 0])
    }

    /// Waiting would block the only thread in the conformance harness, so the
    /// specified answer when nothing is pending is [`Status::WouldBlock`].
    fn notification_wait(&mut self, request: &Request) -> Response {
        if let Err(status) = self.resolve_typed(
            request.capability,
            ObjectType::Notification,
            Rights::RECEIVE,
        ) {
            return Response::fail(status);
        }
        if !self.notification_pending {
            return Response::fail(Status::WouldBlock);
        }
        self.notification_poll(request)
    }

    fn clock_now(&mut self, request: &Request) -> Response {
        if let Err(status) = self.resolve_typed(request.capability, ObjectType::Clock, Rights::READ)
        {
            return Response::fail(status);
        }
        if let Err(status) = Self::reserved(&request.arguments, 0) {
            return Response::fail(status);
        }
        let now = self.clock;
        self.clock = self.clock.saturating_add(1);
        Response::ok1(now)
    }
}

#[derive(Clone, Copy)]
enum DeriveMode {
    Copy,
    Mint,
}

impl Kernel for ModelKernel {
    fn reset_to_conformance_domain(&mut self) {
        self.slots = [Slot::EMPTY; SLOTS];
        self.next_id = 1;
        self.queue = [Message::EMPTY; QUEUE];
        self.queued = 0;
        self.notification_pending = false;
        self.notification_badge = 0;
        self.clock = 0;
        self.install(slot::CNODE, ObjectType::CNode, Rights::CONTROL, 0);
        self.install(
            slot::ENDPOINT,
            ObjectType::Endpoint,
            Rights(Rights::SEND.0 | Rights::RECEIVE.0 | Rights::GRANT.0),
            0,
        );
        self.install(
            slot::NOTIFICATION,
            ObjectType::Notification,
            Rights(Rights::SEND.0 | Rights::RECEIVE.0),
            1,
        );
        self.install(
            slot::FRAME,
            ObjectType::Frame,
            Rights(Rights::READ.0 | Rights::WRITE.0),
            0,
        );
        self.install(slot::CLOCK, ObjectType::Clock, Rights::READ, 0);
    }

    fn invoke(&mut self, request: &Request) -> Response {
        match request.operation {
            Operation::Nop => {
                if request.capability != slot::NULL {
                    Response::fail(Status::InvalidArgument)
                } else {
                    Response::ok1(version_word())
                }
            }
            Operation::BootInfo => self.boot_info(request),

            Operation::CapCopy => self.cap_copy(request),
            Operation::CapMint => self.cap_mint(request),
            Operation::CapAttenuate => self.cap_attenuate(request),
            Operation::CapMove => self.cap_move(request),
            Operation::CapRevoke => self.cap_revoke(request),

            Operation::EndpointSend => self.endpoint_send(request),
            Operation::EndpointReceive => self.endpoint_receive(request),
            Operation::EndpointCall => self.endpoint_call(request),
            Operation::EndpointReply => self.endpoint_reply(request),

            Operation::NotificationSignal => self.notification_signal(request),
            Operation::NotificationPoll => self.notification_poll(request),
            Operation::NotificationWait => self.notification_wait(request),

            Operation::ClockMonotonicNow => self.clock_now(request),

            // Outside the v1.0 profile. A backend must say so, not guess.
            _ => Response::fail(Status::InvalidOperation),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kernel() -> ModelKernel {
        ModelKernel::new()
    }

    #[test]
    fn boot_info_publishes_the_implemented_profile() {
        let response = kernel().invoke(&Request::new(Operation::BootInfo, slot::NULL));
        assert_eq!(response.status, Status::Ok);
        assert_eq!(response.values[0], version_word());
        assert_eq!(response.values[3], group::V1_PROFILE);
        assert_eq!(response.values[3] & group::MEMORY, 0);
    }

    #[test]
    fn naming_an_empty_slot_is_not_authority() {
        let mut kernel = kernel();
        let response = kernel.invoke(&Request::new(
            Operation::EndpointSend,
            slot::FIRST_FREE + 10,
        ));
        assert_eq!(response.status, Status::InvalidCapability);
    }

    #[test]
    fn minting_cannot_widen_rights() {
        let mut kernel = kernel();
        // The frame capability deliberately lacks EXECUTE.
        let widened = kernel.invoke(&Request::with(
            Operation::CapMint,
            slot::CNODE,
            [
                u64::from(slot::FRAME),
                u64::from(slot::FIRST_FREE),
                u64::from(Rights::ALL.0),
                0,
            ],
        ));
        assert_eq!(widened.status, Status::InsufficientRights);

        let narrowed = kernel.invoke(&Request::with(
            Operation::CapMint,
            slot::CNODE,
            [
                u64::from(slot::FRAME),
                u64::from(slot::FIRST_FREE),
                u64::from(Rights::READ.0),
                0,
            ],
        ));
        assert_eq!(narrowed.status, Status::Ok);
    }

    #[test]
    fn revocation_kills_descendants_transitively_and_fails_closed() {
        let mut kernel = kernel();
        let child = Request::with(
            Operation::CapMint,
            slot::CNODE,
            [
                u64::from(slot::NOTIFICATION),
                u64::from(slot::FIRST_FREE),
                u64::from(Rights::SEND.0),
                0x2,
            ],
        );
        assert_eq!(kernel.invoke(&child).status, Status::Ok);
        let grandchild = Request::with(
            Operation::CapMint,
            slot::CNODE,
            [
                u64::from(slot::FIRST_FREE),
                u64::from(slot::FIRST_FREE + 1),
                u64::from(Rights::SEND.0),
                0x4,
            ],
        );
        assert_eq!(kernel.invoke(&grandchild).status, Status::Ok);

        let revoked = kernel.invoke(&Request::with(
            Operation::CapRevoke,
            slot::CNODE,
            [u64::from(slot::NOTIFICATION), 0, 0, 0],
        ));
        assert_eq!(revoked.status, Status::Ok);
        assert_eq!(revoked.values[0], 2);

        // Both derived handles fail closed, and say why.
        for index in [slot::FIRST_FREE, slot::FIRST_FREE + 1] {
            let response = kernel.invoke(&Request::new(Operation::NotificationSignal, index));
            assert_eq!(response.status, Status::Revoked);
        }
        // The revoked capability itself survives.
        assert_eq!(
            kernel
                .invoke(&Request::new(
                    Operation::NotificationSignal,
                    slot::NOTIFICATION
                ))
                .status,
            Status::Ok
        );
    }

    #[test]
    fn the_endpoint_queue_is_bounded_and_pushes_back() {
        let mut kernel = kernel();
        for expected in 1..=CONFORMANCE_ENDPOINT_CAPACITY {
            let response = kernel.invoke(&Request::with(
                Operation::EndpointSend,
                slot::ENDPOINT,
                [0, expected, 0, 0],
            ));
            assert_eq!(response.status, Status::Ok);
            assert_eq!(response.values[0], expected);
        }
        assert_eq!(
            kernel
                .invoke(&Request::with(
                    Operation::EndpointSend,
                    slot::ENDPOINT,
                    [0, 99, 0, 0]
                ))
                .status,
            Status::QueueFull
        );
        // Draining restores capacity in order.
        for expected in 1..=CONFORMANCE_ENDPOINT_CAPACITY {
            let response = kernel.invoke(&Request::new(Operation::EndpointReceive, slot::ENDPOINT));
            assert_eq!(response.status, Status::Ok);
            assert_eq!(response.values[1], expected);
        }
        assert_eq!(
            kernel
                .invoke(&Request::new(Operation::EndpointReceive, slot::ENDPOINT))
                .status,
            Status::WouldBlock
        );
    }

    #[test]
    fn notifications_coalesce_and_accumulate_badges() {
        let mut kernel = kernel();
        assert_eq!(
            kernel
                .invoke(&Request::with(
                    Operation::CapMint,
                    slot::CNODE,
                    [
                        u64::from(slot::NOTIFICATION),
                        u64::from(slot::FIRST_FREE),
                        u64::from(Rights::SEND.0),
                        0x8,
                    ],
                ))
                .status,
            Status::Ok
        );
        for capability in [slot::NOTIFICATION, slot::FIRST_FREE, slot::NOTIFICATION] {
            assert_eq!(
                kernel
                    .invoke(&Request::new(Operation::NotificationSignal, capability))
                    .status,
                Status::Ok
            );
        }
        let polled = kernel.invoke(&Request::new(
            Operation::NotificationPoll,
            slot::NOTIFICATION,
        ));
        assert_eq!(polled.status, Status::Ok);
        assert_eq!(polled.values[0], 1);
        assert_eq!(
            polled.values[1], 0x9,
            "three signals, one delivery, ORed badges"
        );
        let drained = kernel.invoke(&Request::new(
            Operation::NotificationPoll,
            slot::NOTIFICATION,
        ));
        assert_eq!(drained.values[0], 0);
    }

    #[test]
    fn reserved_argument_words_must_be_zero() {
        let mut kernel = kernel();
        assert_eq!(
            kernel
                .invoke(&Request::with(
                    Operation::EndpointReceive,
                    slot::ENDPOINT,
                    [0, 0, 0, 1]
                ))
                .status,
            Status::InvalidArgument
        );
    }

    #[test]
    fn the_clock_is_monotonic() {
        let mut kernel = kernel();
        let first = kernel.invoke(&Request::new(Operation::ClockMonotonicNow, slot::CLOCK));
        let second = kernel.invoke(&Request::new(Operation::ClockMonotonicNow, slot::CLOCK));
        assert!(second.values[0] > first.values[0]);
    }

    #[test]
    fn unimplemented_operations_are_answered_not_ignored() {
        let mut kernel = kernel();
        for operation in [
            Operation::PdCreate,
            Operation::AsMap,
            Operation::FrameAllocate,
            Operation::IrqBind,
            Operation::SchedBudget,
        ] {
            assert_eq!(
                kernel.invoke(&Request::new(operation, slot::NULL)).status,
                Status::InvalidOperation
            );
        }
    }
}
