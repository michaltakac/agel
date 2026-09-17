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
// The shared prefix is the point: every variant is a *user* mapping, because
// supervisor mappings are not expressible here at all.
#[allow(clippy::enum_variant_names)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// User read/execute, never writable. Shared program text.
    UserCode,
    /// User read-only, never executable. Immutable evaluator constants.
    UserReadOnly,
    /// User read/write, never executable. Stacks, heaps, shared buffers.
    UserData,
    /// A device register window granted to exactly one domain. Read/write,
    /// never executable, and never cached or reordered.
    ///
    /// x86-64 never constructs this: its console lives behind I/O ports, so a
    /// device grant there is a task-state-segment bitmap entry rather than a
    /// mapping. The variant is still part of the shared vocabulary because the
    /// other two architectures grant devices by mapping them.
    #[cfg_attr(
        all(target_arch = "x86_64", not(feature = "native-graphics")),
        allow(dead_code)
    )]
    UserDevice,
    /// A framebuffer granted to the compositor: read/write, never
    /// executable, and ordinary memory rather than a device register
    /// window, since pixels are written with whatever width and alignment
    /// the painter chooses; uncached where the machine distinguishes.
    #[cfg_attr(
        not(all(target_arch = "aarch64", feature = "native-graphics")),
        allow(dead_code)
    )]
    UserFramebuffer,
}

/// What a domain on a machine with memory-mapped devices is granted beyond
/// its stack and shared page. x86-64 grants I/O ports instead.
#[derive(Clone, Copy)]
#[cfg_attr(target_arch = "x86_64", allow(dead_code))]
pub enum DeviceGrant {
    /// Nothing: an ordinary world.
    Nothing,
    /// One page of device registers at the console window.
    Console(u64),
    /// One page of device registers at the storage window plus one ordinary
    /// frame, allocated with the domain, at the DMA window. The domain's
    /// shared page is told where the registers are inside the window and the
    /// frame's physical address, which is what the device must be told.
    Storage { page: u64, register_offset: u64 },
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

/// Frames in the pool, and the words of a bitmap over them: one bit a
/// frame, so a set of frames costs the same however many it names.
const POOL_FRAMES: u64 = (arch::POOL_END - arch::POOL_START) / PAGE;
const POOL_WORDS: usize = POOL_FRAMES.div_ceil(64) as usize;

/// The bit for a frame of the pool.
#[inline(always)]
fn frame_bit(frame: u64) -> (usize, u64) {
    let index = (frame - arch::POOL_START) / PAGE;
    ((index / 64) as usize, 1 << (index % 64))
}

/// Frames a domain was built from, so that a replaced domain gives them back.
///
/// A bitmap over the pool: a domain may hold any number of its frames, a
/// loaded process with a canvas and a large heap as much as a driver with
/// a page, and every domain carries the same fixed-size record of them.
/// The pool records every frame it hands out while a ledger is open, and
/// extending a domain records both its new pages and their page tables.
#[derive(Clone, Copy)]
pub struct FrameLedger {
    bits: [u64; POOL_WORDS],
    count: u32,
}

impl FrameLedger {
    pub const EMPTY: Self = Self {
        bits: [0; POOL_WORDS],
        count: 0,
    };

    /// How many frames the ledger names.
    #[cfg(not(any(feature = "isolated-repl", feature = "native-graphics")))]
    pub fn count(&self) -> usize {
        self.count as usize
    }

    fn record(&mut self, frame: u64) -> Result<(), MemoryError> {
        if !(arch::POOL_START..arch::POOL_END).contains(&frame) {
            return Err(MemoryError::OutsideDomainWindow);
        }
        let (word, bit) = frame_bit(frame);
        if self.bits[word] & bit == 0 {
            self.bits[word] |= bit;
            self.count += 1;
        }
        Ok(())
    }
}

/// The architecture's fixed physical frame range: a bump allocator for
/// frames never handed out yet, and a bitmap of frames given back by
/// replaced domains, which are handed out again first.
///
/// Every frame is zeroed when it is handed out, whichever way it came,
/// so nothing a dead domain wrote reaches its successor.
pub struct FramePool {
    next: u64,
    freed: [u64; POOL_WORDS],
    free_count: u64,
    ledger: Option<FrameLedger>,
}

impl FramePool {
    /// A pool covering the whole fixed range this architecture reserves.
    pub const fn new() -> Self {
        Self {
            next: arch::POOL_START,
            freed: [0; POOL_WORDS],
            free_count: 0,
            ledger: None,
        }
    }

    /// Frames still available: never handed out, plus given back.
    #[cfg(not(any(feature = "isolated-repl", feature = "native-graphics")))]
    pub fn remaining(&self) -> u64 {
        (arch::POOL_END - self.next) / PAGE + self.free_count
    }

    /// Take one zeroed frame, a given-back one first.
    pub fn allocate(&mut self) -> Result<u64, MemoryError> {
        let frame = match self.take_freed() {
            Some(frame) => frame,
            None => self.bump()?,
        };
        if let Some(ledger) = self.ledger.as_mut() {
            if let Err(error) = ledger.record(frame) {
                // The frame is not lost: an unrecorded frame goes straight
                // back, and the domain being built is refused.
                self.give_back(frame);
                return Err(error);
            }
        }
        // Safety: the frame is inside the pool, which the kernel identity-maps
        // and which no live domain holds.
        unsafe { zero_frame(frame) };
        Ok(frame)
    }

    /// The lowest given-back frame, taken out of the bitmap.
    fn take_freed(&mut self) -> Option<u64> {
        if self.free_count == 0 {
            return None;
        }
        let (word, bits) = self
            .freed
            .iter()
            .enumerate()
            .find(|(_, bits)| **bits != 0)?;
        let bit = bits.trailing_zeros() as u64;
        self.freed[word] &= !(1 << bit);
        self.free_count -= 1;
        Some(arch::POOL_START + (word as u64 * 64 + bit) * PAGE)
    }

    fn bump(&mut self) -> Result<u64, MemoryError> {
        if self.next >= arch::POOL_END {
            return Err(MemoryError::OutOfFrames);
        }
        let frame = self.next;
        self.next += PAGE;
        Ok(frame)
    }

    /// Start recording the frames handed out, for a domain being built.
    pub fn open_ledger(&mut self) {
        self.ledger = Some(FrameLedger::EMPTY);
    }

    /// Stop recording and return what was handed out since the ledger opened.
    pub fn close_ledger(&mut self) -> FrameLedger {
        self.ledger.take().unwrap_or(FrameLedger::EMPTY)
    }

    /// Record every allocation made while extending a domain, including
    /// intermediate page tables. Keep them even on failure: a partially
    /// installed table is still reachable until the domain is destroyed.
    #[cfg(feature = "pages")]
    pub fn record_allocations<T>(
        &mut self,
        frames: &mut FrameLedger,
        action: impl FnOnce(&mut Self) -> Result<T, MemoryError>,
    ) -> Result<T, MemoryError> {
        self.open_ledger();
        let result = action(self);
        let added = self.close_ledger();
        for (held, new) in frames.bits.iter_mut().zip(added.bits) {
            frames.count += (new & !*held).count_ones();
            *held |= new;
        }
        result
    }

    /// Give a dead domain's frames back. The caller must have dropped every
    /// translation to them: the frames are handed out again to whoever
    /// allocates next, zeroed, and a domain that still mapped one would be
    /// sharing memory with its successor.
    pub fn reclaim(&mut self, ledger: &FrameLedger) {
        for (word, bits) in ledger.bits.iter().enumerate() {
            let mut bits = *bits;
            while bits != 0 {
                let bit = bits.trailing_zeros() as u64;
                bits &= !(1 << bit);
                self.give_back(arch::POOL_START + (word as u64 * 64 + bit) * PAGE);
            }
        }
    }

    fn give_back(&mut self, frame: u64) {
        let (word, bit) = frame_bit(frame);
        if self.freed[word] & bit == 0 {
            self.freed[word] |= bit;
            self.free_count += 1;
        }
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
