//! A second, independent implementation of the kernel contract.
//!
//! This module was written from `docs/kernel-contract.md`, the type
//! definitions in the crate root, the conformance corpus and its frozen
//! transcript, and deliberately **not** from [`crate::model`]. It shares no
//! code with the reference model beyond the contract's own types and the
//! well-known slot and profile constants. Where the corpus does not pin an
//! ordering or a value, the choice made here is documented at the point it is
//! made, so a future corpus step can freeze it knowingly.
//!
//! The seL4 broker answers the contract with this implementation, so the
//! byte-identical transcript CI demands from the seL4 backend is now the
//! agreement of two implementations, not one implementation behind two
//! boundaries.

use crate::model::{group, slot};
use crate::{
    version_word, Kernel, ObjectType, Operation, Request, Response, Rights, Status,
    CONFORMANCE_ENDPOINT_CAPACITY, CONFORMANCE_SLOTS, WORDS,
};

const SLOTS: usize = CONFORMANCE_SLOTS as usize;
const QUEUE: usize = CONFORMANCE_ENDPOINT_CAPACITY as usize;
/// Objects in the conformance domain: cnode, endpoint, notification, frame,
/// clock. The contract's other object types are declared but outside the v1
/// profile, so no operation can reach one.
const OBJECTS: usize = 5;

/// One queued endpoint message: the sending capability's badge and three words.
#[derive(Clone, Copy, Default)]
struct Message {
    badge: u64,
    words: [u64; 3],
}

#[derive(Clone, Copy)]
struct Endpoint {
    queue: [Message; QUEUE],
    head: usize,
    length: usize,
}

#[derive(Clone, Copy)]
struct Notification {
    pending: bool,
    badges: u64,
}

#[derive(Clone, Copy)]
enum Object {
    CNode,
    Endpoint(Endpoint),
    Notification(Notification),
    Frame,
    Clock { ticks: u64 },
}

impl Object {
    fn kind(&self) -> ObjectType {
        match self {
            Self::CNode => ObjectType::CNode,
            Self::Endpoint(_) => ObjectType::Endpoint,
            Self::Notification(_) => ObjectType::Notification,
            Self::Frame => ObjectType::Frame,
            Self::Clock { .. } => ObjectType::Clock,
        }
    }
}

/// A live capability: which object, with which rights and badge, and where it
/// sits in the derivation tree.
#[derive(Clone, Copy)]
struct Capability {
    object: usize,
    rights: Rights,
    badge: u64,
    /// Monotonic derivation identifier; also the value derivation returns.
    id: u64,
    /// Derivation identifier of the capability this one was derived from, or
    /// zero for a root the domain was constructed with.
    parent: u64,
}

#[derive(Clone, Copy)]
enum Slot {
    Empty,
    Live(Capability),
    /// Invalidated by revoking an ancestor. Distinguishable from `Empty` so a
    /// stale holder is told `revoked`, and refillable by derivation.
    Tombstone,
}

/// The independent backend.
#[derive(Clone, Copy)]
pub struct IndependentKernel {
    objects: [Object; OBJECTS],
    slots: [Slot; SLOTS],
    next_id: u64,
}

impl Default for IndependentKernel {
    fn default() -> Self {
        Self::new()
    }
}

impl IndependentKernel {
    /// A kernel holding the conformance domain.
    pub fn new() -> Self {
        let mut kernel = Self {
            objects: [Object::CNode; OBJECTS],
            slots: [Slot::Empty; SLOTS],
            next_id: 1,
        };
        kernel.reset_to_conformance_domain();
        kernel
    }

    fn root(&mut self, slot: u32, object: usize, rights: Rights, badge: u64) {
        let id = self.next_id;
        self.next_id += 1;
        self.slots[slot as usize] = Slot::Live(Capability {
            object,
            rights,
            badge,
            id,
            parent: 0,
        });
    }

    /// Look a slot up as the subject of an operation. Naming a slot is never
    /// authority: out of range, empty and the null slot are all
    /// `invalid-capability`, and a tombstone is `revoked`.
    fn capability(&self, slot: u32) -> Result<Capability, Status> {
        match self.slots.get(slot as usize) {
            None | Some(Slot::Empty) => Err(Status::InvalidCapability),
            Some(Slot::Tombstone) => Err(Status::Revoked),
            Some(Slot::Live(capability)) => Ok(*capability),
        }
    }

    /// A slot named by an argument word. Widths above the slot register are
    /// never truncated into a valid slot number.
    fn subject(&self, word: u64) -> Result<(usize, Capability), Status> {
        let slot = u32::try_from(word).map_err(|_| Status::InvalidCapability)?;
        Ok((slot as usize, self.capability(slot)?))
    }

    /// Look a slot up and require an object type, checking type before rights.
    fn typed(&self, slot: u32, kind: ObjectType) -> Result<Capability, Status> {
        let capability = self.capability(slot)?;
        if self.objects[capability.object].kind() != kind {
            return Err(Status::WrongObjectType);
        }
        Ok(capability)
    }

    fn require(capability: &Capability, rights: Rights) -> Result<(), Status> {
        if capability.rights.contains(rights) {
            Ok(())
        } else {
            Err(Status::InsufficientRights)
        }
    }

    /// A destination slot for derivation or move. Out of range is an argument
    /// error; a live occupant is `already-exists`; empty and tombstoned slots
    /// are both free.
    fn destination(&self, slot: u64) -> Result<usize, Status> {
        let index = usize::try_from(slot).map_err(|_| Status::InvalidArgument)?;
        if index >= SLOTS || index == slot::NULL as usize {
            return Err(Status::InvalidArgument);
        }
        match self.slots[index] {
            Slot::Live(_) => Err(Status::AlreadyExists),
            Slot::Empty | Slot::Tombstone => Ok(index),
        }
    }

    fn defined_rights(word: u64) -> Result<Rights, Status> {
        if word & !u64::from(Rights::ALL.0) != 0 {
            return Err(Status::InvalidArgument);
        }
        Ok(Rights(word as u32))
    }

    fn allocate_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// True when `id` is `ancestor` or descends from it, walking the
    /// derivation tree recorded in `slots`. Revocation walks a snapshot taken
    /// before any slot is tombstoned, so a child is still reachable through a
    /// parent that the same revocation is removing.
    fn descends(slots: &[Slot; SLOTS], mut id: u64, ancestor: u64) -> bool {
        // Parents always have smaller identifiers, so this walk terminates.
        loop {
            if id == ancestor {
                return true;
            }
            if id == 0 {
                return false;
            }
            let Some(parent) = slots.iter().find_map(|slot| match slot {
                Slot::Live(capability) if capability.id == id => Some(capability.parent),
                _ => None,
            }) else {
                return false;
            };
            id = parent;
        }
    }

    fn invoke_checked(&mut self, request: &Request) -> Result<[u64; WORDS], Status> {
        let arguments = request.arguments;
        match request.operation {
            // -- core -------------------------------------------------------
            Operation::Nop => {
                if request.capability != slot::NULL || arguments != [0; WORDS] {
                    return Err(Status::InvalidArgument);
                }
                Ok([version_word(), 0, 0, 0])
            }
            Operation::BootInfo => {
                if request.capability != slot::NULL || arguments != [0; WORDS] {
                    return Err(Status::InvalidArgument);
                }
                Ok([
                    version_word(),
                    u64::from(CONFORMANCE_SLOTS),
                    CONFORMANCE_ENDPOINT_CAPACITY,
                    group::V1_PROFILE,
                ])
            }

            // -- capability derivation: the cnode is the subject -----------
            Operation::CapCopy | Operation::CapMint => {
                let cnode = self.typed(request.capability, ObjectType::CNode)?;
                Self::require(&cnode, Rights::CONTROL)?;
                let (_, source) = self.subject(arguments[0])?;
                let (rights, badge) = if request.operation == Operation::CapCopy {
                    if arguments[2] != 0 || arguments[3] != 0 {
                        return Err(Status::InvalidArgument);
                    }
                    (source.rights, source.badge)
                } else {
                    let rights = Self::defined_rights(arguments[2])?;
                    if !rights.is_attenuation_of(source.rights) {
                        return Err(Status::InsufficientRights);
                    }
                    (rights, arguments[3])
                };
                let destination = self.destination(arguments[1])?;
                let id = self.allocate_id();
                self.slots[destination] = Slot::Live(Capability {
                    object: source.object,
                    rights,
                    badge,
                    id,
                    parent: source.id,
                });
                Ok([id, 0, 0, 0])
            }
            Operation::CapAttenuate => {
                let cnode = self.typed(request.capability, ObjectType::CNode)?;
                Self::require(&cnode, Rights::CONTROL)?;
                if arguments[2] != 0 || arguments[3] != 0 {
                    return Err(Status::InvalidArgument);
                }
                let (index, target) = self.subject(arguments[0])?;
                let rights = Self::defined_rights(arguments[1])?;
                if !rights.is_attenuation_of(target.rights) {
                    return Err(Status::InsufficientRights);
                }
                self.slots[index] = Slot::Live(Capability { rights, ..target });
                Ok([u64::from(rights.0), 0, 0, 0])
            }
            Operation::CapMove => {
                let cnode = self.typed(request.capability, ObjectType::CNode)?;
                Self::require(&cnode, Rights::CONTROL)?;
                if arguments[2] != 0 || arguments[3] != 0 {
                    return Err(Status::InvalidArgument);
                }
                let (index, source) = self.subject(arguments[0])?;
                // Moving onto itself is refused as an occupied destination.
                let destination = self.destination(arguments[1])?;
                self.slots[index] = Slot::Empty;
                self.slots[destination] = Slot::Live(source);
                Ok([source.id, 0, 0, 0])
            }
            Operation::CapRevoke => {
                let cnode = self.typed(request.capability, ObjectType::CNode)?;
                Self::require(&cnode, Rights::CONTROL)?;
                if arguments[1] != 0 || arguments[2] != 0 || arguments[3] != 0 {
                    return Err(Status::InvalidArgument);
                }
                let (_, target) = self.subject(arguments[0])?;
                // Transitive to a fixed point: every live capability whose
                // ancestry reaches the target, except the target itself.
                let before = self.slots;
                let mut revoked = 0;
                for index in 0..SLOTS {
                    let descendant = match before[index] {
                        Slot::Live(capability) => {
                            capability.id != target.id
                                && Self::descends(&before, capability.id, target.id)
                        }
                        _ => false,
                    };
                    if descendant {
                        self.slots[index] = Slot::Tombstone;
                        revoked += 1;
                    }
                }
                Ok([revoked, 0, 0, 0])
            }

            // -- endpoints ---------------------------------------------------
            Operation::EndpointSend => {
                let capability = self.typed(request.capability, ObjectType::Endpoint)?;
                Self::require(&capability, Rights::SEND)?;
                if arguments[0] != 0 {
                    return Err(Status::InvalidArgument);
                }
                let Object::Endpoint(endpoint) = &mut self.objects[capability.object] else {
                    return Err(Status::WrongObjectType);
                };
                if endpoint.length == QUEUE {
                    return Err(Status::QueueFull);
                }
                let tail = (endpoint.head + endpoint.length) % QUEUE;
                endpoint.queue[tail] = Message {
                    badge: capability.badge,
                    words: [arguments[1], arguments[2], arguments[3]],
                };
                endpoint.length += 1;
                Ok([endpoint.length as u64, 0, 0, 0])
            }
            Operation::EndpointReceive => {
                let capability = self.typed(request.capability, ObjectType::Endpoint)?;
                Self::require(&capability, Rights::RECEIVE)?;
                if arguments != [0; WORDS] {
                    return Err(Status::InvalidArgument);
                }
                let Object::Endpoint(endpoint) = &mut self.objects[capability.object] else {
                    return Err(Status::WrongObjectType);
                };
                if endpoint.length == 0 {
                    return Err(Status::WouldBlock);
                }
                let message = endpoint.queue[endpoint.head];
                endpoint.head = (endpoint.head + 1) % QUEUE;
                endpoint.length -= 1;
                Ok([
                    message.badge,
                    message.words[0],
                    message.words[1],
                    message.words[2],
                ])
            }
            Operation::EndpointCall => {
                // A single-domain harness has nobody to call. The message is
                // not queued: a call that cannot complete has no effect.
                let capability = self.typed(request.capability, ObjectType::Endpoint)?;
                Self::require(&capability, Rights::SEND)?;
                if arguments[0] != 0 {
                    return Err(Status::InvalidArgument);
                }
                Err(Status::WouldBlock)
            }
            Operation::EndpointReply => {
                let capability = self.typed(request.capability, ObjectType::Endpoint)?;
                Self::require(&capability, Rights::SEND)?;
                if arguments[0] != 0 {
                    return Err(Status::InvalidArgument);
                }
                Err(Status::NotFound)
            }

            // -- notifications ----------------------------------------------
            Operation::NotificationSignal => {
                let capability = self.typed(request.capability, ObjectType::Notification)?;
                Self::require(&capability, Rights::SEND)?;
                if arguments != [0; WORDS] {
                    return Err(Status::InvalidArgument);
                }
                let Object::Notification(notification) = &mut self.objects[capability.object]
                else {
                    return Err(Status::WrongObjectType);
                };
                notification.pending = true;
                notification.badges |= capability.badge;
                Ok([0; WORDS])
            }
            Operation::NotificationPoll | Operation::NotificationWait => {
                let capability = self.typed(request.capability, ObjectType::Notification)?;
                Self::require(&capability, Rights::RECEIVE)?;
                if arguments != [0; WORDS] {
                    return Err(Status::InvalidArgument);
                }
                let Object::Notification(notification) = &mut self.objects[capability.object]
                else {
                    return Err(Status::WrongObjectType);
                };
                if !notification.pending {
                    return if request.operation == Operation::NotificationWait {
                        Err(Status::WouldBlock)
                    } else {
                        Ok([0; WORDS])
                    };
                }
                let badges = notification.badges;
                notification.pending = false;
                notification.badges = 0;
                Ok([1, badges, 0, 0])
            }

            // -- clock -------------------------------------------------------
            Operation::ClockMonotonicNow => {
                let capability = self.typed(request.capability, ObjectType::Clock)?;
                Self::require(&capability, Rights::READ)?;
                if arguments != [0; WORDS] {
                    return Err(Status::InvalidArgument);
                }
                let Object::Clock { ticks } = &mut self.objects[capability.object] else {
                    return Err(Status::WrongObjectType);
                };
                let now = *ticks;
                *ticks += 1;
                Ok([now, 0, 0, 0])
            }

            // -- everything outside the v1.0 profile answers, it does not guess
            _ => Err(Status::InvalidOperation),
        }
    }
}

impl Kernel for IndependentKernel {
    fn invoke(&mut self, request: &Request) -> Response {
        match self.invoke_checked(request) {
            Ok(values) => Response::ok(values),
            Err(status) => Response::fail(status),
        }
    }

    fn reset_to_conformance_domain(&mut self) {
        self.slots = [Slot::Empty; SLOTS];
        self.next_id = 1;
        self.objects = [
            Object::CNode,
            Object::Endpoint(Endpoint {
                queue: [Message::default(); QUEUE],
                head: 0,
                length: 0,
            }),
            Object::Notification(Notification {
                pending: false,
                badges: 0,
            }),
            Object::Frame,
            Object::Clock { ticks: 0 },
        ];
        // Derivation identifiers are allocated in construction order, which
        // the corpus asserts; the badge on the notification is part of the
        // specified domain.
        self.root(slot::CNODE, 0, Rights::CONTROL, 0);
        self.root(
            slot::ENDPOINT,
            1,
            Rights(Rights::SEND.0 | Rights::RECEIVE.0 | Rights::GRANT.0),
            0,
        );
        self.root(
            slot::NOTIFICATION,
            2,
            Rights(Rights::SEND.0 | Rights::RECEIVE.0),
            1,
        );
        self.root(slot::FRAME, 3, Rights(Rights::READ.0 | Rights::WRITE.0), 0);
        self.root(slot::CLOCK, 4, Rights::READ, 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conformance;
    use crate::model::ModelKernel;
    use core::fmt::Write as _;

    extern crate std;
    use std::string::String;

    #[test]
    fn independent_implementation_reproduces_the_frozen_transcript() {
        let mut kernel = IndependentKernel::new();
        let mut transcript = String::new();
        writeln!(
            transcript,
            "agel-kernel-contract v{}.{}.{} corpus={} steps",
            crate::VERSION_MAJOR,
            crate::VERSION_MINOR,
            crate::VERSION_PATCH,
            conformance::CORPUS.len()
        )
        .unwrap();
        conformance::transcribe(&mut kernel, &mut transcript).unwrap();
        assert_eq!(
            transcript,
            include_str!("../../../bootstrap/kernel-contract.trace")
        );
    }

    #[test]
    fn independent_and_reference_implementations_agree_on_every_step() {
        let agreed =
            conformance::compare(&mut ModelKernel::new(), &mut IndependentKernel::new()).unwrap();
        assert_eq!(agreed, conformance::CORPUS.len());
        conformance::check_invariants(&mut IndependentKernel::new()).unwrap();
    }

    #[test]
    fn the_corpus_still_catches_a_widening_backend() {
        struct Widening(IndependentKernel);
        impl Kernel for Widening {
            fn invoke(&mut self, request: &Request) -> Response {
                // Pretend widening to every right is a plain read derivation.
                if request.operation == Operation::CapMint
                    && request.arguments[2] == u64::from(Rights::ALL.0)
                {
                    let mut lax = *request;
                    lax.arguments[2] = u64::from(Rights::READ.0);
                    return self.0.invoke(&lax);
                }
                self.0.invoke(request)
            }
            fn reset_to_conformance_domain(&mut self) {
                self.0.reset_to_conformance_domain();
            }
        }
        let divergence = conformance::compare(
            &mut ModelKernel::new(),
            &mut Widening(IndependentKernel::new()),
        )
        .unwrap_err();
        assert_eq!(divergence.label, "derive/mint-cannot-widen");
    }
}
