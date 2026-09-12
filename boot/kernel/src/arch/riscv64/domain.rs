//! RISC-V protection domains: the trap decode and the translation root.
//! The domain itself, shared with AArch64, is in [`super::paged`].

use super::cpu::{self, reg, TrapFrame};
use super::paged::Domain;
use crate::world::{Fault, Stop};

/// The kernel's own `satp`, reinstalled whenever no domain is running, so the
/// supervisor never executes with a world's translations live.
static mut KERNEL_SATP: u64 = 0;

/// Record the address space the kernel runs in when no domain is active.
///
/// # Safety
/// Must be called once, with the kernel's own `satp`, before any domain runs.
pub unsafe fn set_kernel_satp(satp: u64) {
    unsafe { KERNEL_SATP = satp };
}

/// # Safety
/// Only correct once [`set_kernel_satp`] has recorded a valid value.
pub(super) unsafe fn restore_kernel_space() {
    let satp = unsafe { KERNEL_SATP };
    unsafe { super::hal::write_satp(satp) };
}

/// The domain [`cpu::enter_domain`] is currently running, or null.
///
/// The trap handler reaches the running domain through this pointer while
/// [`Domain::run`] still holds `&mut self` further up the same call chain. That
/// is a deliberate aliasing of a manual coroutine switch, not an oversight:
/// control leaves `run` at `sret` and only comes back through the trap path, so
/// the two references are never live at the same instant. The kernel is
/// single-hart and runs S-mode with interrupts masked, which is what makes that
/// argument hold.
pub(super) static mut CURRENT: *mut Domain = core::ptr::null_mut();

/// Handle every trap: contract calls, timer interrupts, and faults.
///
/// Returns the frame to resume. When the current domain must not be resumed it
/// does not return at all: it unwinds to the supervisor through
/// [`cpu::leave_domain`].
///
/// # Safety
/// Called only from the trap entry stub, with a valid saved frame.
pub unsafe extern "C" fn dispatch_trap(frame: *mut TrapFrame) -> *mut TrapFrame {
    let saved = unsafe { &mut *frame };
    if !saved.in_user_mode() {
        crate::report_supervisor_trap(saved.scause, saved.stval, saved.sepc);
    }
    // Run kernel policy under the kernel root, never under a domain's user
    // mappings. This is also what lets the same physical compiler helpers have
    // supervisor permissions here and user permissions in the domain root.
    unsafe { restore_kernel_space() };
    let domain = unsafe { &mut *CURRENT };
    if saved.is_timer() {
        // Re-arm before deciding, so that stopping the domain does not also
        // stop the clock the supervisor needs.
        unsafe { cpu::acknowledge_timer() };
        if domain.core.charge_tick() {
            unsafe { domain.space.activate() };
            return frame;
        }
    } else if saved.scause == cpu::CAUSE_USER_ECALL {
        // `ecall` leaves `sepc` on the instruction itself; resuming there would
        // trap forever.
        saved.sepc += 4;
        let arguments = [
            saved.x[reg::A1],
            saved.x[reg::A2],
            saved.x[reg::A3],
            saved.x[reg::A4],
        ];
        if let Some(response) = domain
            .core
            .syscall(saved.x[reg::A7], saved.x[reg::A0], arguments)
        {
            #[cfg(feature = "contract-memory")]
            if response.status == agel_kernel_abi::Status::Ok
                && matches!(saved.x[reg::A7] >> 8, 0x02 | 0x07)
            {
                domain.reconcile_window();
            }
            saved.x[reg::A0] = u64::from(response.status.code());
            saved.x[reg::A1] = response.values[0];
            saved.x[reg::A2] = response.values[1];
            saved.x[reg::A3] = response.values[2];
            saved.x[reg::A4] = response.values[3];
            unsafe { domain.space.activate() };
            return frame;
        }
    } else {
        domain.core.record_stop(Stop::Faulted(Fault {
            cause: saved.exception_code(),
            detail: saved.scause,
            pc: saved.sepc,
            address: saved.stval,
        }));
    }
    unsafe {
        crate::world::copy_supervisor_words(
            (&raw mut domain.frame).cast(),
            (saved as *mut TrapFrame).cast(),
            core::mem::size_of::<TrapFrame>(),
        )
    };
    unsafe { cpu::leave_domain() }
}
