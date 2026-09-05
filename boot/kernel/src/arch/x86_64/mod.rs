//! x86-64: the BIOS bootstrap seed and the first isolation backend.
//!
//! This is the architecture Agel's native work started on, so it carries the
//! extra weight the others do not: a 512-byte BIOS stage, a 1 MiB raw disk
//! image, and the serial Agel workshop. The isolation half of it answers the
//! same contract as AArch64 and RISC-V.

pub mod hal;

#[cfg(feature = "isolated-repl")]
mod disk;

#[cfg(feature = "isolation-selftest")]
pub mod cpu;
#[cfg(feature = "isolation-selftest")]
mod domain;
#[cfg(feature = "isolation-selftest")]
mod memory;

#[cfg(feature = "isolation-selftest")]
pub use domain::Domain;

#[cfg(feature = "isolation-selftest")]
use crate::world::Provocation;

/// Short label used in serial reports.
pub const NAME: &str = "x86_64";

/// Start of the frame pool. Everything below is BIOS structures, the boot
/// sector, the kernel image, and the kernel stack.
#[cfg(feature = "isolation-selftest")]
pub const POOL_START: u64 = 0x0020_0000;
/// End of the frame pool. QEMU is started with more memory than this; the bound
/// is a deliberate fixed resource policy, not a probe result.
#[cfg(feature = "isolation-selftest")]
pub const POOL_END: u64 = 0x0100_0000;

/// A supervisor-only address a world may try to write. It is the first page of
/// the kernel image, which every domain maps without the user bit.
#[cfg(feature = "isolation-selftest")]
pub const KERNEL_PROBE_ADDRESS: u64 = 0x0001_0000;

const COM1: u16 = 0x3f8;
const DEBUG_EXIT_PORT: u16 = 0xf4;

/// Prepare COM1.
pub fn console_initialize() {
    unsafe {
        hal::out8(COM1 + 1, 0x00);
        hal::out8(COM1 + 3, 0x80);
        hal::out8(COM1, 0x03);
        hal::out8(COM1 + 1, 0x00);
        hal::out8(COM1 + 3, 0x03);
        hal::out8(COM1 + 2, 0xc7);
        hal::out8(COM1 + 4, 0x0b);
    }
}

/// Emit one byte on COM1.
pub fn console_write_byte(byte: u8) {
    while unsafe { hal::in8(COM1 + 5) } & 0x20 == 0 {}
    unsafe { hal::out8(COM1, byte) };
}

/// Block until COM1 delivers a byte.
#[cfg(any(
    feature = "isolated-repl",
    not(any(
        feature = "selftest",
        feature = "monitor-selftest",
        feature = "native-selftest",
        feature = "isolation-selftest"
    ))
))]
pub fn console_read_byte() -> u8 {
    while unsafe { hal::in8(COM1 + 5) } & 1 == 0 {}
    unsafe { hal::in8(COM1) }
}

#[cfg(feature = "isolated-repl")]
pub use disk::{flush_disk, read_disk_sector, write_disk_sector};

/// Leave QEMU through the debug-exit device.
///
/// The device maps a guest value `v` to host status `(v << 1) | 1`, so 0x10
/// becomes 33 and 0x11 becomes 35. Without the device the write is inert and
/// the machine simply halts, which is the right behavior on real hardware.
pub fn exit(success: bool) -> ! {
    unsafe { hal::out32(DEBUG_EXIT_PORT, if success { 0x10 } else { 0x11 }) };
    halt()
}

/// Stop this processor permanently with interrupts masked.
pub fn halt() -> ! {
    hal::halt()
}

/// Bounds of the user-executable section.
#[cfg(feature = "isolation-selftest")]
pub fn user_text_range() -> core::ops::Range<u64> {
    extern "C" {
        static __user_text_start: u8;
        static __user_text_end: u8;
    }
    // Only the addresses are taken; the bytes are never read through these.
    (&raw const __user_text_start) as u64..(&raw const __user_text_end) as u64
}

/// Bounds of immutable data readable by evaluator domains.
#[cfg(feature = "isolation-selftest")]
pub fn user_rodata_range() -> core::ops::Range<u64> {
    extern "C" {
        static __rodata_start: u8;
        static __rodata_end: u8;
    }
    (&raw const __rodata_start) as u64..(&raw const __rodata_end) as u64
}

/// The shared name for an x86-64 trap vector.
#[cfg(feature = "isolation-selftest")]
pub fn fault_name(cause: u64) -> &'static str {
    match cause {
        0 => "divide-error",
        6 => "invalid-opcode",
        8 => "double-fault",
        13 => "general-protection",
        14 => "page-fault",
        _ => "exception",
    }
}

/// Every way an x86-64 world can misbehave, and the containment each must earn.
#[cfg(feature = "isolation-selftest")]
pub const PROVOCATIONS: &[Provocation] = &[
    Provocation {
        command: crate::world::shared::COMMAND_FAULT_WRITE,
        expected: Some("page-fault"),
        description: "writing to kernel memory",
    },
    Provocation {
        command: crate::world::shared::COMMAND_FAULT_PRIVILEGED,
        expected: Some("general-protection"),
        description: "masking interrupts",
    },
    Provocation {
        command: crate::world::shared::COMMAND_FAULT_ILLEGAL,
        expected: Some("invalid-opcode"),
        description: "executing an undefined instruction",
    },
    Provocation {
        command: crate::world::shared::COMMAND_FAULT_DIVIDE,
        expected: Some("divide-error"),
        description: "dividing by zero",
    },
    Provocation {
        command: crate::world::shared::COMMAND_FAULT_DEVICE,
        expected: Some("general-protection"),
        description: "touching a device it was not granted",
    },
    Provocation {
        command: crate::world::shared::COMMAND_SPIN,
        expected: None,
        description: "that never yields",
    },
];

/// The x86-64 machine: a frame pool and the shared identity window above it.
#[cfg(feature = "isolation-selftest")]
pub struct Machine {
    pool: crate::memory::FramePool,
    identity: u64,
}

#[cfg(feature = "isolation-selftest")]
impl Machine {
    /// Build page tables the kernel owns, install descriptors and trap entry,
    /// and start the preemption timer.
    pub fn bring_up() -> Result<Self, &'static str> {
        hal::disable_interrupts();
        if !memory::enable_no_execute() {
            return Err("processor does not support the no-execute bit");
        }
        let mut pool = crate::memory::FramePool::new();
        let kernel_identity =
            memory::build_identity_window(&mut pool, 0..0, 0..0).map_err(|error| error.name())?;
        let identity =
            memory::build_identity_window(&mut pool, user_text_range(), user_rodata_range())
                .map_err(|error| error.name())?;
        let kernel_space =
            memory::AddressSpace::new(&mut pool, kernel_identity).map_err(|error| error.name())?;
        // Safety: the new space maps every address this code and its stack use.
        unsafe {
            kernel_space.activate();
            domain::set_kernel_root(kernel_space.root());
        }
        let trap_stack = pool.allocate_stack(4).map_err(|error| error.name())?;
        let fault_stack = pool.allocate_stack(2).map_err(|error| error.name())?;
        // Safety: called once, from ring 0, with interrupts disabled, and with
        // two distinct kernel stacks.
        unsafe {
            cpu::install(trap_stack, fault_stack);
            // 1_193_182 Hz / 11_932 is very close to 100 Hz.
            cpu::remap_interrupts(11_932);
        }
        Ok(Self { pool, identity })
    }

    /// Build a protection domain entered in ring 3 at `entry`.
    pub fn create_world(&mut self, entry: u64, ticks: u32) -> Result<Domain, &'static str> {
        Domain::new(
            &mut self.pool,
            self.identity,
            entry,
            ticks,
            false,
            crate::world::STACK_PAGES,
        )
        .map_err(|error| error.name())
    }

    /// Build a domain with the fixed stack budget required by the native evaluator.
    pub fn create_evaluator_world(
        &mut self,
        entry: u64,
        ticks: u32,
    ) -> Result<Domain, &'static str> {
        Domain::new(
            &mut self.pool,
            self.identity,
            entry,
            ticks,
            false,
            crate::world::EVALUATOR_STACK_PAGES,
        )
        .map_err(|error| error.name())
    }

    /// Build a protection domain that is additionally granted the console
    /// device: on x86-64, eight I/O ports and nothing else.
    pub fn create_console_world(&mut self, entry: u64, ticks: u32) -> Result<Domain, &'static str> {
        Domain::new(
            &mut self.pool,
            self.identity,
            entry,
            ticks,
            true,
            crate::world::STACK_PAGES,
        )
        .map_err(|error| error.name())
    }

    /// Frames the pool has not handed out.
    pub fn frames_remaining(&self) -> u64 {
        self.pool.remaining()
    }
}

/// The BIOS stage's entry point.
///
/// The linker keeps `.text.entry` first so helper-function reordering cannot
/// move the address the 512-byte stage calls.
#[no_mangle]
#[link_section = ".text.entry"]
extern "C" fn agel_boot() -> ! {
    crate::zero_bss();
    crate::agel_main()
}
