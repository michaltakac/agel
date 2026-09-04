//! The complete set of privileged RISC-V operations the research kernel uses.
//!
//! Every instruction here is one a U-mode world is not allowed to execute.
//! Having them in one file means the privileged surface can be read in a
//! sitting and reviewed as a whole; nothing here makes a policy decision.

use core::arch::asm;

/// `sstatus.SIE`: supervisor interrupts enabled.
pub const SSTATUS_SIE: u64 = 1 << 1;
/// `sstatus.SPIE`: the interrupt-enable state to restore on `sret`.
pub const SSTATUS_SPIE: u64 = 1 << 5;
/// `sstatus.SPP`: the privilege `sret` returns to. Zero is U-mode.
pub const SSTATUS_SPP: u64 = 1 << 8;
/// `sie.STIE`: supervisor timer interrupts.
pub const SIE_STIE: u64 = 1 << 5;

/// Install the trap vector in direct mode.
///
/// # Safety
/// `handler` must be a four-byte-aligned trap entry point that stays mapped.
pub unsafe fn write_stvec(handler: u64) {
    unsafe { asm!("csrw stvec, {}", in(reg) handler, options(nomem, nostack)) };
}

/// Install a page-table root and flush the translation caches.
///
/// # Safety
/// `satp` must name a correctly formed Sv39 root mapping every address the
/// kernel is about to execute from and touch.
pub unsafe fn write_satp(satp: u64) {
    unsafe {
        asm!(
            "csrw satp, {satp}",
            "sfence.vma",
            satp = in(reg) satp,
            options(nostack),
        )
    };
}

/// Read `sstatus`.
pub fn read_sstatus() -> u64 {
    let value: u64;
    unsafe { asm!("csrr {}, sstatus", out(reg) value, options(nomem, nostack)) };
    value
}

/// Set bits in `sie`, the supervisor interrupt-enable mask.
///
/// # Safety
/// Only meaningful once a trap handler exists for the interrupts enabled.
pub unsafe fn set_sie(bits: u64) {
    unsafe { asm!("csrs sie, {}", in(reg) bits, options(nomem, nostack)) };
}

/// Clear bits in `sie`.
///
/// # Safety
/// See [`set_sie`].
pub unsafe fn clear_sie(bits: u64) {
    unsafe { asm!("csrc sie, {}", in(reg) bits, options(nomem, nostack)) };
}

/// Read the machine's monotonic cycle counter.
pub fn read_time() -> u64 {
    let value: u64;
    unsafe { asm!("rdtime {}", out(reg) value, options(nomem, nostack)) };
    value
}

/// Ask the firmware to deliver a timer interrupt at an absolute time.
///
/// The kernel runs in S-mode and does not own the machine timer, so this is a
/// call into the SBI implementation below it. That is one more layer than the
/// other two backends have, and it is worth naming: on RISC-V the research
/// kernel is already a guest of firmware it did not write.
///
/// # Safety
/// Requires an SBI implementation providing the TIME extension.
pub unsafe fn set_timer(absolute: u64) {
    const SBI_TIME_EXTENSION: u64 = 0x5449_4d45;
    unsafe {
        asm!(
            "ecall",
            in("a7") SBI_TIME_EXTENSION,
            in("a6") 0_u64,
            inout("a0") absolute => _,
            out("a1") _,
            options(nostack),
        )
    };
}

/// Mask supervisor interrupts.
///
/// There is no matching unmask: this kernel runs S-mode with `sstatus.SIE`
/// clear throughout. Interrupts reach it only while a world is running, because
/// RISC-V delivers S-mode interrupts to a U-mode hart regardless of `SIE`. A
/// supervisor that cannot be interrupted cannot be interrupted halfway through
/// deciding what to do about a world.
pub fn mask_interrupts() {
    unsafe { asm!("csrc sstatus, {}", in(reg) SSTATUS_SIE, options(nomem, nostack)) };
}

/// Stop this hart permanently with interrupts masked.
pub fn halt() -> ! {
    loop {
        unsafe {
            asm!(
                "csrc sstatus, {sie}",
                "wfi",
                sie = in(reg) SSTATUS_SIE,
                options(nomem, nostack),
            )
        };
    }
}

/// Leave the emulator through QEMU's `virt` test device.
///
/// Unlike AArch64, this platform can report a status as well as stop, so a
/// failing run is distinguishable from a passing one without reading the
/// transcript.
///
/// # Safety
/// Does not return under QEMU.
pub unsafe fn exit_emulator(success: bool) {
    const TEST_DEVICE: *mut u32 = 0x0010_0000 as *mut u32;
    const PASS: u32 = 0x5555;
    const FAIL: u32 = 0x3333;
    unsafe { TEST_DEVICE.write_volatile(if success { PASS } else { FAIL | (1 << 16) }) };
}
