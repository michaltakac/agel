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

/// The clock driver's entry point: the CMOS real-time clock through its two
/// ports, read when the supervisor asks, decoded from BCD and twelve-hour
/// form to plain numbers. It has no other port and no other job.
///
/// # Safety
/// Entered by the architecture's return-from-exception instruction with a
/// private stack, a valid shared page, and the CMOS ports granted.
#[cfg(all(target_arch = "x86_64", feature = "native-graphics"))]
#[no_mangle]
#[link_section = ".user_text"]
pub unsafe extern "C" fn agel_clock_main(shared_page: u64) -> ! {
    let page = shared_page as *mut u64;
    loop {
        let command = unsafe { page.add(shared::COMMAND).read_volatile() };
        if command == shared::COMMAND_READ_CLOCK {
            match unsafe { read_clock() } {
                Some(packed) => unsafe {
                    page.add(shared::STATUS).write_volatile(1);
                    page.add(shared::VALUES).write_volatile(packed);
                },
                None => unsafe { page.add(shared::STATUS).write_volatile(0) },
            }
        } else if command == shared::COMMAND_FAULT_WRITE {
            unsafe { (crate::arch::KERNEL_PROBE_ADDRESS as *mut u64).write_volatile(0xdead) };
        } else {
            unsafe { page.add(shared::STATUS).write_volatile(0) };
        }
        unsafe { yield_to_supervisor() };
    }
}

#[cfg(all(target_arch = "x86_64", feature = "native-graphics"))]
#[inline(always)]
unsafe fn cmos(register: u8) -> u8 {
    unsafe {
        port_out8(0x70, 0x80 | register);
        port_in8(0x71)
    }
}

/// Seconds, minutes, hours, day, month, year as bytes from the low end, or
/// `None` when an update was in progress for too long.
#[cfg(all(target_arch = "x86_64", feature = "native-graphics"))]
#[inline(always)]
unsafe fn read_clock() -> Option<u64> {
    let mut polls = 0;
    while unsafe { cmos(0x0a) } & 0x80 != 0 {
        polls += 1;
        if polls > 100_000 {
            return None;
        }
    }
    let status = unsafe { cmos(0x0b) };
    let binary = status & 0x04 != 0;
    let twenty_four = status & 0x02 != 0;
    let decode = |value: u8| -> u8 {
        if binary {
            value
        } else {
            (value >> 4) * 10 + (value & 0x0f)
        }
    };
    let seconds = decode(unsafe { cmos(0x00) });
    let minutes = decode(unsafe { cmos(0x02) });
    let raw_hours = unsafe { cmos(0x04) };
    let mut hours = decode(raw_hours & 0x7f);
    if !twenty_four {
        if raw_hours & 0x80 != 0 {
            hours = (hours % 12) + 12;
        } else if hours == 12 {
            hours = 0;
        }
    }
    let day = decode(unsafe { cmos(0x07) });
    let month = decode(unsafe { cmos(0x08) });
    let year = decode(unsafe { cmos(0x09) });
    Some(
        u64::from(seconds)
            | (u64::from(minutes) << 8)
            | (u64::from(hours) << 16)
            | (u64::from(day) << 24)
            | (u64::from(month) << 32)
            | (u64::from(year) << 40),
    )
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

#[cfg(not(any(feature = "board-raspi4", feature = "board-raspi5")))]
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

#[cfg(all(
    not(target_arch = "x86_64"),
    not(any(feature = "board-raspi4", feature = "board-raspi5"))
))]
#[inline(always)]
fn fence() {
    core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
}

#[cfg(not(any(feature = "board-raspi4", feature = "board-raspi5")))]
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

#[cfg(not(any(feature = "board-raspi4", feature = "board-raspi5")))]
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

#[cfg(not(any(feature = "board-raspi4", feature = "board-raspi5")))]
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
#[cfg(all(
    not(target_arch = "x86_64"),
    not(any(feature = "board-raspi4", feature = "board-raspi5"))
))]
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

// ---------------------------------------------------------------------------
// The SD host controller driver: the Raspberry Pi's card, by programmed I/O.
// ---------------------------------------------------------------------------

/// Registers of an SD Host Controller (the simplified specification's
/// layout, version 2 and up), as offsets from the granted page. Every
/// access is a whole 32-bit word: the Arasan controller on the Pi takes
/// nothing narrower, so the 8- and 16-bit registers are reached through the
/// word that holds them.
#[cfg(any(feature = "board-raspi4", feature = "board-raspi5"))]
mod sdhci {
    /// Block size (low half) and block count (high half).
    pub const BLOCK: u64 = 0x04;
    pub const ARGUMENT: u64 = 0x08;
    /// Transfer mode (low half) and command (high half).
    pub const COMMAND: u64 = 0x0c;
    /// Four words of response.
    pub const RESPONSE: u64 = 0x10;
    pub const BUFFER: u64 = 0x20;
    pub const PRESENT_STATE: u64 = 0x24;
    /// Host control 1, power control, block gap and wakeup, one byte each.
    pub const HOST_CONTROL: u64 = 0x28;
    /// Clock control (low half), timeout control and software reset.
    pub const CLOCK_CONTROL: u64 = 0x2c;
    /// Normal (low half) and error (high half) interrupt status.
    pub const INTERRUPT_STATUS: u64 = 0x30;
    pub const INTERRUPT_STATUS_ENABLE: u64 = 0x34;
    pub const INTERRUPT_SIGNAL_ENABLE: u64 = 0x38;

    pub const STATE_COMMAND_INHIBIT: u32 = 1 << 0;
    pub const STATE_DATA_INHIBIT: u32 = 1 << 1;
    pub const STATE_CARD_INSERTED: u32 = 1 << 16;

    /// Software reset for all, in the byte at 0x2f.
    pub const RESET_ALL: u32 = 1 << 24;
    pub const CLOCK_INTERNAL_ENABLE: u32 = 1 << 0;
    pub const CLOCK_INTERNAL_STABLE: u32 = 1 << 1;
    pub const CLOCK_SD_ENABLE: u32 = 1 << 2;
    /// The largest data timeout, in the byte at 0x2e.
    pub const TIMEOUT_LONGEST: u32 = 0xe << 16;
    /// The clock divider field, base clock over twice this.
    pub const fn divider(value: u32) -> u32 {
        (value & 0xff) << 8
    }
    /// Bus power on at 3.3 V, in the byte at 0x29.
    pub const POWER_ON_3V3: u32 = 0x0f << 8;

    pub const INT_COMMAND_COMPLETE: u32 = 1 << 0;
    pub const INT_TRANSFER_COMPLETE: u32 = 1 << 1;
    pub const INT_BUFFER_WRITE_READY: u32 = 1 << 4;
    pub const INT_BUFFER_READ_READY: u32 = 1 << 5;
    pub const INT_ERROR: u32 = 1 << 15;

    /// Command flags, in the command half-word.
    pub const RESPONSE_NONE: u32 = 0;
    pub const RESPONSE_136: u32 = 1;
    pub const RESPONSE_48: u32 = 2;
    pub const RESPONSE_48_BUSY: u32 = 3;
    pub const CHECK_CRC: u32 = 1 << 3;
    pub const CHECK_INDEX: u32 = 1 << 4;
    pub const DATA_PRESENT: u32 = 1 << 5;
    /// Transfer mode: the card sends.
    pub const TRANSFER_READ: u32 = 1 << 4;

    pub const GO_IDLE_STATE: u32 = 0;
    pub const ALL_SEND_CID: u32 = 2;
    pub const SEND_RELATIVE_ADDR: u32 = 3;
    pub const SELECT_CARD: u32 = 7;
    pub const SEND_IF_COND: u32 = 8;
    pub const SEND_CSD: u32 = 9;
    pub const READ_SINGLE_BLOCK: u32 = 17;
    pub const WRITE_BLOCK: u32 = 24;
    pub const SD_SEND_OP_COND: u32 = 41;
    pub const APP_CMD: u32 = 55;

    /// Register polls before a step is reported as timed out: well inside
    /// the driver's tick budget on an emulated machine.
    pub const POLL_LIMIT: usize = 4_000_000;
}

/// What the card said about itself at initialization.
#[cfg(any(feature = "board-raspi4", feature = "board-raspi5"))]
#[derive(Clone, Copy)]
struct Card {
    /// In 512-byte sectors.
    capacity: u64,
    /// A high-capacity card is addressed by sector, a standard one by byte.
    high_capacity: bool,
}

/// Wait until `mask` is set in the interrupt status, clear it, and say
/// so; an error interrupt or the poll limit is the failure it names.
#[cfg(any(feature = "board-raspi4", feature = "board-raspi5"))]
#[link_section = ".user_text"]
unsafe fn sd_wait(mmio: u64, mask: u32) -> Result<(), u64> {
    let mut polls = 0;
    loop {
        let status = unsafe { mmio_read(mmio, sdhci::INTERRUPT_STATUS) };
        if status & sdhci::INT_ERROR != 0 {
            unsafe { mmio_write(mmio, sdhci::INTERRUPT_STATUS, status) };
            return Err(storage_status::DEVICE_ERROR);
        }
        if status & mask != 0 {
            unsafe { mmio_write(mmio, sdhci::INTERRUPT_STATUS, mask) };
            return Ok(());
        }
        polls += 1;
        if polls > sdhci::POLL_LIMIT {
            return Err(storage_status::DATA_TIMEOUT);
        }
    }
}

/// Issue one command and return its first response word.
#[cfg(any(feature = "board-raspi4", feature = "board-raspi5"))]
#[link_section = ".user_text"]
unsafe fn sd_command(
    mmio: u64,
    index: u32,
    argument: u32,
    flags: u32,
    transfer: u32,
) -> Result<u32, u64> {
    let mut polls = 0;
    while unsafe { mmio_read(mmio, sdhci::PRESENT_STATE) }
        & (sdhci::STATE_COMMAND_INHIBIT | sdhci::STATE_DATA_INHIBIT)
        != 0
    {
        polls += 1;
        if polls > sdhci::POLL_LIMIT {
            return Err(storage_status::BUSY);
        }
    }
    unsafe {
        mmio_write(mmio, sdhci::INTERRUPT_STATUS, 0xffff_ffff);
        mmio_write(mmio, sdhci::ARGUMENT, argument);
        mmio_write(
            mmio,
            sdhci::COMMAND,
            (index << 24) | (flags << 16) | transfer,
        );
        sd_wait(mmio, sdhci::INT_COMMAND_COMPLETE)?;
        Ok(mmio_read(mmio, sdhci::RESPONSE))
    }
}

/// Set the SD clock divider with the clock stopped, and wait for it.
#[cfg(any(feature = "board-raspi4", feature = "board-raspi5"))]
#[link_section = ".user_text"]
unsafe fn sd_clock(mmio: u64, divider: u32) -> Result<(), u64> {
    unsafe {
        mmio_write(
            mmio,
            sdhci::CLOCK_CONTROL,
            sdhci::TIMEOUT_LONGEST | sdhci::divider(divider) | sdhci::CLOCK_INTERNAL_ENABLE,
        );
    }
    let mut polls = 0;
    while unsafe { mmio_read(mmio, sdhci::CLOCK_CONTROL) } & sdhci::CLOCK_INTERNAL_STABLE == 0 {
        polls += 1;
        if polls > sdhci::POLL_LIMIT {
            return Err(storage_status::BUSY);
        }
    }
    unsafe {
        let control = mmio_read(mmio, sdhci::CLOCK_CONTROL);
        mmio_write(mmio, sdhci::CLOCK_CONTROL, control | sdhci::CLOCK_SD_ENABLE);
    }
    Ok(())
}

/// Bring the controller and the card up: reset, power, a slow clock, then
/// the identification sequence, then a faster clock and 512-byte blocks.
#[cfg(any(feature = "board-raspi4", feature = "board-raspi5"))]
#[link_section = ".user_text"]
unsafe fn sd_initialize(mmio: u64) -> Result<Card, u64> {
    unsafe { mmio_write(mmio, sdhci::CLOCK_CONTROL, sdhci::RESET_ALL) };
    let mut polls = 0;
    while unsafe { mmio_read(mmio, sdhci::CLOCK_CONTROL) } & sdhci::RESET_ALL != 0 {
        polls += 1;
        if polls > sdhci::POLL_LIMIT {
            return Err(storage_status::BUSY);
        }
    }
    unsafe {
        mmio_write(mmio, sdhci::HOST_CONTROL, sdhci::POWER_ON_3V3);
        sd_clock(mmio, 0x80)?;
        // Every status is recorded and none signals: the driver polls.
        mmio_write(mmio, sdhci::INTERRUPT_STATUS_ENABLE, 0xffff_ffff);
        mmio_write(mmio, sdhci::INTERRUPT_SIGNAL_ENABLE, 0);
        if mmio_read(mmio, sdhci::PRESENT_STATE) & sdhci::STATE_CARD_INSERTED == 0 {
            return Err(storage_status::ABSENT);
        }
        sd_command(mmio, sdhci::GO_IDLE_STATE, 0, sdhci::RESPONSE_NONE, 0)?;
        // A card that answers CMD8 speaks version 2 and may be high capacity.
        let version_2 = sd_command(
            mmio,
            sdhci::SEND_IF_COND,
            0x1aa,
            sdhci::RESPONSE_48 | sdhci::CHECK_CRC | sdhci::CHECK_INDEX,
            0,
        )
        .is_ok_and(|response| response & 0xfff == 0x1aa);
        let mut tries = 0;
        let ocr = loop {
            sd_command(
                mmio,
                sdhci::APP_CMD,
                0,
                sdhci::RESPONSE_48 | sdhci::CHECK_CRC | sdhci::CHECK_INDEX,
                0,
            )?;
            let ocr = sd_command(
                mmio,
                sdhci::SD_SEND_OP_COND,
                if version_2 { 0x40ff_8000 } else { 0x00ff_8000 },
                sdhci::RESPONSE_48,
                0,
            )?;
            if ocr & (1 << 31) != 0 {
                break ocr;
            }
            tries += 1;
            if tries > 10_000 {
                return Err(storage_status::DATA_TIMEOUT);
            }
        };
        let high_capacity = ocr & (1 << 30) != 0;
        sd_command(
            mmio,
            sdhci::ALL_SEND_CID,
            0,
            sdhci::RESPONSE_136 | sdhci::CHECK_CRC,
            0,
        )?;
        let rca = sd_command(
            mmio,
            sdhci::SEND_RELATIVE_ADDR,
            0,
            sdhci::RESPONSE_48 | sdhci::CHECK_CRC | sdhci::CHECK_INDEX,
            0,
        )? & 0xffff_0000;
        sd_command(
            mmio,
            sdhci::SEND_CSD,
            rca,
            sdhci::RESPONSE_136 | sdhci::CHECK_CRC,
            0,
        )?;
        // The response words hold the CSD less its CRC byte: word 1 is
        // CSD[71:40], word 2 CSD[103:72], word 3 CSD[127:104].
        let word1 = mmio_read(mmio, sdhci::RESPONSE + 4);
        let word2 = mmio_read(mmio, sdhci::RESPONSE + 8);
        let word3 = mmio_read(mmio, sdhci::RESPONSE + 12);
        let capacity = if word3 >> 30 == 1 {
            // Version 2: C_SIZE is CSD[69:48], in 512 KiB units.
            (u64::from((word1 >> 8) & 0x3f_ffff) + 1) * 1024
        } else {
            // Version 1: C_SIZE is CSD[73:62], C_SIZE_MULT CSD[49:47] and
            // READ_BL_LEN CSD[83:80]; bytes are (C_SIZE + 1) << (MULT + 2 + BL_LEN).
            let c_size = ((word2 & 3) << 10) | (word1 >> 22);
            let mult = (word1 >> 7) & 7;
            let block_length = (word2 >> 8) & 0xf;
            (u64::from(c_size) + 1) << (mult + 2 + block_length) >> 9
        };
        sd_command(
            mmio,
            sdhci::SELECT_CARD,
            rca,
            sdhci::RESPONSE_48_BUSY | sdhci::CHECK_CRC | sdhci::CHECK_INDEX,
            0,
        )?;
        // Base clock over eight for transfers: safe on every board.
        sd_clock(mmio, 4)?;
        mmio_write(mmio, sdhci::BLOCK, 512 | (1 << 16));
        Ok(Card {
            capacity,
            high_capacity,
        })
    }
}

/// Move one sector between the card and the block area, a word at a time
/// through the controller's buffer.
#[cfg(any(feature = "board-raspi4", feature = "board-raspi5"))]
#[link_section = ".user_text"]
unsafe fn sd_transfer(mmio: u64, card: Card, lba: u64, block: *mut u8, reading: bool) -> u64 {
    let address = if card.high_capacity {
        lba as u32
    } else {
        (lba * 512) as u32
    };
    let (index, transfer, ready) = if reading {
        (
            sdhci::READ_SINGLE_BLOCK,
            sdhci::TRANSFER_READ,
            sdhci::INT_BUFFER_READ_READY,
        )
    } else {
        (sdhci::WRITE_BLOCK, 0, sdhci::INT_BUFFER_WRITE_READY)
    };
    unsafe {
        mmio_write(mmio, sdhci::BLOCK, 512 | (1 << 16));
        if let Err(status) = sd_command(
            mmio,
            index,
            address,
            sdhci::RESPONSE_48 | sdhci::CHECK_CRC | sdhci::CHECK_INDEX | sdhci::DATA_PRESENT,
            transfer,
        ) {
            return status;
        }
        if let Err(status) = sd_wait(mmio, ready) {
            return status;
        }
        for word in 0..crate::world::BLOCK_BYTES / 4 {
            let at = block.add(word * 4);
            if reading {
                let value = mmio_read(mmio, sdhci::BUFFER);
                for (offset, byte) in value.to_le_bytes().iter().enumerate() {
                    at.add(offset).write_volatile(*byte);
                }
            } else {
                let mut bytes = [0_u8; 4];
                for (offset, byte) in bytes.iter_mut().enumerate() {
                    *byte = at.add(offset).read_volatile();
                }
                mmio_write(mmio, sdhci::BUFFER, u32::from_le_bytes(bytes));
            }
        }
        match sd_wait(mmio, sdhci::INT_TRANSFER_COMPLETE) {
            Ok(()) => storage_status::OK,
            Err(status) => status,
        }
    }
}

/// The storage driver on the Raspberry Pi: the granted SD host controller,
/// spoken to by programmed I/O, one sector per request. Writes complete
/// before the answer, so a flush is nothing to do. As on every machine,
/// the driver sees a page of registers and the block area and nothing
/// else; what the sectors mean is the supervisor's.
///
/// # Safety
/// Entered by the architecture's return-from-exception instruction with a
/// private stack, a valid shared page and the device page mapped.
#[cfg(any(feature = "board-raspi4", feature = "board-raspi5"))]
#[no_mangle]
#[link_section = ".user_text"]
pub unsafe extern "C" fn agel_storage_main(shared_page: u64) -> ! {
    let page = shared_page as *mut u64;
    let block = (shared_page as usize + crate::world::BLOCK_OFFSET) as *mut u8;
    let mmio = unsafe { page.add(shared::DEVICE_MMIO).read_volatile() };
    let card = unsafe { sd_initialize(mmio) };
    loop {
        let command = unsafe { page.add(shared::COMMAND).read_volatile() };
        let lba = unsafe { page.add(shared::ARGUMENTS).read_volatile() };
        let status = match card {
            Err(reason) => reason,
            Ok(card) => {
                let transfer = command == shared::COMMAND_READ_SECTOR
                    || command == shared::COMMAND_WRITE_SECTOR;
                if transfer && lba >= card.capacity {
                    storage_status::OUT_OF_RANGE
                } else if transfer {
                    let reading = command == shared::COMMAND_READ_SECTOR;
                    unsafe { sd_transfer(mmio, card, lba, block, reading) }
                } else if command == shared::COMMAND_FLUSH_DISK {
                    storage_status::OK
                } else if command == shared::COMMAND_FAULT_WRITE {
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

// ---------------------------------------------------------------------------
// The filesystem service: an unprivileged world that owns a region of the
// disk it cannot touch, asking the supervisor for every sector.
// ---------------------------------------------------------------------------

/// The on-disk shape: a superblock, four directory sectors, then one
/// eight-sector extent per directory entry. Entry 0 is the root directory.
#[cfg(feature = "process")]
mod agelfs {
    pub const MAGIC: &[u8; 8] = b"AGELFS1\0";
    pub const SUPERBLOCK: u64 = crate::world::fs::FIRST_SECTOR as u64;
    pub const DIRECTORY: u64 = SUPERBLOCK + 1;
    pub const DIRECTORY_SECTORS: u64 = 4;
    pub const DATA: u64 = DIRECTORY + DIRECTORY_SECTORS;
    pub const EXTENT_SECTORS: u64 = crate::world::fs::FILE_BYTES / 512;
    pub const ENTRIES: usize = crate::world::fs::ENTRIES as usize;
    pub const ENTRY_BYTES: usize = 64;
    pub const PER_SECTOR: usize = 512 / ENTRY_BYTES;
    pub const NAME_BYTES: usize = 32;
}

/// One directory entry as the service keeps it in memory.
#[cfg(feature = "process")]
#[derive(Clone, Copy)]
struct Entry {
    name: [u8; agelfs::NAME_BYTES],
    name_len: u8,
    kind: u8,
    parent: u16,
    length: u32,
}

#[cfg(feature = "process")]
impl Entry {
    const EMPTY: Self = Self {
        name: [0; agelfs::NAME_BYTES],
        name_len: 0,
        kind: 0,
        parent: 0,
        length: 0,
    };
}

/// The service's whole state: the directory, and whether it has been read.
#[cfg(feature = "process")]
struct Filesystem {
    page: *mut u64,
    entries: [Entry; agelfs::ENTRIES],
    mounted: bool,
    sector: [u8; 512],
}

#[cfg(feature = "process")]
impl Filesystem {
    /// Ask the supervisor for one sector into this world's block area, then
    /// copy it into the local buffer. Yields; the supervisor resumes here.
    #[link_section = ".user_text"]
    unsafe fn read_sector(&mut self, sector: u64) -> Result<(), u64> {
        use crate::world::fs;
        unsafe {
            self.page.add(fs::DISK_SECTOR).write_volatile(sector);
            self.page
                .add(fs::DISK_OPERATION)
                .write_volatile(fs::DISK_READ);
            yield_to_supervisor();
            let status = self.page.add(fs::DISK_STATUS).read_volatile();
            if status != 0 {
                return Err(status);
            }
            let block = (self.page as usize + crate::world::BLOCK_OFFSET) as *const u8;
            for (offset, byte) in self.sector.iter_mut().enumerate() {
                *byte = block.add(offset).read_volatile();
            }
        }
        Ok(())
    }

    #[link_section = ".user_text"]
    unsafe fn write_sector(&mut self, sector: u64) -> Result<(), u64> {
        use crate::world::fs;
        unsafe {
            let block = (self.page as usize + crate::world::BLOCK_OFFSET) as *mut u8;
            for (offset, byte) in self.sector.iter().enumerate() {
                block.add(offset).write_volatile(*byte);
            }
            self.page.add(fs::DISK_SECTOR).write_volatile(sector);
            self.page
                .add(fs::DISK_OPERATION)
                .write_volatile(fs::DISK_WRITE);
            yield_to_supervisor();
            let status = self.page.add(fs::DISK_STATUS).read_volatile();
            if status != 0 {
                return Err(status);
            }
        }
        Ok(())
    }

    /// The entry at `index`, when the index names one. Nothing in this world
    /// may panic: a panic would leave the world's text for the kernel's and
    /// be contained as a fault, so every lookup is checked instead.
    #[link_section = ".user_text"]
    fn entry(&self, index: usize) -> Result<Entry, u64> {
        self.entries
            .get(index)
            .copied()
            .ok_or(crate::world::fs::EINVAL)
    }

    /// Read the directory from disk; an unformatted region is an error the
    /// caller reports as `EIO`, not a filesystem it invents.
    #[link_section = ".user_text"]
    unsafe fn mount(&mut self) -> Result<(), u64> {
        use crate::world::fs;
        if self.mounted {
            return Ok(());
        }
        unsafe { self.read_sector(agelfs::SUPERBLOCK)? };
        if !self.sector.starts_with(agelfs::MAGIC) {
            return Err(fs::EIO);
        }
        for sector in 0..agelfs::DIRECTORY_SECTORS {
            unsafe { self.read_sector(agelfs::DIRECTORY + sector)? };
            let (rows, _) = self.sector.as_chunks::<{ agelfs::ENTRY_BYTES }>();
            let first = sector as usize * agelfs::PER_SECTOR;
            for (raw, slot) in rows.iter().zip(self.entries.iter_mut().skip(first)) {
                let mut entry = Entry::EMPTY;
                for (byte, stored) in entry.name.iter_mut().zip(raw.iter()) {
                    *byte = *stored;
                }
                entry.name_len = raw[32];
                entry.kind = raw[33];
                entry.parent = u16::from_le_bytes([raw[34], raw[35]]);
                entry.length = u32::from_le_bytes([raw[36], raw[37], raw[38], raw[39]]);
                if entry.name_len as usize > agelfs::NAME_BYTES {
                    entry.name_len = 0;
                }
                *slot = entry;
            }
        }
        self.mounted = true;
        Ok(())
    }

    /// Write the directory sector holding `index` back to disk.
    #[link_section = ".user_text"]
    unsafe fn flush_entry(&mut self, index: usize) -> Result<(), u64> {
        let sector = index / agelfs::PER_SECTOR;
        self.sector = [0; 512];
        let (rows, _) = self.sector.as_chunks_mut::<{ agelfs::ENTRY_BYTES }>();
        let first = sector * agelfs::PER_SECTOR;
        for (raw, entry) in rows.iter_mut().zip(self.entries.iter().skip(first)) {
            for (stored, byte) in raw.iter_mut().zip(entry.name.iter()) {
                *stored = *byte;
            }
            raw[32] = entry.name_len;
            raw[33] = entry.kind;
            let [parent_low, parent_high] = entry.parent.to_le_bytes();
            raw[34] = parent_low;
            raw[35] = parent_high;
            let [l0, l1, l2, l3] = entry.length.to_le_bytes();
            raw[36] = l0;
            raw[37] = l1;
            raw[38] = l2;
            raw[39] = l3;
        }
        unsafe { self.write_sector(agelfs::DIRECTORY + sector as u64) }
    }

    #[link_section = ".user_text"]
    unsafe fn format(&mut self) -> Result<(), u64> {
        self.sector = [0; 512];
        for (stored, byte) in self.sector.iter_mut().zip(agelfs::MAGIC.iter()) {
            *stored = *byte;
        }
        self.sector[8] = 1;
        unsafe { self.write_sector(agelfs::SUPERBLOCK)? };
        self.entries = [Entry::EMPTY; agelfs::ENTRIES];
        self.entries[0].kind = crate::world::fs::KIND_DIRECTORY as u8;
        for sector in 0..agelfs::DIRECTORY_SECTORS as usize {
            unsafe { self.flush_entry(sector * agelfs::PER_SECTOR)? };
        }
        self.mounted = true;
        Ok(())
    }

    /// The child of `directory` named `name`, if any.
    #[link_section = ".user_text"]
    fn child(&self, directory: u16, name: &[u8]) -> Option<usize> {
        self.entries
            .iter()
            .enumerate()
            .skip(1)
            .find_map(|(index, entry)| {
                (entry.kind != 0
                    && entry.parent == directory
                    && entry.name.get(..entry.name_len as usize) == Some(name))
                .then_some(index)
            })
    }

    /// Resolve `path` from `root`, refusing to climb above it: a process's
    /// namespace is the whole of what it can name.
    #[link_section = ".user_text"]
    unsafe fn open(&mut self, root: u16, flags: u64, path: &[u8]) -> Result<(usize, u32, u8), u64> {
        use crate::world::fs;
        unsafe { self.mount()? };
        if self.entry(root as usize)?.kind != fs::KIND_DIRECTORY as u8 {
            return Err(fs::ENOTDIR);
        }
        let mut current = root;
        let mut components = path
            .split(|byte| *byte == b'/')
            .filter(|component| !component.is_empty())
            .peekable();
        while let Some(component) = components.next() {
            let last = components.peek().is_none();
            if component == b"." {
                continue;
            }
            if component == b".." {
                if current == root {
                    return Err(fs::EACCES);
                }
                current = self.entry(current as usize)?.parent;
                continue;
            }
            if component.len() > agelfs::NAME_BYTES {
                return Err(fs::EINVAL);
            }
            match self.child(current, component) {
                Some(index) => {
                    if !last && self.entry(index)?.kind != fs::KIND_DIRECTORY as u8 {
                        return Err(fs::ENOTDIR);
                    }
                    current = index as u16;
                }
                None if last && flags & fs::O_CREAT_BIT != 0 => {
                    let Some(index) = self
                        .entries
                        .iter()
                        .enumerate()
                        .skip(1)
                        .find_map(|(index, entry)| (entry.kind == 0).then_some(index))
                    else {
                        return Err(fs::ENOSPC);
                    };
                    let mut entry = Entry::EMPTY;
                    for (stored, byte) in entry.name.iter_mut().zip(component.iter()) {
                        *stored = *byte;
                    }
                    entry.name_len = component.len() as u8;
                    entry.kind = if flags & fs::O_DIRECTORY_BIT != 0 {
                        fs::KIND_DIRECTORY as u8
                    } else {
                        fs::KIND_FILE as u8
                    };
                    entry.parent = current;
                    if let Some(slot) = self.entries.get_mut(index) {
                        *slot = entry;
                    }
                    unsafe { self.flush_entry(index)? };
                    current = index as u16;
                }
                None => return Err(fs::ENOENT),
            }
        }
        let entry = self.entry(current as usize)?;
        if flags & fs::O_DIRECTORY_BIT != 0 && entry.kind != fs::KIND_DIRECTORY as u8 {
            return Err(fs::ENOTDIR);
        }
        if flags & fs::O_DIRECTORY_BIT == 0
            && entry.kind == fs::KIND_DIRECTORY as u8
            && flags & (fs::O_WRONLY_BIT | fs::O_RDWR_BIT) != 0
        {
            return Err(fs::EISDIR);
        }
        Ok((current as usize, entry.length, entry.kind))
    }

    /// The bytes of `entry` at `offset`, at most one block, into the block
    /// area.
    #[link_section = ".user_text"]
    unsafe fn read(&mut self, index: usize, offset: u64, length: u64) -> Result<u64, u64> {
        use crate::world::fs;
        unsafe { self.mount()? };
        let entry = self.entry(index)?;
        if entry.kind != fs::KIND_FILE as u8 {
            return Err(fs::EINVAL);
        }
        let size = u64::from(entry.length);
        if offset >= size {
            return Ok(0);
        }
        let length = length.min(512).min(size - offset);
        let block = (self.page as usize + crate::world::BLOCK_OFFSET) as *mut u8;
        let mut done = 0_u64;
        while done < length {
            let at = offset + done;
            let sector = agelfs::DATA + index as u64 * agelfs::EXTENT_SECTORS + at / 512;
            unsafe { self.read_sector(sector)? };
            let inside = (at % 512) as usize;
            let take = ((512 - inside) as u64).min(length - done) as usize;
            for (position, byte) in self.sector.iter().skip(inside).take(take).enumerate() {
                unsafe { block.add(done as usize + position).write_volatile(*byte) };
            }
            done += take as u64;
        }
        Ok(done)
    }

    /// Store the block area's first `length` bytes at `offset` of `entry`.
    #[link_section = ".user_text"]
    unsafe fn write(&mut self, index: usize, offset: u64, length: u64) -> Result<u64, u64> {
        use crate::world::fs;
        unsafe { self.mount()? };
        let entry = self.entry(index)?;
        if entry.kind != fs::KIND_FILE as u8 {
            return Err(fs::EINVAL);
        }
        let length = length.min(512);
        if offset + length > fs::FILE_BYTES {
            return Err(fs::EFBIG);
        }
        let block = (self.page as usize + crate::world::BLOCK_OFFSET) as *const u8;
        let mut data = [0_u8; 512];
        for (position, byte) in data.iter_mut().enumerate().take(length as usize) {
            *byte = unsafe { block.add(position).read_volatile() };
        }
        let mut done = 0_u64;
        while done < length {
            let at = offset + done;
            let sector = agelfs::DATA + index as u64 * agelfs::EXTENT_SECTORS + at / 512;
            let inside = (at % 512) as usize;
            let take = ((512 - inside) as u64).min(length - done) as usize;
            unsafe { self.read_sector(sector)? };
            for (stored, byte) in self
                .sector
                .iter_mut()
                .skip(inside)
                .zip(data.iter().skip(done as usize))
                .take(take)
            {
                *stored = *byte;
            }
            unsafe { self.write_sector(sector)? };
            done += take as u64;
        }
        let end = (offset + length) as u32;
        if end > entry.length {
            if let Some(slot) = self.entries.get_mut(index) {
                slot.length = end;
            }
            unsafe { self.flush_entry(index)? };
        }
        Ok(length)
    }

    /// Remove what `path` names from `root`: a file, or a directory with
    /// nothing in it. The extent's sectors keep their bytes; the entry is
    /// what makes them a file, and it is gone.
    #[link_section = ".user_text"]
    unsafe fn unlink(&mut self, root: u16, path: &[u8]) -> Result<(), u64> {
        use crate::world::fs;
        let (index, _, kind) = unsafe { self.open(root, 0, path)? };
        if index == 0 || index == usize::from(root) {
            return Err(fs::EACCES);
        }
        if kind == fs::KIND_DIRECTORY as u8
            && self
                .entries
                .iter()
                .any(|entry| entry.kind != 0 && entry.parent == index as u16)
        {
            return Err(fs::ENOTEMPTY);
        }
        if let Some(slot) = self.entries.get_mut(index) {
            *slot = Entry::EMPTY;
        }
        unsafe { self.flush_entry(index) }
    }

    /// Give what `old` names the name and place `new` spells, both from
    /// `root`; the destination must not exist, and a directory cannot be
    /// moved into itself.
    #[link_section = ".user_text"]
    unsafe fn rename(&mut self, root: u16, old: &[u8], new: &[u8]) -> Result<(), u64> {
        use crate::world::fs;
        let (index, _, kind) = unsafe { self.open(root, 0, old)? };
        if index == 0 || index == usize::from(root) {
            return Err(fs::EACCES);
        }
        let trimmed = new.strip_suffix(b"/").unwrap_or(new);
        let (directory, name) = match trimmed.iter().rposition(|byte| *byte == b'/') {
            Some(at) => (&trimmed[..at], &trimmed[at + 1..]),
            None => (&trimmed[..0], trimmed),
        };
        if name.is_empty() || name.len() > agelfs::NAME_BYTES || name == b"." || name == b".." {
            return Err(fs::EINVAL);
        }
        let (parent, _, parent_kind) = unsafe { self.open(root, fs::O_DIRECTORY_BIT, directory)? };
        if parent_kind != fs::KIND_DIRECTORY as u8 {
            return Err(fs::ENOTDIR);
        }
        // Climbing from the destination's directory must not reach the
        // entry being moved: a directory inside itself is unreachable.
        let mut ancestor = parent;
        while ancestor != 0 {
            if ancestor == index {
                return Err(fs::EINVAL);
            }
            ancestor = usize::from(self.entry(ancestor)?.parent);
            if kind != fs::KIND_DIRECTORY as u8 {
                break;
            }
        }
        if self.child(parent as u16, name).is_some() {
            return Err(fs::EEXIST);
        }
        if let Some(slot) = self.entries.get_mut(index) {
            slot.name = [0; agelfs::NAME_BYTES];
            for (stored, byte) in slot.name.iter_mut().zip(name.iter()) {
                *stored = *byte;
            }
            slot.name_len = name.len() as u8;
            slot.parent = parent as u16;
        }
        unsafe { self.flush_entry(index) }
    }

    /// The `position`-th child of `directory`.
    #[link_section = ".user_text"]
    unsafe fn list(&mut self, directory: u16, position: u64) -> Result<(usize, u8, u32), u64> {
        use crate::world::fs;
        unsafe { self.mount()? };
        let mut seen = 0_u64;
        for (index, entry) in self.entries.iter().enumerate().skip(1) {
            if entry.kind != 0 && entry.parent == directory {
                if seen == position {
                    let payload = (self.page as usize + crate::world::PAYLOAD_OFFSET) as *mut u8;
                    let name_len = entry.name_len as usize;
                    for (offset, byte) in entry.name.iter().take(name_len).enumerate() {
                        unsafe { payload.add(offset).write_volatile(*byte) };
                    }
                    unsafe {
                        self.page
                            .add(shared::VALUES + 3)
                            .write_volatile(u64::from(entry.name_len))
                    };
                    return Ok((index, entry.kind, entry.length));
                }
                seen += 1;
            }
        }
        Err(fs::ENOENT)
    }
}

/// The filesystem service's entry point.
///
/// # Safety
/// Entered by the architecture's return-from-exception instruction with a
/// private stack and a valid shared page.
#[cfg(feature = "process")]
#[no_mangle]
#[link_section = ".user_text"]
pub unsafe extern "C" fn agel_fs_main(shared_page: u64) -> ! {
    use crate::world::fs;
    let page = shared_page as *mut u64;
    let mut filesystem = Filesystem {
        page,
        entries: [Entry::EMPTY; agelfs::ENTRIES],
        mounted: false,
        sector: [0; 512],
    };
    loop {
        let command = unsafe { page.add(shared::COMMAND).read_volatile() };
        let arguments = [
            unsafe { page.add(shared::ARGUMENTS).read_volatile() },
            unsafe { page.add(shared::ARGUMENTS + 1).read_volatile() },
            unsafe { page.add(shared::ARGUMENTS + 2).read_volatile() },
        ];
        let outcome: Result<[u64; 3], u64> = if command == fs::COMMAND_FORMAT {
            unsafe { filesystem.format() }.map(|()| [0, 0, 0])
        } else if command == fs::COMMAND_OPEN {
            let length = (arguments[2] as usize).min(crate::world::PAYLOAD_BYTES);
            let payload = (shared_page as usize + crate::world::PAYLOAD_OFFSET) as *const u8;
            let mut path = [0_u8; crate::world::PAYLOAD_BYTES];
            for (offset, byte) in path.iter_mut().enumerate().take(length) {
                *byte = unsafe { payload.add(offset).read_volatile() };
            }
            let path = path.get(..length).unwrap_or(&[]);
            unsafe { filesystem.open(arguments[0] as u16, arguments[1], path) }
                .map(|(entry, length, kind)| [entry as u64, u64::from(length), u64::from(kind)])
        } else if command == fs::COMMAND_READ {
            unsafe { filesystem.read(arguments[0] as usize, arguments[1], arguments[2]) }
                .map(|count| [count, 0, 0])
        } else if command == fs::COMMAND_WRITE {
            unsafe { filesystem.write(arguments[0] as usize, arguments[1], arguments[2]) }
                .map(|count| [count, 0, 0])
        } else if command == fs::COMMAND_LIST {
            unsafe { filesystem.list(arguments[0] as u16, arguments[1]) }
                .map(|(entry, kind, length)| [entry as u64, u64::from(kind), u64::from(length)])
        } else if command == fs::COMMAND_UNLINK || command == fs::COMMAND_RENAME {
            let first = (arguments[1] as usize).min(crate::world::PAYLOAD_BYTES);
            let second = (arguments[2] as usize).min(crate::world::PAYLOAD_BYTES - first);
            let payload = (shared_page as usize + crate::world::PAYLOAD_OFFSET) as *const u8;
            let mut paths = [0_u8; crate::world::PAYLOAD_BYTES];
            for (offset, byte) in paths.iter_mut().enumerate().take(first + second) {
                *byte = unsafe { payload.add(offset).read_volatile() };
            }
            let old = paths.get(..first).unwrap_or(&[]);
            let new = paths.get(first..first + second).unwrap_or(&[]);
            if command == fs::COMMAND_UNLINK {
                unsafe { filesystem.unlink(arguments[0] as u16, old) }.map(|()| [0, 0, 0])
            } else {
                unsafe { filesystem.rename(arguments[0] as u16, old, new) }.map(|()| [0, 0, 0])
            }
        } else if command == shared::COMMAND_FAULT_WRITE {
            unsafe { (crate::arch::KERNEL_PROBE_ADDRESS as *mut u64).write_volatile(0xdead) };
            Err(fs::EINVAL)
        } else {
            Err(fs::EINVAL)
        };
        unsafe {
            page.add(fs::DISK_OPERATION).write_volatile(0);
            match outcome {
                Ok(values) => {
                    page.add(shared::STATUS).write_volatile(0);
                    page.add(shared::VALUES).write_volatile(values[0]);
                    page.add(shared::VALUES + 1).write_volatile(values[1]);
                    page.add(shared::VALUES + 2).write_volatile(values[2]);
                }
                Err(status) => page.add(shared::STATUS).write_volatile(status),
            }
            yield_to_supervisor();
        }
    }
}
