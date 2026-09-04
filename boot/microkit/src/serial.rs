//! Text from a protection domain that owns no device.
//!
//! Only the serial domain has a capability to the UART. Everything else buffers
//! bytes in a page it shares with that domain and asks it to print them, which
//! is the contract's rule for bulk data: shared memory for the bytes, a bounded
//! control message to say how many.
//!
//! The virtual addresses are fixed by `agel.system` rather than patched into
//! the images, so they are stated once here and referenced by both ends.

use core::fmt;

use crate::microkit::{self, Channel, MessageInfo};

/// Bytes in one shared output page.
pub const BUFFER_BYTES: usize = 4096;

/// Where the world's output page is mapped, in the world and in the serial
/// domain. Must match `agel.system`.
pub const WORLD_BUFFER_VADDR: usize = 0x0300_0000;
/// Where the recovery domain's output page is mapped, in both domains. Must
/// match `agel.system`.
pub const RECOVERY_BUFFER_VADDR: usize = 0x0301_0000;
/// Where the serial domain sees the UART. Must match `agel.system`.
pub const UART_VADDR: usize = 0x0200_0000;

/// A `core::fmt` sink that fills a shared page and asks the serial domain to
/// drain it.
///
/// The buffer is flushed when it fills and at every newline, so a domain that
/// faults deliberately at the end of its work has still had its last line
/// printed before it goes.
pub struct Writer {
    buffer: *mut u8,
    channel: Channel,
    filled: usize,
}

impl Writer {
    /// A writer over the shared page at `buffer`, flushing through `channel`.
    ///
    /// # Safety
    /// `buffer` must be a page this domain has mapped writable and shares with
    /// the serial domain, and `channel` must be a protected-procedure end to it.
    pub const unsafe fn new(buffer: usize, channel: Channel) -> Self {
        Self {
            buffer: buffer as *mut u8,
            channel,
            filled: 0,
        }
    }

    /// Print everything buffered so far.
    pub fn flush(&mut self) {
        if self.filled == 0 {
            return;
        }
        let info = MessageInfo::new(self.filled as u64, 0);
        // Safety: the channel and the shared page are declared in
        // `agel.system`; the count cannot exceed the page.
        unsafe { microkit::protected_call(self.channel, info, [0; 4]) };
        self.filled = 0;
    }

    fn push(&mut self, byte: u8) {
        if self.filled == BUFFER_BYTES {
            self.flush();
        }
        // Safety: `filled` is below `BUFFER_BYTES` after the flush above, and
        // the page is mapped writable for this domain.
        unsafe { self.buffer.add(self.filled).write_volatile(byte) };
        self.filled += 1;
        if byte == b'\n' {
            self.flush();
        }
    }
}

impl fmt::Write for Writer {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        for byte in text.bytes() {
            if byte == b'\n' {
                self.push(b'\r');
            }
            self.push(byte);
        }
        Ok(())
    }
}

/// The PL011 the serial domain owns.
///
/// It is the only device in the system, and exactly one protection domain has a
/// capability to it. Nothing else can print except by asking.
pub struct Uart {
    /// Held as an address rather than as pointers so the device can be a
    /// `static`: this domain has exactly one of them, for its whole life.
    base: usize,
}

impl Uart {
    /// The PL011 mapped at `base`.
    ///
    /// # Safety
    /// `base` must be a device memory region this domain has mapped.
    pub const unsafe fn new(base: usize) -> Self {
        Self { base }
    }

    /// Emit one byte, waiting for room in the transmit FIFO.
    pub fn write_byte(&self, byte: u8) {
        let data = self.base as *mut u8;
        let flags = (self.base + 0x18) as *const u32;
        // Safety: the device window is mapped for this domain alone.
        unsafe {
            while flags.read_volatile() & (1 << 5) != 0 {}
            data.write_volatile(byte);
        }
    }

    /// Emit `count` bytes from a page shared with another domain.
    ///
    /// `count` is untrusted: it came from whichever domain asked. It is clamped
    /// to the page rather than believed.
    ///
    /// # Safety
    /// `buffer` must be a page this domain has mapped readable.
    pub unsafe fn write_shared(&self, buffer: usize, count: usize) {
        let count = count.min(BUFFER_BYTES);
        for offset in 0..count {
            let byte = unsafe { (buffer as *const u8).add(offset).read_volatile() };
            self.write_byte(byte);
        }
    }
}
