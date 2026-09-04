//! The ring-3 world program.
//!
//! Everything here executes unprivileged. It is written to be self-contained —
//! volatile word loads and stores, no slices, no library calls — because it is
//! linked into `.user_text`, and `.user_text` is the only range of the kernel
//! image the page tables mark user-executable. A call out of this section would
//! fault, which is a blunt but effective way to keep the ring-3 code surface
//! honest about its size.
//!
//! The program has no capability to the serial port, the timer, the frame pool,
//! or any other world. Its entire vocabulary is the trap gate at `int 0x80` and
//! one page it shares with its supervisor.

use crate::contract::SUPERVISOR_ENDPOINT;
use crate::domain::shared;
use agel_kernel_abi::Operation;
use core::arch::asm;

/// Invoke the kernel contract.
///
/// The register convention avoids `rbx` and `rbp` because Rust's inline
/// assembler reserves them, and it is the same convention the trap handler
/// reads out of the saved frame.
///
/// # Safety
/// Executes a trap into the kernel. The kernel validates everything.
#[inline(always)]
unsafe fn contract_call(operation: u16, capability: u64, arguments: [u64; 4]) -> (u64, [u64; 4]) {
    let status: u64;
    let value0: u64;
    let value1: u64;
    let value2: u64;
    let value3: u64;
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

/// The ring-3 entry point.
///
/// `shared_page` is the only address the supervisor tells the world about. Every
/// other address in the world's space is its own stack.
///
/// # Safety
/// Entered by `iretq` from the kernel with a valid mapped stack and shared page.
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
            unsafe { (0x0001_0000_u64 as *mut u64).write_volatile(0xdead) };
        } else if command == shared::COMMAND_FAULT_DIVIDE {
            unsafe { divide_by_zero() };
        } else if command == shared::COMMAND_FAULT_PRIVILEGED {
            // Masking interrupts is exactly the instruction an unprivileged
            // world must never get away with: it would defeat preemption.
            unsafe { asm!("cli", options(nomem, nostack)) };
        } else if command == shared::COMMAND_SPIN {
            // No syscall, no memory fault, no cooperation. Only the timer can
            // end this, which is the property the test exists to demonstrate.
            loop {
                unsafe { asm!("nop", options(nomem, nostack)) };
            }
        }
        unsafe { yield_to_supervisor() };
    }
}

/// Divide by zero without letting the optimizer fold it away.
///
/// # Safety
/// Raises `#DE`.
#[inline(always)]
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
