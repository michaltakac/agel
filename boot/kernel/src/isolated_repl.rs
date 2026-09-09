//! The interactive x86-64 workshop with the evaluator outside the supervisor.
//!
//! Since v0.2.27 serial input is read through the console driver domain as
//! well, so the supervisor touches no serial port on this path. Source then
//! crosses one bounded shared page into an unprivileged evaluator domain. The
//! result crosses back as bytes and is printed by the separate console-driver
//! domain introduced in v0.1.5. Neither mutable component owns the recovery
//! monitor or can address the other's private stack.
//!
//! v0.1.7 adds a tiny structural editor and a dual-slot source workspace.
//! Since v0.2.26 the disk is driven by its own unprivileged, restartable
//! domain granted exactly the ATA ports; the supervisor keeps the slot policy
//! and the codec, and persisted bytes are bounded named Agel forms which are
//! replayed into a fresh evaluator rather than a dump of Rust memory or
//! authority-bearing state.

use crate::arch;
use crate::monitor::RecoveryMonitor;
use crate::native_session::{
    replay as replay_workspace, request as evaluator_request_raw, reset as reset_evaluator,
    ReplayFailure,
};
use crate::service::{ServiceDomain, ServiceKind, ServiceWriter};
use crate::workspace::{Workspace, MAX_CELL_NAME};
use crate::world::{shared, PAYLOAD_BYTES};
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
        Ok(domain) => ServiceDomain::new(domain, ServiceKind::Console, worker_entry, 8),
        Err(reason) => fatal(reason),
    };
    // Only x86-64 has a disk. The other machines run the same workshop with an
    // in-memory workspace and say so rather than pretending to persist.
    #[cfg(target_arch = "x86_64")]
    let mut storage = {
        let storage_entry = crate::user::agel_storage_main as *const () as usize as u64;
        match machine.create_storage_world(storage_entry, 50) {
            Ok(domain) => Some(ServiceDomain::new(
                domain,
                ServiceKind::Storage,
                storage_entry,
                50,
            )),
            Err(reason) => fatal(reason),
        }
    };
    #[cfg(not(target_arch = "x86_64"))]
    let mut storage: Option<ServiceDomain> = None;
    let mut evaluator = match machine.create_evaluator_world(evaluator_entry, 20) {
        Ok(domain) => domain,
        Err(reason) => fatal(reason),
    };
    let mut monitor = RecoveryMonitor::new();
    let mut revision = 0;
    let mut line = [0_u8; PAYLOAD_BYTES];
    let mut workspace = Workspace::new();
    let mut committed_workspace = Workspace::new();
    let mut generation = 0_u64;
    let mut dirty = false;

    driver_line(&mut driver, b"AGEL_NATIVE_READY");
    driver_line(
        &mut driver,
        b"Evaluator: unprivileged domain; output: restartable console domain; storage: unprivileged disk driver domain; source workspace: dual-slot disk image. Type :help.",
    );

    match load_replay_candidates(&mut evaluator, &mut driver, storage.as_mut()) {
        Ok(DiskWorkspace::Restored(loaded, restored_revision)) => {
            workspace = loaded.workspace;
            committed_workspace = loaded.workspace;
            generation = loaded.generation;
            revision = restored_revision;
            report_restored(&mut driver, workspace.count(), generation);
        }
        Ok(DiskWorkspace::Empty) => driver_line(
            &mut driver,
            b"workspace: no persisted image; starting empty",
        ),
        Ok(DiskWorkspace::Rejected(highest_generation)) => {
            generation = highest_generation;
            driver_line(
                &mut driver,
                b"all persisted workspaces rejected; starting with an empty evaluator",
            );
            let _ = reset_evaluator(&mut evaluator);
        }
        Err(reason) => {
            driver_text_error(
                &mut driver,
                b"workspace storage unavailable: ",
                reason.as_bytes(),
            );
        }
    }

    loop {
        {
            let mut out = ServiceWriter::new(&mut driver);
            let _ = write!(out, "agel-native[{revision}]> ");
            out.flush();
            if out.failure().is_some() {
                fatal("console driver failed while writing the prompt");
            }
        }
        let length = read_form(&mut driver, &mut line);
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
                b"forms: quote if begin def fn | builtins: + - * / = < eval | agents: spawn send step run inspect/restart | workspace: :edit NAME :run NAME :show NAME :delete NAME :cells :workspace :save :reload | recovery: :revision :rollback :defs :limits :recovery-status :verify :promote :fault :shutdown",
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
            b":cells" => list_cells(&mut driver, &workspace),
            b":workspace" => report_workspace(
                &mut driver,
                workspace.count(),
                generation,
                dirty,
            ),
            b":save" => match crate::native_session::save(
                &mut evaluator,
                storage.as_mut(),
                &workspace,
                generation,
            ) {
                    Ok((next_generation, candidate_revision)) => {
                        generation = next_generation;
                        committed_workspace = workspace;
                        dirty = false;
                        revision = candidate_revision;
                        report_saved(&mut driver, workspace.count(), generation);
                    }
                    Err(reason) => {
                        driver_text_error(
                            &mut driver,
                            b"workspace save failed: ",
                            reason.as_bytes(),
                        );
                    }
            },
            b":reload" => match load_replay_candidates(&mut evaluator, &mut driver, storage.as_mut()) {
                Ok(DiskWorkspace::Restored(loaded, restored_revision)) => {
                    workspace = loaded.workspace;
                    committed_workspace = loaded.workspace;
                    generation = loaded.generation;
                    dirty = false;
                    revision = restored_revision;
                    report_restored(&mut driver, workspace.count(), generation);
                }
                Ok(DiskWorkspace::Empty) => {
                    workspace = Workspace::new();
                    committed_workspace = Workspace::new();
                    generation = 0;
                    dirty = false;
                    revision = restore_workspace(&mut evaluator, &workspace, &mut driver);
                    driver_line(&mut driver, b"workspace reload restored empty state");
                }
                Ok(DiskWorkspace::Rejected(highest_generation)) => {
                    generation = generation.max(highest_generation);
                    revision = restore_workspace(
                        &mut evaluator,
                        &committed_workspace,
                        &mut driver,
                    );
                    driver_line(
                        &mut driver,
                        b"workspace reload rejected all disk generations; staged state retained",
                    );
                }
                Err(reason) => driver_text_error(
                    &mut driver,
                    b"workspace reload failed: ",
                    reason.as_bytes(),
                ),
            },
            b":recovery-status" => monitor.status(),
            b":verify" => monitor.verify(),
            b":promote" => monitor.promote(),
            b":fault" => monitor.fault(),
            b":shutdown" => arch::exit(true),
            b"" => {}
            _ => {
                if let Some(name) = command_argument(source, b":edit ") {
                    if name.len() > MAX_CELL_NAME {
                        driver_line(&mut driver, b"error: cell name exceeds native limit");
                    } else {
                        let mut owned_name = [0_u8; MAX_CELL_NAME];
                        owned_name[..name.len()].copy_from_slice(name);
                        let name_length = name.len();
                        edit_cell(
                            &mut driver,
                            &mut workspace,
                            &owned_name[..name_length],
                            &mut line,
                        );
                        dirty = workspace != committed_workspace;
                    }
                } else if let Some(name) = command_argument(source, b":run ") {
                    match workspace.find(name) {
                        Some(cell) => {
                            revision = evaluator_request(
                                &mut evaluator,
                                &mut driver,
                                shared::COMMAND_EVALUATE,
                                cell.source(),
                            );
                        }
                        None => driver_line(&mut driver, b"error: no such workspace cell"),
                    }
                } else if let Some(name) = command_argument(source, b":show ") {
                    match workspace.find(name) {
                        Some(cell) => driver_line(&mut driver, cell.source()),
                        None => driver_line(&mut driver, b"error: no such workspace cell"),
                    }
                } else if let Some(name) = command_argument(source, b":delete ") {
                    match workspace.delete(name) {
                        Ok(()) => {
                            dirty = workspace != committed_workspace;
                            driver_line(
                                &mut driver,
                                b"cell deleted from staged workspace; :save to commit",
                            );
                        }
                        Err(reason) => driver_text_error(
                            &mut driver,
                            b"workspace edit failed: ",
                            reason.as_bytes(),
                        ),
                    }
                } else {
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
}

// Boxing would introduce an allocator into the native supervisor. The large
// variant is a deliberately bounded source workspace on its 512 KiB stack.
#[allow(clippy::large_enum_variant)]
// Only x86-64 has a disk to construct these from; see `load_replay_candidates`.
#[cfg_attr(not(target_arch = "x86_64"), allow(dead_code))]
enum DiskWorkspace {
    Empty,
    Restored(crate::workspace::LoadedWorkspace, u64),
    Rejected(u64),
}

fn load_replay_candidates(
    evaluator: &mut arch::Domain,
    driver: &mut ServiceDomain,
    storage: Option<&mut ServiceDomain>,
) -> Result<DiskWorkspace, &'static str> {
    let Some(storage) = storage else {
        return Err("no storage device on this machine");
    };
    #[cfg(not(target_arch = "x86_64"))]
    {
        let _ = (evaluator, driver, storage);
        Err("no storage device on this machine")
    }
    #[cfg(target_arch = "x86_64")]
    load_from_disk(evaluator, driver, storage)
}

#[cfg(target_arch = "x86_64")]
fn load_from_disk(
    evaluator: &mut arch::Domain,
    driver: &mut ServiceDomain,
    storage: &mut ServiceDomain,
) -> Result<DiskWorkspace, &'static str> {
    let candidates = crate::workspace::load(storage)?;
    let mut found = false;
    let mut highest_generation = 0;
    for loaded in candidates.into_iter().flatten() {
        found = true;
        highest_generation = highest_generation.max(loaded.generation);
        match replay_workspace(evaluator, &loaded.workspace) {
            Ok(revision) => return Ok(DiskWorkspace::Restored(loaded, revision)),
            Err(failure) => {
                report_replay_failure(driver, &loaded.workspace, failure, evaluator);
                driver_line(driver, b"trying previous workspace generation");
            }
        }
    }
    Ok(if found {
        DiskWorkspace::Rejected(highest_generation)
    } else {
        DiskWorkspace::Empty
    })
}

fn evaluator_request(
    evaluator: &mut arch::Domain,
    driver: &mut ServiceDomain,
    command: u64,
    source: &[u8],
) -> u64 {
    let reply = match evaluator_request_raw(evaluator, command, source) {
        Ok(reply) => reply,
        Err(reason) => {
            driver_text_error(driver, b"native evaluator contained: ", reason.as_bytes());
            return evaluator.core().read_shared(shared::VALUES + 1);
        }
    };
    if driver
        .write_console(driver.handle(), &reply.bytes[..reply.length])
        .is_err()
    {
        fatal("console driver failed while writing an evaluator response");
    }
    if reply.error {
        driver_line(driver, b" (transaction rolled back)");
    } else {
        driver_line(driver, b"");
    }
    reply.revision
}

fn restore_workspace(
    evaluator: &mut arch::Domain,
    workspace: &Workspace,
    driver: &mut ServiceDomain,
) -> u64 {
    match replay_workspace(evaluator, workspace) {
        Ok(revision) => revision,
        Err(_) => {
            driver_line(
                driver,
                b"fatal: committed workspace could not be reconstructed",
            );
            fatal("committed workspace reconstruction failed")
        }
    }
}

fn edit_cell(
    driver: &mut ServiceDomain,
    workspace: &mut Workspace,
    name: &[u8],
    line: &mut [u8; PAYLOAD_BYTES],
) {
    if name.is_empty() {
        driver_line(driver, b"usage: :edit NAME");
        return;
    }
    {
        let mut out = ServiceWriter::new(driver);
        let _ = write!(out, "edit[");
        out.flush();
    }
    if driver.write_console(driver.handle(), name).is_err()
        || driver.write_console(driver.handle(), b"]> ").is_err()
    {
        fatal("console driver stopped while opening the editor");
    }
    let length = read_form(driver, line);
    match workspace.upsert(name, &line[..length]) {
        Ok(()) => {
            driver_line(
                driver,
                b"cell staged; :run NAME to evaluate, :save to persist",
            );
        }
        Err(reason) => driver_text_error(driver, b"workspace edit failed: ", reason.as_bytes()),
    }
}

fn command_argument<'a>(source: &'a [u8], prefix: &[u8]) -> Option<&'a [u8]> {
    source.strip_prefix(prefix).map(trim_ascii)
}

fn trim_ascii(mut bytes: &[u8]) -> &[u8] {
    while bytes.first().is_some_and(u8::is_ascii_whitespace) {
        bytes = &bytes[1..];
    }
    while bytes.last().is_some_and(u8::is_ascii_whitespace) {
        bytes = &bytes[..bytes.len() - 1];
    }
    bytes
}

fn list_cells(driver: &mut ServiceDomain, workspace: &Workspace) {
    let mut out = ServiceWriter::new(driver);
    let _ = write!(out, "cells ({}):", workspace.count());
    out.flush();
    for ordinal in 0..workspace.count() {
        if let Some(cell) = workspace.cell(ordinal) {
            let _ = driver.write_console(driver.handle(), b" ");
            let _ = driver.write_console(driver.handle(), cell.name());
        }
    }
    driver_line(driver, b"");
}

fn report_workspace(driver: &mut ServiceDomain, count: usize, generation: u64, dirty: bool) {
    let mut out = ServiceWriter::new(driver);
    let _ = writeln!(
        out,
        "workspace generation {generation}, {count} cells, {}",
        if dirty { "staged changes" } else { "clean" }
    );
    out.flush();
}

fn report_saved(driver: &mut ServiceDomain, count: usize, generation: u64) {
    let mut out = ServiceWriter::new(driver);
    let _ = writeln!(
        out,
        "workspace generation {generation} committed: {count} cells; evaluator rebuilt from cells; previous slot retained"
    );
    out.flush();
}

fn report_restored(driver: &mut ServiceDomain, count: usize, generation: u64) {
    let mut out = ServiceWriter::new(driver);
    let _ = writeln!(
        out,
        "workspace generation {generation} restored: {count} cells replayed"
    );
    out.flush();
}

#[cfg_attr(not(target_arch = "x86_64"), allow(dead_code))]
fn report_replay_failure(
    driver: &mut ServiceDomain,
    workspace: &Workspace,
    failure: ReplayFailure,
    evaluator: &mut arch::Domain,
) {
    let ordinal = match failure {
        ReplayFailure::Language(ordinal) | ReplayFailure::Transport { ordinal, .. } => ordinal,
    };
    let mut out = ServiceWriter::new(driver);
    let _ = write!(out, "workspace replay rejected at cell ");
    out.flush();
    if let Some(cell) = workspace.cell(ordinal) {
        let _ = driver.write_console(driver.handle(), cell.name());
    } else {
        let _ = driver.write_console(driver.handle(), b"<reset>");
    }
    let _ = driver.write_console(driver.handle(), b": ");
    match failure {
        ReplayFailure::Language(_) => {
            let length = (evaluator.core().read_shared(shared::VALUES) as usize).min(PAYLOAD_BYTES);
            let mut response = [0_u8; PAYLOAD_BYTES];
            for (offset, byte) in response.iter_mut().take(length).enumerate() {
                *byte = evaluator.core().read_payload(offset);
            }
            let _ = driver.write_console(driver.handle(), &response[..length]);
        }
        ReplayFailure::Transport { reason, .. } => {
            let _ = driver.write_console(driver.handle(), reason.as_bytes());
        }
    }
    driver_line(driver, b"");
}

fn driver_text_error(driver: &mut ServiceDomain, prefix: &[u8], detail: &[u8]) {
    if driver.write_console(driver.handle(), prefix).is_err()
        || driver.write_console(driver.handle(), detail).is_err()
    {
        fatal("console driver stopped while reporting an error");
    }
    driver_line(driver, b"");
}

fn driver_line(driver: &mut ServiceDomain, bytes: &[u8]) {
    if driver.write_console(driver.handle(), bytes).is_err()
        || driver.write_console(driver.handle(), b"\r\n").is_err()
    {
        fatal("console driver stopped");
    }
}

/// Block until the console driver delivers a byte. The driver never blocks;
/// the supervisor polls it, so each poll is one bounded domain entry.
fn read_byte(driver: &mut ServiceDomain) -> u8 {
    loop {
        match driver.read_console(driver.handle()) {
            Ok(Some(byte)) => return byte,
            Ok(None) => {}
            Err(_) => fatal("console driver stopped while reading input"),
        }
    }
}

fn echo(driver: &mut ServiceDomain, bytes: &[u8]) {
    if driver.write_console(driver.handle(), bytes).is_err() {
        fatal("console driver stopped while echoing input");
    }
}

fn read_line(driver: &mut ServiceDomain, buffer: &mut [u8]) -> usize {
    let mut length = 0;
    loop {
        match read_byte(driver) {
            b'\r' | b'\n' => {
                echo(driver, b"\r\n");
                return length;
            }
            8 | 127 if length > 0 => {
                length -= 1;
                echo(driver, b"\x08 \x08");
            }
            byte if (byte.is_ascii_graphic() || byte == b' ') && length < buffer.len() => {
                buffer[length] = byte;
                length += 1;
                echo(driver, &[byte]);
            }
            _ => {}
        }
    }
}

fn read_form(driver: &mut ServiceDomain, buffer: &mut [u8]) -> usize {
    let mut length = 0;
    loop {
        length += read_line(driver, &mut buffer[length..]);
        if !needs_more_input(&buffer[..length]) || length == buffer.len() {
            return length;
        }
        buffer[length] = b'\n';
        length += 1;
        echo(driver, b"             ... ");
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
