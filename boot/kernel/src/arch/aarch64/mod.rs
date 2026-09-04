//! AArch64 on QEMU's `virt` machine.
//!
//! This is the architecture the seL4 assurance spike will target, because the
//! verified-configuration table gives AArch64 functional correctness, integrity
//! and availability, and confidentiality — where x86-64 has only functional
//! correctness. Standing the research backend up here first means the contract,
//! the corpus, and the containment tests are already portable when that spike
//! starts.

pub mod hal;

pub mod cpu;
mod domain;
mod memory;

pub use domain::Domain;

use crate::world::Provocation;

/// Short label used in serial reports.
pub const NAME: &str = "aarch64";

/// Start of the frame pool, well above the loaded image and its stack.
pub const POOL_START: u64 = 0x4100_0000;
/// End of the frame pool. QEMU is started with more memory than this; the bound
/// is a deliberate fixed resource policy, not a probe result.
pub const POOL_END: u64 = 0x4400_0000;

/// Physical base of the console device, granted to the driver domain alone.
pub const CONSOLE_DEVICE_PHYSICAL: u64 = 0x0900_0000;

/// Where the driver domain sees the console device in its own address space.
pub const CONSOLE_DEVICE_VADDR: u64 = domain::DEVICE_BASE;

/// A supervisor-only address a world may try to write: the kernel's own text.
pub const KERNEL_PROBE_ADDRESS: u64 = 0x4008_0000;

/// PL011 data and flag registers on the `virt` machine.
const UART_DATA: *mut u8 = 0x0900_0000 as *mut u8;
const UART_FLAGS: *const u32 = 0x0900_0018 as *const u32;
/// Transmit FIFO full.
const UART_TX_FULL: u32 = 1 << 5;

/// The PL011 QEMU hands us is already configured by the machine model, so
/// bring-up has nothing to do beyond agreeing that it exists.
pub fn console_initialize() {}

/// Emit one byte on the PL011.
pub fn console_write_byte(byte: u8) {
    // Safety: the device window maps this register for the supervisor only.
    unsafe {
        while UART_FLAGS.read_volatile() & UART_TX_FULL != 0 {}
        UART_DATA.write_volatile(byte);
    }
}

/// Leave the emulator.
///
/// There is no debug-exit device on this platform, so the machine is powered
/// off through PSCI and the result is read from the serial transcript. The
/// harness requires both a clean shutdown and the success token, so a failure
/// that powers off cannot be mistaken for a pass.
pub fn exit(success: bool) -> ! {
    if !success {
        console_write_byte(b'!');
    }
    // Safety: stopping the timer first means a powered-down machine is not
    // still being woken by an interrupt it will never service.
    unsafe {
        cpu::stop_preemption();
        hal::power_off();
    }
    halt()
}

/// Stop this processor permanently with interrupts masked.
pub fn halt() -> ! {
    hal::halt()
}

/// Bounds of the EL0-executable section.
pub fn user_text_range() -> core::ops::Range<u64> {
    extern "C" {
        static __user_text_start: u8;
        static __user_text_end: u8;
    }
    // Only the addresses are taken; the bytes are never read through these.
    (&raw const __user_text_start) as u64..(&raw const __user_text_end) as u64
}

/// The shared name for an AArch64 exception class.
///
/// `0x18` is a trapped system-register access, which is what an EL0 world earns
/// for naming an EL1 register. `0x00` is "unknown reason", which is what the
/// architecture calls an undefined instruction.
pub fn fault_name(cause: u64) -> &'static str {
    match cause {
        0x00 => "illegal-instruction",
        0x18 => "privileged-instruction",
        0x20 | 0x21 | 0x24 | 0x25 => "page-fault",
        _ => "exception",
    }
}

/// Every way an AArch64 world can misbehave, and the containment each must
/// earn. There is no divide provocation: AArch64 has no integer divide
/// exception, and inventing one for symmetry would be a test that proves
/// nothing about this machine.
pub const PROVOCATIONS: &[Provocation] = &[
    Provocation {
        command: crate::world::shared::COMMAND_FAULT_WRITE,
        expected: Some("page-fault"),
        description: "writing to kernel memory",
    },
    Provocation {
        command: crate::world::shared::COMMAND_FAULT_PRIVILEGED,
        expected: Some("privileged-instruction"),
        description: "reading the timer that preempts it",
    },
    Provocation {
        command: crate::world::shared::COMMAND_FAULT_ILLEGAL,
        expected: Some("illegal-instruction"),
        description: "executing an undefined instruction",
    },
    Provocation {
        command: crate::world::shared::COMMAND_FAULT_DEVICE,
        expected: Some("page-fault"),
        description: "touching a device it was not granted",
    },
    Provocation {
        command: crate::world::shared::COMMAND_SPIN,
        expected: None,
        description: "that never yields",
    },
];

/// The AArch64 machine: a frame pool and the shared supervisor window above it.
pub struct Machine {
    pool: crate::memory::FramePool,
    identity: memory::IdentityWindow,
}

impl Machine {
    /// Build translation tables the kernel owns, install exception vectors, and
    /// start the preemption timer.
    pub fn bring_up() -> Result<Self, &'static str> {
        hal::mask_interrupts();
        if hal::current_exception_level() != 1 {
            // QEMU's `virt` machine enters an ELF kernel at EL1 unless
            // virtualization is enabled. Dropping from EL2 is a different
            // bring-up sequence, so say so rather than misbehave subtly.
            return Err("kernel was not entered at EL1");
        }
        let mut pool = crate::memory::FramePool::new();
        let identity = memory::build_identity_window(&mut pool, user_text_range())
            .map_err(|error| error.name())?;
        let kernel_space =
            memory::AddressSpace::new(&mut pool, identity).map_err(|error| error.name())?;
        // Safety: the new tables map the kernel image, its stack, and the
        // device window, which is everything this code touches.
        unsafe {
            memory::enable_translation(kernel_space.root());
            domain::set_kernel_root(kernel_space.root());
        }
        let trap_stack = pool.allocate_stack(4).map_err(|error| error.name())?;
        // Safety: called once, at EL1, with interrupts masked.
        unsafe {
            cpu::install(trap_stack)?;
            cpu::start_preemption(100);
        }
        Ok(Self { pool, identity })
    }

    /// Build a protection domain entered in EL0 at `entry`.
    pub fn create_world(&mut self, entry: u64, ticks: u32) -> Result<Domain, &'static str> {
        Domain::new(&mut self.pool, self.identity, entry, ticks, None).map_err(|error| error.name())
    }

    /// Build a protection domain that is additionally granted the console
    /// device: one page of device memory, and nothing else.
    pub fn create_console_world(&mut self, entry: u64, ticks: u32) -> Result<Domain, &'static str> {
        Domain::new(
            &mut self.pool,
            self.identity,
            entry,
            ticks,
            Some(CONSOLE_DEVICE_PHYSICAL),
        )
        .map_err(|error| error.name())
    }

    /// Frames the pool has not handed out.
    pub fn frames_remaining(&self) -> u64 {
        self.pool.remaining()
    }
}

/// The image entry point.
///
/// QEMU loads the ELF by its program headers and jumps here with no stack, so
/// the first thing the kernel does is give itself one out of its own image.
///
/// # Safety
/// Only ever reached from the machine's reset path.
#[no_mangle]
#[link_section = ".text.entry"]
#[unsafe(naked)]
unsafe extern "C" fn agel_boot() -> ! {
    core::arch::naked_asm!(
        "adrp x0, {stack}",
        "add x0, x0, :lo12:{stack}",
        "mov sp, x0",
        "b {start}",
        stack = sym __stack_top,
        start = sym start,
    )
}

extern "C" {
    static __stack_top: u8;
}

extern "C" fn start() -> ! {
    crate::zero_bss();
    crate::agel_main()
}
