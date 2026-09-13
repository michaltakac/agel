//! Protection domains over a page-table MMU: what AArch64 and RISC-V share.
//!
//! Each of the two includes this file as its `paged` module, so `super` is
//! that architecture: its `cpu` supplies the register frame and the entry
//! into a domain, its `memory` the address space, and its `domain` the trap
//! decode, the running-domain pointer and the kernel's translation root.
//! Everything that is not about the machine at all lives in
//! [`crate::world::DomainCore`].

use super::cpu::{self, TrapFrame};
use super::domain::{restore_kernel_space, CURRENT};
use super::memory::{AddressSpace, IdentityWindow, DOMAIN_BASE};
use crate::memory::{Access, DeviceGrant, FrameLedger, FramePool, MemoryError, PAGE};
use crate::world::{DomainCore, Stop};
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

/// Where the compositor sees the framebuffer: room for 16 MiB below the
/// process window.
#[cfg(feature = "native-graphics")]
pub const DISPLAY_BASE: u64 = DOMAIN_BASE + 0x0400_0000;
/// Where the compositor sees its assets: four slots of 2 MiB, above the
/// process window.
#[cfg(feature = "native-graphics")]
pub const ASSET_BASE: u64 = DOMAIN_BASE + 0x2000_0000;
#[cfg(feature = "native-graphics")]
pub const ASSET_SLOT_BYTES: u64 = 0x0020_0000;
/// Where the compositor sees the windows' canvases: after the asset
/// slots, `CANVAS_BYTES` per window, read-only aliases of pages the
/// owning process maps read-write.
#[cfg(feature = "native-graphics")]
pub const CANVAS_BASE: u64 =
    ASSET_BASE + crate::world::shared::ASSET_SLOTS as u64 * ASSET_SLOT_BYTES;

/// Where a loaded process's segments live: a 16 MiB window well above the
/// stack, shared page and frame window, inside the domain's private region.
#[cfg(feature = "process")]
pub const PROCESS_BASE: u64 = DOMAIN_BASE + 0x1000_0000;
#[cfg(feature = "process")]
pub const PROCESS_BYTES: u64 = 0x0100_0000;

/// An unprivileged world. The trap handler in the architecture's `domain`
/// reaches the three it needs through the running-domain pointer.
pub struct Domain {
    pub(super) space: AddressSpace,
    pub(super) frame: TrapFrame,
    pub(super) core: DomainCore,
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
    /// `entry` unprivileged.
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

    /// Build the compositor's domain: an ordinary world with the framebuffer
    /// mapped at the display window, as normal uncached memory. The pages
    /// are never allocated from the pool and no other domain receives
    /// translations for them. Answers the domain and where the framebuffer
    /// starts inside it.
    #[cfg(feature = "native-graphics")]
    pub fn new_display(
        pool: &mut FramePool,
        identity: IdentityWindow,
        entry: u64,
        tick_budget: u32,
        physical: u64,
        bytes: u64,
    ) -> Result<(Self, u64), MemoryError> {
        pool.open_ledger();
        let page_offset = physical & (PAGE - 1);
        let built = Self::build(pool, identity, entry, tick_budget, DeviceGrant::Nothing, 8)
            .and_then(|mut domain| {
                let start = physical - page_offset;
                let pages = page_offset
                    .checked_add(bytes)
                    .ok_or(MemoryError::OutsideDomainWindow)?
                    .div_ceil(PAGE);
                for page in 0..pages {
                    domain.space.map(
                        pool,
                        DISPLAY_BASE + page * PAGE,
                        start + page * PAGE,
                        Access::UserFramebuffer,
                    )?;
                }
                Ok(domain)
            });
        let frames = pool.close_ledger();
        match built {
            Ok(mut domain) => {
                domain.frames = frames;
                Ok((domain, DISPLAY_BASE + page_offset))
            }
            Err(error) => {
                pool.reclaim(&frames);
                Err(error)
            }
        }
    }

    /// The frames this domain was built from.
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

    /// Allocate a frame and map it into this domain at `virtual_address`,
    /// recording it with the domain's frames. How a loaded process gets its
    /// code, data and zero-filled pages; the frame is identity mapped for
    /// the supervisor, which fills it before the domain ever runs.
    #[cfg(feature = "process")]
    pub fn map_extra(
        &mut self,
        pool: &mut FramePool,
        virtual_address: u64,
        access: Access,
    ) -> Result<u64, MemoryError> {
        let frame = pool.allocate()?;
        self.frames.push(frame)?;
        self.space.map(pool, virtual_address, frame, access)?;
        Ok(frame)
    }

    /// Build the tables under `pages` pages from `base`, every one mapped
    /// read-only to one blank frame, so that `alias` and `unmap` never
    /// allocate: the compositor's canvas slots, made once with the domain.
    #[cfg(feature = "native-graphics")]
    pub fn prepare_aliases(
        &mut self,
        pool: &mut FramePool,
        base: u64,
        pages: u64,
    ) -> Result<(), MemoryError> {
        let blank = pool.allocate()?;
        self.frames.push(blank)?;
        for page in 0..pages {
            self.space
                .map(pool, base + page * PAGE, blank, Access::UserReadOnly)?;
        }
        Ok(())
    }

    /// Point a prepared page at a frame another domain owns, read-only.
    /// The frame stays the other's to reclaim; `unmap` must come first.
    #[cfg(feature = "native-graphics")]
    pub fn alias(&mut self, virtual_address: u64, frame: u64) -> Result<(), MemoryError> {
        self.space
            .set_leaf(virtual_address, Some((frame, Access::UserReadOnly)))
    }

    /// Withdraw a prepared page's mapping; a read there faults after.
    #[cfg(feature = "native-graphics")]
    pub fn unmap(&mut self, virtual_address: u64) {
        let _ = self.space.set_leaf(virtual_address, None);
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
        // so the trap path stays reachable across the switch; `CURRENT` is
        // the trap handler's way back to this domain, see `super::domain`.
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
