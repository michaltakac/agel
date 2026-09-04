//! Physical frames and per-domain address spaces.
//!
//! The BIOS stage hands the kernel one identity mapping of the first gibibyte
//! built from supervisor-only 2 MiB pages. That is enough to reach long mode and
//! nothing else: it cannot express a user-accessible page, it cannot express
//! write-xor-execute, and it is one table shared by everything.
//!
//! This module replaces it with tables the kernel owns, and gives every
//! protection domain its own root. Two domains therefore cannot name each
//! other's memory at all — not "are not supposed to", but have no translation
//! for it.

use crate::hal;

/// Bytes in one page.
pub const PAGE: u64 = 4096;

/// Start of the frame pool. Everything below is BIOS structures, the boot
/// sector, the kernel image, and the kernel stack.
const POOL_START: u64 = 0x0020_0000;
/// End of the frame pool. QEMU is started with more memory than this; the bound
/// is a deliberate fixed resource policy, not a probe result.
const POOL_END: u64 = 0x0100_0000;

/// Identity-mapped supervisor window, built from 2 MiB pages except for the
/// first, which is split so that user-executable pages can be carved out of it.
const IDENTITY_BYTES: u64 = 0x0100_0000;

/// Virtual base of every domain's private region. Each domain sees its own
/// memory at the same address, backed by different frames, so a pointer from
/// one domain is meaningless in another rather than dangerous.
///
/// It is 512 GiB rather than something smaller so that it falls in a different
/// top-level slot from the shared kernel window. If the two shared a PML4 entry
/// they would share the table beneath it, and "separate address space" would be
/// a comment rather than a fact.
pub const DOMAIN_BASE: u64 = 0x0000_0080_0000_0000;

// Page-table entry flags.
const PRESENT: u64 = 1 << 0;
const WRITABLE: u64 = 1 << 1;
const USER: u64 = 1 << 2;
const HUGE: u64 = 1 << 7;
const NO_EXECUTE: u64 = 1 << 63;
const ADDRESS_MASK: u64 = 0x000f_ffff_ffff_f000;

/// What a mapping into a domain window is allowed to do.
///
/// There is no "user, writable, and executable" variant, and no way to
/// construct one: write-xor-execute is enforced by the type rather than by
/// review. Supervisor mappings are not expressible here at all; they are built
/// once by [`build_identity_window`] and are never derived from a domain's
/// request.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// User read/execute, never writable. Shared program text.
    UserCode,
    /// User read/write, never executable. Stacks, heaps, shared buffers.
    UserData,
}

impl Access {
    const fn bits(self) -> u64 {
        match self {
            Self::UserCode => PRESENT | USER,
            Self::UserData => PRESENT | WRITABLE | USER | NO_EXECUTE,
        }
    }
}

/// Why a memory request could not be satisfied. Every one is a fixed policy
/// bound being reached, never an unexpected condition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemoryError {
    /// The frame pool is exhausted.
    OutOfFrames,
    /// The requested virtual address is outside a domain's private window.
    OutsideDomainWindow,
    /// The address is not page aligned.
    Misaligned,
}

/// A bump allocator over a fixed physical range.
///
/// Frames are never freed in this phase. That is stated rather than hidden: a
/// reclaim path is Phase 3 work, and pretending to have one would make the
/// resource accounting story look finished when it is not.
pub struct FramePool {
    next: u64,
}

impl FramePool {
    /// A pool covering the whole fixed range.
    pub const fn new() -> Self {
        Self { next: POOL_START }
    }

    /// Frames still available.
    pub fn remaining(&self) -> u64 {
        (POOL_END - self.next) / PAGE
    }

    /// Take one zeroed frame.
    pub fn allocate(&mut self) -> Result<u64, MemoryError> {
        if self.next >= POOL_END {
            return Err(MemoryError::OutOfFrames);
        }
        let frame = self.next;
        self.next += PAGE;
        // Safety: the frame is inside the pool, which the kernel identity-maps
        // and which no other allocation has handed out.
        unsafe { zero_frame(frame) };
        Ok(frame)
    }
}

impl Default for FramePool {
    fn default() -> Self {
        Self::new()
    }
}

/// Zero one physical frame through the identity mapping.
///
/// # Safety
/// `frame` must be a page-aligned, identity-mapped, exclusively owned frame.
unsafe fn zero_frame(frame: u64) {
    let words = frame as *mut u64;
    for index in 0..(PAGE / 8) as usize {
        unsafe { words.add(index).write_volatile(0) };
    }
}

/// Read one page-table entry.
///
/// # Safety
/// `table` must be an identity-mapped page-table frame and `index` below 512.
unsafe fn entry(table: u64, index: usize) -> u64 {
    unsafe { (table as *const u64).add(index).read_volatile() }
}

/// Write one page-table entry.
///
/// # Safety
/// See [`entry`].
unsafe fn set_entry(table: u64, index: usize, value: u64) {
    unsafe { (table as *mut u64).add(index).write_volatile(value) };
}

const fn index_of(address: u64, level: u32) -> usize {
    ((address >> (12 + 9 * level)) & 0x1ff) as usize
}

/// An address space: one PML4 plus the tables reachable from it.
///
/// Every address space shares the kernel's identity window, mapped
/// supervisor-only, because a trap taken in user mode must land in a kernel
/// that is still mapped. Sharing the *mapping* is not sharing *access*: those
/// pages carry no user bit, so ring 3 faults on them.
pub struct AddressSpace {
    root: u64,
}

impl AddressSpace {
    /// The physical address to load into `cr3`.
    pub fn root(&self) -> u64 {
        self.root
    }

    /// Build an address space containing only the shared kernel window.
    ///
    /// `identity_pdpt` is the shared upper-level table produced by
    /// [`build_identity_window`]; every domain points its first PML4 entry at
    /// it, so the kernel is mapped once and described once.
    pub fn new(pool: &mut FramePool, identity_pdpt: u64) -> Result<Self, MemoryError> {
        let root = pool.allocate()?;
        // Safety: `root` is a freshly zeroed frame inside the identity window.
        unsafe { set_entry(root, 0, identity_pdpt | PRESENT | WRITABLE | USER) };
        Ok(Self { root })
    }

    /// Map one frame into this address space at `virtual_address`.
    ///
    /// Only the domain window is mappable: the kernel window is shared and
    /// immutable from here, so a domain cannot install a mapping that overlaps
    /// the kernel however it is asked to.
    pub fn map(
        &mut self,
        pool: &mut FramePool,
        virtual_address: u64,
        frame: u64,
        access: Access,
    ) -> Result<(), MemoryError> {
        if virtual_address % PAGE != 0 || frame % PAGE != 0 {
            return Err(MemoryError::Misaligned);
        }
        if virtual_address < DOMAIN_BASE {
            return Err(MemoryError::OutsideDomainWindow);
        }
        let mut table = self.root;
        for level in (1..4).rev() {
            let index = index_of(virtual_address, level);
            // Safety: `table` is an identity-mapped table frame this address
            // space owns, and `index` is masked to 9 bits.
            let existing = unsafe { entry(table, index) };
            table = if existing & PRESENT == 0 {
                let next = pool.allocate()?;
                // Intermediate entries are permissive; the leaf decides. This is
                // the architectural rule, not a shortcut.
                unsafe { set_entry(table, index, next | PRESENT | WRITABLE | USER) };
                next
            } else {
                existing & ADDRESS_MASK
            };
        }
        let index = index_of(virtual_address, 0);
        // Safety: as above; `table` is now the leaf page table.
        unsafe { set_entry(table, index, frame | access.bits()) };
        Ok(())
    }

    /// Install this address space on the current processor.
    ///
    /// # Safety
    /// The caller must be executing from, and using stacks in, memory this
    /// address space maps.
    pub unsafe fn activate(&self) {
        unsafe { hal::write_cr3(self.root) };
    }
}

/// Build the shared supervisor identity window and return its PDPT frame.
///
/// The first 2 MiB is deliberately split into 4 KiB pages so that the ranges
/// carrying ring-3 program text can be given the user bit without giving it to
/// the whole kernel. `user_code` bounds that range; it must be page aligned and
/// must not overlap any writable kernel data.
pub fn build_identity_window(
    pool: &mut FramePool,
    user_code: core::ops::Range<u64>,
) -> Result<u64, MemoryError> {
    if user_code.start % PAGE != 0 || user_code.end % PAGE != 0 {
        return Err(MemoryError::Misaligned);
    }
    let pdpt = pool.allocate()?;
    let directory = pool.allocate()?;
    // Safety: both frames are freshly zeroed and identity mapped.
    unsafe { set_entry(pdpt, 0, directory | PRESENT | WRITABLE | USER) };

    // The first 2 MiB, page by page, so the user-code hole is expressible.
    let first = pool.allocate()?;
    for page in 0..512_u64 {
        let address = page * PAGE;
        let bits = if user_code.contains(&address) {
            // Ring-3 program text: readable and executable, never writable.
            Access::UserCode.bits()
        } else {
            // Everything else in the low 2 MiB is kernel text, kernel rodata,
            // BIOS tables, and the kernel stack. It is mapped writable and
            // executable together only because the boot-time layout does not
            // separate them; ring 3 carries no user bit for any of it.
            PRESENT | WRITABLE
        };
        unsafe { set_entry(first, page as usize, address | bits) };
    }
    unsafe { set_entry(directory, 0, first | PRESENT | WRITABLE | USER) };

    // The rest of the identity window as supervisor 2 MiB pages.
    let mut address = 0x0020_0000_u64;
    while address < IDENTITY_BYTES {
        let index = index_of(address, 1);
        unsafe { set_entry(directory, index, address | PRESENT | WRITABLE | HUGE) };
        address += 0x0020_0000;
    }
    Ok(pdpt)
}

/// Turn on the no-execute page-table bit.
///
/// Without this, `Access::UserData` silently loses its non-executable promise,
/// so the kernel enables it before building any table and treats failure as
/// fatal rather than continuing with a weaker guarantee than it documents.
pub fn enable_no_execute() -> bool {
    // Safety: EFER exists on every long-mode processor, and we only add a bit.
    unsafe {
        let efer = hal::read_msr(hal::MSR_EFER);
        hal::write_msr(hal::MSR_EFER, efer | hal::EFER_NXE);
        hal::read_msr(hal::MSR_EFER) & hal::EFER_NXE != 0
    }
}
