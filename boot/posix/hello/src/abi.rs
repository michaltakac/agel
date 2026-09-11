//! The process side of the Agel process protocol, shared by the example
//! programs. The constants match `docs/posix-personality.md`; nothing here
//! links against the kernel.
#![allow(dead_code)]

use core::arch::asm;

/// Shared-page word indices of the request block.
pub const KIND: usize = 64;
pub const ARGUMENTS: usize = 65;
pub const RESULT: usize = 69;
/// Byte offset of the block area used for request data.
pub const BLOCK_OFFSET: usize = 1024;
pub const BLOCK_BYTES: usize = 512;
pub const EXIT: u64 = 1;
pub const WRITE: u64 = 2;
/// The contract's `endpoint.send` on the supervisor's well-known slot: how a
/// world hands control back.
const ENDPOINT_SEND: u64 = 0x0402;
const SUPERVISOR_ENDPOINT: u64 = 31;

pub struct Process {
    page: *mut u64,
}

impl Process {
    /// # Safety
    /// `page` must be the shared page the supervisor entered this process with.
    pub const unsafe fn new(page: u64) -> Self {
        Self {
            page: page as *mut u64,
        }
    }

    fn request(&self, kind: u64, arguments: [u64; 4]) -> u64 {
        unsafe {
            self.page.add(KIND).write_volatile(kind);
            for (index, argument) in arguments.iter().enumerate() {
                self.page.add(ARGUMENTS + index).write_volatile(*argument);
            }
            yield_to_supervisor();
            self.page.add(RESULT).read_volatile()
        }
    }

    /// Write `bytes` to descriptor `descriptor`, at most a block at a time.
    pub fn write(&self, descriptor: u64, bytes: &[u8]) -> u64 {
        let mut written = 0;
        while written < bytes.len() {
            let take = (bytes.len() - written).min(BLOCK_BYTES);
            let block = (self.page as usize + BLOCK_OFFSET) as *mut u8;
            for (offset, byte) in bytes[written..written + take].iter().enumerate() {
                unsafe { block.add(offset).write_volatile(*byte) };
            }
            let result = self.request(WRITE, [descriptor, take as u64, 0, 0]);
            if result as i64 <= 0 {
                return result;
            }
            written += result as usize;
        }
        written as u64
    }

    pub fn exit(&self, status: u64) -> ! {
        loop {
            self.request(EXIT, [status, 0, 0, 0]);
        }
    }
}

unsafe fn yield_to_supervisor() {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        asm!("int 0x80", inout("rax") ENDPOINT_SEND => _, inout("rdi") SUPERVISOR_ENDPOINT => _,
             inout("rsi") 0_u64 => _, inout("rdx") 0_u64 => _, inout("r10") 0_u64 => _, in("r8") 0_u64,
             options(nostack));
    }
    #[cfg(target_arch = "aarch64")]
    unsafe {
        asm!("svc #0", in("x8") ENDPOINT_SEND, inout("x0") SUPERVISOR_ENDPOINT => _,
             inout("x1") 0_u64 => _, inout("x2") 0_u64 => _, inout("x3") 0_u64 => _, inout("x4") 0_u64 => _,
             options(nostack));
    }
    #[cfg(target_arch = "riscv64")]
    unsafe {
        asm!("ecall", in("a7") ENDPOINT_SEND, inout("a0") SUPERVISOR_ENDPOINT => _,
             inout("a1") 0_u64 => _, inout("a2") 0_u64 => _, inout("a3") 0_u64 => _, inout("a4") 0_u64 => _,
             options(nostack));
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    loop {}
}
