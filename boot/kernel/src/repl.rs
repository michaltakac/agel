//! The serial Agel workshop: the v1.1 REPL that runs on the BIOS x86-64 seed.
//!
//! This is the one part of the kernel that is still architecture-bound, because
//! it is also the one part that still runs privileged. Moving the evaluator
//! into a protection domain is the next rung; until then it is honest to keep
//! it beside the machine it was written for rather than to pretend it is
//! portable.

#[cfg(not(feature = "native-selftest"))]
use crate::monitor::RecoveryMonitor;
use crate::{arch, console, native};

/// The interactive Agel workshop.
#[cfg(not(feature = "native-selftest"))]
pub fn native_repl() -> ! {
    let mut session = native::Session::new();
    let mut monitor = RecoveryMonitor::new();
    let mut line = [0_u8; 256];
    console::write("AGEL_NATIVE_READY\n");
    console::write("Type :help. Definitions live transactionally in this VM session.\n");
    loop {
        console::write("agel-native[");
        console::write_u64(session.revision());
        console::write("]> ");
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
            b":help" => console::write(
                "forms: quote if begin def fn | builtins: + - * / = < eval\n\
                 commands: :revision :rollback :defs :limits :recovery-status :verify :promote :fault :shutdown\n",
            ),
            b":revision" => {
                console::write("revision ");
                console::write_u64(session.revision());
                console::write("\n");
            }
            b":rollback" => match session.rollback() {
                Ok(()) => console::write("rolled back one committed native world\n"),
                Err(error) => write_error(error),
            },
            b":defs" => {
                console::write("definitions (");
                console::write_u64(session.binding_count() as u64);
                console::write("): ");
                for index in 0..session.binding_count() {
                    if index > 0 {
                        console::write(" ");
                    }
                    if let Some(name) = session.binding_name(index) {
                        console::write_bytes(name);
                    }
                }
                console::write("\n");
            }
            b":limits" => {
                console::write("source=");
                console::write_u64(line.len() as u64);
                for (name, bound) in native::LIMITS {
                    console::write(" ");
                    console::write(name);
                    console::write("=");
                    console::write_u64(*bound);
                }
                console::write("\n");
            }
            b":recovery-status" => monitor.status(),
            b":verify" => monitor.verify(),
            b":promote" => monitor.promote(),
            b":fault" => monitor.fault(),
            b":shutdown" => arch::exit(true),
            b"" => {}
            _ => match session.evaluate(source) {
                Ok(value) => write_value(value, source),
                Err(error) => write_error(error),
            },
        }
    }
}

#[cfg(not(feature = "native-selftest"))]
fn read_line(buffer: &mut [u8]) -> usize {
    let mut length = 0;
    loop {
        match arch::console_read_byte() {
            b'\r' | b'\n' => {
                console::write("\n");
                return length;
            }
            8 | 127 if length > 0 => {
                length -= 1;
                console::write("\x08 \x08");
            }
            byte if (byte.is_ascii_graphic() || byte == b' ') && length < buffer.len() => {
                buffer[length] = byte;
                length += 1;
                console::write_byte(byte);
            }
            _ => {}
        }
    }
}

#[cfg(not(feature = "native-selftest"))]
fn read_form(buffer: &mut [u8]) -> usize {
    let mut length = 0;
    loop {
        length += read_line(&mut buffer[length..]);
        if !needs_more_input(&buffer[..length]) || length == buffer.len() {
            return length;
        }
        buffer[length] = b'\n';
        length += 1;
        console::write("             ... ");
    }
}

#[cfg(not(feature = "native-selftest"))]
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

#[cfg(not(feature = "native-selftest"))]
fn write_value(value: native::Value, source: &[u8]) {
    match value {
        native::Value::Int(value) => console::write_i64(value),
        native::Value::Bool(true) => console::write("#t"),
        native::Value::Bool(false) => console::write("#f"),
        native::Value::Nil => console::write("nil"),
        native::Value::Code { start, end } => {
            console::write("'");
            console::write_bytes(&source[start as usize..end as usize]);
        }
        native::Value::Function => console::write("#<native-function>"),
    }
    console::write("\n");
}

#[cfg(not(feature = "native-selftest"))]
fn write_error(error: native::Error) {
    console::write("error: ");
    console::write(error.0);
    console::write(" (transaction rolled back)\n");
}

/// The non-interactive native evaluator conformance run.
#[cfg(feature = "native-selftest")]
pub fn native_selftest() -> ! {
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
        console::write("AGEL_NATIVE_OK\n");
        arch::exit(true)
    } else {
        console::write("AGEL_NATIVE_FAILED\n");
        arch::exit(false)
    }
}

#[cfg(feature = "native-selftest")]
fn expect_int(session: &mut native::Session, source: &[u8], expected: i64) -> bool {
    session.evaluate(source) == Ok(native::Value::Int(expected))
}
