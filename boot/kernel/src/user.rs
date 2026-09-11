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
pub(crate) unsafe fn yield_to_supervisor() {
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

// LLVM lowers slice equality and overlapping copies to these C ABI symbols.
// Supplying tiny world-local implementations keeps valid evaluator execution
// inside `.user_text`; the supervisor's compiler-builtins remain unreachable.
#[no_mangle]
#[link_section = ".user_text"]
unsafe extern "C" fn memcmp(
    left: *const core::ffi::c_void,
    right: *const core::ffi::c_void,
    count: usize,
) -> i32 {
    let left = left.cast::<u8>();
    let right = right.cast::<u8>();
    let mut offset = 0;
    while offset < count {
        let a = unsafe { left.add(offset).read() };
        let b = unsafe { right.add(offset).read() };
        if a != b {
            return i32::from(a) - i32::from(b);
        }
        offset += 1;
    }
    0
}

#[no_mangle]
#[link_section = ".user_text"]
unsafe extern "C" fn memmove(
    destination: *mut core::ffi::c_void,
    source: *const core::ffi::c_void,
    count: usize,
) -> *mut core::ffi::c_void {
    let destination_bytes = destination.cast::<u8>();
    let source_bytes = source.cast::<u8>();
    if (destination_bytes as usize) <= source_bytes as usize {
        let mut offset = 0;
        while offset < count {
            unsafe {
                destination_bytes
                    .add(offset)
                    .write(source_bytes.add(offset).read())
            };
            offset += 1;
        }
    } else {
        let mut offset = count;
        while offset > 0 {
            offset -= 1;
            unsafe {
                destination_bytes
                    .add(offset)
                    .write(source_bytes.add(offset).read())
            };
        }
    }
    destination
}

/// Put one byte in the bounded evaluator response buffer.
#[inline(always)]
unsafe fn evaluator_push(page: *mut u64, length: &mut usize, byte: u8) {
    if *length < crate::world::PAYLOAD_BYTES {
        let payload = (page as usize + crate::world::PAYLOAD_OFFSET) as *mut u8;
        unsafe { payload.add(*length).write_volatile(byte) };
        *length += 1;
    }
}

#[inline(always)]
unsafe fn evaluator_text(page: *mut u64, length: &mut usize, text: &[u8]) {
    let mut offset = 0;
    while offset < text.len() {
        unsafe { evaluator_push(page, length, text[offset]) };
        offset += 1;
    }
}

#[inline(always)]
unsafe fn evaluator_u64(page: *mut u64, length: &mut usize, mut value: u64) {
    let mut digits = [0_u8; 20];
    let mut cursor = digits.len();
    loop {
        cursor -= 1;
        digits[cursor] = b'0' + (value % 10) as u8;
        value /= 10;
        if value == 0 {
            break;
        }
    }
    unsafe { evaluator_text(page, length, &digits[cursor..]) };
}

#[inline(always)]
unsafe fn evaluator_i64(page: *mut u64, length: &mut usize, value: i64) {
    if value < 0 {
        unsafe { evaluator_push(page, length, b'-') };
    }
    unsafe { evaluator_u64(page, length, value.unsigned_abs()) };
}

#[inline(always)]
unsafe fn evaluator_finish(page: *mut u64, response_length: usize, error: bool, revision: u64) {
    unsafe {
        page.add(shared::STATUS).write_volatile(u64::from(error));
        page.add(shared::VALUES)
            .write_volatile(response_length as u64);
        page.add(shared::VALUES + 1).write_volatile(revision);
    }
}

/// The fixed-memory Agel evaluator as an unprivileged, persistent world.
///
/// Its `Session` lives on this domain's private stack and therefore survives
/// cooperative entries while remaining absent from supervisor memory. Source
/// and results cross through the one bounded shared page. The domain has no
/// device mapping and no console-port grant.
///
/// # Safety
/// Entered by the architecture's return-from-exception instruction with a
/// private stack and a valid shared page.
#[no_mangle]
#[link_section = ".user_text"]
pub unsafe extern "C" fn agel_evaluator_main(shared_page: u64) -> ! {
    let page = shared_page as *mut u64;
    let mut session = crate::native::Session::new();
    loop {
        let command = unsafe { page.add(shared::COMMAND).read_volatile() };
        if command == shared::COMMAND_EVALUATE {
            let requested = unsafe { page.add(shared::ARGUMENTS).read_volatile() } as usize;
            let source_length = requested.min(crate::world::PAYLOAD_BYTES);
            let source_pointer = (shared_page as usize + crate::world::PAYLOAD_OFFSET) as *const u8;
            // Safety: the source is bounded to the shared page.
            let source = unsafe { core::slice::from_raw_parts(source_pointer, source_length) };
            let result = session.evaluate(source);
            let mut response_length = 0;
            match result {
                Ok(crate::native::Value::Int(value)) => unsafe {
                    evaluator_i64(page, &mut response_length, value)
                },
                Ok(crate::native::Value::Bool(true)) => unsafe {
                    evaluator_text(page, &mut response_length, b"#t")
                },
                Ok(crate::native::Value::Bool(false)) => unsafe {
                    evaluator_text(page, &mut response_length, b"#f")
                },
                Ok(crate::native::Value::Nil) => unsafe {
                    evaluator_text(page, &mut response_length, b"nil")
                },
                Ok(crate::native::Value::Agent(id)) => unsafe {
                    let (number, generation) = crate::native::agent_label(id);
                    evaluator_text(page, &mut response_length, b"#<native-agent:");
                    evaluator_u64(page, &mut response_length, u64::from(number));
                    if generation != 0 {
                        evaluator_push(page, &mut response_length, b'.');
                        evaluator_u64(page, &mut response_length, u64::from(generation));
                    }
                    evaluator_push(page, &mut response_length, b'>');
                },
                Ok(crate::native::Value::Data) => unsafe {
                    evaluator_text(page, &mut response_length, session.result())
                },
                Ok(crate::native::Value::Function) => unsafe {
                    evaluator_text(page, &mut response_length, b"#<native-function>")
                },
                Err(error) => unsafe {
                    evaluator_text(page, &mut response_length, b"error: ");
                    evaluator_text(page, &mut response_length, error.0.as_bytes());
                },
            }
            unsafe { evaluator_finish(page, response_length, result.is_err(), session.revision()) };
        } else if command == shared::COMMAND_EVALUATOR_PREVIEW
            || command == shared::COMMAND_EVALUATOR_PROMOTE
            || command == shared::COMMAND_EVALUATOR_DISCARD
            || command == shared::COMMAND_EVALUATOR_SOURCE
            || command == shared::COMMAND_EVALUATOR_REBUILD
            || command == shared::COMMAND_EVALUATOR_STAGE
        {
            let length = (unsafe { page.add(shared::ARGUMENTS).read_volatile() } as usize)
                .min(crate::world::PAYLOAD_BYTES);
            let payload = (shared_page as usize + crate::world::PAYLOAD_OFFSET) as *const u8;
            let source = unsafe { core::slice::from_raw_parts(payload, length) };
            let result = match command {
                shared::COMMAND_EVALUATOR_REBUILD => {
                    session.begin_rebuild();
                    Ok(b"SOURCE CANDIDATE STARTED".as_slice())
                }
                shared::COMMAND_EVALUATOR_STAGE => session
                    .stage(source)
                    .map(|_| b"SOURCE CELL VALIDATED".as_slice()),
                shared::COMMAND_EVALUATOR_PREVIEW => session
                    .preview(source)
                    .map(|_| b"CANDIDATE VALIDATED - :PROMOTE".as_slice()),
                shared::COMMAND_EVALUATOR_PROMOTE => {
                    session.promote().map(|_| b"CANDIDATE PROMOTED".as_slice())
                }
                shared::COMMAND_EVALUATOR_SOURCE => {
                    session.agent_source(source.first().copied().unwrap_or(0))
                }
                _ => {
                    session.discard();
                    Ok(b"CANDIDATE DISCARDED".as_slice())
                }
            };
            let mut response_length = 0;
            match result {
                Ok(text) => unsafe { evaluator_text(page, &mut response_length, text) },
                Err(error) => unsafe {
                    evaluator_text(page, &mut response_length, b"error: ");
                    evaluator_text(page, &mut response_length, error.0.as_bytes());
                },
            }
            unsafe { evaluator_finish(page, response_length, result.is_err(), session.revision()) };
        } else if command == shared::COMMAND_EVALUATOR_ROLLBACK {
            let result = session.rollback();
            let mut response_length = 0;
            match result {
                Ok(()) => unsafe {
                    evaluator_text(
                        page,
                        &mut response_length,
                        b"rolled back one committed native world",
                    )
                },
                Err(error) => unsafe {
                    evaluator_text(page, &mut response_length, b"error: ");
                    evaluator_text(page, &mut response_length, error.0.as_bytes());
                },
            }
            unsafe { evaluator_finish(page, response_length, result.is_err(), session.revision()) };
        } else if command == shared::COMMAND_EVALUATOR_DEFS {
            let mut response_length = 0;
            unsafe { evaluator_text(page, &mut response_length, b"definitions (") };
            unsafe { evaluator_u64(page, &mut response_length, session.binding_count() as u64) };
            unsafe { evaluator_text(page, &mut response_length, b"): ") };
            let mut index = 0;
            while index < session.binding_count() {
                if index > 0 {
                    unsafe { evaluator_push(page, &mut response_length, b' ') };
                }
                if let Some(name) = session.binding_name(index) {
                    unsafe { evaluator_text(page, &mut response_length, name) };
                }
                index += 1;
            }
            unsafe { evaluator_finish(page, response_length, false, session.revision()) };
        } else if command == shared::COMMAND_EVALUATOR_LIMITS {
            let mut response_length = 0;
            unsafe { evaluator_text(page, &mut response_length, b"source=") };
            unsafe {
                evaluator_u64(
                    page,
                    &mut response_length,
                    crate::world::PAYLOAD_BYTES as u64,
                )
            };
            for (name, bound) in crate::native::LIMITS {
                unsafe { evaluator_push(page, &mut response_length, b' ') };
                unsafe { evaluator_text(page, &mut response_length, name.as_bytes()) };
                unsafe { evaluator_push(page, &mut response_length, b'=') };
                unsafe { evaluator_u64(page, &mut response_length, *bound) };
            }
            unsafe { evaluator_finish(page, response_length, false, session.revision()) };
        } else if command == shared::COMMAND_EVALUATOR_SCENE {
            let payload = (shared_page as usize + crate::world::PAYLOAD_OFFSET) as *mut u8;
            let index = unsafe { payload.read_volatile() } as usize;
            let mut response_length = 0;
            let scene = if index & 128 != 0 {
                session.candidate_scene(index & 127)
            } else {
                Some((session.scene_count(), session.scene_record(index)))
            };
            let (count, record) = scene.unwrap_or((0, None));
            unsafe { evaluator_push(page, &mut response_length, count as u8) };
            if let Some(record) = record {
                for word in record {
                    for byte in word.to_le_bytes() {
                        unsafe { evaluator_push(page, &mut response_length, byte) };
                    }
                }
            }
            unsafe { evaluator_finish(page, response_length, scene.is_none(), session.revision()) };
        } else if command == shared::COMMAND_EVALUATOR_RESET {
            session.reset();
            unsafe { evaluator_finish(page, 0, false, session.revision()) };
        }
        unsafe { yield_to_supervisor() };
    }
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
        } else if cfg!(feature = "contract-memory") && command == touch_window_command() {
            // Touch a page of the frame window. Whether this returns or
            // faults is what the memory group's mappings are worth.
            let index = unsafe { page.add(shared::ARGUMENTS).read_volatile() };
            let value = unsafe { page.add(shared::ARGUMENTS + 1).read_volatile() };
            let write = unsafe { page.add(shared::ARGUMENTS + 2).read_volatile() };
            let address = (frame_window_base() + (index % 8) * 4096) as *mut u64;
            if write != 0 {
                unsafe { address.write_volatile(value) };
            }
            let seen = unsafe { address.read_volatile() };
            unsafe { page.add(shared::VALUES).write_volatile(seen) };
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
        } else if command == shared::COMMAND_READ_CONSOLE {
            // Nonblocking: the supervisor polls, so a driver entry never waits
            // on a human and its tick budget still means something.
            let (available, byte) = unsafe { console_try_byte() };
            unsafe {
                page.add(shared::STATUS)
                    .write_volatile(u64::from(available));
                page.add(shared::VALUES).write_volatile(u64::from(byte));
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
            #[cfg(target_arch = "x86_64")]
            if command == shared::COMMAND_FAULT_STORAGE_DEVICE {
                // The same status read the storage driver performs, in a world
                // that was never granted the disk.
                let _ = unsafe { port_in8(0x1f7) };
            }
            #[cfg(not(target_arch = "x86_64"))]
            if command == shared::COMMAND_FAULT_STORAGE_DEVICE {
                // The same register read the storage driver performs, in a
                // world that was never granted the device window.
                let _ =
                    unsafe { (crate::arch::STORAGE_DEVICE_VADDR as *const u32).read_volatile() };
            }
            #[cfg(target_arch = "x86_64")]
            if command == shared::COMMAND_FAULT_INPUT_DEVICE {
                // The same status read the input driver performs, in a world
                // that was never granted the keyboard controller.
                let _ = unsafe { port_in8(0x64) };
            }
        }
        unsafe { yield_to_supervisor() };
    }
}

/// The touch-window command, or an unreachable code where the frame window
/// is not built.
#[inline(always)]
fn touch_window_command() -> u64 {
    #[cfg(feature = "contract-memory")]
    {
        shared::COMMAND_TOUCH_WINDOW
    }
    #[cfg(not(feature = "contract-memory"))]
    {
        u64::MAX
    }
}

#[inline(always)]
fn frame_window_base() -> u64 {
    #[cfg(feature = "contract-memory")]
    {
        crate::arch::FRAME_WINDOW_BASE
    }
    #[cfg(not(feature = "contract-memory"))]
    {
        0
    }
}

/// Read one byte from the console device if one is waiting.
///
/// # Safety
/// Faults unless this domain was granted the console device.
#[inline(always)]
unsafe fn console_try_byte() -> (bool, u8) {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let mut status: u8;
        asm!("in al, dx", in("dx") 0x3fd_u16, out("al") status, options(nomem, nostack));
        if status & 1 == 0 {
            return (false, 0);
        }
        let byte: u8;
        asm!("in al, dx", in("dx") 0x3f8_u16, out("al") byte, options(nomem, nostack));
        (true, byte)
    }
    #[cfg(target_arch = "aarch64")]
    unsafe {
        let base = crate::arch::CONSOLE_DEVICE_VADDR;
        if ((base + 0x18) as *const u32).read_volatile() & (1 << 4) != 0 {
            return (false, 0);
        }
        (true, (base as *const u32).read_volatile() as u8)
    }
    #[cfg(target_arch = "riscv64")]
    unsafe {
        let base = crate::arch::CONSOLE_DEVICE_VADDR;
        if ((base + 5) as *const u8).read_volatile() & 1 == 0 {
            return (false, 0);
        }
        (true, (base as *const u8).read_volatile())
    }
}

/// The keyboard and pointer driver: the one domain granted the 8042 ports.
///
/// It reports raw bytes with their origin flag and performs the pointer
/// enable handshake on request. Scan-code decoding, packet assembly and every
/// policy about what a key means stay in the supervisor.
///
/// # Safety
/// Entered by the architecture's return-from-exception instruction with a
/// private stack, a valid shared page, and the 8042 ports granted.
#[cfg(all(target_arch = "x86_64", feature = "native-graphics"))]
#[no_mangle]
#[link_section = ".user_text"]
pub unsafe extern "C" fn agel_input_main(shared_page: u64) -> ! {
    let page = shared_page as *mut u64;
    loop {
        let command = unsafe { page.add(shared::COMMAND).read_volatile() };
        if command == shared::COMMAND_READ_INPUT {
            let status = unsafe { port_in8(0x64) };
            if status & 1 == 0 {
                unsafe { page.add(shared::STATUS).write_volatile(0) };
            } else {
                let byte = unsafe { port_in8(0x60) };
                unsafe {
                    page.add(shared::STATUS).write_volatile(1);
                    page.add(shared::VALUES).write_volatile(u64::from(byte));
                    page.add(shared::VALUES + 1)
                        .write_volatile(u64::from(status & 0x20 != 0));
                }
            }
        } else if command == shared::COMMAND_ENABLE_POINTER {
            let enabled = unsafe { enable_pointer() };
            unsafe { page.add(shared::STATUS).write_volatile(u64::from(enabled)) };
        } else if command == shared::COMMAND_FAULT_WRITE {
            unsafe { (crate::arch::KERNEL_PROBE_ADDRESS as *mut u64).write_volatile(0xdead) };
        } else {
            unsafe { page.add(shared::STATUS).write_volatile(0) };
        }
        unsafe { yield_to_supervisor() };
    }
}

/// Write a byte to the 8042 once its input buffer is empty, within a bound.
#[cfg(all(target_arch = "x86_64", feature = "native-graphics"))]
#[inline(always)]
unsafe fn controller_write(port: u16, byte: u8) -> bool {
    let mut polls = 0;
    while polls < 100_000 {
        if unsafe { port_in8(0x64) } & 2 == 0 {
            unsafe { port_out8(port, byte) };
            return true;
        }
        polls += 1;
    }
    false
}

/// Enable the auxiliary device and ask it to stream packets, with bounded
/// waits so a missing pointer leaves the keyboard usable.
#[cfg(all(target_arch = "x86_64", feature = "native-graphics"))]
#[inline(always)]
unsafe fn enable_pointer() -> bool {
    if !unsafe { controller_write(0x64, 0xa8) } {
        return false;
    }
    let mut step = 0;
    while step < 2 {
        let command = if step == 0 { 0xf6 } else { 0xf4 };
        if !unsafe { controller_write(0x64, 0xd4) } || !unsafe { controller_write(0x60, command) } {
            return false;
        }
        let mut acknowledged = false;
        let mut polls = 0;
        while polls < 100_000 {
            let status = unsafe { port_in8(0x64) };
            if status & 1 != 0 {
                let byte = unsafe { port_in8(0x60) };
                if status & 0x20 != 0 {
                    acknowledged = byte == 0xfa;
                    break;
                }
            }
            polls += 1;
        }
        if !acknowledged {
            return false;
        }
        step += 1;
    }
    true
}

/// One byte in from a port. Faults unless this domain was granted the port.
#[cfg(target_arch = "x86_64")]
#[inline(always)]
unsafe fn port_in8(port: u16) -> u8 {
    let value: u8;
    unsafe { asm!("in al, dx", in("dx") port, out("al") value, options(nomem, nostack)) };
    value
}

#[cfg(target_arch = "x86_64")]
#[inline(always)]
unsafe fn port_out8(port: u16, value: u8) {
    unsafe { asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack)) };
}

#[cfg(target_arch = "x86_64")]
#[inline(always)]
unsafe fn port_in16(port: u16) -> u16 {
    let value: u16;
    unsafe { asm!("in ax, dx", in("dx") port, out("ax") value, options(nomem, nostack)) };
    value
}

#[cfg(target_arch = "x86_64")]
#[inline(always)]
unsafe fn port_out16(port: u16, value: u16) {
    unsafe { asm!("out dx, ax", in("dx") port, in("ax") value, options(nomem, nostack)) };
}

/// Outcomes the storage driver reports in the status word. The supervisor
/// turns them into the workspace's error messages; the driver never carries
/// text.
pub mod storage_status {
    pub const OK: u64 = 0;
    pub const ABSENT: u64 = 1;
    /// The ATA controller never became ready; a virtio device has no
    /// equivalent state, so only x86-64 reports it.
    #[cfg_attr(not(target_arch = "x86_64"), allow(dead_code))]
    pub const BUSY: u64 = 2;
    pub const DEVICE_ERROR: u64 = 3;
    pub const DATA_TIMEOUT: u64 = 4;
    pub const OUT_OF_RANGE: u64 = 5;
    pub const UNKNOWN_COMMAND: u64 = 6;
}

#[cfg(target_arch = "x86_64")]
const ATA_DATA: u16 = 0x1f0;
#[cfg(target_arch = "x86_64")]
const ATA_SECTOR_COUNT: u16 = 0x1f2;
#[cfg(target_arch = "x86_64")]
const ATA_LBA_LOW: u16 = 0x1f3;
#[cfg(target_arch = "x86_64")]
const ATA_LBA_MID: u16 = 0x1f4;
#[cfg(target_arch = "x86_64")]
const ATA_LBA_HIGH: u16 = 0x1f5;
#[cfg(target_arch = "x86_64")]
const ATA_DRIVE: u16 = 0x1f6;
#[cfg(target_arch = "x86_64")]
const ATA_STATUS_COMMAND: u16 = 0x1f7;
#[cfg(target_arch = "x86_64")]
const ATA_ALTERNATE_STATUS: u16 = 0x3f6;
#[cfg(target_arch = "x86_64")]
const ATA_POLL_LIMIT: usize = 10_000_000;

/// Wait until a new command may be issued. A completed command can leave ERR
/// latched; only the next command clears it, so this must not treat that as a
/// permanent lockout.
#[cfg(target_arch = "x86_64")]
#[inline(always)]
unsafe fn ata_wait_ready() -> u64 {
    let mut polls = 0;
    while polls < ATA_POLL_LIMIT {
        let status = unsafe { port_in8(ATA_STATUS_COMMAND) };
        if status == 0 || status == 0xff {
            return storage_status::ABSENT;
        }
        if status & 0x80 == 0 {
            return storage_status::OK;
        }
        polls += 1;
    }
    storage_status::BUSY
}

/// Wait for the drive to be ready to transfer data, or to report an error.
#[cfg(target_arch = "x86_64")]
#[inline(always)]
unsafe fn ata_wait_data() -> u64 {
    let mut polls = 0;
    while polls < ATA_POLL_LIMIT {
        let status = unsafe { port_in8(ATA_STATUS_COMMAND) };
        if status == 0 || status == 0xff {
            return storage_status::ABSENT;
        }
        if status & 0x80 == 0 {
            if status & 0x21 != 0 {
                return storage_status::DEVICE_ERROR;
            }
            if status & 0x08 != 0 {
                return storage_status::OK;
            }
        }
        polls += 1;
    }
    storage_status::DATA_TIMEOUT
}

/// Wait for a command to finish.
#[cfg(target_arch = "x86_64")]
#[inline(always)]
unsafe fn ata_wait_finished() -> u64 {
    let mut polls = 0;
    while polls < ATA_POLL_LIMIT {
        let status = unsafe { port_in8(ATA_STATUS_COMMAND) };
        if status == 0 || status == 0xff {
            return storage_status::ABSENT;
        }
        if status & 0x80 == 0 {
            if status & 0x21 != 0 {
                return storage_status::DEVICE_ERROR;
            }
            return storage_status::OK;
        }
        polls += 1;
    }
    storage_status::BUSY
}

/// Select one LBA28 sector and issue `command`.
#[cfg(target_arch = "x86_64")]
#[inline(always)]
unsafe fn ata_select(lba: u64, command: u8) -> u64 {
    if lba >= 1 << 28 {
        return storage_status::OUT_OF_RANGE;
    }
    let ready = unsafe { ata_wait_ready() };
    if ready != storage_status::OK {
        return ready;
    }
    unsafe {
        port_out8(ATA_DRIVE, 0xe0 | ((lba >> 24) as u8 & 0x0f));
        // ATA requires 400 ns after selecting a drive; four alternate-status
        // reads provide it without acknowledging anything.
        let _ = port_in8(ATA_ALTERNATE_STATUS);
        let _ = port_in8(ATA_ALTERNATE_STATUS);
        let _ = port_in8(ATA_ALTERNATE_STATUS);
        let _ = port_in8(ATA_ALTERNATE_STATUS);
        port_out8(ATA_SECTOR_COUNT, 1);
        port_out8(ATA_LBA_LOW, lba as u8);
        port_out8(ATA_LBA_MID, (lba >> 8) as u8);
        port_out8(ATA_LBA_HIGH, (lba >> 16) as u8);
        port_out8(ATA_STATUS_COMMAND, command);
    }
    storage_status::OK
}

/// The storage driver: the one domain granted the primary ATA controller.
///
/// It moves single sectors between the disk and the block area of its shared
/// page on the supervisor's request. It holds no policy: which sectors are
/// workspace slots, what a header means, and when to publish a generation are
/// the supervisor's decisions, made on bytes this domain merely carried.
///
/// # Safety
/// Entered by the architecture's return-from-exception instruction with a
/// private stack, a valid shared page, and the disk ports granted.
#[cfg(target_arch = "x86_64")]
#[no_mangle]
#[link_section = ".user_text"]
pub unsafe extern "C" fn agel_storage_main(shared_page: u64) -> ! {
    let page = shared_page as *mut u64;
    let block = (shared_page as usize + crate::world::BLOCK_OFFSET) as *mut u8;
    loop {
        let command = unsafe { page.add(shared::COMMAND).read_volatile() };
        let lba = unsafe { page.add(shared::ARGUMENTS).read_volatile() };
        let status = if command == shared::COMMAND_READ_SECTOR {
            let selected = unsafe { ata_select(lba, 0x20) };
            if selected != storage_status::OK {
                selected
            } else {
                let ready = unsafe { ata_wait_data() };
                if ready != storage_status::OK {
                    ready
                } else {
                    let mut offset = 0;
                    while offset < crate::world::BLOCK_BYTES {
                        let word = unsafe { port_in16(ATA_DATA) };
                        unsafe {
                            block.add(offset).write_volatile(word as u8);
                            block.add(offset + 1).write_volatile((word >> 8) as u8);
                        }
                        offset += 2;
                    }
                    unsafe { ata_wait_finished() }
                }
            }
        } else if command == shared::COMMAND_WRITE_SECTOR {
            let selected = unsafe { ata_select(lba, 0x30) };
            if selected != storage_status::OK {
                selected
            } else {
                let ready = unsafe { ata_wait_data() };
                if ready != storage_status::OK {
                    ready
                } else {
                    let mut offset = 0;
                    while offset < crate::world::BLOCK_BYTES {
                        let low = unsafe { block.add(offset).read_volatile() };
                        let high = unsafe { block.add(offset + 1).read_volatile() };
                        unsafe { port_out16(ATA_DATA, u16::from(low) | (u16::from(high) << 8)) };
                        offset += 2;
                    }
                    unsafe { ata_wait_finished() }
                }
            }
        } else if command == shared::COMMAND_FLUSH_DISK {
            let ready = unsafe { ata_wait_ready() };
            if ready != storage_status::OK {
                ready
            } else {
                unsafe { port_out8(ATA_STATUS_COMMAND, 0xe7) };
                unsafe { ata_wait_finished() }
            }
        } else if command == shared::COMMAND_FAULT_WRITE {
            // For the restart test: a driver that misbehaves is contained like
            // any other world.
            unsafe { (crate::arch::KERNEL_PROBE_ADDRESS as *mut u64).write_volatile(0xdead) };
            storage_status::UNKNOWN_COMMAND
        } else {
            storage_status::UNKNOWN_COMMAND
        };
        unsafe { page.add(shared::STATUS).write_volatile(status) };
        unsafe { yield_to_supervisor() };
    }
}

// ---------------------------------------------------------------------------
// The storage driver on machines with memory-mapped devices: a virtio block
// device behind a modern virtio-mmio transport, driven by polling.
// ---------------------------------------------------------------------------

#[cfg(not(target_arch = "x86_64"))]
mod virtio {
    pub const MAGIC: u64 = 0x000;
    pub const VERSION: u64 = 0x004;
    pub const DEVICE_ID: u64 = 0x008;
    pub const DEVICE_FEATURES: u64 = 0x010;
    pub const DEVICE_FEATURES_SEL: u64 = 0x014;
    pub const DRIVER_FEATURES: u64 = 0x020;
    pub const DRIVER_FEATURES_SEL: u64 = 0x024;
    pub const QUEUE_SEL: u64 = 0x030;
    pub const QUEUE_NUM_MAX: u64 = 0x034;
    pub const QUEUE_NUM: u64 = 0x038;
    pub const QUEUE_READY: u64 = 0x044;
    pub const QUEUE_NOTIFY: u64 = 0x050;
    pub const INTERRUPT_STATUS: u64 = 0x060;
    pub const INTERRUPT_ACK: u64 = 0x064;
    pub const STATUS: u64 = 0x070;
    pub const QUEUE_DESC_LOW: u64 = 0x080;
    pub const QUEUE_DESC_HIGH: u64 = 0x084;
    pub const QUEUE_DRIVER_LOW: u64 = 0x090;
    pub const QUEUE_DRIVER_HIGH: u64 = 0x094;
    pub const QUEUE_DEVICE_LOW: u64 = 0x0a0;
    pub const QUEUE_DEVICE_HIGH: u64 = 0x0a4;
    /// Device configuration: the capacity in 512-byte sectors.
    pub const CONFIG_CAPACITY: u64 = 0x100;

    pub const STATUS_ACKNOWLEDGE: u32 = 1;
    pub const STATUS_DRIVER: u32 = 2;
    pub const STATUS_DRIVER_OK: u32 = 4;
    pub const STATUS_FEATURES_OK: u32 = 8;

    /// Feature bit 32: the modern (non-legacy) device semantics.
    pub const F_VERSION_1: u32 = 1 << 0;
    /// Feature bit 9: the device honours flush requests.
    pub const BLK_F_FLUSH: u32 = 1 << 9;

    pub const DESC_NEXT: u16 = 1;
    pub const DESC_WRITE: u16 = 2;

    pub const T_IN: u32 = 0;
    pub const T_OUT: u32 = 1;
    pub const T_FLUSH: u32 = 4;

    /// Queue depth: one request in flight needs three descriptors.
    pub const QUEUE_SIZE: u64 = 4;

    /// Layout of the one DMA page the driver was granted, in bytes from its
    /// start. Descriptors need 16-byte alignment, the rings 2 and 4.
    pub const DESC: u64 = 0x000;
    pub const AVAIL: u64 = 0x100;
    pub const USED: u64 = 0x200;
    pub const HEADER: u64 = 0x300;
    pub const DATA: u64 = 0x400;
    pub const STATUS_BYTE: u64 = 0x600;

    /// Polls of the used ring before a request is reported timed out. Sized
    /// to expire inside the driver's tick budget on an emulated machine, so a
    /// flush that takes a slow host disk most of a second still completes and
    /// a device that never answers is reported rather than the driver lost.
    pub const POLL_LIMIT: usize = 200_000_000;
}

#[cfg(not(target_arch = "x86_64"))]
#[inline(always)]
unsafe fn mmio_read(base: u64, offset: u64) -> u32 {
    unsafe { ((base + offset) as *const u32).read_volatile() }
}

#[cfg(not(target_arch = "x86_64"))]
#[inline(always)]
unsafe fn mmio_write(base: u64, offset: u64, value: u32) {
    unsafe { ((base + offset) as *mut u32).write_volatile(value) }
}

#[cfg(not(target_arch = "x86_64"))]
#[inline(always)]
fn fence() {
    core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
}

/// Bring the block device up: acknowledge it, negotiate the modern feature
/// bit (and flush, if offered), and give it the one queue in the DMA page.
/// Returns whether flush was negotiated.
#[cfg(not(target_arch = "x86_64"))]
#[link_section = ".user_text"]
unsafe fn virtio_initialize(mmio: u64, dma_physical: u64) -> Result<bool, u64> {
    unsafe {
        if mmio_read(mmio, virtio::MAGIC) != 0x7472_6976
            || mmio_read(mmio, virtio::VERSION) != 2
            || mmio_read(mmio, virtio::DEVICE_ID) != 2
        {
            return Err(storage_status::ABSENT);
        }
        mmio_write(mmio, virtio::STATUS, 0);
        mmio_write(mmio, virtio::STATUS, virtio::STATUS_ACKNOWLEDGE);
        mmio_write(
            mmio,
            virtio::STATUS,
            virtio::STATUS_ACKNOWLEDGE | virtio::STATUS_DRIVER,
        );
        mmio_write(mmio, virtio::DEVICE_FEATURES_SEL, 0);
        let low = mmio_read(mmio, virtio::DEVICE_FEATURES);
        mmio_write(mmio, virtio::DEVICE_FEATURES_SEL, 1);
        let high = mmio_read(mmio, virtio::DEVICE_FEATURES);
        if high & virtio::F_VERSION_1 == 0 {
            return Err(storage_status::ABSENT);
        }
        let flush = low & virtio::BLK_F_FLUSH != 0;
        mmio_write(mmio, virtio::DRIVER_FEATURES_SEL, 0);
        mmio_write(
            mmio,
            virtio::DRIVER_FEATURES,
            if flush { virtio::BLK_F_FLUSH } else { 0 },
        );
        mmio_write(mmio, virtio::DRIVER_FEATURES_SEL, 1);
        mmio_write(mmio, virtio::DRIVER_FEATURES, virtio::F_VERSION_1);
        let negotiated =
            virtio::STATUS_ACKNOWLEDGE | virtio::STATUS_DRIVER | virtio::STATUS_FEATURES_OK;
        mmio_write(mmio, virtio::STATUS, negotiated);
        if mmio_read(mmio, virtio::STATUS) & virtio::STATUS_FEATURES_OK == 0 {
            return Err(storage_status::DEVICE_ERROR);
        }
        mmio_write(mmio, virtio::QUEUE_SEL, 0);
        if u64::from(mmio_read(mmio, virtio::QUEUE_NUM_MAX)) < virtio::QUEUE_SIZE {
            return Err(storage_status::DEVICE_ERROR);
        }
        mmio_write(mmio, virtio::QUEUE_NUM, virtio::QUEUE_SIZE as u32);
        let desc = dma_physical + virtio::DESC;
        let avail = dma_physical + virtio::AVAIL;
        let used = dma_physical + virtio::USED;
        mmio_write(mmio, virtio::QUEUE_DESC_LOW, desc as u32);
        mmio_write(mmio, virtio::QUEUE_DESC_HIGH, (desc >> 32) as u32);
        mmio_write(mmio, virtio::QUEUE_DRIVER_LOW, avail as u32);
        mmio_write(mmio, virtio::QUEUE_DRIVER_HIGH, (avail >> 32) as u32);
        mmio_write(mmio, virtio::QUEUE_DEVICE_LOW, used as u32);
        mmio_write(mmio, virtio::QUEUE_DEVICE_HIGH, (used >> 32) as u32);
        mmio_write(mmio, virtio::QUEUE_READY, 1);
        mmio_write(mmio, virtio::STATUS, negotiated | virtio::STATUS_DRIVER_OK);
        Ok(flush)
    }
}

/// Write one descriptor into the DMA page.
#[cfg(not(target_arch = "x86_64"))]
#[link_section = ".user_text"]
unsafe fn descriptor(dma: u64, index: u64, address: u64, length: u32, flags: u16, next: u16) {
    let entry = (dma + virtio::DESC + index * 16) as *mut u8;
    unsafe {
        (entry as *mut u64).write_volatile(address);
        (entry.add(8) as *mut u32).write_volatile(length);
        (entry.add(12) as *mut u16).write_volatile(flags);
        (entry.add(14) as *mut u16).write_volatile(next);
    }
}

/// Submit one request already laid out in the DMA page and wait for the
/// device to retire it. `last_used` is the driver's copy of the used index.
#[cfg(not(target_arch = "x86_64"))]
#[link_section = ".user_text"]
unsafe fn virtio_submit(mmio: u64, dma: u64, last_used: &mut u16) -> u64 {
    unsafe {
        let avail = (dma + virtio::AVAIL) as *mut u16;
        let index = avail.add(1).read_volatile();
        avail
            .add(2 + usize::from(index % virtio::QUEUE_SIZE as u16))
            .write_volatile(0);
        fence();
        avail.add(1).write_volatile(index.wrapping_add(1));
        fence();
        mmio_write(mmio, virtio::QUEUE_NOTIFY, 0);
        let used = (dma + virtio::USED) as *const u16;
        let mut polls = 0;
        loop {
            fence();
            if used.add(1).read_volatile() != *last_used {
                break;
            }
            polls += 1;
            if polls > virtio::POLL_LIMIT {
                return storage_status::DATA_TIMEOUT;
            }
            pause();
        }
        *last_used = last_used.wrapping_add(1);
        let pending = mmio_read(mmio, virtio::INTERRUPT_STATUS);
        if pending != 0 {
            mmio_write(mmio, virtio::INTERRUPT_ACK, pending);
        }
        match ((dma + virtio::STATUS_BYTE) as *const u8).read_volatile() {
            0 => storage_status::OK,
            _ => storage_status::DEVICE_ERROR,
        }
    }
}

/// The storage driver: the one domain granted the virtio block device.
///
/// It moves single sectors between the disk and the block area of its shared
/// page on the supervisor's request, through a request laid out in the one
/// DMA frame it was granted. It holds no policy: which sectors are workspace
/// slots, what a header means, and when to publish a generation are the
/// supervisor's decisions, made on bytes this domain merely carried.
///
/// # Safety
/// Entered by the architecture's return-from-exception instruction with a
/// private stack, a valid shared page, the device page and the DMA page
/// mapped.
#[cfg(not(target_arch = "x86_64"))]
#[no_mangle]
#[link_section = ".user_text"]
pub unsafe extern "C" fn agel_storage_main(shared_page: u64) -> ! {
    let page = shared_page as *mut u64;
    let block = (shared_page as usize + crate::world::BLOCK_OFFSET) as *mut u8;
    let mmio = unsafe { page.add(shared::DEVICE_MMIO).read_volatile() };
    let dma_physical = unsafe { page.add(shared::DEVICE_DMA).read_volatile() };
    let dma = crate::arch::STORAGE_DMA_VADDR;
    let device = unsafe { virtio_initialize(mmio, dma_physical) };
    let capacity = unsafe {
        u64::from(mmio_read(mmio, virtio::CONFIG_CAPACITY))
            | (u64::from(mmio_read(mmio, virtio::CONFIG_CAPACITY + 4)) << 32)
    };
    let mut last_used: u16 = 0;
    loop {
        let command = unsafe { page.add(shared::COMMAND).read_volatile() };
        let lba = unsafe { page.add(shared::ARGUMENTS).read_volatile() };
        let status = match device {
            Err(reason) => reason,
            Ok(flush_supported) => {
                let transfer = command == shared::COMMAND_READ_SECTOR
                    || command == shared::COMMAND_WRITE_SECTOR;
                if transfer && lba >= capacity {
                    storage_status::OUT_OF_RANGE
                } else if transfer {
                    let reading = command == shared::COMMAND_READ_SECTOR;
                    unsafe {
                        let header = (dma + virtio::HEADER) as *mut u8;
                        (header as *mut u32).write_volatile(if reading {
                            virtio::T_IN
                        } else {
                            virtio::T_OUT
                        });
                        (header.add(4) as *mut u32).write_volatile(0);
                        (header.add(8) as *mut u64).write_volatile(lba);
                        let data = (dma + virtio::DATA) as *mut u8;
                        if !reading {
                            for offset in 0..crate::world::BLOCK_BYTES {
                                data.add(offset)
                                    .write_volatile(block.add(offset).read_volatile());
                            }
                        }
                        descriptor(
                            dma,
                            0,
                            dma_physical + virtio::HEADER,
                            16,
                            virtio::DESC_NEXT,
                            1,
                        );
                        descriptor(
                            dma,
                            1,
                            dma_physical + virtio::DATA,
                            crate::world::BLOCK_BYTES as u32,
                            virtio::DESC_NEXT | if reading { virtio::DESC_WRITE } else { 0 },
                            2,
                        );
                        descriptor(
                            dma,
                            2,
                            dma_physical + virtio::STATUS_BYTE,
                            1,
                            virtio::DESC_WRITE,
                            0,
                        );
                        let status = virtio_submit(mmio, dma, &mut last_used);
                        if reading && status == storage_status::OK {
                            for offset in 0..crate::world::BLOCK_BYTES {
                                block
                                    .add(offset)
                                    .write_volatile(data.add(offset).read_volatile());
                            }
                        }
                        status
                    }
                } else if command == shared::COMMAND_FLUSH_DISK {
                    if flush_supported {
                        unsafe {
                            let header = (dma + virtio::HEADER) as *mut u8;
                            (header as *mut u32).write_volatile(virtio::T_FLUSH);
                            (header.add(4) as *mut u32).write_volatile(0);
                            (header.add(8) as *mut u64).write_volatile(0);
                            descriptor(
                                dma,
                                0,
                                dma_physical + virtio::HEADER,
                                16,
                                virtio::DESC_NEXT,
                                2,
                            );
                            descriptor(
                                dma,
                                2,
                                dma_physical + virtio::STATUS_BYTE,
                                1,
                                virtio::DESC_WRITE,
                                0,
                            );
                            virtio_submit(mmio, dma, &mut last_used)
                        }
                    } else {
                        storage_status::OK
                    }
                } else if command == shared::COMMAND_FAULT_WRITE {
                    // For the restart test: a driver that misbehaves is
                    // contained like any other world.
                    unsafe {
                        (crate::arch::KERNEL_PROBE_ADDRESS as *mut u64).write_volatile(0xdead)
                    };
                    storage_status::UNKNOWN_COMMAND
                } else {
                    storage_status::UNKNOWN_COMMAND
                }
            }
        };
        unsafe { page.add(shared::STATUS).write_volatile(status) };
        unsafe { yield_to_supervisor() };
    }
}
