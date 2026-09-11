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
    /// Where a storage driver domain on a machine with memory-mapped devices
    /// finds its register window (a virtual address in its own space) and the
    /// physical address of its DMA page. Written once by the supervisor when
    /// the domain is built; the driver treats both as configuration, not as
    /// authority, since it can reach nothing else either way.
    #[cfg(not(target_arch = "x86_64"))]
    pub const DEVICE_MMIO: usize = 12;
    #[cfg(not(target_arch = "x86_64"))]
    pub const DEVICE_DMA: usize = 13;

    // The original contract operations stay sparse so their tiny dispatcher
    // remains easy to inspect. Evaluator domains additionally receive the
    // kernel's immutable `.rodata` mapping because compiled Rust dispatch
    // tables and language constants live there; it remains read-only and
    // non-executable.

    /// Perform one contract invocation and report what came back.
    pub const COMMAND_INVOKE: u64 = 0x0001;
    /// Touch a page of the frame window: the first argument word is the page,
    /// the second a value to write when the third is non-zero; the first
    /// result word is what the page then holds. What happens is the page
    /// tables' decision, which is the point of asking.
    #[cfg(feature = "contract-memory")]
    pub const COMMAND_TOUCH_WINDOW: u64 = 0x5100;
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
    /// Read one byte from the console device if one is waiting; the driver
    /// answers status 1 with the byte in the first value word, or status 0.
    pub const COMMAND_READ_CONSOLE: u64 = 0x6100;
    /// Read one byte from the 8042 controller if one is waiting; the second
    /// value word says whether it came from the auxiliary (pointer) device.
    #[cfg(all(target_arch = "x86_64", feature = "native-graphics"))]
    pub const COMMAND_READ_INPUT: u64 = 0x6200;
    /// Enable the PS/2 pointer; status 1 when the device acknowledged.
    #[cfg(all(target_arch = "x86_64", feature = "native-graphics"))]
    pub const COMMAND_ENABLE_POINTER: u64 = 0x6300;
    /// Touch the 8042 controller without having been granted it.
    #[cfg(target_arch = "x86_64")]
    pub const COMMAND_FAULT_INPUT_DEVICE: u64 = 0x6400;
    /// Ask the clock driver for the CMOS real-time clock: the answer's first
    /// value packs seconds, minutes, hours, day, month and year (from 2000)
    /// as bytes from the low end; status 0 means the clock did not answer.
    #[cfg(all(target_arch = "x86_64", feature = "native-graphics"))]
    pub const COMMAND_READ_CLOCK: u64 = 0x6500;
    /// Read the sector named by the first argument word into the block area.
    pub const COMMAND_READ_SECTOR: u64 = 0xa000;
    /// Write the block area to the sector named by the first argument word.
    pub const COMMAND_WRITE_SECTOR: u64 = 0xa100;
    /// Flush the disk's write cache.
    pub const COMMAND_FLUSH_DISK: u64 = 0xa200;
    /// Touch the disk controller without having been granted it.
    pub const COMMAND_FAULT_STORAGE_DEVICE: u64 = 0xa300;
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
    /// Read one committed native scene rectangle, without evaluating code.
    pub const COMMAND_EVALUATOR_SCENE: u64 = 0x8500;
    pub const COMMAND_EVALUATOR_PREVIEW: u64 = 0x8600;
    pub const COMMAND_EVALUATOR_PROMOTE: u64 = 0x8700;
    pub const COMMAND_EVALUATOR_DISCARD: u64 = 0x8800;
    pub const COMMAND_EVALUATOR_SOURCE: u64 = 0x8900;
    pub const COMMAND_EVALUATOR_REBUILD: u64 = 0x8a00;
    pub const COMMAND_EVALUATOR_STAGE: u64 = 0x8b00;
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
    /// The compositor's assets: for slot `i` in 0..4, the address the asset
    /// is mapped at and its length in bytes, zero for none. Slots 0 to 2
    /// are font faces (regular sans, medium sans, mono), slot 3 the sprite
    /// sheet. Words 54 to 61.
    #[cfg(feature = "native-graphics")]
    pub const ASSET_WORDS: usize = 54;
    #[cfg(feature = "native-graphics")]
    pub const ASSET_SLOTS: usize = 4;
    #[cfg(feature = "native-graphics")]
    pub const SPRITE_SLOT: usize = 3;
    /// A clip rectangle for drawing: x, y, width, height in physical pixels;
    /// a zero width means the whole surface. Words 62 to 65.
    #[cfg(feature = "native-graphics")]
    pub const CLIP_X: usize = 62;
    #[cfg(feature = "native-graphics")]
    pub const CLIP_Y: usize = 63;
    #[cfg(feature = "native-graphics")]
    pub const CLIP_WIDTH: usize = 64;
    #[cfg(feature = "native-graphics")]
    pub const CLIP_HEIGHT: usize = 65;
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
#[cfg(not(any(feature = "isolated-repl", feature = "native-graphics")))]
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

    /// The frame window's contents at `page`, from the object table.
    #[cfg(feature = "contract-memory")]
    pub fn mapping(&self, page: usize) -> Option<(u8, agel_kernel_abi::Rights)> {
        self.objects.mapping(page)
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
    #[cfg(not(any(feature = "isolated-repl", feature = "native-graphics")))]
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
    #[cfg(not(any(feature = "isolated-repl", feature = "native-graphics")))]
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

/// Tick budget of a storage driver entry. A disk request is a wait on a
/// device, and on an emulated machine with a slow host disk a flush can take
/// most of a second; the driver's own poll bound is set to expire inside this
/// budget, so a device that never answers costs one request's wait and a
/// clean timeout status rather than the driver.
pub const STORAGE_TICKS: u32 = 300;

/// The request block a loaded process fills in its shared page before it
/// yields to its supervisor. It sits past the handshake words the supervisor
/// owns and before the payload area, and the process's data travels in the
/// block area. Nothing here is a contract operation: it is the supervisor's
/// service protocol, like a driver's, and the process holds no authority
/// beyond what the supervisor answers.
#[cfg(feature = "process")]
pub mod process {
    /// What the process is asking for.
    pub const KIND: usize = 64;
    /// First of four argument words.
    pub const ARGUMENTS: usize = 65;
    /// The supervisor's answer: a result or a negated error.
    pub const RESULT: usize = 69;
    /// Leave with the status in the first argument word.
    pub const EXIT: u64 = 1;
    /// Write the block area's first `arguments[1]` bytes to descriptor
    /// `arguments[0]`. Descriptors 1 and 2 are the console; 3 and up are
    /// files opened through the process's namespace.
    pub const WRITE: u64 = 2;
    /// Open the path in the payload area (`arguments[1]` bytes) with the
    /// flags in `arguments[0]`, resolved inside the process's namespace;
    /// answers a descriptor or a negated error number.
    pub const OPEN: u64 = 3;
    /// Read up to `arguments[1]` bytes from descriptor `arguments[0]` into
    /// the block area; answers the count, zero at the end of the file.
    pub const READ: u64 = 4;
    /// Close descriptor `arguments[0]`.
    pub const CLOSE: u64 = 5;
    /// Start the program named in the payload area (`arguments[0]` bytes)
    /// as a child: its descriptor 0 is the parent's `arguments[1]` and its
    /// descriptor 1 the parent's `arguments[2]` (`NO_DESCRIPTOR` for none),
    /// its descriptor 2 the console, its namespace the parent's, read-only
    /// when `arguments[3]` has `SPAWN_READ_ONLY`. Answers the child's id.
    /// A child receives exactly what is named here; nothing is inherited.
    pub const SPAWN: u64 = 6;
    /// Make a pipe; answers the read descriptor in the low sixteen bits and
    /// the write descriptor in the next sixteen.
    pub const PIPE: u64 = 7;
    /// Wait for the child with id `arguments[0]` to end; answers its exit
    /// status, or `WAIT_SIGNALED` with the signal number when the machine
    /// stopped it. Blocks the caller until then.
    pub const WAIT: u64 = 8;
    /// Move descriptor `arguments[0]`'s offset: to `arguments[1]` from the
    /// start (`arguments[2]` = 0), the current offset (1) or the end (2);
    /// answers the new offset. Pipes and the console have no offset.
    pub const SEEK: u64 = 9;
    /// Ask the display for a window of `arguments[0]` by `arguments[1]`
    /// pixels of content, titled by the payload area's first
    /// `arguments[2]` bytes; answers the window's number. `-ENODEV` where
    /// there is no display (the serial workshop), `-EBUSY` when every
    /// window is taken or the process already owns one, `-EINVAL` for a
    /// size outside `WINDOW_MIN`..`WINDOW_MAX`. Graphics only.
    pub const WINDOW: u64 = 10;
    /// Draw the block area's first `arguments[1]` records (64 bytes each,
    /// at most `DRAW_RECORDS`) into window `arguments[0]`, relative to its
    /// content; `arguments[2]` with `DRAW_CLEAR` empties the window first.
    /// The supervisor keeps the records and repaints them with the desktop,
    /// so nothing a process draws outlives a check: every record must be a
    /// permitted operation lying wholly inside the window's content, or the
    /// whole request is `-EINVAL` and nothing is drawn. Answers the number
    /// of records the window now holds; `-ENOSPC` when they would exceed
    /// `WINDOW_RECORDS`, `-EBADF` for a window the process does not own.
    pub const DRAW: u64 = 11;
    /// The next event for window `arguments[0]`, which the process must
    /// own: a press in its content or a key while it has focus, packed as
    /// `EVENT_*`; 0 when there is none, or, with `arguments[1]` set, the
    /// process waits until there is one and the desktop runs meanwhile.
    /// `-ENODEV` without a display, `-EBADF` for a window not its own.
    pub const EVENT: u64 = 12;
    /// An event's kind is its top byte; a press carries the content
    /// coordinates in bits 32..48 and 16..32, a key its byte in the low
    /// eight bits.
    #[cfg(feature = "native-graphics")]
    pub const EVENT_PRESS: u64 = 1 << 56;
    #[cfg(feature = "native-graphics")]
    pub const EVENT_KEY: u64 = 2 << 56;
    /// Events a window queues before the oldest is dropped.
    #[cfg(feature = "native-graphics")]
    pub const WINDOW_EVENTS: usize = 8;
    #[cfg(feature = "native-graphics")]
    pub const DRAW_CLEAR: u64 = 1;
    /// Records one draw request carries: the block area's 512 bytes.
    pub const DRAW_RECORDS: usize = 8;
    /// Windows the desktop keeps at once, and what one retains.
    #[cfg(feature = "native-graphics")]
    pub const WINDOWS: usize = 2;
    #[cfg(feature = "native-graphics")]
    pub const WINDOW_RECORDS: usize = 24;
    /// The smallest and largest content a window may have.
    #[cfg(feature = "native-graphics")]
    pub const WINDOW_MIN: (u32, u32) = (64, 48);
    #[cfg(feature = "native-graphics")]
    pub const WINDOW_MAX: (u32, u32) = (1280, 720);
    /// The number of NUL-terminated arguments the supervisor placed in the
    /// payload area before the process first ran; the first is its name.
    pub const ARGUMENT_COUNT: usize = 70;
    /// A descriptor argument that names none.
    pub const NO_DESCRIPTOR: u64 = 0xffff;
    pub const SPAWN_READ_ONLY: u64 = 1;
    /// Set in a `wait` answer when the child did not exit but was stopped:
    /// the low byte is then the signal a POSIX parent would see.
    pub const WAIT_SIGNALED: u64 = 0x100;
    pub const SIGNAL_KILLED: u64 = 9;
    pub const SIGNAL_FAULT: u64 = 11;
    /// Processes that may exist at once, the `:exec`'d one included.
    pub const PROCESSES: usize = 4;
    /// Pipes that may exist at once, and what one holds.
    pub const PIPES: usize = 4;
    pub const PIPE_BYTES: usize = 512;
    /// `open` flags, the POSIX values.
    pub const O_WRONLY: u64 = 0o1;
    pub const O_RDWR: u64 = 0o2;
    pub const O_CREAT: u64 = 0o100;
    pub const O_APPEND: u64 = 0o2000;
    pub const O_DIRECTORY: u64 = 0o200000;
    /// Descriptors a process may hold at once, numbered from 3.
    pub const DESCRIPTORS: usize = 16;
    /// Stack pages a process is built with.
    pub const STACK_PAGES: u64 = 16;
    /// Tick budget per entry: a process that computes for longer yields
    /// nothing and is stopped like any world that never yields.
    pub const TICKS: u32 = 100;
}

/// The filesystem service's protocol: commands the supervisor issues on a
/// process's behalf, with the answer in the status and value words, and
/// the sector requests the service makes of the supervisor while it works,
/// which the supervisor serves through the storage driver domain and then
/// resumes the service. The service never names a device; it names sectors
/// inside the region it was told it owns.
#[cfg(feature = "process")]
pub mod fs {
    /// Write an empty filesystem over the region.
    pub const COMMAND_FORMAT: u64 = 0xb000;
    /// Resolve the payload path from directory `arguments[0]` with `open`
    /// flags `arguments[1]` and length `arguments[2]`; values: entry, length,
    /// kind.
    pub const COMMAND_OPEN: u64 = 0xb100;
    /// Read `arguments[2]` bytes at offset `arguments[1]` of entry
    /// `arguments[0]` into the block area; values: bytes read.
    pub const COMMAND_READ: u64 = 0xb200;
    /// Write the block area's `arguments[2]` bytes at offset `arguments[1]`
    /// of entry `arguments[0]`; values: bytes written.
    pub const COMMAND_WRITE: u64 = 0xb300;
    /// The `arguments[1]`-th child of directory `arguments[0]`: its name in
    /// the payload, values: entry, kind, length, or status `not found` past
    /// the last.
    pub const COMMAND_LIST: u64 = 0xb400;
    /// Sector request words the service fills before it yields mid-command:
    /// operation (0 none, 1 read, 2 write), sector, and the answer.
    pub const DISK_OPERATION: usize = 72;
    pub const DISK_SECTOR: usize = 73;
    pub const DISK_STATUS: usize = 74;
    pub const DISK_READ: u64 = 1;
    pub const DISK_WRITE: u64 = 2;
    /// The region the service owns on the disk, inclusive.
    pub const FIRST_SECTOR: u32 = 1536;
    pub const LAST_SECTOR: u32 = 2047;
    /// Kinds of directory entry.
    pub const KIND_FILE: u64 = 1;
    pub const KIND_DIRECTORY: u64 = 2;
    /// The most a file may hold: one extent.
    pub const FILE_BYTES: u64 = 4096;
    /// Entries in the directory, the root included.
    pub const ENTRIES: u64 = 32;
    /// Error numbers the service answers with, POSIX values.
    pub const ENOENT: u64 = 2;
    pub const EIO: u64 = 5;
    pub const EACCES: u64 = 13;
    pub const ENOTDIR: u64 = 20;
    pub const EISDIR: u64 = 21;
    pub const EINVAL: u64 = 22;
    pub const EFBIG: u64 = 27;
    pub const ENOSPC: u64 = 28;
    /// `open` flag bits the service interprets, the POSIX values.
    pub const O_WRONLY_BIT: u64 = 0o1;
    pub const O_RDWR_BIT: u64 = 0o2;
    pub const O_CREAT_BIT: u64 = 0o100;
    pub const O_DIRECTORY_BIT: u64 = 0o200000;
    /// Stack pages the service is built with.
    pub const STACK_PAGES: u64 = 8;
    /// Its tick budget per entry, generous because a command may relay
    /// several sectors.
    pub const TICKS: u32 = 300;
}

/// The native scene's geometry, as the evaluator world defines it (it
/// validates language-drawn rectangles first) and the screen's height.
#[cfg(feature = "native-graphics")]
pub use crate::native::{SCENE_DRAWABLE_HEIGHT, SCENE_WIDTH};
#[cfg(feature = "native-graphics")]
pub const SCENE_HEIGHT: u32 = 1080;

/// Byte offset in the shared page where console payload bytes begin.
///
/// The handshake words occupy the start of the page; bytes a domain is asked to
/// print start well past them so a long line cannot walk into the protocol.
pub const PAYLOAD_OFFSET: usize = 128;

/// Bytes of console payload one request may carry.
pub const PAYLOAD_BYTES: usize = 256;

/// Byte offset in the shared page of the one-sector block area a storage
/// driver domain reads from and writes to. It sits well past the text payload
/// so the two can never overlap.
pub const BLOCK_OFFSET: usize = 1024;

/// Bytes in the block area: exactly one disk sector.
pub const BLOCK_BYTES: usize = 512;

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

    /// Write one byte of the block area.
    #[cfg(any(feature = "isolated-repl", feature = "native-graphics"))]
    pub fn write_block(&mut self, offset: usize, byte: u8) {
        let offset = BLOCK_OFFSET + (offset % BLOCK_BYTES);
        // Safety: as in `write_payload`; the block area is inside the page.
        unsafe {
            (self.shared_physical as *mut u8)
                .add(offset)
                .write_volatile(byte)
        };
    }

    /// Read one untrusted byte of the block area.
    pub fn read_block(&self, offset: usize) -> u8 {
        let offset = BLOCK_OFFSET + (offset % BLOCK_BYTES);
        // Safety: as in `read_payload`.
        unsafe {
            (self.shared_physical as *const u8)
                .add(offset)
                .read_volatile()
        }
    }
}
