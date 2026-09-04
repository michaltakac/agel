//! Protection domains: unprivileged worlds the kernel can contain.
//!
//! A domain owns an address space, a capability space, a register context, a
//! tick budget, and a fault record. It owns no authority it was not handed, and
//! the kernel keeps none of its state inside the domain's memory. That is the
//! whole point: the supervisor must survive anything the world does, including
//! the world doing it on purpose.

use crate::contract::DomainObjects;
use crate::cpu::{self, TrapFrame};
use crate::memory::{Access, AddressSpace, FramePool, MemoryError, DOMAIN_BASE, PAGE};
use agel_kernel_abi::{Operation, Request, Response, Status};

/// Virtual address of a domain's stack region.
pub const STACK_BASE: u64 = DOMAIN_BASE;
/// Pages of stack given to a domain.
pub const STACK_PAGES: u64 = 4;
/// Virtual address of the page a domain shares with the supervisor.
///
/// Bulk data crosses the boundary here, never through the contract's control
/// path: kernel IPC carries bounded words and capability handles, and a page of
/// bytes is neither.
pub const SHARED_BASE: u64 = DOMAIN_BASE + 0x0010_0000;

/// Offsets, in 64-bit words, of the supervisor/world handshake block.
pub mod shared {
    /// What the supervisor is asking the world to do.
    pub const COMMAND: usize = 0;
    /// Contract operation code for [`COMMAND_INVOKE`].
    pub const OPERATION: usize = 1;
    /// Capability slot for [`COMMAND_INVOKE`].
    pub const CAPABILITY: usize = 2;
    /// First of four argument words.
    pub const ARGUMENTS: usize = 3;
    /// Canonical status the world observed.
    pub const STATUS: usize = 7;
    /// First of four result words.
    pub const VALUES: usize = 8;

    // The command codes are deliberately sparse. A dense set makes the ring-3
    // dispatch compile to an indirect jump through a table in `.rodata`, and
    // `.rodata` is supervisor-only: the world would page-fault reading its own
    // switch statement. `./scripts/test-isolation.sh` checks the built
    // `.user_text` for indirect jumps so this cannot silently come back.

    /// Perform one contract invocation and report what came back.
    pub const COMMAND_INVOKE: u64 = 0x0001;
    /// Write to an address only the kernel may touch.
    pub const COMMAND_FAULT_WRITE: u64 = 0x1000;
    /// Divide by zero.
    pub const COMMAND_FAULT_DIVIDE: u64 = 0x2000;
    /// Execute a privileged instruction.
    pub const COMMAND_FAULT_PRIVILEGED: u64 = 0x3000;
    /// Never return.
    pub const COMMAND_SPIN: u64 = 0x4000;
}

/// Why a domain stopped running.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stop {
    /// The world finished its work and yielded through the supervisor endpoint.
    Replied,
    /// The world took a fault it is not allowed to take.
    Faulted(Fault),
    /// The world used its whole tick budget without yielding.
    BudgetExhausted,
}

/// What the hardware reported when a domain faulted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fault {
    /// Architectural exception vector.
    pub vector: u64,
    /// Hardware error code, or zero.
    pub error: u64,
    /// The instruction that faulted.
    pub rip: u64,
    /// The address that faulted, for page faults; otherwise zero.
    pub address: u64,
}

impl Fault {
    /// A short, stable name for the vector, for serial reports and tests.
    pub fn name(&self) -> &'static str {
        match self.vector {
            0 => "divide-error",
            6 => "invalid-opcode",
            8 => "double-fault",
            13 => "general-protection",
            14 => "page-fault",
            _ => "exception",
        }
    }
}

/// An unprivileged world.
pub struct Domain {
    space: AddressSpace,
    frame: TrapFrame,
    objects: DomainObjects,
    shared_physical: u64,
    ticks: u32,
    tick_budget: u32,
    stop: Option<Stop>,
}

impl Domain {
    /// Build a domain with a private stack and one shared page, entering at
    /// `entry` in ring 3.
    pub fn new(
        pool: &mut FramePool,
        identity_pdpt: u64,
        entry: u64,
        tick_budget: u32,
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
            objects: DomainObjects::new(),
            shared_physical,
            ticks: 0,
            tick_budget,
            stop: None,
        })
    }

    /// Write one word of the shared handshake block.
    pub fn write_shared(&mut self, index: usize, value: u64) {
        // Safety: the frame came from the pool, is identity mapped for the
        // kernel, and `index` is bounded by the page.
        unsafe {
            (self.shared_physical as *mut u64)
                .add(index & 0x1ff)
                .write_volatile(value)
        };
    }

    /// Read one word of the shared handshake block.
    ///
    /// The value is whatever an unprivileged world put there. It is data to be
    /// validated, never a kernel decision.
    pub fn read_shared(&self, index: usize) -> u64 {
        // Safety: as in `write_shared`.
        unsafe {
            (self.shared_physical as *const u64)
                .add(index & 0x1ff)
                .read_volatile()
        }
    }

    /// Ask the world to perform one contract invocation and report the answer.
    pub fn invoke_in_world(&mut self, request: &Request) -> Response {
        self.write_shared(shared::OPERATION, u64::from(request.operation.code()));
        self.write_shared(shared::CAPABILITY, u64::from(request.capability));
        for (offset, word) in request.arguments.iter().enumerate() {
            self.write_shared(shared::ARGUMENTS + offset, *word);
        }
        self.write_shared(shared::COMMAND, shared::COMMAND_INVOKE);
        match self.run() {
            Stop::Replied => {}
            // A world that faults or overruns while answering has not answered.
            // Reporting anything else would let a crash masquerade as a result.
            _ => return Response::fail(Status::FaultedDomain),
        }
        let status = Status::from_code(self.read_shared(shared::STATUS) as u16)
            .unwrap_or(Status::InvalidOperation);
        let mut values = [0_u64; agel_kernel_abi::WORDS];
        for (offset, word) in values.iter_mut().enumerate() {
            *word = self.read_shared(shared::VALUES + offset);
        }
        if status == Status::Ok {
            Response::ok(values)
        } else {
            Response::fail(status)
        }
    }

    /// Ask the world to do something it is not allowed to do, and report how it
    /// was stopped.
    pub fn provoke(&mut self, command: u64) -> Stop {
        self.write_shared(shared::COMMAND, command);
        self.run()
    }

    /// Run the domain until it yields, faults, or exhausts its budget.
    pub fn run(&mut self) -> Stop {
        if let Some(stop) = self.stop {
            // A stopped domain stays stopped. Restarting it is a supervisor
            // decision with a new generation, not an automatic retry.
            return stop;
        }
        self.ticks = 0;
        // Safety: the domain's address space maps the whole kernel window, so
        // the trap path stays reachable across the switch.
        unsafe {
            self.space.activate();
            set_current(self);
            cpu::enter_domain(&raw mut self.frame);
            clear_current();
            restore_kernel_space();
        }
        self.stop.unwrap_or(Stop::Replied)
    }

    /// The domain's recorded stop reason, if it has one.
    pub fn stopped(&self) -> Option<Stop> {
        self.stop
    }

    fn record_stop(&mut self, stop: Stop) {
        // Only a fault or an overrun latches. A world that yields politely is
        // expected to be entered again.
        if !matches!(stop, Stop::Replied) {
            self.stop = Some(stop);
        }
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
    unsafe { crate::hal::write_cr3(root) };
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

unsafe fn set_current(domain: *mut Domain) {
    unsafe { CURRENT = domain };
}

unsafe fn clear_current() {
    unsafe { CURRENT = core::ptr::null_mut() };
}

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
        // A trap from ring 0 is a kernel defect, not a containable event. There
        // is no domain to blame and no safe way to continue.
        crate::report_kernel_trap(saved);
    }
    let domain = unsafe { &mut *CURRENT };
    match saved.vector {
        cpu::VECTOR_SYSCALL => {
            if handle_contract_call(domain, saved) {
                return frame;
            }
        }
        cpu::VECTOR_TIMER => {
            // Acknowledge before deciding, so that stopping the domain does not
            // also stop the clock the supervisor needs.
            unsafe { cpu::end_of_interrupt() };
            domain.ticks = domain.ticks.saturating_add(1);
            if domain.ticks <= domain.tick_budget {
                return frame;
            }
            domain.record_stop(Stop::BudgetExhausted);
        }
        _ => {
            domain.record_stop(Stop::Faulted(Fault {
                vector: saved.vector,
                error: saved.error,
                rip: saved.rip,
                address: if saved.vector == 14 {
                    crate::hal::read_cr2()
                } else {
                    0
                },
            }));
        }
    }
    domain.frame = *saved;
    unsafe { cpu::leave_domain() }
}

/// Answer one `int 0x80`. Returns true when the world should be resumed.
fn handle_contract_call(domain: &mut Domain, saved: &mut TrapFrame) -> bool {
    let Some(operation) = Operation::from_code(saved.rax as u16) else {
        write_response(saved, &Response::fail(Status::InvalidOperation));
        return true;
    };
    let Ok(capability) = u32::try_from(saved.rdi) else {
        write_response(saved, &Response::fail(Status::InvalidCapability));
        return true;
    };
    let request = Request::with(
        operation,
        capability,
        [saved.rsi, saved.rdx, saved.r10, saved.r8],
    );
    if domain.objects.is_supervisor_yield(&request) {
        domain.record_stop(Stop::Replied);
        return false;
    }
    let response = domain.objects.invoke(&request);
    write_response(saved, &response);
    true
}

fn write_response(saved: &mut TrapFrame, response: &Response) {
    saved.rax = u64::from(response.status.code());
    saved.rdi = response.values[0];
    saved.rsi = response.values[1];
    saved.rdx = response.values[2];
    saved.r10 = response.values[3];
}
