//! The complete set of privileged AArch64 operations the research kernel uses.
//!
//! Every instruction here is one an EL0 world is not allowed to execute. Having
//! them in one file means the privileged surface can be read in a sitting and
//! reviewed as a whole; nothing here makes a policy decision.

use core::arch::asm;

/// Read `CurrentEL`, shifted down to a plain exception-level number.
pub fn current_exception_level() -> u64 {
    let value: u64;
    unsafe { asm!("mrs {}, CurrentEL", out(reg) value, options(nomem, nostack)) };
    (value >> 2) & 3
}

/// Install the exception vector base.
///
/// # Safety
/// `base` must be a 2 KiB-aligned, sixteen-entry vector table that stays mapped.
pub unsafe fn write_vbar(base: u64) {
    unsafe { asm!("msr vbar_el1, {}", in(reg) base, options(nomem, nostack)) };
}

/// Install the memory attribute indirection register.
///
/// # Safety
/// The value must describe the attributes every live translation entry indexes.
pub unsafe fn write_mair(value: u64) {
    unsafe { asm!("msr mair_el1, {}", in(reg) value, options(nomem, nostack)) };
}

/// Install the translation control register.
///
/// # Safety
/// Must describe the geometry of the tables `TTBR0_EL1` points at.
pub unsafe fn write_tcr(value: u64) {
    unsafe { asm!("msr tcr_el1, {}", in(reg) value, options(nomem, nostack)) };
}

/// Install a page-table root and flush the translation caches for it.
///
/// # Safety
/// `root` must be a correctly formed level-1 table mapping every address the
/// kernel is about to execute from and touch.
pub unsafe fn write_ttbr0(root: u64) {
    unsafe {
        asm!(
            "msr ttbr0_el1, {root}",
            "dsb ish",
            "tlbi vmalle1",
            "dsb ish",
            "isb",
            root = in(reg) root,
            options(nostack),
        )
    };
}

/// Turn on the MMU, the data cache, and the instruction cache.
///
/// # Safety
/// Translation tables and `TTBR0_EL1` must already describe the code that is
/// about to execute; the first instruction after this either translates or the
/// machine is gone.
pub unsafe fn enable_mmu() {
    unsafe {
        asm!(
            "mrs {scratch}, sctlr_el1",
            "orr {scratch}, {scratch}, #(1 << 0)",   // M: enable translation
            "orr {scratch}, {scratch}, #(1 << 2)",   // C: data cache
            "orr {scratch}, {scratch}, #(1 << 12)",  // I: instruction cache
            "msr sctlr_el1, {scratch}",
            "isb",
            scratch = out(reg) _,
            options(nostack),
        )
    };
}

/// Program the EL1 physical timer to fire after `ticks` of the system counter.
///
/// # Safety
/// Only meaningful once an interrupt controller will deliver the timer's
/// private peripheral interrupt.
pub unsafe fn arm_timer(ticks: u64) {
    unsafe {
        asm!(
            "msr cntp_tval_el0, {ticks}",
            "mov {scratch}, #1",
            "msr cntp_ctl_el0, {scratch}",  // enable, unmasked
            "isb",
            ticks = in(reg) ticks,
            scratch = out(reg) _,
            options(nostack),
        )
    };
}

/// Stop the EL1 physical timer.
///
/// # Safety
/// See [`arm_timer`].
pub unsafe fn disable_timer() {
    unsafe { asm!("msr cntp_ctl_el0, xzr", "isb", options(nomem, nostack),) };
}

/// Deny EL0 every access to the timer and the system counter.
///
/// The reset value of `CNTKCTL_EL1` is architecturally unknown, so the kernel
/// states the policy rather than inheriting one: a world must not be able to
/// read, program, or disable the timer that preempts it.
///
/// # Safety
/// Only correct at EL1.
pub unsafe fn deny_el0_timer_access() {
    unsafe { asm!("msr cntkctl_el1, xzr", options(nomem, nostack)) };
}

/// The system counter's frequency in hertz, as the firmware reported it.
pub fn read_counter_frequency() -> u64 {
    let value: u64;
    unsafe { asm!("mrs {}, cntfrq_el0", out(reg) value, options(nomem, nostack)) };
    value
}

/// Mask interrupts.
///
/// There is no matching unmask: this kernel runs EL1 with interrupts masked
/// throughout and only ever unmasks them by entering EL0 with a saved `SPSR`
/// whose `DAIF` bits are clear. A supervisor that cannot be interrupted cannot
/// be interrupted halfway through deciding what to do about a world.
pub fn mask_interrupts() {
    unsafe { asm!("msr daifset, #0xf", options(nomem, nostack)) };
}

/// Stop this processor permanently with interrupts masked.
pub fn halt() -> ! {
    loop {
        unsafe { asm!("msr daifset, #0xf", "wfe", options(nomem, nostack)) };
    }
}

/// Ask the platform's power controller to switch the machine off.
///
/// QEMU's `virt` machine implements PSCI, and `SYSTEM_OFF` is the one clean way
/// off it: there is no debug-exit device on this platform. The hypervisor call
/// is tried first because that is the conduit QEMU installs when the guest is
/// entered at EL1, and the secure monitor call second so that the same image
/// works on a board configured the other way.
///
/// # Safety
/// Does not return on a machine that implements PSCI.
pub unsafe fn power_off() {
    const PSCI_SYSTEM_OFF: u64 = 0x8400_0008;
    unsafe {
        asm!("hvc #0", in("x0") PSCI_SYSTEM_OFF, options(nostack));
        asm!("smc #0", in("x0") PSCI_SYSTEM_OFF, options(nostack));
    }
}
