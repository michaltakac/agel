//! The Agel research kernel.
//!
//! It has two jobs, and it is worth being clear about which is which.
//!
//! The first is the v1.1 Agel workshop: a fixed-memory evaluator and a serial
//! REPL on a reproducible BIOS x86-64 seed. That still runs privileged, and it
//! is a bootstrap implementation rather than a security boundary.
//!
//! The second is the Phase 1 isolation backend: address spaces, trap entry,
//! preemption, and protection domains that answer the Agel kernel contract from
//! the machine's lowest privilege level. That part is architecture-neutral and
//! builds for x86-64, AArch64, and RISC-V from one source.

#![no_std]
#![no_main]

use core::panic::PanicInfo;

mod arch;
mod console;

#[cfg(feature = "isolation-selftest")]
mod world;

#[cfg(not(any(feature = "selftest", feature = "native-selftest")))]
mod monitor;

#[cfg(feature = "isolation-selftest")]
mod contract;
#[cfg(feature = "isolation-selftest")]
mod isolation;
#[cfg(feature = "isolation-selftest")]
mod memory;
#[cfg(feature = "isolation-selftest")]
mod user;

#[cfg(all(
    target_arch = "x86_64",
    not(any(
        feature = "selftest",
        feature = "monitor-selftest",
        feature = "isolation-selftest"
    ))
))]
mod native;
#[cfg(all(
    target_arch = "x86_64",
    not(any(
        feature = "selftest",
        feature = "monitor-selftest",
        feature = "isolation-selftest"
    ))
))]
mod repl;

/// The architecture-neutral kernel entry, called by each boot stub once a stack
/// exists and `.bss` has been zeroed.
pub fn agel_main() -> ! {
    console::initialize();
    console::write("\nAgel v1.4 research kernel: ");
    console::write(arch::NAME);
    console::write("\n");
    console::write("recovery monitor is outside the mutable agent world\n");

    #[cfg(feature = "selftest")]
    {
        console::write("self-check: BIOS seed -> long mode -> Rust HAL [ok]\n");
        console::write("AGEL_BOOT_OK\n");
        arch::exit(true)
    }

    #[cfg(all(feature = "monitor-selftest", not(feature = "selftest")))]
    {
        let mut monitor = monitor::RecoveryMonitor::new();
        monitor.status();
        monitor.promote();
        monitor.verify();
        monitor.promote();
        monitor.status();
        monitor.verify();
        monitor.promote();
        monitor.fault();
        monitor.status();
        console::write("AGEL_MONITOR_OK\n");
        arch::exit(true)
    }

    #[cfg(all(
        feature = "isolation-selftest",
        not(any(feature = "selftest", feature = "monitor-selftest"))
    ))]
    {
        isolation::run()
    }

    #[cfg(all(
        target_arch = "x86_64",
        feature = "native-selftest",
        not(any(
            feature = "selftest",
            feature = "monitor-selftest",
            feature = "isolation-selftest"
        ))
    ))]
    {
        repl::native_selftest()
    }

    #[cfg(all(
        target_arch = "x86_64",
        not(any(
            feature = "selftest",
            feature = "monitor-selftest",
            feature = "native-selftest",
            feature = "isolation-selftest"
        ))
    ))]
    {
        repl::native_repl()
    }
}

/// Zero the `.bss` section named by the linker script.
///
/// Nothing else has: `.bss` is a NOBITS section, so it occupies addresses in
/// the image's memory range but contributes no bytes to the loaded image.
/// Relying on the emulator handing us zeroed memory would make correctness a
/// property of QEMU rather than of this kernel.
pub fn zero_bss() {
    extern "C" {
        static mut __bss_start: u8;
        static mut __bss_end: u8;
    }
    // Safety: the linker guarantees the two symbols bound one contiguous,
    // 8-byte-aligned region that belongs to this image and that nothing has
    // read yet.
    unsafe {
        let mut cursor = &raw mut __bss_start as *mut u64;
        let end = &raw mut __bss_end as *mut u64;
        while cursor < end {
            cursor.write_volatile(0);
            cursor = cursor.add(1);
        }
    }
}

/// A trap taken in supervisor mode is a defect in the kernel itself.
///
/// There is no domain to contain and no state that can be trusted, so the only
/// honest action is to say what fired and stop. Silently continuing would turn
/// a kernel bug into a world that appears to have been contained.
#[cfg(feature = "isolation-selftest")]
pub fn report_supervisor_trap(cause: u64, detail: u64, pc: u64) -> ! {
    kprint!(
        "AGEL_ISOLATION_FAILED: supervisor trap cause {cause:#x} detail {detail:#x} at {pc:#x}\n"
    );
    arch::exit(false)
}

#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    console::write("KERNEL PANIC: recovery monitor halted the mutable world\n");
    arch::halt()
}
