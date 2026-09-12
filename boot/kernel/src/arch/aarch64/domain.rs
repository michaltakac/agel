//! AArch64 protection domains: the exception decode and the translation
//! root. The domain itself, shared with RISC-V, is in [`super::paged`].

use super::cpu::{self, TrapFrame};
use super::paged::Domain;
use crate::world::{Fault, Stop};

/// The kernel's own translation root, reinstalled whenever no domain is
/// running, so the supervisor never executes with a world's translations live.
static mut KERNEL_ROOT: u64 = 0;

/// Record the address space the kernel runs in when no domain is active.
///
/// # Safety
/// Must be called once, with the kernel's own root, before any domain runs.
pub unsafe fn set_kernel_root(root: u64) {
    unsafe { KERNEL_ROOT = root };
}

/// # Safety
/// Only correct once [`set_kernel_root`] has recorded a valid root.
pub(super) unsafe fn restore_kernel_space() {
    let root = unsafe { KERNEL_ROOT };
    unsafe { super::hal::write_ttbr0(root) };
}

/// The domain [`cpu::enter_domain`] is currently running, or null.
///
/// The trap handler reaches the running domain through this pointer while
/// [`Domain::run`] still holds `&mut self` further up the same call chain. That
/// is a deliberate aliasing of a manual coroutine switch, not an oversight:
/// control leaves `run` at `eret` and only comes back through the trap path, so
/// the two references are never live at the same instant. The kernel is
/// single-processor and runs EL1 with interrupts masked, which is what makes
/// that argument hold.
pub(super) static mut CURRENT: *mut Domain = core::ptr::null_mut();

/// Handle every exception: contract calls, timer interrupts, and faults.
///
/// Returns the frame to resume. When the current domain must not be resumed it
/// does not return at all: it unwinds to the supervisor through
/// [`cpu::leave_domain`].
///
/// # Safety
/// Called only from the exception entry stub, with a valid saved frame.
pub unsafe extern "C" fn dispatch_trap(frame: *mut TrapFrame) -> *mut TrapFrame {
    let saved = unsafe { &mut *frame };
    if !saved.in_user_mode() {
        crate::report_supervisor_trap(saved.vector, saved.esr, saved.elr);
    }
    // Supervisor policy always runs under the kernel translation root. The
    // domain root may grant the evaluator access to shared compiler helpers;
    // it must never become the supervisor's ambient address space.
    unsafe { restore_kernel_space() };
    let domain = unsafe { &mut *CURRENT };
    if saved.vector == cpu::VECTOR_LOWER_IRQ {
        // Acknowledge before deciding, so that stopping the domain does not
        // also stop the clock the supervisor needs.
        unsafe { cpu::acknowledge_interrupt() };
        if domain.core.charge_tick() {
            unsafe { domain.space.activate() };
            return frame;
        }
    } else if saved.exception_class() == cpu::EC_SVC {
        let arguments = [saved.x[1], saved.x[2], saved.x[3], saved.x[4]];
        if let Some(response) = domain.core.syscall(saved.x[8], saved.x[0], arguments) {
            #[cfg(feature = "contract-memory")]
            if response.status == agel_kernel_abi::Status::Ok
                && matches!(saved.x[8] >> 8, 0x02 | 0x07)
            {
                domain.reconcile_window();
            }
            saved.x[0] = u64::from(response.status.code());
            saved.x[1] = response.values[0];
            saved.x[2] = response.values[1];
            saved.x[3] = response.values[2];
            saved.x[4] = response.values[3];
            unsafe { domain.space.activate() };
            return frame;
        }
    } else {
        domain.core.record_stop(Stop::Faulted(Fault {
            cause: saved.exception_class(),
            detail: saved.esr,
            pc: saved.elr,
            address: saved.far,
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
