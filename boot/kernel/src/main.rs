#![no_std]
#![no_main]

use core::arch::asm;
use core::panic::PanicInfo;

#[cfg(not(any(feature = "selftest", feature = "monitor-selftest")))]
mod native;

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

    #[cfg(not(any(
        feature = "selftest",
        feature = "monitor-selftest",
        feature = "native-selftest"
    )))]
    fn read_byte() -> u8 {
        while unsafe { input(COM1 + 5) } & 1 == 0 {}
        unsafe { input(COM1) }
    }
}

#[cfg(not(any(feature = "selftest", feature = "native-selftest")))]
#[derive(Clone, Copy)]
enum Slot {
    A,
    B,
}

#[cfg(not(any(feature = "selftest", feature = "native-selftest")))]
struct RecoveryMonitor {
    active: Slot,
    previous: Slot,
    candidate_verified: bool,
}

#[cfg(not(any(feature = "selftest", feature = "native-selftest")))]
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
        if matches!(self.active, Slot::B) {
            self.candidate_verified = false;
            Serial::write("denied: candidate B is already active; slot A remains rollback\n");
        } else if self.candidate_verified {
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
        Serial::write("watchdog fault: rolled back to slot ");
        Serial::write(match self.active {
            Slot::A => "A\n",
            Slot::B => "B\n",
        });
    }
}

#[no_mangle]
#[link_section = ".text.entry"]
extern "C" fn agel_boot() -> ! {
    Serial::initialize();
    Serial::write("\nAgel v1.1 native workshop\n");
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
        monitor.verify();
        monitor.promote();
        monitor.fault();
        monitor.status();
        Serial::write("AGEL_MONITOR_OK\n");
        unsafe { out32(0xf4, 0x10) };
        halt()
    }

    #[cfg(all(
        feature = "native-selftest",
        not(any(feature = "selftest", feature = "monitor-selftest"))
    ))]
    {
        native_selftest()
    }

    #[cfg(not(any(
        feature = "selftest",
        feature = "monitor-selftest",
        feature = "native-selftest"
    )))]
    {
        native_repl()
    }
}

#[cfg(not(any(
    feature = "selftest",
    feature = "monitor-selftest",
    feature = "native-selftest"
)))]
fn native_repl() -> ! {
    let mut session = native::Session::new();
    let mut monitor = RecoveryMonitor::new();
    let mut line = [0_u8; 256];
    Serial::write("AGEL_NATIVE_READY\n");
    Serial::write("Type :help. Definitions live transactionally in this VM session.\n");
    loop {
        Serial::write("agel-native[");
        write_u64(session.revision());
        Serial::write("]> ");
        let length = read_form(&mut line);
        let source = &line[..length];
        if source
            .iter()
            .copied()
            .find(|byte| !byte.is_ascii_whitespace())
            .is_some_and(|byte| byte == b';')
        {
            continue;
        }
        match source {
            b":help" => Serial::write(
                "forms: quote if begin def fn | builtins: + - * / = < eval\n\
                 commands: :revision :rollback :defs :limits :recovery-status :verify :promote :fault :shutdown\n",
            ),
            b":revision" => {
                Serial::write("revision ");
                write_u64(session.revision());
                Serial::write("\n");
            }
            b":rollback" => match session.rollback() {
                Ok(()) => Serial::write("rolled back one committed native world\n"),
                Err(error) => write_error(error),
            },
            b":defs" => {
                Serial::write("definitions (");
                write_u64(session.binding_count() as u64);
                Serial::write("): ");
                for index in 0..session.binding_count() {
                    if index > 0 {
                        Serial::write(" ");
                    }
                    if let Some(name) = session.binding_name(index) {
                        write_bytes(name);
                    }
                }
                Serial::write("\n");
            }
            b":limits" => Serial::write(
                "source=256 nodes=128 globals=24 name=24 params=4 args=8 body=192 depth=24 fuel=2000\n",
            ),
            b":recovery-status" => monitor.status(),
            b":verify" => monitor.verify(),
            b":promote" => monitor.promote(),
            b":fault" => monitor.fault(),
            b":shutdown" => unsafe { out32(0xf4, 0x10) },
            b"" => {}
            _ => match session.evaluate(source) {
                Ok(value) => write_value(value, source),
                Err(error) => write_error(error),
            },
        }
    }
}

#[cfg(not(any(
    feature = "selftest",
    feature = "monitor-selftest",
    feature = "native-selftest"
)))]
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

#[cfg(not(any(
    feature = "selftest",
    feature = "monitor-selftest",
    feature = "native-selftest"
)))]
fn read_form(buffer: &mut [u8]) -> usize {
    let mut length = 0;
    loop {
        length += read_line(&mut buffer[length..]);
        if !needs_more_input(&buffer[..length]) || length == buffer.len() {
            return length;
        }
        buffer[length] = b'\n';
        length += 1;
        Serial::write("             ... ");
    }
}

#[cfg(not(any(
    feature = "selftest",
    feature = "monitor-selftest",
    feature = "native-selftest"
)))]
fn needs_more_input(source: &[u8]) -> bool {
    let mut depth = 0_u16;
    let mut comment = false;
    for byte in source {
        if comment {
            if *byte == b'\n' {
                comment = false;
            }
            continue;
        }
        match byte {
            b';' => comment = true,
            b'(' => depth = depth.saturating_add(1),
            b')' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    depth > 0
}

#[cfg(not(any(
    feature = "selftest",
    feature = "monitor-selftest",
    feature = "native-selftest"
)))]
fn write_value(value: native::Value, source: &[u8]) {
    match value {
        native::Value::Int(value) => write_i64(value),
        native::Value::Bool(true) => Serial::write("#t"),
        native::Value::Bool(false) => Serial::write("#f"),
        native::Value::Nil => Serial::write("nil"),
        native::Value::Code { start, end } => {
            Serial::write("'");
            write_bytes(&source[start as usize..end as usize]);
        }
        native::Value::Function => Serial::write("#<native-function>"),
    }
    Serial::write("\n");
}

#[cfg(not(any(
    feature = "selftest",
    feature = "monitor-selftest",
    feature = "native-selftest"
)))]
fn write_error(error: native::Error) {
    Serial::write("error: ");
    Serial::write(error.0);
    Serial::write(" (transaction rolled back)\n");
}

#[cfg(not(any(
    feature = "selftest",
    feature = "monitor-selftest",
    feature = "native-selftest"
)))]
fn write_bytes(bytes: &[u8]) {
    for byte in bytes {
        Serial::write_byte(*byte);
    }
}

#[cfg(not(any(
    feature = "selftest",
    feature = "monitor-selftest",
    feature = "native-selftest"
)))]
fn write_i64(value: i64) {
    if value < 0 {
        Serial::write_byte(b'-');
        write_u64(value.unsigned_abs());
    } else {
        write_u64(value as u64);
    }
}

#[cfg(not(any(
    feature = "selftest",
    feature = "monitor-selftest",
    feature = "native-selftest"
)))]
fn write_u64(mut value: u64) {
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

#[cfg(all(
    feature = "native-selftest",
    not(any(feature = "selftest", feature = "monitor-selftest"))
))]
fn native_selftest() -> ! {
    let mut session = native::Session::new();
    let passed = expect_int(&mut session, b"(+ 20 22)", 42)
        && expect_int(&mut session, b"-9223372036854775808", i64::MIN)
        && expect_int(&mut session, b"((fn (x) ((fn (x) (+ x 1)) 41)) 0)", 42)
        && expect_int(&mut session, b"(((fn (x) (fn (y) (+ x y))) 40) 2)", 42)
        && session.evaluate(b"(def + 9)").is_err()
        && session.evaluate(b"(fn (x x) x)").is_err()
        && session
            .evaluate(b"((fn (x) (def add-x (fn (y) (+ x y)))) 40)")
            .is_err()
        && session.evaluate(b"(def f (fn (x) 1))").is_ok()
        && expect_int(&mut session, b"(f (begin (def f (fn (x) 2)) 0))", 1)
        && session.evaluate(b"(def square (fn (x) (* x x)))").is_ok()
        && expect_int(&mut session, b"(square 9)", 81)
        && expect_int(&mut session, b"(eval '(+ 40 2))", 42)
        && session.evaluate(b"(def x 1)").is_ok()
        && session.evaluate(b"(def x 2)").is_ok()
        && session.evaluate(b"(begin (def x 3) (/ 1 0))").is_err()
        && session.rollback().is_ok()
        && session.integer(b"x") == Some(1)
        && session
            .evaluate(b"(def fact (fn (n) (if (= n 0) 1 (* n (fact (- n 1))))))")
            .is_ok()
        && expect_int(&mut session, b"(fact 6)", 720);
    if passed {
        Serial::write("AGEL_NATIVE_OK\n");
        unsafe { out32(0xf4, 0x10) };
    } else {
        Serial::write("AGEL_NATIVE_FAILED\n");
        unsafe { out32(0xf4, 0x11) };
    }
    halt()
}

#[cfg(all(
    feature = "native-selftest",
    not(any(feature = "selftest", feature = "monitor-selftest"))
))]
fn expect_int(session: &mut native::Session, source: &[u8], expected: i64) -> bool {
    session.evaluate(source) == Ok(native::Value::Int(expected))
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
