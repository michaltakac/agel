//! The supervisor's own last-resort path to the console.
//!
//! Since v0.1.5 this is *not* how most output happens. The console driver runs in
//! its own unprivileged domain and holds the device; ordinary output goes
//! through [`crate::service::ServiceWriter`], and if that driver does not work
//! nothing is printed.
//!
//! What remains here is the path the supervisor keeps for itself: its own
//! reports, and the panic handler. That is deliberate rather than a leftover. A
//! recovery plane whose ability to report a failure runs through the component
//! that failed cannot report that component failing, which is the one moment it
//! most needs to. So the supervisor retains a direct route it uses sparingly,
//! and the driver domain owns the device for everything else.
//!
//! Each architecture provides the byte sink; everything above that is shared.

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
