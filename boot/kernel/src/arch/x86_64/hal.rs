//! The complete set of privileged x86-64 operations the research kernel uses.
//!
//! Every `unsafe` block in the kernel that touches the machine rather than
//! memory lives here, so the privileged surface can be read in one sitting and
//! reviewed as a whole. Nothing in this module makes a policy decision.

use core::arch::asm;

/// Write a byte to an I/O port.
///
/// # Safety
/// The caller must know that writing `value` to `port` is meaningful and not
/// destructive on this platform.
#[inline]
pub unsafe fn out8(port: u16, value: u8) {
    unsafe { asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack)) };
}

/// Write a word to an I/O port.
///
/// # Safety
/// See [`out8`].
#[inline]
#[cfg(feature = "isolated-repl")]
pub unsafe fn out16(port: u16, value: u16) {
    unsafe { asm!("out dx, ax", in("dx") port, in("ax") value, options(nomem, nostack)) };
}

/// Write a doubleword to an I/O port.
///
/// # Safety
/// See [`out8`].
#[inline]
pub unsafe fn out32(port: u16, value: u32) {
    unsafe { asm!("out dx, eax", in("dx") port, in("eax") value, options(nomem, nostack)) };
}

/// Read a byte from an I/O port.
///
/// # Safety
/// See [`out8`]. Reading some ports has side effects.
#[inline]
pub unsafe fn in8(port: u16) -> u8 {
    let value: u8;
    unsafe { asm!("in al, dx", in("dx") port, out("al") value, options(nomem, nostack)) };
    value
}

/// Read a word from an I/O port.
///
/// # Safety
/// See [`out8`]. Reading some ports has side effects.
#[inline]
#[cfg(feature = "isolated-repl")]
pub unsafe fn in16(port: u16) -> u16 {
    let value: u16;
    unsafe { asm!("in ax, dx", in("dx") port, out("ax") value, options(nomem, nostack)) };
    value
}

/// Stop this processor permanently with interrupts masked.
pub fn halt() -> ! {
    loop {
        unsafe { asm!("cli; hlt", options(nomem, nostack)) };
    }
}

/// Address-space, descriptor-table, and interrupt control.
///
/// Only the isolation backend performs these; the BIOS workshop seed reaches
/// nothing more privileged than a serial port. Keeping them behind one gate
/// makes the privileged surface of each build a fact the compiler enforces
/// rather than a claim in a comment.
#[cfg(feature = "isolation-selftest")]
pub use privileged::*;

#[cfg(feature = "isolation-selftest")]
mod privileged {
    use core::arch::asm;

    /// Read a model-specific register.
    ///
    /// # Safety
    /// `register` must be implemented by this CPU.
    #[inline]
    pub unsafe fn read_msr(register: u32) -> u64 {
        let (high, low): (u32, u32);
        unsafe {
            asm!("rdmsr", in("ecx") register, out("edx") high, out("eax") low, options(nomem, nostack))
        };
        (u64::from(high) << 32) | u64::from(low)
    }

    /// Write a model-specific register.
    ///
    /// # Safety
    /// `register` must be implemented by this CPU and `value` must be a legal
    /// contents for it; several MSRs can make the machine unrecoverable.
    #[inline]
    pub unsafe fn write_msr(register: u32, value: u64) {
        unsafe {
            asm!(
                "wrmsr",
                in("ecx") register,
                in("edx") (value >> 32) as u32,
                in("eax") value as u32,
                options(nomem, nostack),
            )
        };
    }

    /// Install a page-table root.
    ///
    /// # Safety
    /// `physical_root` must be the physical address of a correctly formed PML4 that
    /// maps every address the kernel is about to execute from and touch.
    #[inline]
    pub unsafe fn write_cr3(physical_root: u64) {
        unsafe { asm!("mov cr3, {}", in(reg) physical_root, options(nostack, preserves_flags)) };
    }

    /// Read the faulting address recorded by a page fault.
    #[inline]
    pub fn read_cr2() -> u64 {
        let value: u64;
        unsafe { asm!("mov {}, cr2", out(reg) value, options(nomem, nostack, preserves_flags)) };
        value
    }

    /// Load the global descriptor table register.
    ///
    /// # Safety
    /// `descriptor` must point at a valid GDT pseudo-descriptor whose table stays
    /// alive for as long as it is loaded.
    #[inline]
    pub unsafe fn load_gdt(descriptor: &Pseudo) {
        unsafe {
            asm!("lgdt [{}]", in(reg) descriptor, options(readonly, nostack, preserves_flags))
        };
    }

    /// Load the interrupt descriptor table register.
    ///
    /// # Safety
    /// See [`load_gdt`].
    #[inline]
    pub unsafe fn load_idt(descriptor: &Pseudo) {
        unsafe {
            asm!("lidt [{}]", in(reg) descriptor, options(readonly, nostack, preserves_flags))
        };
    }

    /// Load the task register with a TSS selector.
    ///
    /// # Safety
    /// `selector` must name a present 64-bit TSS descriptor in the current GDT.
    #[inline]
    pub unsafe fn load_task_register(selector: u16) {
        unsafe { asm!("ltr {0:x}", in(reg) selector, options(nostack, preserves_flags)) };
    }

    /// Reload the segment registers after installing a new GDT.
    ///
    /// # Safety
    /// `code` and `data` must be present descriptors of the right type in the
    /// current GDT.
    #[inline]
    pub unsafe fn reload_segments(code: u16, data: u16) {
        unsafe {
            asm!(
                // A far return is the portable way to reload CS in long mode.
                "push {code}",
                "lea {scratch}, [rip + 2f]",
                "push {scratch}",
                "retfq",
                "2:",
                "mov ds, {data:x}",
                "mov es, {data:x}",
                "mov ss, {data:x}",
                "mov fs, {data:x}",
                "mov gs, {data:x}",
                code = in(reg) u64::from(code),
                data = in(reg) data,
                // `out`, never `lateout`: a late output may share a register with
                // an input, and `data` is still needed after the far return. That
                // aliasing loads the data segments with the address of the label
                // instead of the selector, which is a general-protection fault one
                // instruction later.
                scratch = out(reg) _,
                options(preserves_flags),
            )
        };
    }

    /// Mask interrupts.
    ///
    /// There is no matching enable: this kernel runs ring 0 with interrupts masked
    /// throughout and only ever unmasks them by entering ring 3 with a frame whose
    /// flags set `IF`. A supervisor that cannot be interrupted cannot be
    /// interrupted halfway through deciding what to do about a world.
    #[inline]
    pub fn disable_interrupts() {
        unsafe { asm!("cli", options(nomem, nostack)) };
    }

    /// The pseudo-descriptor `lgdt` and `lidt` consume.
    #[repr(C, packed)]
    pub struct Pseudo {
        /// Table size in bytes, minus one.
        pub limit: u16,
        /// Linear base address of the table.
        pub base: u64,
    }

    /// `IA32_EFER`.
    pub const MSR_EFER: u32 = 0xc000_0080;
    /// `IA32_EFER.NXE`: enables the no-execute page-table bit.
    pub const EFER_NXE: u64 = 1 << 11;
}
