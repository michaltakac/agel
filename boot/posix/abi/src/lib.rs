//! The process side of the Agel process protocol, shared by the programs
//! under `boot/posix`. The constants match `docs/posix-personality.md`;
//! nothing here links against the kernel.
#![no_std]

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
pub const OPEN: u64 = 3;
pub const READ: u64 = 4;
pub const CLOSE: u64 = 5;
/// `open` flags, with POSIX's values.
pub const O_RDONLY: u64 = 0;
pub const O_WRONLY: u64 = 0o1;
pub const O_RDWR: u64 = 0o2;
pub const O_CREAT: u64 = 0o100;
pub const O_DIRECTORY: u64 = 0o200000;
/// Byte offset of the payload area, where `open` reads its path: the same
/// 256 bytes at byte 128 that every world's shared page carries.
pub const PAYLOAD_OFFSET: usize = 128;
pub const PAYLOAD_BYTES: usize = 256;
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

    /// Open `path` through the process's namespace. A descriptor, or a
    /// negated error number: `-ENOENT` for a name the namespace does not
    /// hold, `-EACCES` for a right it does not grant, however the path is
    /// spelled.
    pub fn open(&self, path: &[u8], flags: u64) -> i64 {
        let take = path.len().min(PAYLOAD_BYTES);
        let payload = (self.page as usize + PAYLOAD_OFFSET) as *mut u8;
        for (offset, byte) in path.iter().take(take).enumerate() {
            unsafe { payload.add(offset).write_volatile(*byte) };
        }
        self.request(OPEN, [flags, take as u64, 0, 0]) as i64
    }

    /// Read at most one block from `descriptor` into `buffer`: the count,
    /// zero at the end of the file, or a negated error number.
    pub fn read(&self, descriptor: u64, buffer: &mut [u8]) -> i64 {
        let take = buffer.len().min(BLOCK_BYTES);
        let result = self.request(READ, [descriptor, take as u64, 0, 0]) as i64;
        if result > 0 {
            let block = (self.page as usize + BLOCK_OFFSET) as *const u8;
            for (offset, byte) in buffer.iter_mut().take(result as usize).enumerate() {
                *byte = unsafe { block.add(offset).read_volatile() };
            }
        }
        result
    }

    pub fn close(&self, descriptor: u64) -> i64 {
        self.request(CLOSE, [descriptor, 0, 0, 0]) as i64
    }

    /// Write `text`, then the decimal of `number`, then `tail`, to the
    /// console: enough reporting for a program without a formatter.
    pub fn report(&self, descriptor: u64, text: &[u8], number: i64, tail: &[u8]) {
        self.write(descriptor, text);
        let mut digits = [0_u8; 21];
        let mut position = digits.len();
        let mut rest = number.unsigned_abs();
        loop {
            position -= 1;
            digits[position] = b'0' + (rest % 10) as u8;
            rest /= 10;
            if rest == 0 {
                break;
            }
        }
        if number < 0 {
            position -= 1;
            digits[position] = b'-';
        }
        self.write(descriptor, &digits[position..]);
        self.write(descriptor, tail);
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
