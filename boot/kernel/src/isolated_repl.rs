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
use crate::native_session::{
    replay as replay_workspace, request as evaluator_request_raw, reset as reset_evaluator,
    ReplayFailure,
};
#[cfg(target_arch = "x86_64")]
use crate::recovery::{slot_name, Admission, KernelRecovery};
use crate::recovery::{BootPlan, LiveRecovery};
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
    // A machine without a disk runs the same workshop with an in-memory
    // workspace and says so rather than pretending to persist.
    let mut storage = {
        let storage_entry = crate::user::agel_storage_main as *const () as usize as u64;
        match machine.create_storage_world(storage_entry, crate::world::STORAGE_TICKS) {
            Ok(domain) => Some(ServiceDomain::new(
                domain,
                ServiceKind::Storage,
                storage_entry,
                crate::world::STORAGE_TICKS,
            )),
            Err(reason) => {
                driver_text_error(&mut driver, b"storage: ", reason.as_bytes());
                None
            }
        }
    };
    let mut evaluator = match machine.create_evaluator_world(evaluator_entry, 20) {
        Ok(domain) => domain,
        Err(reason) => fatal(reason),
    };
    let mut recovery = match storage.as_mut().map(LiveRecovery::load) {
        Some(Ok(recovery)) => Some(recovery),
        Some(Err(reason)) => {
            driver_text_error(
                &mut driver,
                b"recovery record unavailable: ",
                reason.as_bytes(),
            );
            None
        }
        None => None,
    };
    #[cfg(target_arch = "x86_64")]
    let mut kernel = match storage.as_mut().map(KernelRecovery::load) {
        Some(Ok(kernel)) => Some(kernel),
        Some(Err(reason)) => {
            driver_text_error(
                &mut driver,
                b"kernel selector unavailable: ",
                reason.as_bytes(),
            );
            None
        }
        None => None,
    };
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
    #[cfg(target_arch = "x86_64")]
    if let (Some(kernel), Some(storage)) = (kernel.as_mut(), storage.as_mut()) {
        match kernel.admit(storage) {
            Ok(Admission::Nothing) => {}
            Ok(Admission::Admitted(slot)) => {
                let mut out = ServiceWriter::new(&mut driver);
                let _ = writeln!(
                    out,
                    "candidate kernel slot {} admitted: signature verified against the kernel's trust key; next boot tries it",
                    slot_name(slot)
                );
                out.flush();
            }
            Ok(Admission::Refused(slot, reason)) => {
                let mut out = ServiceWriter::new(&mut driver);
                let _ = writeln!(
                    out,
                    "candidate kernel slot {} refused: {reason}; slot cleared",
                    slot_name(slot)
                );
                out.flush();
            }
            Err(reason) => driver_text_error(
                &mut driver,
                b"candidate kernel could not be checked: ",
                reason.as_bytes(),
            ),
        }
    }
    #[cfg(target_arch = "x86_64")]
    kernel_status(&mut driver, kernel.as_ref(), true);

    match load_replay_candidates(
        &mut evaluator,
        &mut driver,
        storage.as_mut(),
        recovery.as_mut(),
    ) {
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
                b"forms: quote if begin def fn | builtins: + - * / = < eval | agents: spawn send step run inspect/restart | workspace: :edit NAME :run NAME :show NAME :delete NAME :cells :workspace :save :reload | recovery: :revision :rollback :defs :limits :recovery-status :verify :promote :fault :kernel-status :kernel-promote :kernel-fault :cut-power N :shutdown",
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
                recovery.as_ref().map_or(0, |r| r.record().trusted),
            ) {
                    Ok((next_generation, candidate_revision)) => {
                        generation = next_generation;
                        committed_workspace = workspace;
                        dirty = false;
                        revision = candidate_revision;
                        report_saved(&mut driver, workspace.count(), generation);
                        if let (Some(recovery), Some(storage)) = (recovery.as_mut(), storage.as_mut()) {
                            if let Err(reason) = recovery.on_saved(storage, generation) {
                                driver_text_error(
                                    &mut driver,
                                    b"recovery record not updated: ",
                                    reason.as_bytes(),
                                );
                            }
                        }
                    }
                    Err(reason) => {
                        driver_text_error(
                            &mut driver,
                            b"workspace save failed: ",
                            reason.as_bytes(),
                        );
                    }
            },
            b":reload" => match load_replay_candidates(
                &mut evaluator,
                &mut driver,
                storage.as_mut(),
                None,
            ) {
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
            #[cfg(target_arch = "x86_64")]
            b":kernel-status" => kernel_status(&mut driver, kernel.as_ref(), false),
            #[cfg(target_arch = "x86_64")]
            b":kernel-promote" => match (storage.as_mut(), kernel.as_mut()) {
                (Some(storage), Some(kernel)) => match kernel.promote(storage) {
                    Ok((selected, previous)) => {
                        let mut out = ServiceWriter::new(&mut driver);
                        let _ = writeln!(
                            out,
                            "selected kernel slot {}; slot {} retained for rollback",
                            slot_name(selected),
                            slot_name(previous)
                        );
                        out.flush();
                    }
                    Err(reason) => driver_line(&mut driver, reason.as_bytes()),
                },
                _ => driver_line(&mut driver, b"denied: kernel selector unavailable"),
            },
            #[cfg(target_arch = "x86_64")]
            b":kernel-fault" => match (storage.as_mut(), kernel.as_mut()) {
                (Some(storage), Some(kernel)) => match kernel.fault(storage) {
                    Ok(trusted) => {
                        let mut out = ServiceWriter::new(&mut driver);
                        let _ = writeln!(
                            out,
                            "watchdog fault: candidate kernel slot {} given up; next boot loads trusted slot {}",
                            slot_name(kernel.selector().candidate),
                            slot_name(trusted)
                        );
                        out.flush();
                    }
                    Err(reason) => driver_line(&mut driver, reason.as_bytes()),
                },
                _ => driver_line(&mut driver, b"denied: kernel selector unavailable"),
            },
            b":recovery-status" => recovery_status(&mut driver, recovery.as_ref()),
            b":verify" => recovery_verify(
                &mut driver,
                &mut evaluator,
                &workspace,
                storage.as_mut(),
                recovery.as_mut(),
            ),
            b":promote" => recovery_promote(&mut driver, storage.as_mut(), recovery.as_mut()),
            b":fault" => {
                let target = match (storage.as_mut(), recovery.as_mut()) {
                    (Some(storage), Some(recovery)) => recovery.fault(storage),
                    _ => Err("denied: recovery record unavailable"),
                };
                match target {
                    Ok(trusted) => {
                        match load_generation(&mut evaluator, storage.as_mut(), trusted) {
                            Ok(Some((loaded, restored_revision))) => {
                                workspace = loaded.workspace;
                                committed_workspace = loaded.workspace;
                                generation = loaded.generation;
                                revision = restored_revision;
                                dirty = false;
                                let mut out = ServiceWriter::new(&mut driver);
                                let _ = writeln!(out, "watchdog fault: rolled back to generation {trusted}");
                                out.flush();
                            }
                            Ok(None) => driver_line(
                                &mut driver,
                                b"watchdog fault: trusted generation is not on disk; live world retained",
                            ),
                            Err(reason) => driver_text_error(
                                &mut driver,
                                b"watchdog fault: rollback failed: ",
                                reason.as_bytes(),
                            ),
                        }
                    }
                    Err(reason) => driver_line(&mut driver, reason.as_bytes()),
                }
            }
            b":shutdown" => arch::exit(true),
            _ if source.starts_with(b":cut-power ") => {
                let count = command_argument(source, b":cut-power ")
                    .and_then(|text| core::str::from_utf8(text).ok())
                    .and_then(|text| text.parse::<u32>().ok());
                match (count, storage.as_mut()) {
                    (Some(count), Some(storage)) if count > 0 => {
                        storage.cut_power_after(count);
                        let mut out = ServiceWriter::new(&mut driver);
                        let _ = writeln!(
                            out,
                            "power cut armed: sector write {count} will be torn and the machine halted"
                        );
                        out.flush();
                    }
                    (Some(_), None) => driver_line(&mut driver, b"denied: no storage device"),
                    _ => driver_line(&mut driver, b"usage: :cut-power N (N >= 1)"),
                }
            }
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
                    let before = revision;
                    revision = evaluator_request(
                        &mut evaluator,
                        &mut driver,
                        shared::COMMAND_EVALUATE,
                        source,
                    );
                    // A form evaluated after boot is the health oracle every
                    // generation gets for free: the system reached an
                    // interactive, working state.
                    if revision > before {
                        if let (Some(recovery), Some(storage)) =
                            (recovery.as_mut(), storage.as_mut())
                        {
                            if let Ok(Some(verified)) = recovery.healthy(storage, generation) {
                                let mut out = ServiceWriter::new(&mut driver);
                                let _ = writeln!(
                                    out,
                                    "candidate generation {verified} verified by a healthy boot"
                                );
                                out.flush();
                            }
                        }
                        #[cfg(target_arch = "x86_64")]
                        if let (Some(kernel), Some(storage)) = (kernel.as_mut(), storage.as_mut())
                        {
                            if let Ok(Some(slot)) = kernel.healthy(storage) {
                                let mut out = ServiceWriter::new(&mut driver);
                                let _ = writeln!(
                                    out,
                                    "kernel slot {} verified by a healthy boot",
                                    slot_name(slot)
                                );
                                out.flush();
                            }
                        }
                    }
                }
            }
        }
    }
}

// Boxing would introduce an allocator into the native supervisor. The large
// variant is a deliberately bounded source workspace on its 512 KiB stack.
#[allow(clippy::large_enum_variant)]
enum DiskWorkspace {
    Empty,
    Restored(crate::workspace::LoadedWorkspace, u64),
    Rejected(u64),
}

fn load_replay_candidates(
    evaluator: &mut arch::Domain,
    driver: &mut ServiceDomain,
    storage: Option<&mut ServiceDomain>,
    recovery: Option<&mut LiveRecovery>,
) -> Result<DiskWorkspace, &'static str> {
    let Some(storage) = storage else {
        return Err("no storage device on this machine");
    };
    load_from_disk(evaluator, driver, storage, recovery)
}

/// Replay exactly one generation from disk, if it is there and valid.
fn load_generation(
    evaluator: &mut arch::Domain,
    storage: Option<&mut ServiceDomain>,
    generation: u64,
) -> Result<Option<(crate::workspace::LoadedWorkspace, u64)>, &'static str> {
    let storage = storage.ok_or("no storage device on this machine")?;
    let candidates = crate::workspace::load(storage)?;
    let Some(loaded) = candidates
        .into_iter()
        .flatten()
        .find(|loaded| loaded.generation == generation)
    else {
        return Ok(None);
    };
    match replay_workspace(evaluator, &loaded.workspace) {
        Ok(revision) => Ok(Some((loaded, revision))),
        Err(_) => Err("trusted generation failed to replay"),
    }
}

fn load_from_disk(
    evaluator: &mut arch::Domain,
    driver: &mut ServiceDomain,
    storage: &mut ServiceDomain,
    recovery: Option<&mut LiveRecovery>,
) -> Result<DiskWorkspace, &'static str> {
    let candidates = crate::workspace::load(storage)?;
    let newest = candidates
        .iter()
        .flatten()
        .map(|loaded| loaded.generation)
        .max()
        .unwrap_or(0);
    if let Some(recovery) = recovery {
        if let BootPlan::Rollback {
            trusted,
            candidate,
            attempts,
        } = recovery.plan_boot(storage, newest)?
        {
            {
                let mut out = ServiceWriter::new(driver);
                let _ = writeln!(
                    out,
                    "watchdog fault: candidate generation {candidate} failed {attempts} boots; rolling back to generation {trusted}"
                );
                out.flush();
            }
            if let Some(loaded) = candidates
                .into_iter()
                .flatten()
                .find(|loaded| loaded.generation == trusted)
            {
                match replay_workspace(evaluator, &loaded.workspace) {
                    Ok(revision) => {
                        recovery.note_rollback();
                        return Ok(DiskWorkspace::Restored(loaded, revision));
                    }
                    Err(failure) => {
                        report_replay_failure(driver, &loaded.workspace, failure, evaluator);
                    }
                }
            }
            driver_line(
                driver,
                b"trusted generation unavailable; trying the newest generation instead",
            );
        }
    }
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

/// One line for the kernel's own A/B state, printed at boot and on demand.
#[cfg(target_arch = "x86_64")]
fn kernel_status(driver: &mut ServiceDomain, kernel: Option<&KernelRecovery>, booting: bool) {
    let Some(kernel) = kernel else {
        driver_line(driver, b"kernel: selector unavailable");
        return;
    };
    let Some(booted) = kernel.booted() else {
        driver_line(
            driver,
            b"kernel: boot stage has no slot selector; slot A loaded",
        );
        return;
    };
    let selector = kernel.selector();
    let mut out = ServiceWriter::new(driver);
    if booting && kernel.rolled_back() {
        let _ = writeln!(
            out,
            "watchdog fault: candidate kernel slot {} failed {} boots; booted trusted slot {}",
            slot_name(selector.candidate),
            selector.attempts,
            slot_name(selector.trusted)
        );
    }
    let _ = write!(
        out,
        "kernel: running slot {}; trusted slot {}",
        slot_name(booted),
        slot_name(selector.trusted)
    );
    if selector.candidate == crate::workspace::NO_CANDIDATE {
        let _ = write!(out, "; no candidate");
    } else if !selector.admitted {
        let _ = write!(
            out,
            "; candidate slot {} (staged, not admitted)",
            slot_name(selector.candidate)
        );
    } else {
        let _ = write!(
            out,
            "; candidate slot {} ({}, boots {})",
            slot_name(selector.candidate),
            if selector.verified {
                "verified"
            } else {
                "unverified"
            },
            selector.attempts
        );
    }
    if kernel.rolled_back() {
        let _ = write!(out, "; running trusted slot after watchdog rollback");
    }
    let _ = writeln!(out);
    out.flush();
}

fn recovery_status(driver: &mut ServiceDomain, recovery: Option<&LiveRecovery>) {
    let Some(recovery) = recovery else {
        driver_line(driver, b"recovery: record unavailable");
        return;
    };
    let record = recovery.record();
    let mut out = ServiceWriter::new(driver);
    if record.trusted == 0 && record.candidate == 0 {
        let _ = writeln!(out, "recovery: no generation trusted or proposed");
    } else {
        let _ = write!(out, "recovery: trusted generation {}", record.trusted);
        if record.candidate == 0 {
            let _ = write!(out, "; no candidate");
        } else {
            let _ = write!(
                out,
                "; candidate generation {} ({}, boots {})",
                record.candidate,
                if record.verified {
                    "verified"
                } else {
                    "unverified"
                },
                record.attempts
            );
        }
        if recovery.rolled_back() {
            let _ = write!(out, "; running trusted generation after watchdog rollback");
        }
        let _ = writeln!(out);
    }
    out.flush();
}

/// Explicit health evidence: a source cell named `health` must evaluate
/// without error in an isolated candidate world that is then discarded.
fn recovery_verify(
    driver: &mut ServiceDomain,
    evaluator: &mut arch::Domain,
    workspace: &Workspace,
    storage: Option<&mut ServiceDomain>,
    recovery: Option<&mut LiveRecovery>,
) {
    let (Some(storage), Some(recovery)) = (storage, recovery) else {
        driver_line(driver, b"denied: recovery record unavailable");
        return;
    };
    if recovery.record().candidate == 0 {
        driver_line(driver, b"denied: no candidate generation to verify");
        return;
    }
    if let Some(health) = workspace.find(b"health") {
        let reply = evaluator_request_raw(
            evaluator,
            shared::COMMAND_EVALUATOR_PREVIEW,
            health.source(),
        );
        let _ = evaluator_request_raw(evaluator, shared::COMMAND_EVALUATOR_DISCARD, b"");
        match reply {
            Ok(reply) if !reply.error => {}
            Ok(reply) => {
                let mut out = ServiceWriter::new(driver);
                let _ = write!(
                    out,
                    "candidate generation {}: health cell rejected: ",
                    recovery.record().candidate
                );
                out.flush();
                let _ = driver.write_console(driver.handle(), &reply.bytes[..reply.length]);
                driver_line(driver, b"");
                return;
            }
            Err(reason) => {
                driver_text_error(driver, b"health cell could not run: ", reason.as_bytes());
                return;
            }
        }
    }
    match recovery.verify(storage) {
        Ok(candidate) => {
            let mut out = ServiceWriter::new(driver);
            let _ = writeln!(
                out,
                "candidate generation {candidate}: isolated health evidence accepted"
            );
            out.flush();
        }
        Err(reason) => driver_line(driver, reason.as_bytes()),
    }
}

fn recovery_promote(
    driver: &mut ServiceDomain,
    storage: Option<&mut ServiceDomain>,
    recovery: Option<&mut LiveRecovery>,
) {
    let (Some(storage), Some(recovery)) = (storage, recovery) else {
        driver_line(driver, b"denied: recovery record unavailable");
        return;
    };
    match recovery.promote(storage) {
        Ok((selected, previous)) => {
            let mut out = ServiceWriter::new(driver);
            if previous == 0 {
                let _ = writeln!(
                    out,
                    "selected generation {selected}; no earlier generation to retain"
                );
            } else {
                let _ = writeln!(
                    out,
                    "selected generation {selected}; generation {previous} retained for rollback"
                );
            }
            out.flush();
        }
        Err(reason) => driver_line(driver, reason.as_bytes()),
    }
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
