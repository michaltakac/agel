//! The unprivileged world program, shared by every architecture.
//!
//! Everything here executes at the lowest privilege level the machine offers:
//! ring 3 on x86-64, EL0 on AArch64, U-mode on RISC-V. It is written to be
//! self-contained — volatile word loads and stores, no slices, no library
//! calls — because it is linked into `.user_text`, and `.user_text` is the only
//! range of the kernel image the page tables mark user-executable. A call out
//! of this section would land in supervisor-only memory and fault, so the
//! isolation test rejects any image whose `.user_text` contains one.
//!
//! The program has no capability to the console, the timer, the frame pool, or
//! any other world. Its entire vocabulary is one trap instruction and one page
//! it shares with its supervisor.

use crate::contract::SUPERVISOR_ENDPOINT;
use crate::world::shared;
use agel_kernel_abi::Operation;
use core::arch::asm;

/// Invoke the kernel contract.
///
/// Each architecture uses a spare register for the operation code and its
/// ordinary argument registers for the capability and the four bounded words,
/// so nothing about the call needs memory the kernel would have to validate.
///
/// # Safety
/// Traps into the kernel, which validates everything.
#[inline(always)]
unsafe fn contract_call(operation: u16, capability: u64, arguments: [u64; 4]) -> (u64, [u64; 4]) {
    let status: u64;
    let value0: u64;
    let value1: u64;
    let value2: u64;
    let value3: u64;

    #[cfg(target_arch = "x86_64")]
    // `rbx` and `rbp` are absent because Rust's inline assembler reserves them.
    unsafe {
        asm!(
            "int 0x80",
            inout("rax") u64::from(operation) => status,
            inout("rdi") capability => value0,
            inout("rsi") arguments[0] => value1,
            inout("rdx") arguments[1] => value2,
            inout("r10") arguments[2] => value3,
            in("r8") arguments[3],
            options(nostack),
        )
    };

    #[cfg(target_arch = "aarch64")]
    unsafe {
        asm!(
            "svc #0",
            in("x8") u64::from(operation),
            inout("x0") capability => status,
            inout("x1") arguments[0] => value0,
            inout("x2") arguments[1] => value1,
            inout("x3") arguments[2] => value2,
            inout("x4") arguments[3] => value3,
            options(nostack),
        )
    };

    #[cfg(target_arch = "riscv64")]
    unsafe {
        asm!(
            "ecall",
            in("a7") u64::from(operation),
            inout("a0") capability => status,
            inout("a1") arguments[0] => value0,
            inout("a2") arguments[1] => value1,
            inout("a3") arguments[2] => value2,
            inout("a4") arguments[3] => value3,
            options(nostack),
        )
    };

    (status, [value0, value1, value2, value3])
}

/// Hand control back to the supervisor through the well-known endpoint.
///
/// # Safety
/// See [`contract_call`].
#[inline(always)]
unsafe fn yield_to_supervisor() {
    unsafe {
        contract_call(
            Operation::EndpointSend.code(),
            u64::from(SUPERVISOR_ENDPOINT),
            [0; 4],
        )
    };
}

/// Execute an instruction reserved to the supervisor.
///
/// On x86-64 masking interrupts is exactly the instruction an unprivileged
/// world must never get away with, because it would defeat preemption. The
/// other two architectures reach the same place by reading a supervisor-only
/// system register.
///
/// # Safety
/// Raises a fault, by design.
#[inline(always)]
unsafe fn execute_privileged() {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        asm!("cli", options(nomem, nostack))
    };
    // Reading the physical timer's control register is the AArch64 equivalent
    // of masking interrupts: it is the first move a world would make towards
    // disabling the preemption that contains it. `CNTKCTL_EL1` denies EL0 that
    // access, so the attempt is trapped rather than answered.
    #[cfg(target_arch = "aarch64")]
    unsafe {
        asm!("mrs {}, cntp_ctl_el0", out(reg) _, options(nomem, nostack))
    };
    #[cfg(target_arch = "riscv64")]
    unsafe {
        asm!("csrr {}, sstatus", out(reg) _, options(nomem, nostack))
    };
}

/// Execute an undefined instruction.
///
/// # Safety
/// Raises a fault, by design.
#[inline(always)]
unsafe fn execute_undefined() {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        asm!("ud2", options(nomem, nostack))
    };
    #[cfg(target_arch = "aarch64")]
    unsafe {
        asm!("udf #0", options(nomem, nostack))
    };
    #[cfg(target_arch = "riscv64")]
    unsafe {
        asm!("unimp", options(nomem, nostack))
    };
}

/// Divide by zero without letting the optimizer fold it away.
///
/// Only x86-64 raises an exception for this. RISC-V defines a result for
/// division by zero and AArch64 has no integer divide exception, so only the
/// x86-64 provocation table exercises it.
///
/// # Safety
/// Raises `#DE` on x86-64.
#[inline(always)]
#[cfg(target_arch = "x86_64")]
unsafe fn divide_by_zero() {
    unsafe {
        asm!(
            "xor rdx, rdx",
            "xor rcx, rcx",
            "div rcx",
            inout("rax") 1_u64 => _,
            out("rdx") _,
            out("rcx") _,
            options(nostack),
        )
    };
}

/// Emit one byte on the console device this domain was granted.
///
/// Only the driver domain has the device: an I/O permission bitmap entry on
/// x86-64, a mapped device page on the other two. Every other world executing
/// this same code is refused by hardware rather than by a check, which is what
/// makes the device a capability rather than a convention.
///
/// # Safety
/// Faults unless this domain was granted the console device.
#[inline(always)]
unsafe fn console_byte(byte: u8) {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        // Poll the line-status register, then write the transmit holding
        // register. Both ports are inside the eight this domain was granted.
        let mut status: u8;
        loop {
            asm!("in al, dx", in("dx") 0x3fd_u16, out("al") status, options(nomem, nostack));
            if status & 0x20 != 0 {
                break;
            }
        }
        asm!("out dx, al", in("dx") 0x3f8_u16, in("al") byte, options(nomem, nostack));
    }
    #[cfg(target_arch = "aarch64")]
    unsafe {
        let base = crate::arch::CONSOLE_DEVICE_VADDR;
        while ((base + 0x18) as *const u32).read_volatile() & (1 << 5) != 0 {}
        (base as *mut u8).write_volatile(byte);
    }
    #[cfg(target_arch = "riscv64")]
    unsafe {
        let base = crate::arch::CONSOLE_DEVICE_VADDR;
        while ((base + 5) as *const u8).read_volatile() & (1 << 5) == 0 {}
        (base as *mut u8).write_volatile(byte);
    }
}

/// One `nop`, kept opaque so an empty loop is not optimized into a trap.
#[inline(always)]
unsafe fn pause() {
    unsafe { asm!("nop", options(nomem, nostack)) };
}

/// The unprivileged entry point.
///
/// `shared_page` is the only address the supervisor tells the world about.
/// Every other address in the world's space is its own stack.
///
/// # Safety
/// Entered by the architecture's return-from-exception instruction, with a
/// valid mapped stack and shared page.
#[no_mangle]
#[link_section = ".user_text"]
pub unsafe extern "C" fn agel_world_main(shared_page: u64) -> ! {
    let page = shared_page as *mut u64;
    loop {
        // Safety: the supervisor mapped this page writable for this world, and
        // every index is a compile-time constant below 512.
        let command = unsafe { page.add(shared::COMMAND).read_volatile() };
        if command == shared::COMMAND_INVOKE {
            let operation = unsafe { page.add(shared::OPERATION).read_volatile() } as u16;
            let capability = unsafe { page.add(shared::CAPABILITY).read_volatile() };
            let arguments = [
                unsafe { page.add(shared::ARGUMENTS).read_volatile() },
                unsafe { page.add(shared::ARGUMENTS + 1).read_volatile() },
                unsafe { page.add(shared::ARGUMENTS + 2).read_volatile() },
                unsafe { page.add(shared::ARGUMENTS + 3).read_volatile() },
            ];
            let (status, values) = unsafe { contract_call(operation, capability, arguments) };
            unsafe {
                page.add(shared::STATUS).write_volatile(status);
                page.add(shared::VALUES).write_volatile(values[0]);
                page.add(shared::VALUES + 1).write_volatile(values[1]);
                page.add(shared::VALUES + 2).write_volatile(values[2]);
                page.add(shared::VALUES + 3).write_volatile(values[3]);
            }
        } else if command == shared::COMMAND_FAULT_WRITE {
            // The kernel image is mapped in this address space, without the
            // user bit. Writing to it must fault rather than corrupt the
            // supervisor that is about to judge this world.
            unsafe { (crate::arch::KERNEL_PROBE_ADDRESS as *mut u64).write_volatile(0xdead) };
        } else if command == shared::COMMAND_FAULT_PRIVILEGED {
            unsafe { execute_privileged() };
        } else if command == shared::COMMAND_FAULT_ILLEGAL {
            unsafe { execute_undefined() };
        } else if command == shared::COMMAND_WRITE_CONSOLE {
            let count = unsafe { page.add(shared::ARGUMENTS).read_volatile() } as usize;
            let payload = (shared_page as usize + crate::world::PAYLOAD_OFFSET) as *const u8;
            let mut offset = 0;
            while offset < count && offset < crate::world::PAYLOAD_BYTES {
                // Safety: the payload area is inside the page the supervisor
                // mapped writable for this domain, and the count is bounded by
                // the payload size regardless of what the supervisor wrote.
                let byte = unsafe { payload.add(offset).read_volatile() };
                unsafe { console_byte(byte) };
                offset += 1;
            }
        } else if command == shared::COMMAND_FAULT_DEVICE {
            // The same instruction the driver domain runs, in a world that was
            // never granted the device.
            unsafe { console_byte(b'!') };
        } else if command == shared::COMMAND_SPIN {
            // No trap, no memory fault, no cooperation. Only the timer can end
            // this, which is the property the test exists to demonstrate.
            loop {
                unsafe { pause() };
            }
        } else {
            #[cfg(target_arch = "x86_64")]
            if command == shared::COMMAND_FAULT_DIVIDE {
                unsafe { divide_by_zero() };
            }
        }
        unsafe { yield_to_supervisor() };
    }
}
