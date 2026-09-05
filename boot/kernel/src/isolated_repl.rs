//! The interactive x86-64 workshop with the evaluator outside the supervisor.
//!
//! Serial input still enters through the recovery plane in v1.6. Source then
//! crosses one bounded shared page into an unprivileged evaluator domain. The
//! result crosses back as bytes and is printed by the separate console-driver
//! domain introduced in v1.5. Neither mutable component owns the recovery
//! monitor or can address the other's private stack.

use crate::arch;
use crate::monitor::RecoveryMonitor;
use crate::service::{ServiceDomain, ServiceWriter};
use crate::world::{shared, Stop, PAYLOAD_BYTES};
use core::fmt::Write as _;

/// Boot the protected interactive workshop.
pub fn run() -> ! {
    let mut machine = match arch::Machine::bring_up() {
        Ok(machine) => machine,
        Err(reason) => fatal(reason),
    };
    let worker_entry = crate::user::agel_world_main as *const () as usize as u64;
    let evaluator_entry = crate::user::agel_evaluator_main as *const () as usize as u64;
    let mut driver = match machine.create_console_world(worker_entry, 8) {
        Ok(domain) => ServiceDomain::new(domain, worker_entry, 8),
        Err(reason) => fatal(reason),
    };
    let mut evaluator = match machine.create_evaluator_world(evaluator_entry, 20) {
        Ok(domain) => domain,
        Err(reason) => fatal(reason),
    };
    let mut monitor = RecoveryMonitor::new();
    let mut revision = 0;
    let mut line = [0_u8; PAYLOAD_BYTES];

    driver_line(&mut driver, b"AGEL_NATIVE_READY");
    driver_line(
        &mut driver,
        b"Evaluator: unprivileged domain; output: restartable console domain. Type :help.",
    );

    loop {
        {
            let mut out = ServiceWriter::new(&mut driver);
            let _ = write!(out, "agel-native[{revision}]> ");
            out.flush();
            if out.failure().is_some() {
                fatal("console driver failed while writing the prompt");
            }
        }
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
            b":help" => driver_line(
                &mut driver,
                b"forms: quote if begin def fn | builtins: + - * / = < eval | commands: :revision :rollback :defs :limits :recovery-status :verify :promote :fault :shutdown",
            ),
            b":revision" => {
                let mut out = ServiceWriter::new(&mut driver);
                let _ = writeln!(out, "revision {revision}");
                out.flush();
            }
            b":rollback" => {
                revision = evaluator_request(
                    &mut evaluator,
                    &mut driver,
                    shared::COMMAND_EVALUATOR_ROLLBACK,
                    b"",
                );
            }
            b":defs" => {
                revision = evaluator_request(
                    &mut evaluator,
                    &mut driver,
                    shared::COMMAND_EVALUATOR_DEFS,
                    b"",
                );
            }
            b":limits" => {
                revision = evaluator_request(
                    &mut evaluator,
                    &mut driver,
                    shared::COMMAND_EVALUATOR_LIMITS,
                    b"",
                );
            }
            b":recovery-status" => monitor.status(),
            b":verify" => monitor.verify(),
            b":promote" => monitor.promote(),
            b":fault" => monitor.fault(),
            b":shutdown" => arch::exit(true),
            b"" => {}
            _ => {
                revision = evaluator_request(
                    &mut evaluator,
                    &mut driver,
                    shared::COMMAND_EVALUATE,
                    source,
                );
            }
        }
    }
}

fn evaluator_request(
    evaluator: &mut arch::Domain,
    driver: &mut ServiceDomain,
    command: u64,
    source: &[u8],
) -> u64 {
    for (offset, byte) in source.iter().enumerate() {
        evaluator.core().write_payload(offset, *byte);
    }
    evaluator
        .core()
        .write_shared(shared::ARGUMENTS, source.len() as u64);
    evaluator.core().stage_command(command);
    match evaluator.run() {
        Stop::Replied => {}
        Stop::Faulted(fault) => {
            crate::kprint!(
                "native evaluator contained: {} at {:#x}; restart required\n",
                fault.name(),
                fault.pc
            );
            return evaluator.core().read_shared(shared::VALUES + 1);
        }
        Stop::BudgetExhausted => {
            crate::kprint!("native evaluator contained: tick budget exhausted; restart required\n");
            return evaluator.core().read_shared(shared::VALUES + 1);
        }
    }

    let error = evaluator.core().read_shared(shared::STATUS) != 0;
    let length = (evaluator.core().read_shared(shared::VALUES) as usize).min(PAYLOAD_BYTES);
    let revision = evaluator.core().read_shared(shared::VALUES + 1);
    let mut response = [0_u8; PAYLOAD_BYTES];
    for (offset, byte) in response.iter_mut().take(length).enumerate() {
        *byte = evaluator.core().read_payload(offset);
    }
    if driver
        .write_console(driver.handle(), &response[..length])
        .is_err()
    {
        fatal("console driver failed while writing an evaluator response");
    }
    if error {
        driver_line(driver, b" (transaction rolled back)");
    } else {
        driver_line(driver, b"");
    }
    revision
}

fn driver_line(driver: &mut ServiceDomain, bytes: &[u8]) {
    if driver.write_console(driver.handle(), bytes).is_err()
        || driver.write_console(driver.handle(), b"\r\n").is_err()
    {
        fatal("console driver stopped");
    }
}

fn read_line(buffer: &mut [u8]) -> usize {
    let mut length = 0;
    loop {
        match arch::console_read_byte() {
            b'\r' | b'\n' => {
                crate::console::write("\n");
                return length;
            }
            8 | 127 if length > 0 => {
                length -= 1;
                crate::console::write("\x08 \x08");
            }
            byte if (byte.is_ascii_graphic() || byte == b' ') && length < buffer.len() => {
                buffer[length] = byte;
                length += 1;
                crate::console::write_byte(byte);
            }
            _ => {}
        }
    }
}

fn read_form(buffer: &mut [u8]) -> usize {
    let mut length = 0;
    loop {
        length += read_line(&mut buffer[length..]);
        if !needs_more_input(&buffer[..length]) || length == buffer.len() {
            return length;
        }
        buffer[length] = b'\n';
        length += 1;
        crate::console::write("             ... ");
    }
}

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

fn fatal(reason: &str) -> ! {
    crate::console::write("AGEL ISOLATED WORKSHOP FAILED: ");
    crate::console::write(reason);
    crate::console::write("\n");
    arch::exit(false)
}
