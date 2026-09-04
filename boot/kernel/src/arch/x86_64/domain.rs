//! x86-64 protection domains.
//!
//! Everything that is not about x86-64 lives in [`crate::world::DomainCore`].
//! What remains here is exactly the machine-specific part: an address space, a
//! register frame, the ring transition, and the trap decode.

use super::cpu::{self, TrapFrame};
use super::memory::{AddressSpace, DOMAIN_BASE};
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
    /// Whether this domain is the console driver. The device is granted for
    /// the duration of its entries and withheld for everyone else's.
    console: bool,
}

impl Domain {
    /// Build a domain with a private stack and one shared page, entering at
    /// `entry` in ring 3.
    pub fn new(
        pool: &mut FramePool,
        identity_pdpt: u64,
        entry: u64,
        tick_budget: u32,
        console: bool,
    ) -> Result<Self, MemoryError> {
        let mut space = AddressSpace::new(pool, identity_pdpt)?;
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
            console,
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
        // Safety: the domain's address space maps the whole kernel window, so
        // the trap path stays reachable across the switch, and the port grant
        // is installed and withdrawn around this entry alone.
        unsafe {
            cpu::grant_console_ports(self.console);
            self.space.activate();
            CURRENT = self;
            cpu::enter_domain(&raw mut self.frame);
            CURRENT = core::ptr::null_mut();
            restore_kernel_space();
            cpu::grant_console_ports(false);
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

/// The kernel's own page-table root, reinstalled whenever no domain is running,
/// so the supervisor never executes with a world's translations live.
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
    unsafe { super::hal::write_cr3(root) };
}

/// The domain [`cpu::enter_domain`] is currently running, or null.
///
/// The trap handler reaches the running domain through this pointer while
/// [`Domain::run`] still holds `&mut self` further up the same call chain. That
/// is a deliberate aliasing of a manual coroutine switch, not an oversight:
/// control leaves `run` at `iretq` and only comes back through the trap path,
/// so the two references are never live at the same instant. The kernel is
/// single-processor and runs ring 0 with interrupts masked, which is what makes
/// that argument hold.
static mut CURRENT: *mut Domain = core::ptr::null_mut();

/// Handle every trap: contract invocations, timer ticks, and faults.
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
        crate::report_supervisor_trap(saved.vector, saved.error, saved.rip);
    }
    let domain = unsafe { &mut *CURRENT };
    match saved.vector {
        cpu::VECTOR_SYSCALL => {
            let arguments = [saved.rsi, saved.rdx, saved.r10, saved.r8];
            if let Some(response) = domain.core.syscall(saved.rax, saved.rdi, arguments) {
                saved.rax = u64::from(response.status.code());
                saved.rdi = response.values[0];
                saved.rsi = response.values[1];
                saved.rdx = response.values[2];
                saved.r10 = response.values[3];
                return frame;
            }
        }
        cpu::VECTOR_TIMER => {
            // Acknowledge before deciding, so that stopping the domain does not
            // also stop the clock the supervisor needs.
            unsafe { cpu::end_of_interrupt() };
            if domain.core.charge_tick() {
                return frame;
            }
        }
        _ => {
            domain.core.record_stop(Stop::Faulted(Fault {
                cause: saved.vector,
                detail: saved.error,
                pc: saved.rip,
                address: if saved.vector == 14 {
                    super::hal::read_cr2()
                } else {
                    0
                },
            }));
        }
    }
    domain.frame = *saved;
    unsafe { cpu::leave_domain() }
}
