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
                    evaluator_text(page, &mut response_length, b"#<native-agent:");
                    evaluator_u64(page, &mut response_length, u64::from(id));
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
            #[cfg(target_arch = "x86_64")]
            if command == shared::COMMAND_FAULT_STORAGE_DEVICE {
                // The same status read the storage driver performs, in a world
                // that was never granted the disk.
                let _ = unsafe { port_in8(0x1f7) };
            }
        }
        unsafe { yield_to_supervisor() };
    }
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
#[cfg(target_arch = "x86_64")]
pub mod storage_status {
    pub const OK: u64 = 0;
    pub const ABSENT: u64 = 1;
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
