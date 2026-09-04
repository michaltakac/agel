//! AArch64 stage-1 translation tables for EL1 and EL0.
//!
//! Three levels over a 39-bit address space, 4 KiB granule: one gibibyte per
//! top-level entry, two mebibytes per second-level block, four kibibytes per
//! leaf. The geometry is chosen so the kernel window, the device window, and
//! every domain window land in *different top-level entries* — if they shared
//! one they would share the table beneath it, and "separate address space"
//! would be a comment rather than a fact.

use super::hal;
use crate::memory::{read_entry, table_index, write_entry, Access, FramePool, MemoryError, PAGE};

/// Physical base of the device window: the UART and the interrupt controller.
const DEVICE_BASE: u64 = 0x0000_0000;
/// Physical base of RAM on QEMU's `virt` machine.
const RAM_BASE: u64 = 0x4000_0000;
/// How much RAM the identity window describes.
const RAM_BYTES: u64 = 0x0800_0000;
/// Size of one second-level block.
const BLOCK: u64 = 0x0020_0000;

/// Virtual base of every domain's private region, in its own top-level entry.
pub const DOMAIN_BASE: u64 = 0x8000_0000;

const VALID: u64 = 1 << 0;
/// At levels 1 and 2 this bit means "table"; at level 3 it means "page".
const TABLE_OR_PAGE: u64 = 1 << 1;
const ATTR_NORMAL: u64 = 0 << 2;
const ATTR_DEVICE: u64 = 1 << 2;
const AP_EL1_RW: u64 = 0 << 6;
const AP_RW_ANY: u64 = 1 << 6;
const AP_RO_ANY: u64 = 3 << 6;
const SH_INNER: u64 = 3 << 8;
const ACCESS_FLAG: u64 = 1 << 10;
const PRIVILEGED_EXECUTE_NEVER: u64 = 1 << 53;
const USER_EXECUTE_NEVER: u64 = 1 << 54;
const ADDRESS_MASK: u64 = 0x0000_ffff_ffff_f000;

/// `MAIR_EL1`: attribute 0 is normal write-back memory, attribute 1 is
/// device-nGnRnE. Two attributes are all a kernel with one UART needs.
const MAIR: u64 = 0xff;

/// `TCR_EL1` for a 39-bit `TTBR0` space with 4 KiB granules, inner-shareable
/// write-back walks, a 40-bit physical size, and `TTBR1` walks disabled.
const TCR: u64 = 25            // T0SZ: 64 - 39
    | (1 << 8)                 // IRGN0: write-back write-allocate
    | (1 << 10)                // ORGN0: write-back write-allocate
    | (3 << 12)                // SH0: inner shareable
    | (25 << 16)               // T1SZ, unused but must be legal
    | (1 << 23)                // EPD1: no TTBR1 walks
    | (2 << 32); // IPS: 40-bit physical addresses

const fn leaf_bits(access: Access) -> u64 {
    let common = VALID | TABLE_OR_PAGE | ACCESS_FLAG;
    match access {
        // EL0 may read and execute; nobody may write, and EL1 may not execute.
        Access::UserCode => common | ATTR_NORMAL | SH_INNER | AP_RO_ANY | PRIVILEGED_EXECUTE_NEVER,
        Access::UserReadOnly => {
            common
                | ATTR_NORMAL
                | SH_INNER
                | AP_RO_ANY
                | PRIVILEGED_EXECUTE_NEVER
                | USER_EXECUTE_NEVER
        }
        // EL0 and EL1 may read and write; nobody may execute.
        Access::UserData => {
            common
                | ATTR_NORMAL
                | SH_INNER
                | AP_RW_ANY
                | PRIVILEGED_EXECUTE_NEVER
                | USER_EXECUTE_NEVER
        }
        // A device granted to one domain: device-nGnRnE rather than normal
        // memory, because a UART is not somewhere the processor may reorder or
        // combine accesses.
        Access::UserDevice => {
            common | ATTR_DEVICE | AP_RW_ANY | PRIVILEGED_EXECUTE_NEVER | USER_EXECUTE_NEVER
        }
    }
}

/// A supervisor-only leaf that EL1 may execute: the kernel image itself.
const KERNEL_TEXT: u64 =
    VALID | TABLE_OR_PAGE | ATTR_NORMAL | AP_EL1_RW | SH_INNER | ACCESS_FLAG | USER_EXECUTE_NEVER;

/// A supervisor-only 2 MiB block of ordinary memory that nobody may execute.
const KERNEL_BLOCK: u64 = VALID
    | ATTR_NORMAL
    | AP_EL1_RW
    | SH_INNER
    | ACCESS_FLAG
    | PRIVILEGED_EXECUTE_NEVER
    | USER_EXECUTE_NEVER;

/// A supervisor-only 2 MiB block of device memory.
const DEVICE_BLOCK: u64 =
    VALID | ATTR_DEVICE | AP_EL1_RW | ACCESS_FLAG | PRIVILEGED_EXECUTE_NEVER | USER_EXECUTE_NEVER;

/// The shared supervisor window: device memory, RAM, and the kernel image.
#[derive(Clone, Copy)]
pub struct IdentityWindow {
    device: u64,
    ram: u64,
}

/// An address space: one top-level table plus everything reachable from it.
pub struct AddressSpace {
    root: u64,
}

impl AddressSpace {
    /// The physical address to install in `TTBR0_EL1`.
    pub fn root(&self) -> u64 {
        self.root
    }

    /// Build an address space containing only the shared supervisor window.
    ///
    /// The device and RAM tables are shared by every domain, because a trap
    /// taken at EL0 must land in a kernel that is still mapped. Sharing the
    /// *mapping* is not sharing *access*: those entries carry `AP_EL1_RW`, so
    /// EL0 faults on all of them.
    pub fn new(pool: &mut FramePool, identity: IdentityWindow) -> Result<Self, MemoryError> {
        let root = pool.allocate()?;
        // Safety: `root` is a freshly zeroed frame inside the identity window.
        unsafe {
            write_entry(
                root,
                table_index(DEVICE_BASE, 2),
                identity.device | VALID | TABLE_OR_PAGE,
            );
            write_entry(
                root,
                table_index(RAM_BASE, 2),
                identity.ram | VALID | TABLE_OR_PAGE,
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
        // and the leaf by 20:12. There is no fourth level to walk.
        for level in (1..3).rev() {
            let index = table_index(virtual_address, level);
            // Safety: `table` is an identity-mapped table frame this address
            // space owns, and `index` is masked to nine bits.
            let existing = unsafe { read_entry(table, index) };
            table = if existing & VALID == 0 {
                let next = pool.allocate()?;
                unsafe { write_entry(table, index, next | VALID | TABLE_OR_PAGE) };
                next
            } else {
                existing & ADDRESS_MASK
            };
        }
        let index = table_index(virtual_address, 0);
        // Safety: as above; `table` is now the leaf table.
        unsafe { write_entry(table, index, frame | leaf_bits(access)) };
        Ok(())
    }

    /// Install this address space on the current processor.
    ///
    /// # Safety
    /// The caller must be executing from, and using stacks in, memory this
    /// address space maps.
    pub unsafe fn activate(&self) {
        unsafe { hal::write_ttbr0(self.root) };
    }
}

/// Build the shared supervisor window and return the two tables every address
/// space points at.
///
/// The 2 MiB block holding the kernel image is deliberately split into 4 KiB
/// pages so that the range carrying EL0 program text can be given EL0 access
/// without giving it to the whole kernel.
pub fn build_identity_window(
    pool: &mut FramePool,
    user_code: core::ops::Range<u64>,
    user_rodata: core::ops::Range<u64>,
) -> Result<IdentityWindow, MemoryError> {
    if user_code.start % PAGE != 0
        || user_code.end % PAGE != 0
        || user_rodata.start % PAGE != 0
        || user_rodata.end % PAGE != 0
    {
        return Err(MemoryError::Misaligned);
    }

    // Device memory: the whole first gibibyte, so the UART and the interrupt
    // controller are reachable without a probe. Nothing here is executable and
    // nothing here is reachable from EL0.
    let device = pool.allocate()?;
    for block in 0..512_u64 {
        // Safety: freshly zeroed, identity mapped, index below 512.
        unsafe {
            write_entry(
                device,
                block as usize,
                (DEVICE_BASE + block * BLOCK) | DEVICE_BLOCK,
            )
        };
    }

    // RAM, as supervisor blocks, with the kernel image's block split into pages.
    let ram = pool.allocate()?;
    let image = pool.allocate()?;
    for page in 0..512_u64 {
        let address = RAM_BASE + page * PAGE;
        let bits = if user_code.contains(&address) {
            // EL0 program text: readable and executable there, never writable.
            leaf_bits(Access::UserCode)
        } else if user_rodata.contains(&address) {
            leaf_bits(Access::UserReadOnly)
        } else {
            // Kernel text, rodata, data, bss and the supervisor stack. It is
            // mapped writable and EL1-executable together only because the
            // boot-time layout does not separate them; EL0 reaches none of it.
            KERNEL_TEXT
        };
        unsafe { write_entry(image, page as usize, address | bits) };
    }
    unsafe { write_entry(ram, 0, image | VALID | TABLE_OR_PAGE) };
    let mut block = BLOCK;
    while block < RAM_BYTES {
        let index = table_index(RAM_BASE + block, 1);
        unsafe { write_entry(ram, index, (RAM_BASE + block) | KERNEL_BLOCK) };
        block += BLOCK;
    }

    Ok(IdentityWindow { device, ram })
}

/// Install the translation registers and turn the MMU on.
///
/// # Safety
/// `root` must map every address the kernel is currently executing from and
/// using as a stack, or the instruction after this never retires.
pub unsafe fn enable_translation(root: u64) {
    unsafe {
        hal::write_mair(MAIR);
        hal::write_tcr(TCR);
        hal::write_ttbr0(root);
        hal::enable_mmu();
    }
}
