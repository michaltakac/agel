//! Descriptor tables, trap entry, and the ring transition.
//!
//! Everything in this module exists to answer one question the v1.1 kernel
//! could not: what happens when the mutable Agel world does something wrong?
//! Before this, the answer was "the machine executes it". After this, the
//! answer is a trap into a kernel the world cannot write, on a stack the world
//! cannot name, through descriptors the world cannot edit.

use super::hal::{self, Pseudo};
use core::arch::naked_asm;

/// Kernel code selector.
pub const KERNEL_CODE: u16 = 0x08;
/// Kernel data selector.
pub const KERNEL_DATA: u16 = 0x10;
/// User data selector, requested at privilege level 3.
pub const USER_DATA: u16 = 0x18 | 3;
/// User code selector, requested at privilege level 3.
pub const USER_CODE: u16 = 0x20 | 3;
/// Task-state-segment selector.
pub const TSS_SELECTOR: u16 = 0x28;

/// Vector the programmable interval timer is remapped to.
pub const VECTOR_TIMER: u64 = 0x20;
/// Vector ring 3 uses to invoke the kernel contract.
pub const VECTOR_SYSCALL: u64 = 0x80;

/// Register state saved on every trap, in the exact order the entry stub
/// pushes it. `iretq` consumes the tail, so restoring a domain and entering one
/// for the first time are the same code path with the same layout.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct TrapFrame {
    /// Callee state, pushed last and therefore lowest in memory.
    pub r15: u64,
    /// See [`TrapFrame::r15`].
    pub r14: u64,
    /// See [`TrapFrame::r15`].
    pub r13: u64,
    /// See [`TrapFrame::r15`].
    pub r12: u64,
    /// See [`TrapFrame::r15`].
    pub r11: u64,
    /// See [`TrapFrame::r15`].
    pub r10: u64,
    /// See [`TrapFrame::r15`].
    pub r9: u64,
    /// See [`TrapFrame::r15`].
    pub r8: u64,
    /// See [`TrapFrame::r15`].
    pub rbp: u64,
    /// Contract result word 3 on the way out.
    pub rdi: u64,
    /// Contract result word 2 on the way out.
    pub rsi: u64,
    /// Contract result word 1 on the way out.
    pub rdx: u64,
    /// Contract result word 0 on the way out.
    pub rcx: u64,
    /// Capability slot on the way in.
    pub rbx: u64,
    /// Operation code on the way in, status code on the way out.
    pub rax: u64,
    /// Trap vector, pushed by the entry stub.
    pub vector: u64,
    /// Hardware error code, or zero for vectors that do not push one.
    pub error: u64,
    /// Interrupted instruction pointer.
    pub rip: u64,
    /// Interrupted code selector; its low two bits are the privilege level.
    pub cs: u64,
    /// Interrupted flags.
    pub rflags: u64,
    /// Interrupted stack pointer.
    pub rsp: u64,
    /// Interrupted stack selector.
    pub ss: u64,
}

impl TrapFrame {
    /// Build the initial state of a ring-3 thread.
    ///
    /// `rflags` sets only the always-one bit and the interrupt flag: the world
    /// starts preemptible, and with I/O privilege level zero, so a port
    /// instruction faults instead of reaching a device.
    pub fn user(entry: u64, stack_top: u64, argument: u64) -> Self {
        Self {
            rdi: argument,
            rip: entry,
            cs: u64::from(USER_CODE),
            rflags: 0x202,
            rsp: stack_top,
            ss: u64::from(USER_DATA),
            ..Self::default()
        }
    }

    /// True when the trap was taken while the processor was in ring 3.
    pub fn in_user_mode(&self) -> bool {
        self.cs & 3 == 3
    }
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct TaskStateSegment {
    reserved0: u32,
    rsp: [u64; 3],
    reserved1: u64,
    ist: [u64; 7],
    reserved2: u64,
    reserved3: u16,
    iomap_base: u16,
}

impl TaskStateSegment {
    const EMPTY: Self = Self {
        reserved0: 0,
        rsp: [0; 3],
        reserved1: 0,
        ist: [0; 7],
        reserved2: 0,
        reserved3: 0,
        // Past the end of the segment: ring 3 has no I/O permission bitmap, so
        // every port instruction faults rather than reaching a device.
        iomap_base: core::mem::size_of::<Self>() as u16,
    };
}

#[repr(C, packed)]
#[derive(Clone, Copy, Default)]
struct GateDescriptor {
    offset_low: u16,
    selector: u16,
    ist: u8,
    attributes: u8,
    offset_middle: u16,
    offset_high: u32,
    reserved: u32,
}

impl GateDescriptor {
    const fn empty() -> Self {
        Self {
            offset_low: 0,
            selector: 0,
            ist: 0,
            attributes: 0,
            offset_middle: 0,
            offset_high: 0,
            reserved: 0,
        }
    }

    fn interrupt(handler: u64, ist: u8, user_callable: bool) -> Self {
        Self {
            offset_low: handler as u16,
            selector: KERNEL_CODE,
            ist,
            // Present, 64-bit interrupt gate. Ring 3 may only reach the gates
            // that are explicitly marked callable from ring 3.
            attributes: if user_callable { 0xee } else { 0x8e },
            offset_middle: (handler >> 16) as u16,
            offset_high: (handler >> 32) as u32,
            reserved: 0,
        }
    }
}

const GDT_ENTRIES: usize = 7;
const IDT_ENTRIES: usize = 256;

static mut GDT: [u64; GDT_ENTRIES] = [0; GDT_ENTRIES];
static mut TSS: TaskStateSegment = TaskStateSegment::EMPTY;
static mut IDT: [GateDescriptor; IDT_ENTRIES] = [GateDescriptor::empty(); IDT_ENTRIES];
/// Kernel stack pointer to restore when a trap decides not to resume the world.
static mut SUPERVISOR_RESUME: u64 = 0;

/// Install descriptor tables and trap entry.
///
/// `trap_stack_top` and `fault_stack_top` must be the tops of two distinct
/// kernel stacks. The second is used only by the double-fault gate, so that a
/// domain which corrupts or exhausts the primary trap stack still lands
/// somewhere the kernel can run.
///
/// # Safety
/// Must be called once, with interrupts disabled, from ring 0.
pub unsafe fn install(trap_stack_top: u64, fault_stack_top: u64) {
    let tss = &raw mut TSS;
    unsafe {
        (*tss).rsp[0] = trap_stack_top;
        (*tss).ist[0] = fault_stack_top;
    }
    let tss_base = tss as u64;
    let tss_limit = (core::mem::size_of::<TaskStateSegment>() - 1) as u64;

    let gdt = &raw mut GDT;
    unsafe {
        (*gdt)[0] = 0;
        (*gdt)[1] = 0x00af_9a00_0000_ffff; // kernel code, DPL 0, long mode
        (*gdt)[2] = 0x00cf_9200_0000_ffff; // kernel data, DPL 0
        (*gdt)[3] = 0x00cf_f200_0000_ffff; // user data, DPL 3
        (*gdt)[4] = 0x00af_fa00_0000_ffff; // user code, DPL 3, long mode
        (*gdt)[5] = (tss_limit & 0xffff)
            | ((tss_base & 0x00ff_ffff) << 16)
            | (0x89 << 40)
            | (((tss_limit >> 16) & 0xf) << 48)
            | (((tss_base >> 24) & 0xff) << 56);
        (*gdt)[6] = tss_base >> 32;
    }
    let gdt_pointer = Pseudo {
        limit: (GDT_ENTRIES * 8 - 1) as u16,
        base: gdt as u64,
    };
    unsafe {
        hal::load_gdt(&gdt_pointer);
        hal::reload_segments(KERNEL_CODE, KERNEL_DATA);
        hal::load_task_register(TSS_SELECTOR);
    }

    let idt = &raw mut IDT;
    for (vector, stub) in EXCEPTION_STUBS.iter().enumerate() {
        // The double-fault gate runs on its own stack; everything else uses the
        // TSS's ring-0 stack.
        let ist = if vector == 8 { 1 } else { 0 };
        unsafe { (*idt)[vector] = GateDescriptor::interrupt(*stub as usize as u64, ist, false) };
    }
    unsafe {
        (*idt)[VECTOR_TIMER as usize] =
            GateDescriptor::interrupt(timer_stub as usize as u64, 0, false);
        (*idt)[VECTOR_SYSCALL as usize] =
            GateDescriptor::interrupt(syscall_stub as usize as u64, 0, true);
    }
    let idt_pointer = Pseudo {
        limit: (IDT_ENTRIES * core::mem::size_of::<GateDescriptor>() - 1) as u16,
        base: idt as u64,
    };
    unsafe { hal::load_idt(&idt_pointer) };
}

/// Route the legacy interrupt controllers away from the exception vectors and
/// unmask only the timer.
///
/// Leaving IRQ 0 at its power-on vector would deliver every timer tick to the
/// double-fault handler, which is exactly the kind of collision that makes a
/// kernel look haunted.
///
/// # Safety
/// Must be called once, with interrupts disabled, after [`install`].
pub unsafe fn remap_interrupts(timer_divisor: u16) {
    unsafe {
        // Initialize both PICs and move them to vectors 0x20 and 0x28.
        hal::out8(0x20, 0x11);
        hal::out8(0xa0, 0x11);
        hal::out8(0x21, 0x20);
        hal::out8(0xa1, 0x28);
        hal::out8(0x21, 0x04);
        hal::out8(0xa1, 0x02);
        hal::out8(0x21, 0x01);
        hal::out8(0xa1, 0x01);
        // Mask everything except IRQ 0. The kernel has one time source and no
        // drivers; an unmasked line with no handler is an outage waiting.
        hal::out8(0x21, 0xfe);
        hal::out8(0xa1, 0xff);

        // Channel 0, low/high byte, rate generator.
        hal::out8(0x43, 0x34);
        hal::out8(0x40, timer_divisor as u8);
        hal::out8(0x40, (timer_divisor >> 8) as u8);
    }
}

/// Tell the interrupt controller the timer interrupt has been handled.
///
/// # Safety
/// Only correct from inside a hardware interrupt handler.
pub unsafe fn end_of_interrupt() {
    unsafe { hal::out8(0x20, 0x20) };
}

/// Enter ring 3 with the register state in `frame`, and return when a trap
/// decides the domain should not be resumed.
///
/// The frame doubles as the transient stack for `iretq`, so entering a domain
/// for the first time and resuming one after a trap execute identical code.
///
/// # Safety
/// `frame` must be a valid, kernel-owned [`TrapFrame`] describing a ring-3
/// context whose code and stack are mapped in the currently active address
/// space.
#[unsafe(naked)]
pub unsafe extern "C" fn enter_domain(frame: *mut TrapFrame) {
    naked_asm!(
        // Preserve the supervisor context so a trap can return into our caller.
        "push rbx",
        "push r12",
        "push r13",
        "push r14",
        "push r15",
        "push rbp",
        "mov [{resume}], rsp",
        // Restore the domain and drop to ring 3.
        "mov rsp, rdi",
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop r11",
        "pop r10",
        "pop r9",
        "pop r8",
        "pop rbp",
        "pop rdi",
        "pop rsi",
        "pop rdx",
        "pop rcx",
        "pop rbx",
        "pop rax",
        "add rsp, 16",
        "iretq",
        resume = sym SUPERVISOR_RESUME,
    )
}

/// Abandon the current domain and return from [`enter_domain`].
///
/// # Safety
/// Only callable from a trap taken while a domain entered through
/// [`enter_domain`] was running.
#[unsafe(naked)]
pub unsafe extern "C" fn leave_domain() -> ! {
    naked_asm!(
        "mov rsp, [{resume}]",
        "pop rbp",
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop rbx",
        "ret",
        resume = sym SUPERVISOR_RESUME,
    )
}

macro_rules! trap_stub {
    ($name:ident, $vector:literal, error) => {
        #[unsafe(naked)]
        unsafe extern "C" fn $name() {
            naked_asm!(
                concat!("push ", $vector),
                "jmp {common}",
                common = sym trap_common,
            )
        }
    };
    ($name:ident, $vector:literal) => {
        #[unsafe(naked)]
        unsafe extern "C" fn $name() {
            naked_asm!(
                "push 0",
                concat!("push ", $vector),
                "jmp {common}",
                common = sym trap_common,
            )
        }
    };
}

macro_rules! trap_stubs {
    ($($name:ident = $vector:literal $(, $error:ident)? ;)*) => {
        $( trap_stub!($name, $vector $(, $error)?); )*
        /// One entry stub per architectural exception vector, so a fault report
        /// names the vector that actually fired instead of a catch-all.
        static EXCEPTION_STUBS: [unsafe extern "C" fn(); 32] = [$($name),*];
    };
}

trap_stubs! {
    trap_00 =  0; trap_01 =  1; trap_02 =  2; trap_03 =  3;
    trap_04 =  4; trap_05 =  5; trap_06 =  6; trap_07 =  7;
    trap_08 =  8, error; trap_09 =  9;
    trap_10 = 10, error; trap_11 = 11, error;
    trap_12 = 12, error; trap_13 = 13, error; trap_14 = 14, error;
    trap_15 = 15; trap_16 = 16; trap_17 = 17, error;
    trap_18 = 18; trap_19 = 19; trap_20 = 20; trap_21 = 21, error;
    trap_22 = 22; trap_23 = 23; trap_24 = 24; trap_25 = 25;
    trap_26 = 26; trap_27 = 27; trap_28 = 28; trap_29 = 29, error;
    trap_30 = 30, error; trap_31 = 31;
}

trap_stub!(timer_stub, 0x20);
trap_stub!(syscall_stub, 0x80);

#[unsafe(naked)]
unsafe extern "C" fn trap_common() {
    naked_asm!(
        "push rax",
        "push rbx",
        "push rcx",
        "push rdx",
        "push rsi",
        "push rdi",
        "push rbp",
        "push r8",
        "push r9",
        "push r10",
        "push r11",
        "push r12",
        "push r13",
        "push r14",
        "push r15",
        "mov rdi, rsp",
        "call {dispatch}",
        // The handler returns the frame to resume, which may be a different
        // domain's saved frame rather than the one we trapped with.
        "mov rsp, rax",
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop r11",
        "pop r10",
        "pop r9",
        "pop r8",
        "pop rbp",
        "pop rdi",
        "pop rsi",
        "pop rdx",
        "pop rcx",
        "pop rbx",
        "pop rax",
        "add rsp, 16",
        "iretq",
        dispatch = sym super::domain::dispatch_trap,
    )
}
