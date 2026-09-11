//! What both workshops do with programs and files: the serial one on the
//! research kernels and the graphical one on x86-64 parse their own command
//! lines, then come here with a console to answer on. Nothing here decides
//! what a command means; it runs the service protocols and reports.

use crate::arch;
use crate::process::{self, Console, Display, Line, Namespace, Services};
use crate::service::{ServiceDomain, ServiceError};
use crate::world::fs;
use core::fmt::Write;

pub enum FilesystemCommand<'a> {
    Format,
    MakeDirectory(&'a [u8]),
    List(&'a [u8]),
}

/// One line on the console, ended as the driver wants it.
pub fn line(console: &mut dyn Console, bytes: &[u8]) {
    console.write(bytes);
    console.write(b"\r\n");
}

/// A prefix and a detail on one line.
pub fn text(console: &mut dyn Console, prefix: &[u8], detail: &[u8]) {
    console.write(prefix);
    line(console, detail);
}

/// Report a filesystem service outcome; the values on success.
#[inline(never)]
fn filesystem_outcome(
    console: &mut dyn Console,
    outcome: Result<(u64, [u64; 3]), ServiceError>,
) -> Option<[u64; 3]> {
    match outcome {
        Ok((0, values)) => Some(values),
        Ok((status, _)) => {
            let mut out = Line::new();
            let _ = write!(out, "filesystem: error {status}");
            line(console, out.get());
            None
        }
        Err(error) => {
            text(console, b"filesystem service: ", error.name().as_bytes());
            None
        }
    }
}

/// Operator actions on the filesystem service, from the root with every
/// right: the operator is the namespace everything else is carved from.
#[inline(never)]
pub fn filesystem_command(
    storage: Option<&mut ServiceDomain>,
    filesystem: Option<&mut ServiceDomain>,
    console: &mut dyn Console,
    command: FilesystemCommand<'_>,
) {
    let (Some(storage), Some(filesystem)) = (storage, filesystem) else {
        line(console, b"denied: no filesystem service");
        return;
    };
    let handle = filesystem.handle();
    match command {
        FilesystemCommand::Format => {
            let outcome =
                filesystem.filesystem_request(handle, storage, fs::COMMAND_FORMAT, [0; 3]);
            if filesystem_outcome(console, outcome).is_some() {
                line(console, b"formatted");
            }
        }
        FilesystemCommand::MakeDirectory(path) => {
            filesystem.write_payload(path);
            let flags = fs::O_CREAT_BIT | fs::O_DIRECTORY_BIT;
            let outcome = filesystem.filesystem_request(
                handle,
                storage,
                fs::COMMAND_OPEN,
                [0, flags, path.len() as u64],
            );
            if filesystem_outcome(console, outcome).is_some() {
                text(console, b"directory ready: ", path);
            }
        }
        FilesystemCommand::List(path) => {
            filesystem.write_payload(path);
            let outcome = filesystem.filesystem_request(
                handle,
                storage,
                fs::COMMAND_OPEN,
                [0, fs::O_DIRECTORY_BIT, path.len() as u64],
            );
            let Some([directory, _, _]) = filesystem_outcome(console, outcome) else {
                return;
            };
            let mut position = 0;
            loop {
                let outcome = filesystem.filesystem_request(
                    handle,
                    storage,
                    fs::COMMAND_LIST,
                    [directory, position, 0],
                );
                let (entry_kind, length) = match outcome {
                    Ok((0, [_, entry_kind, length])) => (entry_kind, length),
                    Ok(_) => break,
                    Err(error) => {
                        text(console, b"filesystem service: ", error.name().as_bytes());
                        return;
                    }
                };
                let name_len = (filesystem.name_length() as usize).min(32);
                let mut name = [0_u8; 32];
                for (offset, byte) in name.iter_mut().enumerate().take(name_len) {
                    *byte = filesystem.read_payload(offset);
                }
                let mut out = Line::new();
                for byte in &name[..name_len] {
                    let _ = out.write_char(char::from(*byte));
                }
                if entry_kind == fs::KIND_DIRECTORY {
                    let _ = out.write_str("/");
                } else {
                    let _ = write!(out, "  {length} bytes");
                }
                line(console, out.get());
                position += 1;
            }
            if position == 0 {
                line(console, b"(empty)");
            }
        }
    }
}

/// `:exec NAME [ROOT] [ro] [-- ARG...]`: the program, the directory it sees
/// as `/`, named from the real root; whether it may only read; its
/// arguments, one per word. Runs it to its end, serving it and every
/// process it spawns, and reports how it ended.
#[cfg(feature = "isolated-repl")]
#[inline(never)]
pub fn exec_program(
    machine: &mut arch::Machine,
    storage: Option<&mut ServiceDomain>,
    filesystem: Option<&mut ServiceDomain>,
    console: &mut dyn Console,
    display: Option<&mut dyn Display>,
    rest: &[u8],
) {
    let Some(storage) = storage else {
        line(console, b"denied: no storage device");
        return;
    };
    let mut filesystem = filesystem;
    let mut display = display;
    let mut slot = process::RunSlot::UNINIT;
    let run = slot.prepare();
    if start_program(
        machine,
        storage,
        filesystem.as_deref_mut(),
        console,
        display
            .as_deref_mut()
            .map(|display| display as &mut dyn Display),
        rest,
        run,
    )
    .is_none()
    {
        return;
    }
    let mut services = Services {
        storage,
        console,
        filesystem,
        display: display.map(|display| &mut *display as &mut dyn Display),
    };
    let exit = loop {
        match process::step_run(machine, &mut services, run) {
            process::Progress::Running => {}
            process::Progress::Listening => {
                // Nothing here delivers events between passes: a process
                // that waits for one on this path waits for nothing.
                break process::abandon(machine, &mut services, run);
            }
            process::Progress::Ended(exit) => break exit,
        }
    };
    finish_program(machine, services.console, run, exit);
}

/// Parse an `:exec` line, resolve the root, find the program and start
/// it in the prepared `run`: `Some` when it runs, or nothing, with the
/// reason already on the console.
#[inline(never)]
pub fn start_program(
    machine: &mut arch::Machine,
    storage: &mut ServiceDomain,
    filesystem: Option<&mut ServiceDomain>,
    console: &mut dyn Console,
    display: Option<&mut dyn Display>,
    rest: &[u8],
    run: &mut process::Run,
) -> Option<()> {
    let (options, arguments) = match rest.windows(2).position(|pair| pair == b"--") {
        Some(at)
            if (at == 0 || rest[at - 1] == b' ')
                && (at + 2 == rest.len() || rest[at + 2] == b' ') =>
        {
            (&rest[..at], &rest[at + 2..])
        }
        _ => (rest, &rest[rest.len()..]),
    };
    let mut words = options
        .split(|byte| *byte == b' ')
        .filter(|word| !word.is_empty());
    let Some(name) = words.next() else {
        line(console, b"usage: :exec NAME [ROOT] [ro] [-- ARG...]");
        return None;
    };
    let mut root_path = words.next();
    let mut read_only = false;
    if root_path == Some(b"ro") {
        root_path = None;
        read_only = true;
    } else if words.next() == Some(b"ro") {
        read_only = true;
    }
    let mut block = [0_u8; crate::world::PAYLOAD_BYTES];
    let mut block_length = 0;
    for word in arguments
        .split(|byte| *byte == b' ')
        .filter(|word| !word.is_empty())
    {
        for byte in word.iter().chain(&[0]) {
            if block_length < block.len() {
                block[block_length] = *byte;
                block_length += 1;
            }
        }
    }
    let mut filesystem = filesystem;
    // No ROOT names the filesystem's root, entry 0, which is the root by
    // construction: a program that never opens a file runs whether or not
    // the region is formatted, and one that does is answered by the
    // service. A named ROOT is resolved now, so a bad one is refused here.
    let root = match (filesystem.as_deref_mut(), root_path) {
        (Some(service), Some(root_path)) => {
            service.write_payload(root_path);
            let handle = service.handle();
            let outcome = service.filesystem_request(
                handle,
                storage,
                fs::COMMAND_OPEN,
                [0, fs::O_DIRECTORY_BIT, root_path.len() as u64],
            );
            let [entry, _, _] = filesystem_outcome(console, outcome)?;
            entry as u16
        }
        _ => 0,
    };
    let namespace = if read_only {
        Namespace::read_only(root)
    } else {
        Namespace::all(root)
    };
    let program = match process::find(storage, name) {
        Ok(Some(program)) => program,
        Ok(None) => {
            text(console, b"no program named ", name);
            return None;
        }
        Err(reason) => {
            text(console, b"program table unreadable: ", reason.as_bytes());
            return None;
        }
    };
    let mut services = Services {
        storage,
        console,
        filesystem,
        // Reborrowed, so the trait object's lifetime is the table's.
        display: display.map(|display| &mut *display as &mut dyn Display),
    };
    match process::start(
        machine,
        &mut services,
        program,
        name,
        &block[..block_length],
        namespace,
        run,
    ) {
        Ok(()) => Some(()),
        Err(reason) => {
            text(
                services.console,
                b"cannot load program: ",
                reason.as_bytes(),
            );
            None
        }
    }
}

/// The one line for how a run's first process ended, and its frames back.
pub fn finish_program(
    machine: &mut arch::Machine,
    console: &mut dyn Console,
    run: &mut process::Run,
    exit: process::Exit,
) {
    let mut out = Line::new();
    let _ = out.write_str("process ");
    for byte in run.name() {
        let _ = out.write_char(char::from(*byte));
    }
    process::report(&mut out, exit);
    console.write(out.get());
    process::finish(machine, run);
}
