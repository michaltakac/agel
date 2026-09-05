//! RISC-V trap entry, the S-mode/U-mode transition, and preemption.
//!
//! The shape is the same as the other two backends', because the problem is the
//! same: a supervisor must be able to enter an untrusted world, be re-entered
//! by hardware when that world misbehaves, and decide whether the world runs
//! again. Only the instructions differ.

use super::hal;
use core::arch::naked_asm;

/// Bytes of the saved register frame.
const FRAME_BYTES: usize = 288;

/// `scause` bit set for an interrupt rather than an exception.
const INTERRUPT_BIT: u64 = 1 << 63;
/// Supervisor timer interrupt.
const CAUSE_TIMER: u64 = 5;
/// Environment call from U-mode: the contract call.
pub const CAUSE_USER_ECALL: u64 = 8;

/// Register state saved on every trap, in the layout the entry and exit stubs
/// agree on. Entering a domain for the first time and resuming one after a trap
/// are the same code path over the same structure.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct TrapFrame {
    /// `x0` through `x31`. Slot zero is never used; keeping it makes every
    /// other index the architectural register number.
    pub x: [u64; 32],
    /// The interrupted instruction.
    pub sepc: u64,
    /// The interrupted status, including which mode the trap came from.
    pub sstatus: u64,
    /// Why the trap happened.
    pub scause: u64,
    /// The faulting address, where there is one.
    pub stval: u64,
}

/// Architectural register numbers the contract convention uses.
pub mod reg {
    /// `sp`.
    pub const SP: usize = 2;
    /// `a0`, the capability on the way in and the status on the way out.
    pub const A0: usize = 10;
    /// `a1` through `a4`, the four bounded words.
    pub const A1: usize = 11;
    /// See [`A1`].
    pub const A2: usize = 12;
    /// See [`A1`].
    pub const A3: usize = 13;
    /// See [`A1`].
    pub const A4: usize = 14;
    /// `a7`, the operation code.
    pub const A7: usize = 17;
}

impl TrapFrame {
    /// Build the initial state of a U-mode thread.
    ///
    /// `SPP` is clear, so `sret` lands in U-mode, and `SPIE` is set so the world
    /// starts preemptible. A U-mode hart cannot mask supervisor interrupts at
    /// all, which is why the timer that contains it is out of its reach.
    pub fn user(entry: u64, stack_top: u64, argument: u64) -> Self {
        let mut frame = Self {
            sepc: entry,
            sstatus: hal::SSTATUS_SPIE,
            ..Self::default()
        };
        frame.x[reg::SP] = stack_top;
        frame.x[reg::A0] = argument;
        frame
    }

    /// True when the trap was taken from U-mode.
    pub fn in_user_mode(&self) -> bool {
        self.sstatus & hal::SSTATUS_SPP == 0
    }

    /// True when the trap is the preemption timer.
    pub fn is_timer(&self) -> bool {
        self.scause & INTERRUPT_BIT != 0 && self.scause & 0xff == CAUSE_TIMER
    }

    /// The exception code, with the interrupt bit stripped.
    pub fn exception_code(&self) -> u64 {
        self.scause & !INTERRUPT_BIT
    }
}

/// Kernel stack pointer to restore when a trap decides not to resume the world.
static mut SUPERVISOR_RESUME: u64 = 0;
/// Top of the stack every trap is taken on, kept in `sscratch` while a world
/// runs so the entry stub can find it without touching memory it does not own.
static mut TRAP_STACK_TOP: u64 = 0;
/// System-counter ticks between preemptions.
static mut TIMER_INTERVAL: u64 = 0;

/// Install the trap vector and the stack traps are taken on.
///
/// # Safety
/// Must be called once, with supervisor interrupts masked, in S-mode.
pub unsafe fn install(trap_stack_top: u64) -> Result<(), &'static str> {
    let handler = trap_entry as *const () as usize as u64;
    if !handler.is_multiple_of(4) {
        // The low two bits of `stvec` are the vector mode, so a misaligned
        // handler would silently select vectored dispatch into nothing.
        return Err("trap handler is not four-byte aligned");
    }
    unsafe {
        TRAP_STACK_TOP = trap_stack_top;
        hal::write_stvec(handler);
    }
    Ok(())
}

/// Start the preemption timer.
///
/// # Safety
/// Must be called once, after [`install`].
pub unsafe fn start_preemption(hertz: u64) {
    // QEMU's `virt` machine runs the ACLINT timer at ten megahertz, which
    // OpenSBI reports at boot. The kernel does not have a device tree parser,
    // so the figure is stated here rather than discovered.
    const TIMEBASE_HERTZ: u64 = 10_000_000;
    let interval = (TIMEBASE_HERTZ / hertz).max(1);
    unsafe {
        TIMER_INTERVAL = interval;
        hal::set_sie(hal::SIE_STIE);
        hal::set_timer(hal::read_time() + interval);
    }
}

/// Re-arm the timer for the next preemption.
///
/// # Safety
/// Only correct from inside the timer trap handler.
pub unsafe fn acknowledge_timer() {
    unsafe { hal::set_timer(hal::read_time() + TIMER_INTERVAL) };
}

/// Stop the timer so a halted supervisor is not woken forever.
///
/// # Safety
/// Only correct in S-mode.
pub unsafe fn stop_preemption() {
    unsafe {
        hal::clear_sie(hal::SIE_STIE);
        // A timer already programmed will still fire, so push it out of reach
        // rather than leaving an interrupt pending against a halted hart.
        hal::set_timer(u64::MAX);
    }
}

/// Enter U-mode with the register state in `frame`, and return when a trap
/// decides the domain should not be resumed.
///
/// # Safety
/// `frame` must be a valid, kernel-owned [`TrapFrame`] describing a U-mode
/// context whose code and stack are mapped in the currently active address
/// space.
#[unsafe(naked)]
pub unsafe extern "C" fn enter_domain(frame: *mut TrapFrame) {
    naked_asm!(
        // Preserve the supervisor context so a trap can return into our caller.
        "addi sp, sp, -112",
        "sd ra, 0(sp)",
        "sd s0, 8(sp)",
        "sd s1, 16(sp)",
        "sd s2, 24(sp)",
        "sd s3, 32(sp)",
        "sd s4, 40(sp)",
        "sd s5, 48(sp)",
        "sd s6, 56(sp)",
        "sd s7, 64(sp)",
        "sd s8, 72(sp)",
        "sd s9, 80(sp)",
        "sd s10, 88(sp)",
        "sd s11, 96(sp)",
        "la t0, {resume}",
        "sd sp, 0(t0)",
        // The trap entry stub finds its stack through `sscratch`, because on
        // entry from U-mode `sp` still belongs to the world.
        "la t0, {trap_stack}",
        "ld t0, 0(t0)",
        "csrw sscratch, t0",
        "j {restore}",
        resume = sym SUPERVISOR_RESUME,
        trap_stack = sym TRAP_STACK_TOP,
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
        "la t0, {resume}",
        "ld sp, 0(t0)",
        "ld ra, 0(sp)",
        "ld s0, 8(sp)",
        "ld s1, 16(sp)",
        "ld s2, 24(sp)",
        "ld s3, 32(sp)",
        "ld s4, 40(sp)",
        "ld s5, 48(sp)",
        "ld s6, 56(sp)",
        "ld s7, 64(sp)",
        "ld s8, 72(sp)",
        "ld s9, 80(sp)",
        "ld s10, 88(sp)",
        "ld s11, 96(sp)",
        "addi sp, sp, 112",
        "ret",
        resume = sym SUPERVISOR_RESUME,
    )
}

/// Restore the frame `a0` points at and return to U-mode.
///
/// The frame is read through `a0` rather than through `sp`, because `sp` has to
/// become the world's own stack pointer before `sret`, and because the stack
/// traps are taken on is found through `sscratch` rather than by leaving `sp`
/// somewhere useful.
///
/// # Safety
/// `a0` must point at a valid [`TrapFrame`].
#[unsafe(naked)]
unsafe extern "C" fn restore_and_return() -> ! {
    naked_asm!(
        "ld t0, 256(a0)",
        "csrw sepc, t0",
        "ld t0, 264(a0)",
        "csrw sstatus, t0",
        "ld ra, 8(a0)",
        "ld gp, 24(a0)",
        "ld tp, 32(a0)",
        "ld t0, 40(a0)",
        "ld t1, 48(a0)",
        "ld t2, 56(a0)",
        "ld s0, 64(a0)",
        "ld s1, 72(a0)",
        "ld a1, 88(a0)",
        "ld a2, 96(a0)",
        "ld a3, 104(a0)",
        "ld a4, 112(a0)",
        "ld a5, 120(a0)",
        "ld a6, 128(a0)",
        "ld a7, 136(a0)",
        "ld s2, 144(a0)",
        "ld s3, 152(a0)",
        "ld s4, 160(a0)",
        "ld s5, 168(a0)",
        "ld s6, 176(a0)",
        "ld s7, 184(a0)",
        "ld s8, 192(a0)",
        "ld s9, 200(a0)",
        "ld s10, 208(a0)",
        "ld s11, 216(a0)",
        "ld t3, 224(a0)",
        "ld t4, 232(a0)",
        "ld t5, 240(a0)",
        "ld t6, 248(a0)",
        "ld sp, 16(a0)",
        "ld a0, 80(a0)",
        "sret",
    )
}

/// The one trap vector, in direct mode.
///
/// # Safety
/// Only ever reached by hardware.
#[unsafe(naked)]
unsafe extern "C" fn trap_entry() -> ! {
    naked_asm!(
        // Swap in the supervisor's trap stack; `sscratch` keeps the world's.
        "csrrw sp, sscratch, sp",
        "addi sp, sp, -288",
        "sd ra, 8(sp)",
        "sd gp, 24(sp)",
        "sd tp, 32(sp)",
        "sd t0, 40(sp)",
        "sd t1, 48(sp)",
        "sd t2, 56(sp)",
        "sd s0, 64(sp)",
        "sd s1, 72(sp)",
        "sd a0, 80(sp)",
        "sd a1, 88(sp)",
        "sd a2, 96(sp)",
        "sd a3, 104(sp)",
        "sd a4, 112(sp)",
        "sd a5, 120(sp)",
        "sd a6, 128(sp)",
        "sd a7, 136(sp)",
        "sd s2, 144(sp)",
        "sd s3, 152(sp)",
        "sd s4, 160(sp)",
        "sd s5, 168(sp)",
        "sd s6, 176(sp)",
        "sd s7, 184(sp)",
        "sd s8, 192(sp)",
        "sd s9, 200(sp)",
        "sd s10, 208(sp)",
        "sd s11, 216(sp)",
        "sd t3, 224(sp)",
        "sd t4, 232(sp)",
        "sd t5, 240(sp)",
        "sd t6, 248(sp)",
        // The world's stack pointer is in `sscratch`; put it in its slot and
        // give `sscratch` back the trap stack for the next trap.
        "csrr t0, sscratch",
        "sd t0, 16(sp)",
        "addi t0, sp, 288",
        "csrw sscratch, t0",
        "csrr t0, sepc",
        "sd t0, 256(sp)",
        "csrr t0, sstatus",
        "sd t0, 264(sp)",
        "csrr t0, scause",
        "sd t0, 272(sp)",
        "csrr t0, stval",
        "sd t0, 280(sp)",
        "mv a0, sp",
        "call {dispatch}",
        // The dispatcher returns the frame to resume, which may be a different
        // domain's saved frame rather than the one we trapped with.
        "j {restore}",
        dispatch = sym super::domain::dispatch_trap,
        restore = sym restore_and_return,
    )
}

const _: () = assert!(core::mem::size_of::<TrapFrame>() == FRAME_BYTES);
