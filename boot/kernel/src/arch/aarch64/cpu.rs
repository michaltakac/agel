//! AArch64 exception vectors, the EL1/EL0 transition, and preemption.
//!
//! The shape is the same as the x86-64 backend's, because the problem is the
//! same: a supervisor must be able to enter an untrusted world, be re-entered
//! by hardware when that world misbehaves, and decide whether the world runs
//! again. Only the instructions differ.

use super::hal;
use core::arch::naked_asm;

/// Bytes of the saved register frame. A multiple of sixteen, because AArch64
/// requires a sixteen-byte-aligned stack pointer at every function call.
const FRAME_BYTES: usize = 304;

/// Vector table index for a synchronous exception taken from AArch64 EL0.
pub const VECTOR_LOWER_SYNC: u64 = 8;
/// Vector table index for an IRQ taken from AArch64 EL0.
pub const VECTOR_LOWER_IRQ: u64 = 9;

/// Exception class of an `SVC` executed at EL0: the contract call.
pub const EC_SVC: u64 = 0x15;

/// GICv2 distributor and CPU interface on QEMU's `virt` machine, and the four
/// registers this kernel touches. Naming them is worth the lines: an interrupt
/// controller programmed by bare offsets is where kernels go to be haunted.
const GIC_DISTRIBUTOR: u64 = 0x0800_0000;
const GIC_CPU: u64 = 0x0801_0000;
/// Distributor control.
const GICD_CTLR: u64 = GIC_DISTRIBUTOR;
/// Set-enable for interrupts 0 through 31.
const GICD_ISENABLER0: u64 = GIC_DISTRIBUTOR + 0x100;
/// CPU interface control.
const GICC_CTLR: u64 = GIC_CPU;
/// Priority mask: the lowest priority this interface will accept.
const GICC_PMR: u64 = GIC_CPU + 0x004;
/// Interrupt acknowledge.
const GICC_IAR: u64 = GIC_CPU + 0x00c;
/// End of interrupt.
const GICC_EOIR: u64 = GIC_CPU + 0x010;
/// Private peripheral interrupt of the EL1 physical timer.
const TIMER_INTERRUPT: u32 = 30;

/// Register state saved on every exception, in the layout the entry and exit
/// stubs agree on. Entering a domain for the first time and resuming one after
/// a trap are the same code path over the same structure.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct TrapFrame {
    /// `x0` through `x30`.
    pub x: [u64; 31],
    /// The world's stack pointer, `SP_EL0`.
    pub sp: u64,
    /// The interrupted instruction, `ELR_EL1`.
    pub elr: u64,
    /// The interrupted processor state, `SPSR_EL1`.
    pub spsr: u64,
    /// The exception syndrome, `ESR_EL1`.
    pub esr: u64,
    /// The faulting address, `FAR_EL1`.
    pub far: u64,
    /// Which of the sixteen vector entries was taken.
    pub vector: u64,
    /// Padding to the sixteen-byte alignment the stubs assume.
    reserved: u64,
}

impl TrapFrame {
    /// Build the initial state of an EL0 thread.
    ///
    /// `SPSR` is zero, which is EL0t with every `DAIF` mask clear: the world
    /// starts preemptible and cannot mask the timer that preempts it.
    pub fn user(entry: u64, stack_top: u64, argument: u64) -> Self {
        let mut frame = Self {
            sp: stack_top,
            elr: entry,
            spsr: 0,
            ..Self::default()
        };
        frame.x[0] = argument;
        frame
    }

    /// True when the exception was taken from EL0.
    ///
    /// The vector index says so directly, which is more trustworthy than
    /// re-deriving it from `SPSR` after the fact.
    pub fn in_user_mode(&self) -> bool {
        self.vector >= VECTOR_LOWER_SYNC && self.vector < 12
    }

    /// The exception class of a synchronous exception.
    pub fn exception_class(&self) -> u64 {
        self.esr >> 26
    }
}

/// Kernel stack pointer to restore when a trap decides not to resume the world.
static mut SUPERVISOR_RESUME: u64 = 0;
/// Top of the stack every exception is taken on.
static mut TRAP_STACK_TOP: u64 = 0;

/// Install the exception vector table and the stack exceptions are taken on.
///
/// # Safety
/// Must be called once, with interrupts masked, at EL1, with a valid stack.
pub unsafe fn install(trap_stack_top: u64) -> Result<(), &'static str> {
    let table = vector_table as *const () as usize as u64;
    if !table.is_multiple_of(0x800) {
        // `VBAR_EL1` ignores the low eleven bits, so a misplaced table would
        // silently dispatch every exception into the middle of something else.
        return Err("exception vector table is not 2 KiB aligned");
    }
    unsafe {
        TRAP_STACK_TOP = trap_stack_top;
        hal::write_vbar(table);
    }
    Ok(())
}

/// Route the timer's private peripheral interrupt through the GIC and start it.
///
/// # Safety
/// Must be called once, after [`install`], with the device window mapped.
pub unsafe fn start_preemption(hertz: u64) {
    unsafe {
        hal::deny_el0_timer_access();
        // Distributor: enable, then unmask only the timer. The kernel has one
        // time source and no drivers; an unmasked line with no handler is an
        // outage waiting.
        write_device(GICD_CTLR, 1);
        write_device(GICD_ISENABLER0, 1 << TIMER_INTERRUPT);
        // CPU interface: accept every priority, then enable.
        write_device(GICC_PMR, 0xf0);
        write_device(GICC_CTLR, 1);
    }
    let frequency = hal::read_counter_frequency();
    let interval = (frequency / hertz).max(1);
    unsafe {
        TIMER_INTERVAL = interval;
        hal::arm_timer(interval);
    }
}

static mut TIMER_INTERVAL: u64 = 0;

/// Acknowledge the pending interrupt and re-arm the timer.
///
/// # Safety
/// Only correct from inside an IRQ handler.
pub unsafe fn acknowledge_interrupt() {
    unsafe {
        let interrupt = read_device(GICC_IAR);
        hal::arm_timer(TIMER_INTERVAL);
        write_device(GICC_EOIR, interrupt);
    }
}

/// Stop the timer so a halted supervisor is not woken forever.
///
/// # Safety
/// Only correct at EL1.
pub unsafe fn stop_preemption() {
    unsafe { hal::disable_timer() };
}

/// # Safety
/// `address` must be a mapped device register.
unsafe fn write_device(address: u64, value: u32) {
    unsafe { (address as *mut u32).write_volatile(value) };
}

/// # Safety
/// See [`write_device`].
unsafe fn read_device(address: u64) -> u32 {
    unsafe { (address as *const u32).read_volatile() }
}

/// Enter EL0 with the register state in `frame`, and return when a trap decides
/// the domain should not be resumed.
///
/// # Safety
/// `frame` must be a valid, kernel-owned [`TrapFrame`] describing an EL0
/// context whose code and stack are mapped in the currently active address
/// space.
#[unsafe(naked)]
pub unsafe extern "C" fn enter_domain(frame: *mut TrapFrame) {
    naked_asm!(
        // Preserve the supervisor context so a trap can return into our caller.
        "stp x19, x20, [sp, #-96]!",
        "stp x21, x22, [sp, #16]",
        "stp x23, x24, [sp, #32]",
        "stp x25, x26, [sp, #48]",
        "stp x27, x28, [sp, #64]",
        "stp x29, x30, [sp, #80]",
        "adrp x1, {resume}",
        "add x1, x1, :lo12:{resume}",
        "mov x2, sp",
        "str x2, [x1]",
        "b {restore}",
        resume = sym SUPERVISOR_RESUME,
        restore = sym restore_and_return,
    )
}

/// Abandon the current domain and return from [`enter_domain`].
///
/// # Safety
/// Only callable from a trap taken while a domain entered through
/// [`enter_domain`] was running.
#[unsafe(naked)]
pub unsafe extern "C" fn leave_domain() -> ! {
    naked_asm!(
        "adrp x0, {resume}",
        "add x0, x0, :lo12:{resume}",
        "ldr x1, [x0]",
        "mov sp, x1",
        "ldp x29, x30, [sp, #80]",
        "ldp x27, x28, [sp, #64]",
        "ldp x25, x26, [sp, #48]",
        "ldp x23, x24, [sp, #32]",
        "ldp x21, x22, [sp, #16]",
        "ldp x19, x20, [sp], #96",
        "ret",
        resume = sym SUPERVISOR_RESUME,
    )
}

/// Restore the frame `x0` points at and drop to EL0.
///
/// The frame is read through a register rather than through `sp`, because the
/// stack exceptions are taken on must be reset to its top before `eret`: if it
/// were left pointing just past a domain's saved frame, the next trap would
/// write its own frame straight through that domain's state.
///
/// # Safety
/// `x0` must point at a valid [`TrapFrame`].
#[unsafe(naked)]
unsafe extern "C" fn restore_and_return() -> ! {
    naked_asm!(
        "ldr x1, [x0, #248]",
        "msr sp_el0, x1",
        "ldr x1, [x0, #256]",
        "msr elr_el1, x1",
        "ldr x1, [x0, #264]",
        "msr spsr_el1, x1",
        "adrp x1, {stack}",
        "add x1, x1, :lo12:{stack}",
        "ldr x1, [x1]",
        "mov sp, x1",
        "ldp x2, x3, [x0, #16]",
        "ldp x4, x5, [x0, #32]",
        "ldp x6, x7, [x0, #48]",
        "ldp x8, x9, [x0, #64]",
        "ldp x10, x11, [x0, #80]",
        "ldp x12, x13, [x0, #96]",
        "ldp x14, x15, [x0, #112]",
        "ldp x16, x17, [x0, #128]",
        "ldp x18, x19, [x0, #144]",
        "ldp x20, x21, [x0, #160]",
        "ldp x22, x23, [x0, #176]",
        "ldp x24, x25, [x0, #192]",
        "ldp x26, x27, [x0, #208]",
        "ldp x28, x29, [x0, #224]",
        "ldr x30, [x0, #240]",
        "ldr x1, [x0, #8]",
        "ldr x0, [x0, #0]",
        "eret",
        stack = sym TRAP_STACK_TOP,
    )
}

/// The sixteen-entry exception vector table.
///
/// Each entry is a four-instruction stub that reserves a frame, saves the two
/// registers it is about to use, records which entry fired, and branches to the
/// common path. The `.balign` directives place the entries exactly 128 bytes
/// apart, which is what `VBAR_EL1` requires.
///
/// # Safety
/// Only ever reached by hardware.
#[unsafe(naked)]
#[link_section = ".vectors"]
unsafe extern "C" fn vector_table() {
    naked_asm!(
        // Current EL with SP_EL0.
        ".balign 0x80", "sub sp, sp, #304", "stp x0, x1, [sp]", "mov x0, #0", "b {common}",
        ".balign 0x80", "sub sp, sp, #304", "stp x0, x1, [sp]", "mov x0, #1", "b {common}",
        ".balign 0x80", "sub sp, sp, #304", "stp x0, x1, [sp]", "mov x0, #2", "b {common}",
        ".balign 0x80", "sub sp, sp, #304", "stp x0, x1, [sp]", "mov x0, #3", "b {common}",
        // Current EL with SP_ELx: a supervisor fault.
        ".balign 0x80", "sub sp, sp, #304", "stp x0, x1, [sp]", "mov x0, #4", "b {common}",
        ".balign 0x80", "sub sp, sp, #304", "stp x0, x1, [sp]", "mov x0, #5", "b {common}",
        ".balign 0x80", "sub sp, sp, #304", "stp x0, x1, [sp]", "mov x0, #6", "b {common}",
        ".balign 0x80", "sub sp, sp, #304", "stp x0, x1, [sp]", "mov x0, #7", "b {common}",
        // Lower EL, AArch64: everything a world can do.
        ".balign 0x80", "sub sp, sp, #304", "stp x0, x1, [sp]", "mov x0, #8", "b {common}",
        ".balign 0x80", "sub sp, sp, #304", "stp x0, x1, [sp]", "mov x0, #9", "b {common}",
        ".balign 0x80", "sub sp, sp, #304", "stp x0, x1, [sp]", "mov x0, #10", "b {common}",
        ".balign 0x80", "sub sp, sp, #304", "stp x0, x1, [sp]", "mov x0, #11", "b {common}",
        // Lower EL, AArch32: no world here is ever in that state.
        ".balign 0x80", "sub sp, sp, #304", "stp x0, x1, [sp]", "mov x0, #12", "b {common}",
        ".balign 0x80", "sub sp, sp, #304", "stp x0, x1, [sp]", "mov x0, #13", "b {common}",
        ".balign 0x80", "sub sp, sp, #304", "stp x0, x1, [sp]", "mov x0, #14", "b {common}",
        ".balign 0x80", "sub sp, sp, #304", "stp x0, x1, [sp]", "mov x0, #15", "b {common}",
        common = sym trap_common,
    )
}

/// Save the rest of the state, hand it to the dispatcher, and resume whatever
/// frame the dispatcher chose.
///
/// # Safety
/// Only reached from [`vector_table`].
#[unsafe(naked)]
unsafe extern "C" fn trap_common() -> ! {
    naked_asm!(
        "stp x2, x3, [sp, #16]",
        "stp x4, x5, [sp, #32]",
        "stp x6, x7, [sp, #48]",
        "stp x8, x9, [sp, #64]",
        "stp x10, x11, [sp, #80]",
        "stp x12, x13, [sp, #96]",
        "stp x14, x15, [sp, #112]",
        "stp x16, x17, [sp, #128]",
        "stp x18, x19, [sp, #144]",
        "stp x20, x21, [sp, #160]",
        "stp x22, x23, [sp, #176]",
        "stp x24, x25, [sp, #192]",
        "stp x26, x27, [sp, #208]",
        "stp x28, x29, [sp, #224]",
        "str x30, [sp, #240]",
        "mrs x1, sp_el0",
        "str x1, [sp, #248]",
        "mrs x1, elr_el1",
        "str x1, [sp, #256]",
        "mrs x1, spsr_el1",
        "str x1, [sp, #264]",
        "mrs x1, esr_el1",
        "str x1, [sp, #272]",
        "mrs x1, far_el1",
        "str x1, [sp, #280]",
        "str x0, [sp, #288]",
        "mov x0, sp",
        "bl {dispatch}",
        // The dispatcher returns the frame to resume, which may be a different
        // domain's saved frame rather than the one we trapped with.
        "b {restore}",
        dispatch = sym super::domain::dispatch_trap,
        restore = sym restore_and_return,
    )
}

const _: () = assert!(core::mem::size_of::<TrapFrame>() == FRAME_BYTES);
