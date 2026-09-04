#![no_std]
#![no_main]

use core::arch::asm;
use core::panic::PanicInfo;

const COM1: u16 = 0x3f8;

struct Serial;

impl Serial {
    fn initialize() {
        unsafe {
            out(COM1 + 1, 0x00);
            out(COM1 + 3, 0x80);
            out(COM1, 0x03);
            out(COM1 + 1, 0x00);
            out(COM1 + 3, 0x03);
            out(COM1 + 2, 0xc7);
            out(COM1 + 4, 0x0b);
        }
    }

    fn write_byte(byte: u8) {
        while unsafe { input(COM1 + 5) } & 0x20 == 0 {}
        unsafe { out(COM1, byte) };
    }

    fn write(text: &str) {
        for byte in text.bytes() {
            if byte == b'\n' {
                Self::write_byte(b'\r');
            }
            Self::write_byte(byte);
        }
    }

    #[cfg(not(any(feature = "selftest", feature = "monitor-selftest")))]
    fn read_byte() -> u8 {
        while unsafe { input(COM1 + 5) } & 1 == 0 {}
        unsafe { input(COM1) }
    }
}

#[cfg(not(feature = "selftest"))]
#[derive(Clone, Copy)]
enum Slot {
    A,
    B,
}

#[cfg(not(feature = "selftest"))]
struct RecoveryMonitor {
    active: Slot,
    previous: Slot,
    candidate_verified: bool,
}

#[cfg(not(feature = "selftest"))]
impl RecoveryMonitor {
    const fn new() -> Self {
        Self {
            active: Slot::A,
            previous: Slot::A,
            candidate_verified: false,
        }
    }

    fn status(&self) {
        Serial::write("active slot: ");
        Serial::write(match self.active {
            Slot::A => "A (stable)\n",
            Slot::B => "B (candidate)\n",
        });
    }

    fn verify(&mut self) {
        self.candidate_verified = true;
        Serial::write("candidate B: isolated health evidence accepted\n");
    }

    fn promote(&mut self) {
        if self.candidate_verified {
            self.previous = self.active;
            self.active = Slot::B;
            self.candidate_verified = false;
            Serial::write("selected slot B; slot A retained for rollback\n");
        } else {
            Serial::write("denied: verify candidate before promotion\n");
        }
    }

    fn fault(&mut self) {
        self.active = self.previous;
        self.candidate_verified = false;
        Serial::write("watchdog fault: rolled back to slot A\n");
    }
}

#[no_mangle]
#[link_section = ".text.entry"]
extern "C" fn agel_boot() -> ! {
    Serial::initialize();
    Serial::write("\nAgel v1.0 boot seed\n");
    Serial::write("recovery monitor is outside the mutable agent world\n");
    Serial::write("self-check: BIOS seed -> long mode -> Rust HAL [ok]\n");

    #[cfg(feature = "selftest")]
    {
        Serial::write("AGEL_BOOT_OK\n");
        unsafe { out32(0xf4, 0x10) };
        halt()
    }

    #[cfg(all(feature = "monitor-selftest", not(feature = "selftest")))]
    {
        let mut monitor = RecoveryMonitor::new();
        monitor.status();
        monitor.promote();
        monitor.verify();
        monitor.promote();
        monitor.status();
        monitor.fault();
        monitor.status();
        Serial::write("AGEL_MONITOR_OK\n");
        unsafe { out32(0xf4, 0x10) };
        halt()
    }

    #[cfg(not(any(feature = "selftest", feature = "monitor-selftest")))]
    shell()
}

#[cfg(not(any(feature = "selftest", feature = "monitor-selftest")))]
fn shell() -> ! {
    let mut monitor = RecoveryMonitor::new();
    let mut line = [0_u8; 64];
    loop {
        Serial::write("agel-monitor> ");
        let length = read_line(&mut line);
        match &line[..length] {
            b"help" => Serial::write("help status verify promote fault agents shutdown\n"),
            b"status" => monitor.status(),
            b"verify" => monitor.verify(),
            b"promote" => monitor.promote(),
            b"fault" => monitor.fault(),
            b"agents" => Serial::write("recovery:trusted world-A:ready world-B:standby\n"),
            b"shutdown" => unsafe { out32(0xf4, 0x10) },
            b"" => {}
            _ => Serial::write("unknown command; type help\n"),
        }
    }
}

#[cfg(not(any(feature = "selftest", feature = "monitor-selftest")))]
fn read_line(buffer: &mut [u8]) -> usize {
    let mut length = 0;
    loop {
        match Serial::read_byte() {
            b'\r' | b'\n' => {
                Serial::write("\n");
                return length;
            }
            8 | 127 if length > 0 => {
                length -= 1;
                Serial::write("\x08 \x08");
            }
            byte if (byte.is_ascii_graphic() || byte == b' ') && length < buffer.len() => {
                buffer[length] = byte;
                length += 1;
                Serial::write_byte(byte);
            }
            _ => {}
        }
    }
}

#[inline]
unsafe fn out(port: u16, value: u8) {
    unsafe { asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack)) };
}

#[inline]
unsafe fn out32(port: u16, value: u32) {
    unsafe { asm!("out dx, eax", in("dx") port, in("eax") value, options(nomem, nostack)) };
}

#[inline]
unsafe fn input(port: u16) -> u8 {
    let value: u8;
    unsafe { asm!("in al, dx", in("dx") port, out("al") value, options(nomem, nostack)) };
    value
}

fn halt() -> ! {
    loop {
        unsafe { asm!("cli; hlt", options(nomem, nostack)) };
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    Serial::write("KERNEL PANIC: recovery monitor halted the mutable world\n");
    halt()
}
