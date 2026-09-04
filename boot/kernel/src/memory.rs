//! Architecture-neutral memory policy.
//!
//! Page-table formats differ on every architecture; what a mapping is *allowed
//! to do*, and where frames come from, do not. Both are stated once here so
//! that write-xor-execute is a property of the shared type rather than of three
//! separate opinions about bit layouts.

use crate::arch;

/// Bytes in one page. All three supported architectures use 4 KiB granules.
pub const PAGE: u64 = 4096;

/// What a mapping into a domain window is allowed to do.
///
/// There is no "user, writable, and executable" variant, and no way to
/// construct one: write-xor-execute is enforced by the type rather than by
/// review. Supervisor mappings are not expressible here at all; they are built
/// once during bring-up and are never derived from a domain's request.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// User read/execute, never writable. Shared program text.
    UserCode,
    /// User read/write, never executable. Stacks, heaps, shared buffers.
    UserData,
}

/// Why a memory request could not be satisfied.
///
/// Every one is a fixed policy bound being reached, never an unexpected
/// condition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemoryError {
    /// The frame pool is exhausted.
    OutOfFrames,
    /// The requested virtual address is outside a domain's private window.
    OutsideDomainWindow,
    /// The address is not page aligned.
    Misaligned,
}

impl MemoryError {
    /// A short name for serial reports.
    pub fn name(self) -> &'static str {
        match self {
            Self::OutOfFrames => "frame pool exhausted",
            Self::OutsideDomainWindow => "address outside the domain window",
            Self::Misaligned => "address is not page aligned",
        }
    }
}

/// A bump allocator over the architecture's fixed physical frame range.
///
/// Frames are never freed in this phase. That is stated rather than hidden: a
/// reclaim path is later work, and pretending to have one would make the
/// resource-accounting story look finished when it is not.
pub struct FramePool {
    next: u64,
}

impl FramePool {
    /// A pool covering the whole fixed range this architecture reserves.
    pub const fn new() -> Self {
        Self {
            next: arch::POOL_START,
        }
    }

    /// Frames still available.
    pub fn remaining(&self) -> u64 {
        (arch::POOL_END - self.next) / PAGE
    }

    /// Take one zeroed frame.
    pub fn allocate(&mut self) -> Result<u64, MemoryError> {
        if self.next >= arch::POOL_END {
            return Err(MemoryError::OutOfFrames);
        }
        let frame = self.next;
        self.next += PAGE;
        // Safety: the frame is inside the pool, which the kernel identity-maps
        // and which no other allocation has handed out.
        unsafe { zero_frame(frame) };
        Ok(frame)
    }

    /// Take `pages` contiguous frames and return the address just past the last
    /// one, which is the top of a downward-growing stack.
    pub fn allocate_stack(&mut self, pages: u64) -> Result<u64, MemoryError> {
        let mut top = 0;
        for _ in 0..pages {
            top = self.allocate()? + PAGE;
        }
        Ok(top)
    }
}

impl Default for FramePool {
    fn default() -> Self {
        Self::new()
    }
}

/// Zero one physical frame through the kernel's identity mapping.
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
pub unsafe fn read_entry(table: u64, index: usize) -> u64 {
    unsafe { (table as *const u64).add(index).read_volatile() }
}

/// Write one page-table entry.
///
/// # Safety
/// See [`read_entry`].
pub unsafe fn write_entry(table: u64, index: usize, value: u64) {
    unsafe { (table as *mut u64).add(index).write_volatile(value) };
}

/// The index into the table at `level` that translates `address`.
///
/// All three architectures use 4 KiB granules and nine index bits per level, so
/// this is genuinely shared arithmetic rather than a coincidence.
pub const fn table_index(address: u64, level: u32) -> usize {
    ((address >> (12 + 9 * level)) & 0x1ff) as usize
}
