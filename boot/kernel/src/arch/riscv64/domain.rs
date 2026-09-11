//! RISC-V protection domains.
//!
//! Everything that is not about RISC-V lives in [`crate::world::DomainCore`].
//! What remains here is the machine-specific part: an address space, a register
//! frame, the S-mode/U-mode transition, and the cause decode.

use super::cpu::{self, reg, TrapFrame};
use super::memory::{AddressSpace, IdentityWindow, DOMAIN_BASE};
use crate::memory::{Access, DeviceGrant, FrameLedger, FramePool, MemoryError, PAGE};
use crate::world::{DomainCore, Fault, Stop};
#[cfg(not(any(feature = "isolated-repl", feature = "native-graphics")))]
use agel_kernel_abi::{Request, Response, Status};

/// Virtual address of a domain's stack region.
const STACK_BASE: u64 = DOMAIN_BASE;
/// Virtual address of the page a domain shares with the supervisor.
const SHARED_BASE: u64 = DOMAIN_BASE + 0x0010_0000;
/// Virtual address of the console device, mapped only into the driver domain.
pub const DEVICE_BASE: u64 = DOMAIN_BASE + 0x0020_0000;
/// Virtual address of the storage device registers, mapped only into the
/// storage driver domain.
pub const STORAGE_DEVICE_BASE: u64 = DOMAIN_BASE + 0x0028_0000;
/// Virtual address of the storage driver's DMA page.
pub const DMA_BASE: u64 = DOMAIN_BASE + 0x0030_0000;

/// Virtual address of the frame window: `CONFORMANCE_FRAME_WINDOW` pages a
/// world maps its frames into through the contract's memory group.
#[cfg(feature = "contract-memory")]
pub const FRAME_WINDOW_BASE: u64 = DOMAIN_BASE + 0x0040_0000;
#[cfg(feature = "contract-memory")]
const WINDOW_PAGES: usize = agel_kernel_abi::CONFORMANCE_FRAME_WINDOW as usize;
#[cfg(feature = "contract-memory")]
const WINDOW_FRAMES: usize = agel_kernel_abi::CONFORMANCE_FRAME_BUDGET as usize + 1;

/// The page-table shape of a mapping's rights. `execute` implies `read` on
/// every machine here, and `write` with `execute` never reaches this point:
/// the contract refuses it.
#[cfg(feature = "contract-memory")]
fn window_access(rights: agel_kernel_abi::Rights) -> Access {
    if rights.contains(agel_kernel_abi::Rights::EXECUTE) {
        Access::UserCode
    } else if rights.contains(agel_kernel_abi::Rights::WRITE) {
        Access::UserData
    } else {
        Access::UserReadOnly
    }
}

/// An unprivileged world.
pub struct Domain {
    space: AddressSpace,
    frame: TrapFrame,
    core: DomainCore,
    /// Every frame this domain was built from, for reclamation.
    frames: FrameLedger,
    /// The physical frames behind the domain's frame budget, frame 0 first,
    /// reserved when the domain is built so a memory operation at trap time
    /// never allocates.
    #[cfg(feature = "contract-memory")]
    window_frames: [u64; WINDOW_FRAMES],
    /// What the page tables currently say about each window page: the frame
    /// number and the rights, mirrored from the object table.
    #[cfg(feature = "contract-memory")]
    window: [Option<(u8, u32)>; WINDOW_PAGES],
}

impl Domain {
    /// Build a domain with a private stack and one shared page, entering at
    /// `entry` in U-mode.
    pub fn new(
        pool: &mut FramePool,
        identity: IdentityWindow,
        entry: u64,
        tick_budget: u32,
        grant: DeviceGrant,
        stack_pages: u64,
    ) -> Result<Self, MemoryError> {
        pool.open_ledger();
        let built = Self::build(pool, identity, entry, tick_budget, grant, stack_pages);
        let frames = pool.close_ledger();
        match built {
            Ok(mut domain) => {
                domain.frames = frames;
                Ok(domain)
            }
            Err(error) => {
                // Nothing of a half-built domain is live; its frames go back.
                pool.reclaim(&frames);
                Err(error)
            }
        }
    }

    /// The frames this domain was built from.
    #[cfg(not(any(feature = "isolated-repl", feature = "native-graphics")))]
    pub fn frames(&self) -> &FrameLedger {
        &self.frames
    }

    // Built once in machine code: inlining this into every constructor
    // duplicated the whole domain build and cost the x86-64 image its budget.
    #[inline(never)]
    fn build(
        pool: &mut FramePool,
        identity: IdentityWindow,
        entry: u64,
        tick_budget: u32,
        grant: DeviceGrant,
        stack_pages: u64,
    ) -> Result<Self, MemoryError> {
        let mut space = AddressSpace::new(pool, identity)?;
        for page in 0..stack_pages {
            let frame = pool.allocate()?;
            space.map(pool, STACK_BASE + page * PAGE, frame, Access::UserData)?;
        }
        let shared_physical = pool.allocate()?;
        space.map(pool, SHARED_BASE, shared_physical, Access::UserData)?;
        // The frame window: the frames behind the domain's budget and the
        // tables under every window page, built now so a memory operation
        // never allocates at trap time. The pages start unmapped.
        #[cfg(feature = "contract-memory")]
        let window_frames = {
            let mut frames = [0_u64; WINDOW_FRAMES];
            for frame in frames.iter_mut() {
                *frame = pool.allocate()?;
            }
            for page in 0..WINDOW_PAGES as u64 {
                let address = FRAME_WINDOW_BASE + page * PAGE;
                space.map(pool, address, frames[0], Access::UserReadOnly)?;
                space.set_leaf(address, None)?;
            }
            frames
        };
        // A device is mapped into exactly one domain. Every other world has
        // no translation for it at all, so reaching it is not a permission
        // failure but an absence.
        let mut core = DomainCore::new(shared_physical, tick_budget);
        match grant {
            DeviceGrant::Nothing => {}
            DeviceGrant::Console(device) => {
                space.map(pool, DEVICE_BASE, device, Access::UserDevice)?;
            }
            DeviceGrant::Storage {
                page,
                register_offset,
            } => {
                space.map(pool, STORAGE_DEVICE_BASE, page, Access::UserDevice)?;
                let dma = pool.allocate()?;
                space.map(pool, DMA_BASE, dma, Access::UserData)?;
                // The driver learns where its registers and its DMA frame
                // are from the shared page: the register window's offset
                // inside the granted page, and the frame's physical address,
                // which is what the device must be told.
                core.write_shared(
                    crate::world::shared::DEVICE_MMIO,
                    STORAGE_DEVICE_BASE + register_offset,
                );
                core.write_shared(crate::world::shared::DEVICE_DMA, dma);
            }
        }
        // The stack grows down from the top of the last mapped stack page. The
        // page above is deliberately absent, so an overflowing world faults
        // instead of walking into whatever the allocator handed out next.
        let stack_top = STACK_BASE + stack_pages * PAGE;
        Ok(Self {
            space,
            frame: TrapFrame::user(entry, stack_top, SHARED_BASE),
            core,
            frames: FrameLedger::EMPTY,
            #[cfg(feature = "contract-memory")]
            window_frames,
            #[cfg(feature = "contract-memory")]
            window: [None; WINDOW_PAGES],
        })
    }

    #[cfg(not(any(feature = "isolated-repl", feature = "native-graphics")))]
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

    /// Make the page tables say what the object table says about the frame
    /// window. Called after every memory operation the object table
    /// accepted; the tables under the window were built with the domain, so
    /// nothing here allocates or can fail for want of memory.
    #[cfg(feature = "contract-memory")]
    pub fn reconcile_window(&mut self) {
        for page in 0..WINDOW_PAGES {
            let desired = self
                .core
                .mapping(page)
                .map(|(number, rights)| (number, rights.0));
            if desired == self.window[page] {
                continue;
            }
            let address = FRAME_WINDOW_BASE + page as u64 * PAGE;
            let leaf = desired.map(|(number, rights)| {
                (
                    self.window_frames[usize::from(number) % WINDOW_FRAMES],
                    window_access(agel_kernel_abi::Rights(rights)),
                )
            });
            if self.space.set_leaf(address, leaf).is_ok() {
                self.window[page] = desired;
            }
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
