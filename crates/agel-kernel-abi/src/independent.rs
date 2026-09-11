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
/// Pages in the frame window and frames the domain may hold: the frame it is
/// built with plus its allocation budget.
#[cfg(feature = "memory")]
const PAGES: usize = crate::CONFORMANCE_FRAME_WINDOW as usize;
#[cfg(feature = "memory")]
const FRAME_BUDGET: usize = crate::CONFORMANCE_FRAME_BUDGET as usize;
/// Objects in the conformance domain: cnode, endpoint, notification, the
/// frame the domain is built with, clock, address space, then one object per
/// frame of the allocation budget, unallocated until `frame.allocate`.
#[cfg(feature = "memory")]
const OBJECTS: usize = 6 + FRAME_BUDGET;
#[cfg(not(feature = "memory"))]
const OBJECTS: usize = 6;
/// Object indices of the fixed objects; the budget frames follow the address
/// space in frame-number order.
const OBJECT_FRAME_0: usize = 3;
const OBJECT_ADDRESS_SPACE: usize = 5;
#[cfg(feature = "memory")]
const OBJECT_FIRST_BUDGET_FRAME: usize = 6;

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

/// One page of the frame window: the object index of the frame mapped there
/// and the rights of the mapping, or nothing.
#[cfg(feature = "memory")]
#[derive(Clone, Copy, Default)]
struct Page {
    frame: Option<usize>,
    rights: Rights,
}

/// The domain's address space: its frame window. Without the memory group
/// there is nothing to record, and the object is a marker.
#[derive(Clone, Copy)]
struct AddressSpace {
    #[cfg(feature = "memory")]
    pages: [Page; PAGES],
}

#[derive(Clone, Copy)]
enum Object {
    CNode,
    Endpoint(Endpoint),
    Notification(Notification),
    /// A frame; `number` is what `as.query` reports, 0 for the frame the
    /// domain is built with. A budget frame exists only while `live`.
    Frame {
        #[cfg_attr(not(feature = "memory"), allow(dead_code))]
        number: u8,
        #[cfg_attr(not(feature = "memory"), allow(dead_code))]
        live: bool,
    },
    Clock {
        ticks: u64,
    },
    AddressSpace(AddressSpace),
}

impl Object {
    fn kind(&self) -> ObjectType {
        match self {
            Self::CNode => ObjectType::CNode,
            Self::Endpoint(_) => ObjectType::Endpoint,
            Self::Notification(_) => ObjectType::Notification,
            Self::Frame { .. } => ObjectType::Frame,
            Self::Clock { .. } => ObjectType::Clock,
            Self::AddressSpace(_) => ObjectType::AddressSpace,
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
    /// The groups this instance publishes; anything else is
    /// `invalid-operation`, whatever the request. Without the memory group
    /// compiled in the profile is v1.0 by construction and needs no field.
    #[cfg(feature = "memory")]
    profile: u64,
}

impl Default for IndependentKernel {
    fn default() -> Self {
        Self::new()
    }
}

impl IndependentKernel {
    /// A kernel holding the conformance domain and publishing the v1.1
    /// profile.
    pub fn new() -> Self {
        Self::with_profile(group::V1_1_PROFILE)
    }

    /// A kernel publishing exactly `profile`, which must include the v1.0
    /// groups: what a backend that implements less would answer.
    pub fn with_profile(profile: u64) -> Self {
        debug_assert!(
            profile & group::V1_PROFILE == group::V1_PROFILE,
            "every profile includes the v1.0 groups"
        );
        #[cfg(not(feature = "memory"))]
        debug_assert!(
            profile == group::V1_PROFILE,
            "without the memory group only the v1.0 profile can be published"
        );
        let mut kernel = Self {
            objects: [Object::CNode; OBJECTS],
            slots: [Slot::Empty; SLOTS],
            next_id: 1,
            #[cfg(feature = "memory")]
            profile,
        };
        kernel.reset_to_conformance_domain();
        kernel
    }

    /// The profile `boot.info` reports.
    fn published_profile(&self) -> u64 {
        #[cfg(feature = "memory")]
        {
            self.profile
        }
        #[cfg(not(feature = "memory"))]
        {
            group::V1_PROFILE
        }
    }

    // -- memory helpers -----------------------------------------------------

    #[cfg(feature = "memory")]
    /// A page index inside the frame window; anything else is an argument
    /// error, since the window is the only thing a page can name.
    fn page(word: u64) -> Result<usize, Status> {
        match usize::try_from(word) {
            Ok(page) if page < PAGES => Ok(page),
            _ => Err(Status::InvalidArgument),
        }
    }

    #[cfg(feature = "memory")]
    /// Rights a mapping may carry: some of read, write, execute, and at
    /// least one of them, since a mapping with no rights maps nothing.
    fn mapping_rights(word: u64) -> Result<Rights, Status> {
        let rights = Self::defined_rights(word)?;
        let memory = Rights(Rights::READ.0 | Rights::WRITE.0 | Rights::EXECUTE.0);
        if rights.0 == 0 || !rights.is_attenuation_of(memory) {
            return Err(Status::InvalidArgument);
        }
        Ok(rights)
    }

    /// Writable and executable at once is refused as policy, after the
    /// capability has been found sufficient: the rights are defined and
    /// held, the machines will not grant them together.
    #[cfg(feature = "memory")]
    fn permitted_together(rights: Rights) -> Result<(), Status> {
        if rights.contains(Rights::WRITE) && rights.contains(Rights::EXECUTE) {
            return Err(Status::NotPermitted);
        }
        Ok(())
    }

    #[cfg(feature = "memory")]
    fn address_space(&mut self, capability: u32) -> Result<&mut AddressSpace, Status> {
        let subject = self.typed(capability, ObjectType::AddressSpace)?;
        Self::require(&subject, Rights::CONTROL)?;
        match &mut self.objects[subject.object] {
            Object::AddressSpace(space) => Ok(space),
            _ => Err(Status::WrongObjectType),
        }
    }

    #[cfg(feature = "memory")]
    /// Map `frame` (a live frame capability) at `page` with `rights`, the
    /// shared tail of `frame.map` and `as.map`: the page must be in the
    /// window, the rights defined, the capability sufficient for them, and
    /// the page free.
    fn map(&mut self, frame: Capability, page: u64, rights: u64) -> Result<[u64; WORDS], Status> {
        let page = Self::page(page)?;
        let rights = Self::mapping_rights(rights)?;
        if !rights.is_attenuation_of(frame.rights) {
            return Err(Status::InsufficientRights);
        }
        Self::permitted_together(rights)?;
        let Object::AddressSpace(space) = &mut self.objects[OBJECT_ADDRESS_SPACE] else {
            return Err(Status::WrongObjectType);
        };
        if space.pages[page].frame.is_some() {
            return Err(Status::AlreadyExists);
        }
        space.pages[page] = Page {
            frame: Some(frame.object),
            rights,
        };
        Ok([page as u64, 0, 0, 0])
    }

    /// What the frame window holds at `page`: the frame's number and the
    /// mapping's rights. Not a contract operation: a backend that backs the
    /// window with real translations reads this after every memory
    /// operation to make its page tables agree with the object table.
    #[cfg(feature = "memory")]
    pub fn mapping(&self, page: usize) -> Option<(u8, Rights)> {
        let Object::AddressSpace(space) = &self.objects[OBJECT_ADDRESS_SPACE] else {
            return None;
        };
        let entry = space.pages.get(page)?;
        let object = entry.frame?;
        Some((self.frame_number(object) as u8, entry.rights))
    }

    #[cfg(feature = "memory")]
    fn frame_number(&self, object: usize) -> u64 {
        match self.objects[object] {
            Object::Frame { number, .. } => u64::from(number),
            _ => 0,
        }
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
        // The profile is checked before anything else: an operation outside
        // it is unknown to this backend, whatever slot or words it names.
        // Without the memory group compiled in, the v1.0 groups are all the
        // implementation can answer and the final arm refuses the rest, so
        // the check would only cost the workshop images their budget.
        #[cfg(feature = "memory")]
        if request.operation.group() & self.profile == 0 {
            return Err(Status::InvalidOperation);
        }
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
                    self.published_profile(),
                ])
            }

            // -- memory: frames and the frame window ---------------------
            #[cfg(feature = "memory")]
            Operation::FrameAllocate => {
                // Allocation is an act of the capability space, like
                // derivation: the caller must hold its cnode with `control`.
                let cnode = self.typed(request.capability, ObjectType::CNode)?;
                Self::require(&cnode, Rights::CONTROL)?;
                if arguments[2] != 0 || arguments[3] != 0 {
                    return Err(Status::InvalidArgument);
                }
                let rights = Self::defined_rights(arguments[1])?;
                let holdable =
                    Rights(Rights::READ.0 | Rights::WRITE.0 | Rights::EXECUTE.0 | Rights::GRANT.0);
                if rights.0 == 0 || !rights.is_attenuation_of(holdable) {
                    return Err(Status::InvalidArgument);
                }
                let destination = self.destination(arguments[0])?;
                // Lowest free frame number first, so the number a frame gets
                // is a function of the history the corpus fixes.
                let object = (OBJECT_FIRST_BUDGET_FRAME..OBJECTS)
                    .find(|index| matches!(self.objects[*index], Object::Frame { live: false, .. }))
                    .ok_or(Status::ResourceExhausted)?;
                let number = (object - OBJECT_FIRST_BUDGET_FRAME + 1) as u8;
                self.objects[object] = Object::Frame { number, live: true };
                let id = self.allocate_id();
                self.slots[destination] = Slot::Live(Capability {
                    object,
                    rights,
                    badge: 0,
                    id,
                    parent: 0,
                });
                Ok([id, 0, 0, 0])
            }
            #[cfg(feature = "memory")]
            Operation::FrameMap => {
                let frame = self.typed(request.capability, ObjectType::Frame)?;
                if arguments[2] != 0 || arguments[3] != 0 {
                    return Err(Status::InvalidArgument);
                }
                self.map(frame, arguments[0], arguments[1])
            }
            #[cfg(feature = "memory")]
            Operation::FrameShare => {
                // Sharing is derivation without the cnode: the frame
                // capability itself must carry `grant`.
                let frame = self.typed(request.capability, ObjectType::Frame)?;
                Self::require(&frame, Rights::GRANT)?;
                if arguments[3] != 0 {
                    return Err(Status::InvalidArgument);
                }
                let rights = Self::defined_rights(arguments[1])?;
                if !rights.is_attenuation_of(frame.rights) {
                    return Err(Status::InsufficientRights);
                }
                let destination = self.destination(arguments[0])?;
                let id = self.allocate_id();
                self.slots[destination] = Slot::Live(Capability {
                    object: frame.object,
                    rights,
                    badge: arguments[2],
                    id,
                    parent: frame.id,
                });
                Ok([id, 0, 0, 0])
            }
            #[cfg(feature = "memory")]
            Operation::FrameReclaim => {
                let frame = self.typed(request.capability, ObjectType::Frame)?;
                if arguments != [0; WORDS] {
                    return Err(Status::InvalidArgument);
                }
                // A derived handle is not the frame's owner, and the frame
                // the domain was built with was never the caller's to free.
                if frame.parent != 0 || frame.object == OBJECT_FRAME_0 {
                    return Err(Status::NotPermitted);
                }
                let mut unmapped = 0;
                if let Object::AddressSpace(space) = &mut self.objects[OBJECT_ADDRESS_SPACE] {
                    for page in space.pages.iter_mut() {
                        if page.frame == Some(frame.object) {
                            *page = Page::default();
                            unmapped += 1;
                        }
                    }
                }
                let before = self.slots;
                let mut revoked = 0;
                for index in 0..SLOTS {
                    let descendant = match before[index] {
                        Slot::Live(capability) => {
                            capability.id != frame.id
                                && Self::descends(&before, capability.id, frame.id)
                        }
                        _ => false,
                    };
                    if descendant {
                        self.slots[index] = Slot::Tombstone;
                        revoked += 1;
                    }
                }
                if let Object::Frame { number, .. } = self.objects[frame.object] {
                    self.objects[frame.object] = Object::Frame {
                        number,
                        live: false,
                    };
                }
                self.slots[request.capability as usize] = Slot::Empty;
                Ok([unmapped, revoked, 0, 0])
            }
            #[cfg(feature = "memory")]
            Operation::AsMap => {
                self.address_space(request.capability)?;
                if arguments[3] != 0 {
                    return Err(Status::InvalidArgument);
                }
                let slot = u32::try_from(arguments[0]).map_err(|_| Status::InvalidArgument)?;
                let frame = self.typed(slot, ObjectType::Frame)?;
                self.map(frame, arguments[1], arguments[2])
            }
            #[cfg(feature = "memory")]
            Operation::AsUnmap => {
                let space = self.address_space(request.capability)?;
                if arguments[1] != 0 || arguments[2] != 0 || arguments[3] != 0 {
                    return Err(Status::InvalidArgument);
                }
                let page = Self::page(arguments[0])?;
                let Some(object) = space.pages[page].frame else {
                    return Err(Status::NotFound);
                };
                space.pages[page] = Page::default();
                Ok([self.frame_number(object), 0, 0, 0])
            }
            #[cfg(feature = "memory")]
            Operation::AsProtect => {
                let space = self.address_space(request.capability)?;
                if arguments[2] != 0 || arguments[3] != 0 {
                    return Err(Status::InvalidArgument);
                }
                let page = Self::page(arguments[0])?;
                let rights = Self::mapping_rights(arguments[1])?;
                if space.pages[page].frame.is_none() {
                    return Err(Status::NotFound);
                }
                // A mapping can only lose rights in place, like a capability.
                if !rights.is_attenuation_of(space.pages[page].rights) {
                    return Err(Status::InsufficientRights);
                }
                Self::permitted_together(rights)?;
                space.pages[page].rights = rights;
                Ok([u64::from(rights.0), 0, 0, 0])
            }
            #[cfg(feature = "memory")]
            Operation::AsQuery => {
                let space = self.address_space(request.capability)?;
                if arguments[1] != 0 || arguments[2] != 0 || arguments[3] != 0 {
                    return Err(Status::InvalidArgument);
                }
                let page = Self::page(arguments[0])?;
                let mapping = space.pages[page];
                let Some(object) = mapping.frame else {
                    return Err(Status::NotFound);
                };
                Ok([self.frame_number(object), u64::from(mapping.rights.0), 0, 0])
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

            // -- declared by the contract, in no published profile yet ------
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
        self.objects = [Object::CNode; OBJECTS];
        self.objects[1] = Object::Endpoint(Endpoint {
            queue: [Message::default(); QUEUE],
            head: 0,
            length: 0,
        });
        self.objects[2] = Object::Notification(Notification {
            pending: false,
            badges: 0,
        });
        self.objects[OBJECT_FRAME_0] = Object::Frame {
            number: 0,
            live: true,
        };
        self.objects[4] = Object::Clock { ticks: 0 };
        self.objects[OBJECT_ADDRESS_SPACE] = Object::AddressSpace(AddressSpace {
            #[cfg(feature = "memory")]
            pages: [Page::default(); PAGES],
        });
        #[cfg(feature = "memory")]
        for (offset, object) in self.objects[OBJECT_FIRST_BUDGET_FRAME..]
            .iter_mut()
            .enumerate()
        {
            *object = Object::Frame {
                number: offset as u8 + 1,
                live: false,
            };
        }
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
        self.root(
            slot::ADDRESS_SPACE,
            OBJECT_ADDRESS_SPACE,
            Rights::CONTROL,
            0,
        );
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
    fn both_implementations_agree_under_the_v1_profile_too() {
        use crate::model::group;
        let agreed = conformance::compare(
            &mut ModelKernel::with_profile(group::V1_PROFILE),
            &mut IndependentKernel::with_profile(group::V1_PROFILE),
        )
        .unwrap();
        assert_eq!(agreed, conformance::CORPUS.len());
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
        conformance::transcribe(
            &mut IndependentKernel::with_profile(group::V1_PROFILE),
            &mut transcript,
        )
        .unwrap();
        assert_eq!(
            transcript,
            include_str!("../../../bootstrap/kernel-contract-v1.0.trace")
        );
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
