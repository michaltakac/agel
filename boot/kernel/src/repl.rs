//! The serial Agel workshop: the v0.1.1 REPL that runs on the BIOS x86-64 seed.
//!
//! This is the one part of the kernel that is still architecture-bound, because
//! it is also the legacy privileged path used by the small evaluator self-test.
//! The interactive v0.1.7 workshop lives in `isolated_repl` and runs the same
//! evaluator in a protection domain.

use crate::monitor::RecoveryMonitor;
use crate::{arch, console, native};

/// The interactive Agel workshop.
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
                 agents: spawn send step run agent-state agent-pending agent-turns agent-faulted? restart-agent drop-message agent-count\n\
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
                Ok(value) => write_value(value, session.result()),
                Err(error) => write_error(error),
            },
        }
    }
}

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

fn read_form(buffer: &mut [u8]) -> usize {
    let mut length = 0;
    loop {
        length += read_line(&mut buffer[length..]);
        if !console::needs_more_input(&buffer[..length]) || length == buffer.len() {
            return length;
        }
        buffer[length] = b'\n';
        length += 1;
        console::write("             ... ");
    }
}

fn write_value(value: native::Value, rendered: &[u8]) {
    match value {
        native::Value::Int(value) => console::write_i64(value),
        native::Value::Bool(true) => console::write("#t"),
        native::Value::Bool(false) => console::write("#f"),
        native::Value::Nil => console::write("nil"),
        native::Value::Agent(id) => {
            let (number, generation) = native::agent_label(id);
            console::write("#<native-agent:");
            console::write_u64(u64::from(number));
            if generation != 0 {
                console::write(".");
                console::write_u64(u64::from(generation));
            }
            console::write(">");
        }
        native::Value::Data => console::write_bytes(rendered),
        native::Value::Function => console::write("#<native-function>"),
    }
    console::write("\n");
}

fn write_error(error: native::Error) {
    console::write("error: ");
    console::write(error.0);
    console::write(" (transaction rolled back)\n");
}
