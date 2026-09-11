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
pub const SPAWN: u64 = 6;
pub const PIPE: u64 = 7;
pub const WAIT: u64 = 8;
pub const SEEK: u64 = 9;
/// A window on the desktop, and records drawn into it: graphics only.
pub const WINDOW: u64 = 10;
pub const DRAW: u64 = 11;
pub const DRAW_CLEAR: u64 = 1;
/// The next event for a window: a press with content coordinates in bits
/// 32..48 and 16..32, or a key with its byte in the low eight bits.
pub const EVENT: u64 = 12;
pub const EVENT_PRESS: u64 = 1 << 56;
pub const EVENT_KEY: u64 = 2 << 56;
pub const EVENT_RELEASE: u64 = 3 << 56;
pub const EVENT_MOTION: u64 = 4 << 56;
/// Names in the namespace: remove, move, ask; and the next child of an
/// open directory.
pub const UNLINK: u64 = 13;
pub const RENAME: u64 = 14;
pub const STAT: u64 = 15;
pub const READDIR: u64 = 16;
/// Time since the machine came up, a sleep, and the end of a child.
pub const CLOCK: u64 = 17;
pub const SLEEP: u64 = 18;
pub const KILL: u64 = 19;
pub const SIGNAL_KILLED: u64 = 9;
/// Pages at the break, and a file's length through a descriptor.
pub const BRK: u64 = 20;
/// The most pages one `brk` request maps.
pub const BRK_PAGES: u64 = 64;
pub const FTRUNCATE: u64 = 21;
/// A compositor record is 64 bytes; a draw request carries at most eight.
pub const RECORD_BYTES: usize = 64;
pub const DRAW_RECORDS: usize = 8;
/// Shared-page word holding the number of NUL-terminated arguments the
/// supervisor placed in the payload area before the process first ran.
pub const ARGUMENT_COUNT: usize = 70;
pub const O_APPEND: u64 = 0o2000;
/// A descriptor argument to `spawn` that names none.
pub const NO_DESCRIPTOR: u64 = 0xffff;
pub const SPAWN_READ_ONLY: u64 = 1;
/// Set in a `wait` answer when the child was stopped rather than exiting;
/// the low byte is then the signal a POSIX parent would see.
pub const WAIT_SIGNALED: u64 = 0x100;
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

    /// Start `program` as a child with exactly these descriptors as its 0
    /// and 1 (`NO_DESCRIPTOR` for none), the console as its 2, and this
    /// process's namespace, read-only with `SPAWN_READ_ONLY`. The child's
    /// id, or a negated error number.
    /// `arguments` is the child's argument block, NUL-terminated strings;
    /// empty gives the child its name as its one argument.
    pub fn spawn(
        &self,
        program: &[u8],
        arguments: &[u8],
        stdin: u64,
        stdout: u64,
        flags: u64,
    ) -> i64 {
        let take = program.len().min(16);
        let payload = (self.page as usize + PAYLOAD_OFFSET) as *mut u8;
        for (offset, byte) in program.iter().take(take).enumerate() {
            unsafe { payload.add(offset).write_volatile(*byte) };
        }
        unsafe { payload.add(take).write_volatile(0) };
        let room = PAYLOAD_BYTES - take - 1;
        let block = arguments.len().min(room);
        for (offset, byte) in arguments.iter().take(block).enumerate() {
            unsafe { payload.add(take + 1 + offset).write_volatile(*byte) };
        }
        self.request(
            SPAWN,
            [take as u64, stdin, stdout, flags | ((block as u64) << 16)],
        ) as i64
    }

    /// The arguments the supervisor gave this process: the count, and the
    /// payload bytes holding them.
    pub fn arguments(&self) -> (usize, [u8; PAYLOAD_BYTES]) {
        let count = unsafe { self.page.add(ARGUMENT_COUNT).read_volatile() } as usize;
        let payload = (self.page as usize + PAYLOAD_OFFSET) as *const u8;
        let mut block = [0_u8; PAYLOAD_BYTES];
        for (offset, byte) in block.iter_mut().enumerate() {
            *byte = unsafe { payload.add(offset).read_volatile() };
        }
        (count, block)
    }

    /// Move a file descriptor's offset; `whence` is 0 from the start, 1
    /// from the current offset, 2 from the end.
    pub fn seek(&self, descriptor: u64, offset: i64, whence: u64) -> i64 {
        self.request(SEEK, [descriptor, offset as u64, whence, 0]) as i64
    }

    /// A pipe: the read end and the write end, or a negated error number.
    pub fn pipe(&self) -> Result<(u64, u64), i64> {
        let result = self.request(PIPE, [0; 4]);
        if (result as i64) < 0 {
            Err(result as i64)
        } else {
            Ok((result & 0xffff, (result >> 16) & 0xffff))
        }
    }

    /// Ask the desktop for a window of `width` by `height` pixels of
    /// content, titled `title` (at most 28 bytes): its number, or a
    /// negated error number (`-ENODEV` where there is no display).
    pub fn window(&self, width: u32, height: u32, title: &[u8]) -> i64 {
        let take = title.len().min(28);
        let payload = (self.page as usize + PAYLOAD_OFFSET) as *mut u8;
        for (offset, byte) in title.iter().take(take).enumerate() {
            unsafe { payload.add(offset).write_volatile(*byte) };
        }
        self.request(
            WINDOW,
            [u64::from(width), u64::from(height), take as u64, 0],
        ) as i64
    }

    /// Draw `records` (whole 64-byte compositor records, at most eight)
    /// into window `window`, relative to its content, clearing it first
    /// with `DRAW_CLEAR` in `flags`. The records the window now holds, or
    /// a negated error number; `-EINVAL` and nothing drawn when any record
    /// is not permitted or lies outside the content.
    pub fn draw(&self, window: u64, records: &[u8], flags: u64) -> i64 {
        let count = (records.len() / RECORD_BYTES).min(DRAW_RECORDS);
        let block = (self.page as usize + BLOCK_OFFSET) as *mut u8;
        for (offset, byte) in records.iter().take(count * RECORD_BYTES).enumerate() {
            unsafe { block.add(offset).write_volatile(*byte) };
        }
        self.request(DRAW, [window, count as u64, flags, 0]) as i64
    }

    /// The next event for `window`, waiting for one when `wait`: the
    /// packed event, 0 when there is none, or a negated error number.
    pub fn event(&self, window: u64, wait: bool) -> i64 {
        self.request(EVENT, [window, u64::from(wait), 0, 0]) as i64
    }

    /// Put `first` and `second` in the payload area, back to back.
    fn place_paths(&self, first: &[u8], second: &[u8]) -> (usize, usize) {
        let payload = (self.page as usize + PAYLOAD_OFFSET) as *mut u8;
        let take = first.len().min(PAYLOAD_BYTES);
        let more = second.len().min(PAYLOAD_BYTES - take);
        for (offset, byte) in first
            .iter()
            .take(take)
            .chain(second.iter().take(more))
            .enumerate()
        {
            unsafe { payload.add(offset).write_volatile(*byte) };
        }
        (take, more)
    }

    /// Remove `path`: a file, or an empty directory. 0, or a negated error.
    pub fn unlink(&self, path: &[u8]) -> i64 {
        let (take, _) = self.place_paths(path, &[]);
        self.request(UNLINK, [take as u64, 0, 0, 0]) as i64
    }

    /// Move `old` to `new`, which must not exist. 0, or a negated error.
    pub fn rename(&self, old: &[u8], new: &[u8]) -> i64 {
        let (take, more) = self.place_paths(old, new);
        self.request(RENAME, [take as u64, more as u64, 0, 0]) as i64
    }

    /// What `path` is: the kind (1 a file, 2 a directory) and the length,
    /// or a negated error.
    pub fn stat(&self, path: &[u8]) -> Result<(u8, u64), i64> {
        let (take, _) = self.place_paths(path, &[]);
        let result = self.request(STAT, [take as u64, 0, 0, 0]) as i64;
        if result < 0 {
            Err(result)
        } else {
            Ok(((result & 0xff) as u8, (result as u64) >> 8))
        }
    }

    /// The next child of the directory open at `descriptor`: its name
    /// copied into `name`, its kind and length; `None` past the last, or
    /// a negated error.
    pub fn readdir(
        &self,
        descriptor: u64,
        name: &mut [u8],
    ) -> Result<Option<(usize, u8, u64)>, i64> {
        let result = self.request(READDIR, [descriptor, 0, 0, 0]) as i64;
        if result < 0 {
            return Err(result);
        }
        if result == 0 {
            return Ok(None);
        }
        let result = result as u64;
        let length = ((result >> 8) & 0xff) as usize;
        let payload = (self.page as usize + PAYLOAD_OFFSET) as *const u8;
        for (offset, byte) in name.iter_mut().take(length).enumerate() {
            *byte = unsafe { payload.add(offset).read_volatile() };
        }
        Ok(Some((length, (result & 0xff) as u8, result >> 16)))
    }

    /// Microseconds since the machine came up.
    pub fn clock(&self) -> u64 {
        self.request(CLOCK, [0; 4])
    }

    /// Sleep for `microseconds`; the desktop runs meanwhile.
    pub fn sleep(&self, microseconds: u64) -> i64 {
        self.request(SLEEP, [microseconds, 0, 0, 0]) as i64
    }

    /// End child `id` with `signal`, which can only be `SIGNAL_KILLED`.
    pub fn kill(&self, id: u64, signal: u64) -> i64 {
        self.request(KILL, [id, signal, 0, 0]) as i64
    }

    /// Map `pages` fresh pages at the break: their address, or the break
    /// itself for 0 pages, or a negated error number.
    pub fn brk(&self, pages: u64) -> i64 {
        self.request(BRK, [pages, 0, 0, 0]) as i64
    }

    /// Set the length of the file at `descriptor`. 0, or a negated error.
    pub fn ftruncate(&self, descriptor: u64, length: u64) -> i64 {
        self.request(FTRUNCATE, [descriptor, length, 0, 0]) as i64
    }

    /// Wait for child `id` to end: its exit status, `WAIT_SIGNALED` with a
    /// signal number when the machine stopped it, or a negated error.
    pub fn wait(&self, id: u64) -> i64 {
        self.request(WAIT, [id, 0, 0, 0]) as i64
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
