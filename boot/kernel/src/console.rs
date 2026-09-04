//! The kernel's only output device, and the one place formatting happens.
//!
//! Each architecture provides a byte sink; everything above this line is shared.
//! The kernel has no console *capability* to hand out: a protection domain that
//! wants to say something says it to its supervisor through the contract, and
//! the supervisor decides whether to print it.

use crate::arch;
#[cfg(feature = "isolation-selftest")]
use core::fmt;

/// Prepare the platform's serial device.
pub fn initialize() {
    arch::console_initialize();
}

/// Emit one byte exactly as given.
pub fn write_byte(byte: u8) {
    arch::console_write_byte(byte);
}

/// Emit text, expanding newlines for terminals that expect a carriage return.
pub fn write(text: &str) {
    for byte in text.bytes() {
        if byte == b'\n' {
            write_byte(b'\r');
        }
        write_byte(byte);
    }
}

/// Emit raw bytes with no newline translation.
#[cfg(not(any(
    feature = "selftest",
    feature = "monitor-selftest",
    feature = "native-selftest",
    feature = "isolation-selftest"
)))]
pub fn write_bytes(bytes: &[u8]) {
    for byte in bytes {
        write_byte(*byte);
    }
}

/// Emit an unsigned decimal number.
#[cfg(not(any(
    feature = "selftest",
    feature = "monitor-selftest",
    feature = "native-selftest",
    feature = "isolation-selftest"
)))]
pub fn write_u64(mut value: u64) {
    let mut digits = [0_u8; 20];
    let mut position = digits.len();
    loop {
        position -= 1;
        digits[position] = b'0' + (value % 10) as u8;
        value /= 10;
        if value == 0 {
            break;
        }
    }
    write_bytes(&digits[position..]);
}

/// Emit a signed decimal number.
#[cfg(not(any(
    feature = "selftest",
    feature = "monitor-selftest",
    feature = "native-selftest",
    feature = "isolation-selftest"
)))]
pub fn write_i64(value: i64) {
    if value < 0 {
        write_byte(b'-');
        write_u64(value.unsigned_abs());
    } else {
        write_u64(value as u64);
    }
}

/// A `core::fmt` sink over the console.
///
/// The conformance transcript is rendered with exactly the code the hosted
/// reference model uses, which is the whole point of comparing the two.
#[cfg(feature = "isolation-selftest")]
pub struct Writer;

#[cfg(feature = "isolation-selftest")]
impl fmt::Write for Writer {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        write(text);
        Ok(())
    }
}

/// Write formatted text to the console.
#[macro_export]
macro_rules! kprint {
    ($($argument:tt)*) => {{
        use core::fmt::Write as _;
        let _ = write!($crate::console::Writer, $($argument)*);
    }};
}
