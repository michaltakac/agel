//! RISC-V on QEMU's `virt` machine.
//!
//! The other verified seL4 target, and the one where the research kernel is
//! most obviously not alone: OpenSBI runs in machine mode underneath it, holds
//! the timer, and constrains what S-mode may touch through physical memory
//! protection. That is a useful reminder of what the whole exercise is about —
//! the layer below decides what the layer above is allowed to do, and here the
//! kernel is on the receiving end of that arrangement.

pub mod hal;

pub mod cpu;
mod domain;
mod memory;

pub use domain::Domain;

use crate::world::Provocation;

/// Short label used in serial reports.
pub const NAME: &str = "riscv64";

/// Start of the frame pool, well above the loaded image and its stack.
pub const POOL_START: u64 = 0x8100_0000;
/// End of the frame pool. QEMU is started with more memory than this; the bound
/// is a deliberate fixed resource policy, not a probe result.
pub const POOL_END: u64 = 0x8400_0000;

/// Physical base of the console device, granted to the driver domain alone.
pub const CONSOLE_DEVICE_PHYSICAL: u64 = 0x1000_0000;

/// Where the driver domain sees the console device in its own address space.
pub const CONSOLE_DEVICE_VADDR: u64 = domain::DEVICE_BASE;

/// A supervisor-only address a world may try to write: the kernel's own text.
pub const KERNEL_PROBE_ADDRESS: u64 = 0x8020_0000;

/// NS16550A transmit-holding and line-status registers on the `virt` machine.
const UART_DATA: *mut u8 = 0x1000_0000 as *mut u8;
const UART_STATUS: *const u8 = 0x1000_0005 as *const u8;
/// Transmit holding register empty.
const UART_TX_READY: u8 = 1 << 5;

/// OpenSBI has already configured the UART it printed its own banner on.
pub fn console_initialize() {}

/// Emit one byte on the NS16550A.
pub fn console_write_byte(byte: u8) {
    // Safety: the device window maps this register for the supervisor only.
    unsafe {
        while UART_STATUS.read_volatile() & UART_TX_READY == 0 {}
        UART_DATA.write_volatile(byte);
    }
}

/// Leave the emulator through the `virt` machine's test device, which can
/// report a status as well as stop.
pub fn exit(success: bool) -> ! {
    // Safety: stopping the timer first means a powered-down hart is not still
    // being woken by an interrupt it will never service.
    unsafe {
        cpu::stop_preemption();
        hal::exit_emulator(success);
    }
    halt()
}

/// Stop this hart permanently with interrupts masked.
pub fn halt() -> ! {
    hal::halt()
}

/// Bounds of the U-mode-executable section.
pub fn user_text_range() -> core::ops::Range<u64> {
    extern "C" {
        static __user_text_start: u8;
        static __user_text_end: u8;
    }
    // Only the addresses are taken; the bytes are never read through these.
    (&raw const __user_text_start) as u64..(&raw const __user_text_end) as u64
}

/// The shared name for a RISC-V exception code.
///
/// RISC-V genuinely does not distinguish a privileged instruction from an
/// undefined one: a U-mode hart reading a supervisor control register and a
/// U-mode hart executing a reserved encoding both raise *illegal instruction*.
/// The provocation table below says so rather than inventing a distinction the
/// architecture does not make.
pub fn fault_name(cause: u64) -> &'static str {
    match cause {
        2 => "illegal-instruction",
        12 | 13 | 15 => "page-fault",
        _ => "exception",
    }
}

/// Every way a RISC-V world can misbehave, and the containment each must earn.
///
/// There is no divide provocation: RISC-V defines a result for division by
/// zero rather than trapping, so a test for it would prove nothing about this
/// machine.
pub const PROVOCATIONS: &[Provocation] = &[
    Provocation {
        command: crate::world::shared::COMMAND_FAULT_WRITE,
        expected: Some("page-fault"),
        description: "writing to kernel memory",
    },
    Provocation {
        command: crate::world::shared::COMMAND_FAULT_PRIVILEGED,
        expected: Some("illegal-instruction"),
        description: "reading a supervisor control register",
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

/// The RISC-V machine: a frame pool and the shared supervisor window above it.
pub struct Machine {
    pool: crate::memory::FramePool,
    identity: memory::IdentityWindow,
}

impl Machine {
    /// Build translation tables the kernel owns, install the trap vector, and
    /// start the preemption timer.
    pub fn bring_up() -> Result<Self, &'static str> {
        hal::mask_interrupts();
        if hal::read_sstatus() & hal::SSTATUS_SPP != 0 {
            // OpenSBI hands control to S-mode with `SPP` clear. Anything else
            // means the firmware did something this bring-up does not expect.
            return Err("kernel was not entered from a lower privilege");
        }
        let mut pool = crate::memory::FramePool::new();
        let identity = memory::build_identity_window(&mut pool, user_text_range())
            .map_err(|error| error.name())?;
        let kernel_space =
            memory::AddressSpace::new(&mut pool, identity).map_err(|error| error.name())?;
        // Safety: the new tables map the kernel image, its stack, and the
        // device window, which is everything this code touches.
        unsafe {
            kernel_space.activate();
            domain::set_kernel_satp(kernel_space.satp());
        }
        let trap_stack = pool.allocate_stack(4).map_err(|error| error.name())?;
        // Safety: called once, in S-mode, with supervisor interrupts masked.
        unsafe {
            cpu::install(trap_stack)?;
            cpu::start_preemption(100);
        }
        Ok(Self { pool, identity })
    }

    /// Build a protection domain entered in U-mode at `entry`.
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
/// OpenSBI loads the ELF and jumps here in S-mode with no stack of ours, so the
/// first thing the kernel does is give itself one out of its own image.
///
/// # Safety
/// Only ever reached from the firmware's handoff.
#[no_mangle]
#[link_section = ".text.entry"]
#[unsafe(naked)]
unsafe extern "C" fn agel_boot() -> ! {
    core::arch::naked_asm!(
        "la sp, {stack}",
        "j {start}",
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
