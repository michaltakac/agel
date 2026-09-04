//! AArch64 protection domains.
//!
//! Everything that is not about AArch64 lives in [`crate::world::DomainCore`].
//! What remains here is the machine-specific part: an address space, a register
//! frame, the EL1/EL0 transition, and the syndrome decode.

use super::cpu::{self, TrapFrame};
use super::memory::{AddressSpace, IdentityWindow, DOMAIN_BASE};
use crate::memory::{Access, FramePool, MemoryError, PAGE};
use crate::world::{DomainCore, Fault, Stop, STACK_PAGES};
use agel_kernel_abi::{Request, Response, Status};

/// Virtual address of a domain's stack region.
const STACK_BASE: u64 = DOMAIN_BASE;
/// Virtual address of the page a domain shares with the supervisor.
const SHARED_BASE: u64 = DOMAIN_BASE + 0x0010_0000;
/// Virtual address of the console device, mapped only into the driver domain.
pub const DEVICE_BASE: u64 = DOMAIN_BASE + 0x0020_0000;

/// An unprivileged world.
pub struct Domain {
    space: AddressSpace,
    frame: TrapFrame,
    core: DomainCore,
}

impl Domain {
    /// Build a domain with a private stack and one shared page, entering at
    /// `entry` in EL0.
    pub fn new(
        pool: &mut FramePool,
        identity: IdentityWindow,
        entry: u64,
        tick_budget: u32,
        console: Option<u64>,
    ) -> Result<Self, MemoryError> {
        let mut space = AddressSpace::new(pool, identity)?;
        for page in 0..STACK_PAGES {
            let frame = pool.allocate()?;
            space.map(pool, STACK_BASE + page * PAGE, frame, Access::UserData)?;
        }
        let shared_physical = pool.allocate()?;
        space.map(pool, SHARED_BASE, shared_physical, Access::UserData)?;
        // The console device is mapped into exactly one domain. Every other
        // world has no translation for it at all, so reaching it is not a
        // permission failure but an absence.
        if let Some(device) = console {
            space.map(pool, DEVICE_BASE, device, Access::UserDevice)?;
        }
        // The stack grows down from the top of the last mapped stack page. The
        // page above is deliberately absent, so an overflowing world faults
        // instead of walking into whatever the allocator handed out next.
        let stack_top = STACK_BASE + STACK_PAGES * PAGE;
        Ok(Self {
            space,
            frame: TrapFrame::user(entry, stack_top, SHARED_BASE),
            core: DomainCore::new(shared_physical, tick_budget),
        })
    }

    /// Ask the world to perform one contract invocation and report the answer.
    pub fn invoke_in_world(&mut self, request: &Request) -> Response {
        self.core.stage_invocation(request);
        match self.run() {
            Stop::Replied => self.core.collect_response(),
            // A world that faults or overruns while answering has not answered.
            // Reporting anything else would let a crash masquerade as a result.
            _ => Response::fail(Status::FaultedDomain),
        }
    }

    /// Ask the world to do something it is not allowed to do, and report how it
    /// was stopped.
    pub fn provoke(&mut self, command: u64) -> Stop {
        self.core.stage_command(command);
        self.run()
    }

    /// Run the domain until it yields, faults, or exhausts its budget.
    pub fn run(&mut self) -> Stop {
        if let Some(stop) = self.core.stopped() {
            // A stopped domain stays stopped. Restarting it is a supervisor
            // decision with a new generation, not an automatic retry.
            return stop;
        }
        self.core.begin_entry();
        // Safety: the domain's address space maps the whole supervisor window,
        // so the trap path stays reachable across the switch.
        unsafe {
            self.space.activate();
            CURRENT = self;
            cpu::enter_domain(&raw mut self.frame);
            CURRENT = core::ptr::null_mut();
            restore_kernel_space();
        }
        self.core.outcome()
    }

    /// The domain's recorded stop reason, if it has one.
    pub fn stopped(&self) -> Option<Stop> {
        self.core.stopped()
    }

    /// The architecture-neutral half of this domain.
    pub fn core(&mut self) -> &mut DomainCore {
        &mut self.core
    }
}

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
unsafe fn restore_kernel_space() {
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
static mut CURRENT: *mut Domain = core::ptr::null_mut();

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
    let domain = unsafe { &mut *CURRENT };
    if saved.vector == cpu::VECTOR_LOWER_IRQ {
        // Acknowledge before deciding, so that stopping the domain does not
        // also stop the clock the supervisor needs.
        unsafe { cpu::acknowledge_interrupt() };
        if domain.core.charge_tick() {
            return frame;
        }
    } else if saved.exception_class() == cpu::EC_SVC {
        let arguments = [saved.x[1], saved.x[2], saved.x[3], saved.x[4]];
        if let Some(response) = domain.core.syscall(saved.x[8], saved.x[0], arguments) {
            saved.x[0] = u64::from(response.status.code());
            saved.x[1] = response.values[0];
            saved.x[2] = response.values[1];
            saved.x[3] = response.values[2];
            saved.x[4] = response.values[3];
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
    domain.frame = *saved;
    unsafe { cpu::leave_domain() }
}
