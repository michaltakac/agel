//! What every architecture's protection domains have in common.
//!
//! The register frames, page-table formats, and trap mechanisms differ; the
//! contract between a supervisor and an unprivileged world does not. Keeping
//! that contract here is what lets one conformance corpus and one containment
//! test run unchanged on x86-64, AArch64, and RISC-V.

use crate::arch;

/// Copy fixed supervisor state without calling a compiler-provided memory
/// routine that may also be part of the user evaluator's executable image.
/// Volatile words keep this boundary explicit on every architecture.
///
/// # Safety
/// Both regions must be valid for `bytes`, aligned to eight bytes, and not
/// overlap.
pub unsafe fn copy_supervisor_words(destination: *mut u8, source: *const u8, bytes: usize) {
    let mut offset = 0;
    while offset + core::mem::size_of::<u64>() <= bytes {
        let word = unsafe { source.add(offset).cast::<u64>().read_volatile() };
        unsafe { destination.add(offset).cast::<u64>().write_volatile(word) };
        offset += core::mem::size_of::<u64>();
    }
    while offset < bytes {
        let byte = unsafe { source.add(offset).read_volatile() };
        unsafe { destination.add(offset).write_volatile(byte) };
        offset += 1;
    }
}

/// Pages of stack given to a small contract or driver domain.
pub const STACK_PAGES: u64 = 4;

/// Pages reserved for a native evaluator domain.
///
/// The evaluator keeps its fixed transactional worlds and recursive parser on
/// this private stack. 512 KiB is a hard bound, not a growable heap; the absent
/// page beneath it still turns overflow into a contained fault.
pub const EVALUATOR_STACK_PAGES: u64 = 128;

/// Offsets, in 64-bit words, of the supervisor/world handshake block.
///
/// Bulk data crosses the boundary through this page, never through the
/// contract's control path: kernel IPC carries bounded words and capability
/// handles, and a page of bytes is neither.
pub mod shared {
    /// What the supervisor is asking the world to do.
    pub const COMMAND: usize = 0;
    /// Contract operation code for [`COMMAND_INVOKE`].
    pub const OPERATION: usize = 1;
    /// Capability slot for [`COMMAND_INVOKE`].
    pub const CAPABILITY: usize = 2;
    /// First of four argument words.
    pub const ARGUMENTS: usize = 3;
    /// Canonical status the world observed.
    pub const STATUS: usize = 7;
    /// First of four result words.
    pub const VALUES: usize = 8;

    // The original contract operations stay sparse so their tiny dispatcher
    // remains easy to inspect. Evaluator domains additionally receive the
    // kernel's immutable `.rodata` mapping because compiled Rust dispatch
    // tables and language constants live there; it remains read-only and
    // non-executable.

    /// Perform one contract invocation and report what came back.
    pub const COMMAND_INVOKE: u64 = 0x0001;
    /// Write to an address only the kernel may touch.
    pub const COMMAND_FAULT_WRITE: u64 = 0x1000;
    /// Execute an instruction reserved to the supervisor.
    pub const COMMAND_FAULT_PRIVILEGED: u64 = 0x2000;
    /// Execute an undefined instruction.
    pub const COMMAND_FAULT_ILLEGAL: u64 = 0x3000;
    /// Never return.
    pub const COMMAND_SPIN: u64 = 0x4000;
    /// Write the console payload to the device this domain owns.
    ///
    /// Only the driver domain is granted the device; every other world that
    /// tries this is refused by hardware, which is the point of the command
    /// existing for all of them.
    pub const COMMAND_WRITE_CONSOLE: u64 = 0x6000;
    /// Touch the console device without having been granted it.
    pub const COMMAND_FAULT_DEVICE: u64 = 0x7000;
    /// Evaluate the source bytes in the shared payload using the native Agel
    /// session owned by this domain.
    pub const COMMAND_EVALUATE: u64 = 0x8000;
    /// Roll the evaluator's committed world back by one revision.
    pub const COMMAND_EVALUATOR_ROLLBACK: u64 = 0x8100;
    /// Render the evaluator's persistent definition names.
    pub const COMMAND_EVALUATOR_DEFS: u64 = 0x8200;
    /// Render the evaluator's enforced fixed resource limits.
    pub const COMMAND_EVALUATOR_LIMITS: u64 = 0x8300;
    /// Replace the evaluator session with a fresh empty transactional world.
    /// This is a supervisor-only workspace reconstruction primitive.
    pub const COMMAND_EVALUATOR_RESET: u64 = 0x8400;
    /// Rasterize one validated 64-byte native vector record.
    #[cfg(all(target_arch = "x86_64", feature = "native-graphics"))]
    pub const COMMAND_DISPLAY_DRAW: u64 = 0x9000;
    /// Hash the visible framebuffer from inside its owning domain.
    #[cfg(all(target_arch = "x86_64", feature = "native-graphics"))]
    pub const COMMAND_DISPLAY_CHECKSUM: u64 = 0x9100;
    /// Deliberately touch supervisor memory to prove display fault containment.
    #[cfg(all(target_arch = "x86_64", feature = "native-graphics"))]
    pub const COMMAND_DISPLAY_FAULT: u64 = 0x9200;

    #[cfg(all(target_arch = "x86_64", feature = "native-graphics"))]
    pub const DISPLAY_ADDRESS: usize = 48;
    #[cfg(all(target_arch = "x86_64", feature = "native-graphics"))]
    pub const DISPLAY_WIDTH: usize = 49;
    #[cfg(all(target_arch = "x86_64", feature = "native-graphics"))]
    pub const DISPLAY_HEIGHT: usize = 50;
    #[cfg(all(target_arch = "x86_64", feature = "native-graphics"))]
    pub const DISPLAY_PITCH: usize = 51;
    #[cfg(all(target_arch = "x86_64", feature = "native-graphics"))]
    pub const DISPLAY_LOGICAL_WIDTH: usize = 52;
    #[cfg(all(target_arch = "x86_64", feature = "native-graphics"))]
    pub const DISPLAY_LOGICAL_HEIGHT: usize = 53;
    /// Divide by zero. Only x86-64 traps on this; RISC-V defines a result and
    /// AArch64 has no integer divide exception at all, so the command exists
    /// only where a machine can actually be provoked by it.
    #[cfg(target_arch = "x86_64")]
    pub const COMMAND_FAULT_DIVIDE: u64 = 0x5000;
}

/// Why a domain stopped running.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stop {
    /// The world finished its work and yielded through the supervisor endpoint.
    Replied,
    /// The world took a fault it is not allowed to take.
    Faulted(Fault),
    /// The world used its whole tick budget without yielding.
    BudgetExhausted,
}

/// What the hardware reported when a domain faulted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fault {
    /// The architecture's own trap cause: a vector, an exception class, or a
    /// cause register. It is reported raw as well as named, because a name that
    /// turns out to be wrong should not also hide the evidence.
    pub cause: u64,
    /// Hardware error or syndrome detail, or zero.
    pub detail: u64,
    /// The instruction that faulted.
    pub pc: u64,
    /// The address that faulted, where the architecture reports one.
    pub address: u64,
}

impl Fault {
    /// A short, stable, cross-architecture name for the cause.
    ///
    /// The vocabulary is shared so the containment tests read the same on every
    /// backend, but the mapping is per-architecture and deliberately not
    /// flattened: RISC-V really cannot distinguish a privileged instruction
    /// from an undefined one, and saying otherwise would be a lie told for the
    /// convenience of a test.
    pub fn name(&self) -> &'static str {
        arch::fault_name(self.cause)
    }
}

/// One deliberate misbehaviour and the containment it must produce.
pub struct Provocation {
    /// Command written into the shared page.
    pub command: u64,
    /// The [`Fault::name`] the supervisor must observe, or `None` when the
    /// world is expected to be preempted rather than to fault.
    pub expected: Option<&'static str>,
    /// Human description used in the serial report.
    pub description: &'static str,
}

/// The half of a protection domain that has nothing to do with the machine.
///
/// Register frames, page tables, and trap entry are per-architecture; the
/// capability space, the shared handshake page, the tick budget, and the rule
/// that a stopped world stays stopped are not. Keeping them here means the
/// three backends cannot quietly disagree about what a domain *is*.
pub struct DomainCore {
    objects: crate::contract::DomainObjects,
    shared_physical: u64,
    ticks: u32,
    tick_budget: u32,
    stop: Option<Stop>,
}

impl DomainCore {
    /// A domain holding the conformance capability space, sharing the physical
    /// frame at `shared_physical` with its supervisor.
    pub fn new(shared_physical: u64, tick_budget: u32) -> Self {
        Self {
            objects: crate::contract::DomainObjects::new(),
            shared_physical,
            ticks: 0,
            tick_budget,
            stop: None,
        }
    }

    /// Write one word of the shared handshake block.
    pub fn write_shared(&mut self, index: usize, value: u64) {
        // Safety: the frame came from the pool, is identity mapped for the
        // kernel, and `index` is masked into the page.
        unsafe {
            (self.shared_physical as *mut u64)
                .add(index & 0x1ff)
                .write_volatile(value)
        };
    }

    /// Read one word of the shared handshake block.
    ///
    /// The value is whatever an unprivileged world put there. It is data to be
    /// validated, never a kernel decision.
    pub fn read_shared(&self, index: usize) -> u64 {
        // Safety: as in `write_shared`.
        unsafe {
            (self.shared_physical as *const u64)
                .add(index & 0x1ff)
                .read_volatile()
        }
    }

    /// Place one contract invocation in the shared page for the world to make.
    pub fn stage_invocation(&mut self, request: &agel_kernel_abi::Request) {
        self.write_shared(shared::OPERATION, u64::from(request.operation.code()));
        self.write_shared(shared::CAPABILITY, u64::from(request.capability));
        for (offset, word) in request.arguments.iter().enumerate() {
            self.write_shared(shared::ARGUMENTS + offset, *word);
        }
        self.write_shared(shared::COMMAND, shared::COMMAND_INVOKE);
    }

    /// Ask the world to do something other than answer the contract.
    pub fn stage_command(&mut self, command: u64) {
        self.write_shared(shared::COMMAND, command);
    }

    /// Read back what the world reported, validating it as untrusted input.
    pub fn collect_response(&self) -> agel_kernel_abi::Response {
        use agel_kernel_abi::{Response, Status};
        let status = Status::from_code(self.read_shared(shared::STATUS) as u16)
            .unwrap_or(Status::InvalidOperation);
        let mut values = [0_u64; agel_kernel_abi::WORDS];
        for (offset, word) in values.iter_mut().enumerate() {
            *word = self.read_shared(shared::VALUES + offset);
        }
        if status == Status::Ok {
            Response::ok(values)
        } else {
            Response::fail(status)
        }
    }

    /// Answer one contract invocation from an unprivileged world.
    ///
    /// `None` means the world was not asking a question: it was handing control
    /// back to its supervisor, and must not be resumed from this trap.
    pub fn syscall(
        &mut self,
        operation: u64,
        capability: u64,
        arguments: [u64; agel_kernel_abi::WORDS],
    ) -> Option<agel_kernel_abi::Response> {
        use agel_kernel_abi::{Operation, Request, Response, Status};
        let Some(operation) = Operation::from_code(operation as u16) else {
            return Some(Response::fail(Status::InvalidOperation));
        };
        let Ok(capability) = u32::try_from(capability) else {
            return Some(Response::fail(Status::InvalidCapability));
        };
        let request = Request::with(operation, capability, arguments);
        if self.objects.is_supervisor_yield(&request) {
            self.record_stop(Stop::Replied);
            return None;
        }
        Some(self.objects.invoke(&request))
    }

    /// Start a fresh entry: the tick budget is per entry, not per lifetime.
    pub fn begin_entry(&mut self) {
        self.ticks = 0;
    }

    /// Charge one timer tick. `false` means the budget is spent.
    pub fn charge_tick(&mut self) -> bool {
        self.ticks = self.ticks.saturating_add(1);
        if self.ticks <= self.tick_budget {
            return true;
        }
        self.record_stop(Stop::BudgetExhausted);
        false
    }

    /// Record why the domain stopped.
    ///
    /// Only a fault or an overrun latches. A world that yields politely is
    /// expected to be entered again.
    pub fn record_stop(&mut self, stop: Stop) {
        if !matches!(stop, Stop::Replied) {
            self.stop = Some(stop);
        }
    }

    /// The latched stop reason, if there is one.
    pub fn stopped(&self) -> Option<Stop> {
        self.stop
    }

    /// What [`Stop`] a completed entry should report.
    pub fn outcome(&self) -> Stop {
        self.stop.unwrap_or(Stop::Replied)
    }
}

/// Byte offset in the shared page where console payload bytes begin.
///
/// The handshake words occupy the start of the page; bytes a domain is asked to
/// print start well past them so a long line cannot walk into the protocol.
pub const PAYLOAD_OFFSET: usize = 128;

/// Bytes of console payload one request may carry.
pub const PAYLOAD_BYTES: usize = 256;

impl DomainCore {
    /// Write one byte of the console payload area.
    pub fn write_payload(&mut self, offset: usize, byte: u8) {
        let offset = PAYLOAD_OFFSET + (offset % PAYLOAD_BYTES);
        // Safety: the frame came from the pool, is identity mapped for the
        // kernel, and the offset is inside the payload area of the page.
        unsafe {
            (self.shared_physical as *mut u8)
                .add(offset)
                .write_volatile(byte)
        };
    }

    /// Read one untrusted byte from the shared payload area.
    pub fn read_payload(&self, offset: usize) -> u8 {
        let offset = PAYLOAD_OFFSET + (offset % PAYLOAD_BYTES);
        // Safety: as in `write_payload`; the result remains untrusted data.
        unsafe {
            (self.shared_physical as *const u8)
                .add(offset)
                .read_volatile()
        }
    }
}
