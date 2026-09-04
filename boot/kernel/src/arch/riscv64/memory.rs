//! RISC-V Sv39 translation tables for supervisor and user mode.
//!
//! Three levels over a 39-bit address space, 4 KiB granule: one gibibyte per
//! root entry, two mebibytes per second-level entry, four kibibytes per leaf.
//! The geometry is chosen so the device window, the kernel window, and every
//! domain window land in *different root entries* — if they shared one they
//! would share the table beneath it, and "separate address space" would be a
//! comment rather than a fact.

use super::hal;
use crate::memory::{read_entry, table_index, write_entry, Access, FramePool, MemoryError, PAGE};

/// Physical base of the device window: the UART and the test device.
const DEVICE_BASE: u64 = 0x0000_0000;
/// Physical base of RAM on QEMU's `virt` machine. The first two mebibytes hold
/// OpenSBI, which the kernel maps but must never write.
const RAM_BASE: u64 = 0x8000_0000;
/// How much RAM the identity window describes.
const RAM_BYTES: u64 = 0x0800_0000;
/// Size of one second-level entry's span.
const MEGAPAGE: u64 = 0x0020_0000;
/// Physical base of the two-mebibyte region holding the kernel image.
const IMAGE_BASE: u64 = 0x8020_0000;

/// Virtual base of every domain's private region, in its own root entry.
///
/// Sv39 addresses must be sign-extended from bit 38, so a domain window has to
/// stay inside the lower half. Four gibibytes is comfortably there and is a
/// different root entry from both the device and kernel windows.
pub const DOMAIN_BASE: u64 = 0x0000_0001_0000_0000;

const VALID: u64 = 1 << 0;
const READ: u64 = 1 << 1;
const WRITE: u64 = 1 << 2;
const EXECUTE: u64 = 1 << 3;
const USER: u64 = 1 << 4;
const ACCESSED: u64 = 1 << 6;
const DIRTY: u64 = 1 << 7;

/// Sv39 stores the physical page number ten bits up from the bottom of the
/// entry, so a physical address is shifted rather than masked into place.
const fn address_bits(physical: u64) -> u64 {
    (physical >> 12) << 10
}

const fn entry_address(entry: u64) -> u64 {
    (entry >> 10) << 12
}

const fn leaf_bits(access: Access) -> u64 {
    let common = VALID | ACCESSED | DIRTY | USER;
    match access {
        // U-mode may read and execute; nobody may write.
        Access::UserCode => common | READ | EXECUTE,
        // U-mode may read and write; nobody may execute.
        Access::UserData => common | READ | WRITE,
        // Sv39 leaves have no cache or ordering attributes, so a device page
        // differs from ordinary data only in what is behind it. The ordering
        // that a UART needs comes from the volatile accesses in the driver.
        Access::UserDevice => common | READ | WRITE,
    }
}

/// A supervisor-only leaf the kernel may execute: the kernel image itself.
const KERNEL_TEXT: u64 = VALID | READ | WRITE | EXECUTE | ACCESSED | DIRTY;
/// A supervisor-only megapage of ordinary memory or device registers.
const KERNEL_SPAN: u64 = VALID | READ | WRITE | ACCESSED | DIRTY;

/// The shared supervisor window: device registers, RAM, and the kernel image.
#[derive(Clone, Copy)]
pub struct IdentityWindow {
    device: u64,
    ram: u64,
}

/// An address space: one Sv39 root plus everything reachable from it.
pub struct AddressSpace {
    root: u64,
}

impl AddressSpace {
    /// The value to install in `satp`: Sv39 mode plus the root's page number.
    pub fn satp(&self) -> u64 {
        (8_u64 << 60) | (self.root >> 12)
    }

    /// Build an address space containing only the shared supervisor window.
    ///
    /// The device and RAM tables are shared by every domain, because a trap
    /// taken in U-mode must land in a kernel that is still mapped. Sharing the
    /// *mapping* is not sharing *access*: those entries carry no `U` bit, so
    /// U-mode faults on all of them.
    pub fn new(pool: &mut FramePool, identity: IdentityWindow) -> Result<Self, MemoryError> {
        let root = pool.allocate()?;
        // Safety: `root` is a freshly zeroed frame inside the identity window.
        unsafe {
            write_entry(
                root,
                table_index(DEVICE_BASE, 2),
                address_bits(identity.device) | VALID,
            );
            write_entry(
                root,
                table_index(RAM_BASE, 2),
                address_bits(identity.ram) | VALID,
            );
        }
        Ok(Self { root })
    }

    /// Map one frame into this address space at `virtual_address`.
    ///
    /// Only the domain window is mappable: the supervisor window is shared and
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
        // Three levels: the root is indexed by bits 38:30, the next by 29:21,
        // and the leaf by 20:12.
        for level in (1..3).rev() {
            let index = table_index(virtual_address, level);
            // Safety: `table` is an identity-mapped table frame this address
            // space owns, and `index` is masked to nine bits.
            let existing = unsafe { read_entry(table, index) };
            table = if existing & VALID == 0 {
                let next = pool.allocate()?;
                // A non-leaf entry has none of R, W or X set. Setting one would
                // silently turn it into a huge-page mapping of the table.
                unsafe { write_entry(table, index, address_bits(next) | VALID) };
                next
            } else {
                entry_address(existing)
            };
        }
        let index = table_index(virtual_address, 0);
        // Safety: as above; `table` is now the leaf table.
        unsafe { write_entry(table, index, address_bits(frame) | leaf_bits(access)) };
        Ok(())
    }

    /// Install this address space on the current hart.
    ///
    /// # Safety
    /// The caller must be executing from, and using stacks in, memory this
    /// address space maps.
    pub unsafe fn activate(&self) {
        unsafe { hal::write_satp(self.satp()) };
    }
}

/// Build the shared supervisor window and return the two tables every address
/// space points at.
///
/// The two-mebibyte region holding the kernel image is deliberately split into
/// 4 KiB pages so that the range carrying U-mode program text can be given the
/// user bit without giving it to the whole kernel.
pub fn build_identity_window(
    pool: &mut FramePool,
    user_code: core::ops::Range<u64>,
) -> Result<IdentityWindow, MemoryError> {
    if user_code.start % PAGE != 0 || user_code.end % PAGE != 0 {
        return Err(MemoryError::Misaligned);
    }

    // Device memory: the whole first gibibyte, so the UART and the test device
    // are reachable without a probe. Nothing here is executable, and nothing
    // here is reachable from U-mode.
    let device = pool.allocate()?;
    for span in 0..512_u64 {
        // Safety: freshly zeroed, identity mapped, index below 512.
        unsafe {
            write_entry(
                device,
                span as usize,
                address_bits(DEVICE_BASE + span * MEGAPAGE) | KERNEL_SPAN,
            )
        };
    }

    // RAM, as supervisor megapages, with the kernel image's span split into
    // pages. The first megapage is OpenSBI's and is deliberately left unmapped:
    // the kernel has no business writing the firmware underneath it.
    let ram = pool.allocate()?;
    let image = pool.allocate()?;
    for page in 0..512_u64 {
        let address = IMAGE_BASE + page * PAGE;
        let bits = if user_code.contains(&address) {
            // U-mode program text: readable and executable there, never
            // writable.
            leaf_bits(Access::UserCode)
        } else {
            // Kernel text, rodata, data, bss and the supervisor stack. It is
            // mapped writable and executable together only because the
            // boot-time layout does not separate them; U-mode reaches none of
            // it.
            KERNEL_TEXT
        };
        unsafe { write_entry(image, page as usize, address_bits(address) | bits) };
    }
    unsafe { write_entry(ram, table_index(IMAGE_BASE, 1), address_bits(image) | VALID) };
    let mut span = IMAGE_BASE + MEGAPAGE;
    while span < RAM_BASE + RAM_BYTES {
        let index = table_index(span, 1);
        unsafe { write_entry(ram, index, address_bits(span) | KERNEL_SPAN) };
        span += MEGAPAGE;
    }

    Ok(IdentityWindow { device, ram })
}
