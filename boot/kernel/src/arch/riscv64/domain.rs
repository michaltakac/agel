//! RISC-V protection domains.
//!
//! Everything that is not about RISC-V lives in [`crate::world::DomainCore`].
//! What remains here is the machine-specific part: an address space, a register
//! frame, the S-mode/U-mode transition, and the cause decode.

use super::cpu::{self, reg, TrapFrame};
use super::memory::{AddressSpace, IdentityWindow, DOMAIN_BASE};
use crate::memory::{Access, FramePool, MemoryError, PAGE};
use crate::world::{DomainCore, Fault, Stop, STACK_PAGES};
use agel_kernel_abi::{Request, Response, Status};

/// Virtual address of a domain's stack region.
const STACK_BASE: u64 = DOMAIN_BASE;
/// Virtual address of the page a domain shares with the supervisor.
const SHARED_BASE: u64 = DOMAIN_BASE + 0x0010_0000;

/// An unprivileged world.
pub struct Domain {
    space: AddressSpace,
    frame: TrapFrame,
    core: DomainCore,
}

impl Domain {
    /// Build a domain with a private stack and one shared page, entering at
    /// `entry` in U-mode.
    pub fn new(
        pool: &mut FramePool,
        identity: IdentityWindow,
        entry: u64,
        tick_budget: u32,
    ) -> Result<Self, MemoryError> {
        let mut space = AddressSpace::new(pool, identity)?;
        for page in 0..STACK_PAGES {
            let frame = pool.allocate()?;
            space.map(pool, STACK_BASE + page * PAGE, frame, Access::UserData)?;
        }
        let shared_physical = pool.allocate()?;
        space.map(pool, SHARED_BASE, shared_physical, Access::UserData)?;
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
}

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
unsafe fn restore_kernel_space() {
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
static mut CURRENT: *mut Domain = core::ptr::null_mut();

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
    let domain = unsafe { &mut *CURRENT };
    if saved.is_timer() {
        // Re-arm before deciding, so that stopping the domain does not also
        // stop the clock the supervisor needs.
        unsafe { cpu::acknowledge_timer() };
        if domain.core.charge_tick() {
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
            saved.x[reg::A0] = u64::from(response.status.code());
            saved.x[reg::A1] = response.values[0];
            saved.x[reg::A2] = response.values[1];
            saved.x[reg::A3] = response.values[2];
            saved.x[reg::A4] = response.values[3];
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
    domain.frame = *saved;
    unsafe { cpu::leave_domain() }
}
