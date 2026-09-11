//! Loadable processes: the first stratum of the POSIX personality.
//!
//! A process is an ordinary protection domain whose code did not come from
//! the kernel image: the supervisor reads a static ELF from the disk's
//! program region, maps its segments into a fresh domain with the rights the
//! segments ask for (never writable and executable together), and enters it
//! the way it enters every world, with the shared page in the first argument
//! register. The process asks for things by filling a request block in that
//! page and yielding, exactly as a driver does; the supervisor answers, and
//! `exit` is a request like any other.
//!
//! Nothing here is a contract operation, and nothing here is a path: the
//! program region is a table of names the supervisor owns, and a process
//! reaches the console because the supervisor's policy for descriptors 1 and
//! 2 says so. Namespace capabilities and files come in later strata.

use crate::arch;
use crate::memory::Access;
use crate::region::{Entry, Region};
use crate::service::{ServiceDomain, ServiceError, ServiceHandle};
use crate::workspace::read_sector;
use crate::world::{fs, process, Fault, Stop, BLOCK_BYTES, PAYLOAD_BYTES};
use core::fmt::Write;

/// What a process may do with names: read files, write them, create them.
/// Granted at `exec` by the operator; a process never widens it, and a
/// child's is the parent's or narrower.
#[derive(Clone, Copy)]
pub struct Namespace {
    /// The directory entry the process sees as `/`; it cannot climb above.
    pub root: u16,
    pub read: bool,
    pub write: bool,
    pub create: bool,
}

impl Namespace {
    pub const fn all(root: u16) -> Self {
        Self {
            root,
            read: true,
            write: true,
            create: true,
        }
    }

    pub const fn read_only(root: u16) -> Self {
        Self {
            root,
            read: true,
            write: false,
            create: false,
        }
    }
}

/// What a descriptor number names.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Free,
    Console,
    File,
    PipeRead,
    PipeWrite,
}

/// One entry of a process's descriptor table. A file descriptor is derived
/// from the namespace, never wider than it, and bound to the filesystem
/// service's generation so a restart fails it closed; a pipe end names one
/// of the supervisor's pipes.
#[derive(Clone, Copy)]
struct Descriptor {
    kind: Kind,
    entry: u16,
    pipe: u8,
    offset: u64,
    /// The file's length as last seen, for `O_APPEND` and seeking to the end.
    length: u64,
    readable: bool,
    writable: bool,
    handle: ServiceHandle,
}

impl Descriptor {
    const FREE: Self = Self {
        kind: Kind::Free,
        entry: 0,
        pipe: 0,
        offset: 0,
        length: 0,
        readable: false,
        writable: false,
        handle: ServiceHandle::NONE,
    };
    const CONSOLE: Self = Self {
        kind: Kind::Console,
        writable: true,
        ..Self::FREE
    };
}

const EIO: i64 = 5;
const EBADF: i64 = 9;
const ECHILD: i64 = 10;
const ENODEV: i64 = 19;
const EAGAIN: i64 = 11;
const EACCES: i64 = 13;
const ENFILE: i64 = 23;
const EMFILE: i64 = 24;
const EINVAL: i64 = 22;
const ESPIPE: i64 = 29;
const EPIPE: i64 = 32;
const ENOSYS: i64 = 38;
const ESTALE: i64 = 116;

/// The program region: a table sector followed by the programs it names.
pub const TABLE_SECTOR: u32 = 2048;
/// Last sector of the program region, inclusive.
pub const LAST_SECTOR: u32 = 3071;
const REGION: Region = Region {
    table: TABLE_SECTOR,
    last: LAST_SECTOR,
    magic: b"AGELPR1\0",
};
/// Program names are short and ASCII; the table pads them with zeros.
pub const NAME_BYTES: usize = crate::region::NAME_BYTES;
const MAX_SEGMENTS: usize = 8;
/// Pages a process may be built from, code, data and zero fill together.
const MAX_PAGES: usize = 128;
const PAGE: u64 = 4096;

/// One row of the program table.
pub type Program = Entry;

/// Look a program up by name.
pub fn find(storage: &mut ServiceDomain, name: &[u8]) -> Result<Option<Program>, &'static str> {
    REGION.find(storage, name)
}

/// How a process ended.
#[derive(Clone, Copy)]
pub enum Exit {
    /// It asked to leave with this status.
    Status(u64),
    /// The machine stopped it.
    Faulted(Fault),
    /// It never yielded inside one entry's tick budget.
    BudgetExhausted,
    /// It was blocked on a wait or a pipe that nothing left could answer.
    Blocked,
}

#[derive(Clone, Copy)]
struct Segment {
    offset: u64,
    vaddr: u64,
    filesz: u64,
    memsz: u64,
    access: Access,
}

/// Read `length` bytes of the program image at `offset` into `out`.
fn read_image(
    storage: &mut ServiceDomain,
    program: Program,
    offset: u64,
    out: &mut [u8],
) -> Result<(), &'static str> {
    let end = offset
        .checked_add(out.len() as u64)
        .ok_or("program image offset overflows")?;
    if end > u64::from(program.length) {
        return Err("program image is truncated");
    }
    let mut sector = [0_u8; 512];
    let mut done = 0_usize;
    while done < out.len() {
        let position = offset + done as u64;
        let lba = program.start + (position / 512) as u32;
        let inside = (position % 512) as usize;
        read_sector(storage, lba, &mut sector)?;
        let take = (512 - inside).min(out.len() - done);
        out[done..done + take].copy_from_slice(&sector[inside..inside + take]);
        done += take;
    }
    Ok(())
}

/// Load `program` into a fresh domain and return it with its entry point.
fn load(
    machine: &mut arch::Machine,
    storage: &mut ServiceDomain,
    program: Program,
) -> Result<arch::Domain, &'static str> {
    // The checksum is accidental-corruption detection over the whole image,
    // read once here, before any of it is trusted to describe itself.
    {
        let mut crc = crate::workspace::Crc::new();
        let mut sector = [0_u8; 512];
        let mut remaining = program.length as usize;
        let mut lba = program.start;
        while remaining > 0 {
            read_sector(storage, lba, &mut sector)?;
            let take = remaining.min(512);
            crc.update(&sector[..take]);
            remaining -= take;
            lba += 1;
        }
        if crc.finish() != program.checksum {
            return Err("program image checksum does not match its table entry");
        }
    }
    let mut header = [0_u8; 64];
    read_image(storage, program, 0, &mut header)?;
    if header[..4] != [0x7f, b'E', b'L', b'F'] || header[4] != 2 || header[5] != 1 {
        return Err("program is not a little-endian ELF64 image");
    }
    if u16::from_le_bytes([header[16], header[17]]) != 2 {
        return Err("program is not a static executable");
    }
    if u16::from_le_bytes([header[18], header[19]]) != arch::ELF_MACHINE {
        return Err("program was built for another machine");
    }
    let entry = u64::from_le_bytes(header[24..32].try_into().map_err(|_| "malformed")?);
    let phoff = u64::from_le_bytes(header[32..40].try_into().map_err(|_| "malformed")?);
    let phentsize = usize::from(u16::from_le_bytes([header[54], header[55]]));
    let phnum = usize::from(u16::from_le_bytes([header[56], header[57]]));
    if phentsize != 56 || phnum == 0 || phnum > MAX_SEGMENTS {
        return Err("program has an unsupported program-header table");
    }
    let mut segments = [Segment {
        offset: 0,
        vaddr: 0,
        filesz: 0,
        memsz: 0,
        access: Access::UserReadOnly,
    }; MAX_SEGMENTS];
    let mut count = 0;
    let window = arch::PROCESS_BASE..arch::PROCESS_BASE + arch::PROCESS_BYTES;
    for index in 0..phnum {
        let mut raw = [0_u8; 56];
        read_image(storage, program, phoff + (index * 56) as u64, &mut raw)?;
        let kind = u32::from_le_bytes(raw[0..4].try_into().map_err(|_| "malformed")?);
        if kind != 1 {
            continue;
        }
        let flags = u32::from_le_bytes(raw[4..8].try_into().map_err(|_| "malformed")?);
        let offset = u64::from_le_bytes(raw[8..16].try_into().map_err(|_| "malformed")?);
        let vaddr = u64::from_le_bytes(raw[16..24].try_into().map_err(|_| "malformed")?);
        let filesz = u64::from_le_bytes(raw[32..40].try_into().map_err(|_| "malformed")?);
        let memsz = u64::from_le_bytes(raw[40..48].try_into().map_err(|_| "malformed")?);
        if memsz == 0 {
            continue;
        }
        if flags & 0b011 == 0b011 {
            return Err("a segment asks to be writable and executable");
        }
        let end = vaddr.checked_add(memsz).ok_or("segment overflows")?;
        if !window.contains(&vaddr) || end > window.end || filesz > memsz {
            return Err("a segment lies outside the process window");
        }
        if vaddr % PAGE != offset % PAGE {
            return Err("a segment is not page-congruent with its file offset");
        }
        let access = if flags & 1 != 0 {
            Access::UserCode
        } else if flags & 2 != 0 {
            Access::UserData
        } else {
            Access::UserReadOnly
        };
        segments[count] = Segment {
            offset,
            vaddr,
            filesz,
            memsz,
            access,
        };
        count += 1;
    }
    if count == 0 {
        return Err("program has no loadable segment");
    }
    if !window.contains(&entry) {
        return Err("program entry is outside its segments' window");
    }
    let mut domain = machine.create_process_world(entry)?;
    let mut mapped = [0_u64; MAX_PAGES];
    let mut pages = 0_usize;
    for segment in &segments[..count] {
        let first = segment.vaddr & !(PAGE - 1);
        let last = (segment.vaddr + segment.memsz - 1) & !(PAGE - 1);
        let mut page = first;
        while page <= last {
            if mapped[..pages].contains(&page) {
                return Err("two segments share a page");
            }
            if pages == MAX_PAGES {
                return Err("program needs more pages than a process may hold");
            }
            let frame = machine.map_process_page(&mut domain, page, segment.access)?;
            mapped[pages] = page;
            pages += 1;
            // The bytes of this page that the file supplies; the rest of the
            // frame is already zero.
            let from = page.max(segment.vaddr);
            let to = (page + PAGE).min(segment.vaddr + segment.filesz);
            if from < to {
                let mut chunk = [0_u8; 512];
                let mut at = from;
                while at < to {
                    let take = ((to - at) as usize).min(512);
                    read_image(
                        storage,
                        program,
                        segment.offset + (at - segment.vaddr),
                        &mut chunk[..take],
                    )?;
                    // Safety: the frame is identity mapped for the supervisor
                    // and belongs to the domain being built, which has not run.
                    unsafe {
                        core::ptr::copy_nonoverlapping(
                            chunk.as_ptr(),
                            (frame + (at - page)) as *mut u8,
                            take,
                        )
                    };
                    at += take as u64;
                }
            }
            page += PAGE;
        }
    }
    Ok(domain)
}

/// Where a process's console output goes: the serial console driver, a
/// terminal on the desktop, or both.
pub trait Console {
    fn write(&mut self, bytes: &[u8]);
}

impl Console for ServiceDomain {
    /// The console driver domain, a payload at a time; a driver that has
    /// stopped loses the text, which the workshop reports on its own path.
    fn write(&mut self, bytes: &[u8]) {
        let handle = self.handle();
        for chunk in bytes.chunks(PAYLOAD_BYTES) {
            if self.write_console(handle, chunk).is_err() {
                return;
            }
        }
    }
}

/// A line of text assembled with `core::fmt` before it is written whole.
pub struct Line {
    bytes: [u8; 160],
    length: usize,
}

impl Line {
    pub const fn new() -> Self {
        Self {
            bytes: [0; 160],
            length: 0,
        }
    }

    pub fn get(&self) -> &[u8] {
        &self.bytes[..self.length]
    }
}

impl Default for Line {
    fn default() -> Self {
        Self::new()
    }
}

impl Write for Line {
    fn write_str(&mut self, text: &str) -> core::fmt::Result {
        for byte in text.bytes() {
            if self.length < self.bytes.len() {
                self.bytes[self.length] = byte;
                self.length += 1;
            }
        }
        Ok(())
    }
}

/// The services a process's requests are answered through.
pub struct Services<'a> {
    pub storage: &'a mut ServiceDomain,
    pub console: &'a mut dyn Console,
    /// Absent when the machine has no filesystem service; every file request
    /// then answers `-ENOSYS`.
    pub filesystem: Option<&'a mut ServiceDomain>,
    /// Absent where there is no desktop; a window request then answers
    /// `-ENODEV`.
    pub display: Option<&'a mut dyn Display>,
}

/// A compositor record as a process hands it over: 64 bytes.
pub const RECORD_BYTES: usize = 64;

/// The desktop's side of windows: what serves `WINDOW` and `DRAW`. The
/// implementation owns the windows and the policy of what may be drawn;
/// the process table only relays a process's slot as the owner.
pub trait Display {
    /// A window for process `owner`: its number, or a negated error.
    fn open(&mut self, owner: usize, width: u32, height: u32, title: &[u8]) -> i64;
    /// Records into a window `owner` owns: the count it holds, or a
    /// negated error and nothing drawn.
    fn draw(
        &mut self,
        owner: usize,
        window: u64,
        flags: u64,
        records: &[[u8; RECORD_BYTES]],
    ) -> i64;
    /// Process `owner` has ended: what it owned stays on the desktop, but
    /// no later process in that slot may draw into it.
    fn release(&mut self, owner: usize);
}

/// Why a process is not running.
#[derive(Clone, Copy)]
enum State {
    Runnable,
    /// Waiting for the child in this slot to end.
    Waiting(usize),
    /// A read or write on a pipe that could not complete yet; retried on
    /// every pass until it can.
    Reading {
        descriptor: u64,
        length: u64,
    },
    Writing {
        descriptor: u64,
        length: u64,
    },
    Ended(Exit),
}

/// A running process: its domain, what it may name, what it holds.
struct Process {
    domain: arch::Domain,
    descriptors: [Descriptor; process::DESCRIPTORS],
    namespace: Namespace,
    /// The slot of the process that spawned it; the `:exec`'d one has none.
    parent: Option<usize>,
    state: State,
    name: [u8; NAME_BYTES],
    name_length: usize,
}

/// A pipe: a bounded queue in the supervisor with a count of the read and
/// write ends that name it, so the last writer closing is the end of the
/// stream and a write with no reader is `EPIPE`.
#[derive(Clone, Copy)]
struct Pipe {
    used: bool,
    buffer: [u8; process::PIPE_BYTES],
    head: usize,
    length: usize,
    readers: u8,
    writers: u8,
}

impl Pipe {
    const EMPTY: Self = Self {
        used: false,
        buffer: [0; process::PIPE_BYTES],
        head: 0,
        length: 0,
        readers: 0,
        writers: 0,
    };
}

/// The process table for one `:exec`: the process the operator started and
/// every descendant it makes, all served here until the last has ended.
struct Table {
    processes: [Option<Process>; process::PROCESSES],
    pipes: [Pipe; process::PIPES],
}

/// Load and run the named program to its end, serving its requests inside
/// `namespace`, and the requests of every process it spawns. Returns how
/// the first process ended once every process has; frames go back to the
/// pool as each process ends.
#[inline(never)]
pub fn exec(
    machine: &mut arch::Machine,
    services: &mut Services<'_>,
    program: Program,
    name: &[u8],
    arguments: &[u8],
    namespace: Namespace,
) -> Result<Exit, &'static str> {
    let mut domain = load(machine, services.storage, program)?;
    // The argument block: the name, then each argument, each NUL-terminated,
    // in the payload area, with the count in its word.
    let mut block = [0_u8; PAYLOAD_BYTES];
    let mut used = 0;
    for byte in name.iter().chain(&[0]).chain(arguments) {
        if used < PAYLOAD_BYTES {
            block[used] = *byte;
            used += 1;
        }
    }
    place_arguments(&mut domain, &block[..used]);
    let mut table = Table {
        processes: [None, None, None, None],
        pipes: [Pipe::EMPTY; process::PIPES],
    };
    let mut descriptors = [Descriptor::FREE; process::DESCRIPTORS];
    descriptors[1] = Descriptor::CONSOLE;
    descriptors[2] = Descriptor::CONSOLE;
    table.processes[0] = Some(Process {
        domain,
        descriptors,
        namespace,
        parent: None,
        state: State::Runnable,
        name: padded_name(name),
        name_length: name.len().min(NAME_BYTES),
    });
    let outcome = serve(machine, services, &mut table);
    // Whatever is left, waited for or not, gives its frames back now. Ended
    // processes already did.
    for slot in table.processes.iter_mut() {
        if let Some(process) = slot.take() {
            if !matches!(process.state, State::Ended(_)) {
                machine.reclaim(process.domain.frames());
            }
        }
    }
    Ok(outcome)
}

/// Give a process its arguments: `block` is NUL-terminated strings, and a
/// trailing string without its NUL counts too.
fn place_arguments(domain: &mut arch::Domain, block: &[u8]) {
    let mut count = 0;
    let mut open = false;
    for (offset, byte) in block.iter().enumerate().take(PAYLOAD_BYTES) {
        domain.core().write_payload(offset, *byte);
        if *byte == 0 {
            if open {
                count += 1;
            }
            open = false;
        } else {
            open = true;
        }
    }
    if open {
        count += 1;
    }
    domain.core().write_shared(process::ARGUMENT_COUNT, count);
}

fn padded_name(name: &[u8]) -> [u8; NAME_BYTES] {
    let mut padded = [0_u8; NAME_BYTES];
    for (stored, byte) in padded.iter_mut().zip(name) {
        *stored = *byte;
    }
    padded
}

/// Round-robin over the table: each pass gives every runnable process one
/// entry and retries every blocked one. Ends when every process has, or
/// when nothing can make progress, in which case the blocked ones are
/// stopped as blocked forever.
#[inline(never)]
fn serve(machine: &mut arch::Machine, services: &mut Services<'_>, table: &mut Table) -> Exit {
    loop {
        let mut progressed = false;
        let mut alive = false;
        for index in 0..process::PROCESSES {
            let state = match table.processes[index].as_ref() {
                Some(process) => process.state,
                None => continue,
            };
            match state {
                State::Ended(_) => continue,
                State::Runnable => {
                    alive = true;
                    progressed = true;
                    step(machine, services, table, index);
                }
                State::Waiting(child) => {
                    alive = true;
                    if let Some(code) = reap(table, index, child) {
                        answer(table, index, code);
                        progressed = true;
                    }
                }
                State::Reading { descriptor, length } => {
                    alive = true;
                    if let Some(result) = pipe_read(table, index, descriptor, length) {
                        answer(table, index, result);
                        progressed = true;
                    }
                }
                State::Writing { descriptor, length } => {
                    alive = true;
                    if let Some(result) = pipe_write(table, index, descriptor, length) {
                        answer(table, index, result);
                        progressed = true;
                    }
                }
            }
        }
        if !alive {
            break;
        }
        if !progressed {
            // Every live process is blocked on something no live process
            // will do: a wait for a child that waits for it, a read of a
            // pipe whose writers all wait. Nothing else could resolve it.
            for index in 0..process::PROCESSES {
                if let Some(process) = table.processes[index].as_mut() {
                    if !matches!(process.state, State::Ended(_)) {
                        end(machine, services, table, index, Exit::Blocked);
                    }
                }
            }
            break;
        }
    }
    match table.processes[0].as_ref().map(|process| process.state) {
        Some(State::Ended(exit)) => exit,
        _ => Exit::Blocked,
    }
}

/// Write a request's answer and make the process runnable again.
fn answer(table: &mut Table, index: usize, result: u64) {
    if let Some(process) = table.processes[index].as_mut() {
        process.domain.core().write_shared(process::RESULT, result);
        process.state = State::Runnable;
    }
}

/// One entry of process `index`: run it until it yields or is stopped, and
/// serve what it asked for.
fn step(machine: &mut arch::Machine, services: &mut Services<'_>, table: &mut Table, index: usize) {
    let (kind, arguments) = {
        let Some(process) = table.processes[index].as_mut() else {
            return;
        };
        match process.domain.run() {
            Stop::Replied => {}
            Stop::Faulted(fault) => {
                end(machine, services, table, index, Exit::Faulted(fault));
                return;
            }
            Stop::BudgetExhausted => {
                end(machine, services, table, index, Exit::BudgetExhausted);
                return;
            }
        }
        let core = process.domain.core();
        (
            core.read_shared(process::KIND),
            [
                core.read_shared(process::ARGUMENTS),
                core.read_shared(process::ARGUMENTS + 1),
                core.read_shared(process::ARGUMENTS + 2),
                core.read_shared(process::ARGUMENTS + 3),
            ],
        )
    };
    let result = match kind {
        process::EXIT => {
            end(machine, services, table, index, Exit::Status(arguments[0]));
            return;
        }
        process::WRITE => match kind_of(table, index, arguments[0]) {
            Kind::Console => write_console(table, index, services.console, arguments[1]),
            Kind::File => file_write(table, index, services, arguments[0], arguments[1]),
            Kind::PipeWrite => match pipe_write(table, index, arguments[0], arguments[1]) {
                Some(result) => result,
                None => {
                    block(
                        table,
                        index,
                        State::Writing {
                            descriptor: arguments[0],
                            length: arguments[1],
                        },
                    );
                    return;
                }
            },
            _ => error(EBADF),
        },
        process::READ => match kind_of(table, index, arguments[0]) {
            Kind::File => file_read(table, index, services, arguments[0], arguments[1]),
            Kind::PipeRead => match pipe_read(table, index, arguments[0], arguments[1]) {
                Some(result) => result,
                None => {
                    block(
                        table,
                        index,
                        State::Reading {
                            descriptor: arguments[0],
                            length: arguments[1],
                        },
                    );
                    return;
                }
            },
            _ => error(EBADF),
        },
        process::OPEN => open(table, index, services, arguments[0], arguments[1]),
        process::CLOSE => close(table, index, arguments[0]),
        process::PIPE => pipe(table, index),
        process::SEEK => seek(table, index, arguments[0], arguments[1], arguments[2]),
        process::SPAWN => spawn(machine, services, table, index, arguments),
        process::WINDOW => window(table, index, services, arguments),
        process::DRAW => draw(table, index, services, arguments),
        process::WAIT => {
            let child = arguments[0] as usize;
            let known = child < process::PROCESSES
                && child != index
                && table.processes[child]
                    .as_ref()
                    .is_some_and(|process| process.parent == Some(index));
            if !known {
                error(ECHILD)
            } else {
                match reap(table, index, child) {
                    Some(code) => code,
                    None => {
                        block(table, index, State::Waiting(child));
                        return;
                    }
                }
            }
        }
        _ => error(ENOSYS),
    };
    answer(table, index, result);
}

/// `WINDOW`: relayed to the display with the process's slot as the owner;
/// the title is the payload area's first `arguments[2]` bytes.
fn window(
    table: &mut Table,
    index: usize,
    services: &mut Services<'_>,
    arguments: [u64; 4],
) -> u64 {
    let Some(process) = table.processes[index].as_mut() else {
        return error(EBADF);
    };
    let Some(display) = services.display.as_deref_mut() else {
        return error(ENODEV);
    };
    let (Ok(width), Ok(height)) = (u32::try_from(arguments[0]), u32::try_from(arguments[1])) else {
        return error(EINVAL);
    };
    let length = (arguments[2] as usize).min(28);
    let mut title = [0_u8; 28];
    for (offset, byte) in title.iter_mut().take(length).enumerate() {
        *byte = process.domain.core().read_payload(offset);
    }
    display.open(index, width, height, &title[..length]) as u64
}

/// `DRAW`: the block area's first `arguments[1]` records go to the display
/// as they are; the display decides whether they are permitted.
fn draw(table: &mut Table, index: usize, services: &mut Services<'_>, arguments: [u64; 4]) -> u64 {
    let Some(process) = table.processes[index].as_mut() else {
        return error(EBADF);
    };
    let Some(display) = services.display.as_deref_mut() else {
        return error(ENODEV);
    };
    let count = arguments[1] as usize;
    if count > process::DRAW_RECORDS {
        return error(EINVAL);
    }
    let mut records = [[0_u8; RECORD_BYTES]; process::DRAW_RECORDS];
    for (number, record) in records.iter_mut().take(count).enumerate() {
        for (offset, byte) in record.iter_mut().enumerate() {
            *byte = process
                .domain
                .core()
                .read_block(number * RECORD_BYTES + offset);
        }
    }
    display.draw(index, arguments[0], arguments[2], &records[..count]) as u64
}

fn block(table: &mut Table, index: usize, state: State) {
    if let Some(process) = table.processes[index].as_mut() {
        process.state = state;
    }
}

/// A process is over: its frames go back, its descriptors close (so a pipe
/// it wrote ends for its reader), and a stop the machine imposed is
/// reported on the console, since no `wait` need ever hear of it.
fn end(
    machine: &mut arch::Machine,
    services: &mut Services<'_>,
    table: &mut Table,
    index: usize,
    exit: Exit,
) {
    for descriptor in 0..process::DESCRIPTORS as u64 {
        close(table, index, descriptor);
    }
    let Some(process) = table.processes[index].as_mut() else {
        return;
    };
    process.state = State::Ended(exit);
    machine.reclaim(process.domain.frames());
    if let Some(display) = services.display.as_deref_mut() {
        display.release(index);
    }
    if index != 0 && !matches!(exit, Exit::Status(_)) {
        let mut line = Line::new();
        let _ = line.write_str("process ");
        for byte in &process.name[..process.name_length] {
            let _ = line.write_char(char::from(*byte));
        }
        report(&mut line, exit);
        services.console.write(line.get());
    }
}

/// The one line the workshop prints for how a process ended, with its
/// newline as the console driver wants it.
pub fn report(out: &mut dyn Write, exit: Exit) {
    match exit {
        Exit::Status(status) => {
            let _ = write!(out, " exited with status {status}\r\n");
        }
        Exit::Faulted(fault) => {
            let _ = write!(
                out,
                " faulted: {} at {:#x} touching {:#x}; contained\r\n",
                fault.name(),
                fault.pc,
                fault.address
            );
        }
        Exit::BudgetExhausted => {
            let _ = write!(out, " never yielded; tick budget exhausted; stopped\r\n");
        }
        Exit::Blocked => {
            let _ = write!(
                out,
                " blocked on what nothing left could answer; stopped\r\n"
            );
        }
    }
}

/// If `child` has ended, its slot is freed and its `wait` answer returned.
fn reap(table: &mut Table, parent: usize, child: usize) -> Option<u64> {
    let ended = match table.processes[child].as_ref() {
        Some(process) if process.parent == Some(parent) => match process.state {
            State::Ended(exit) => exit,
            _ => return None,
        },
        _ => return Some(error(ECHILD)),
    };
    table.processes[child] = None;
    Some(match ended {
        Exit::Status(status) => status & 0xff,
        Exit::Faulted(_) => process::WAIT_SIGNALED | process::SIGNAL_FAULT,
        Exit::BudgetExhausted | Exit::Blocked => process::WAIT_SIGNALED | process::SIGNAL_KILLED,
    })
}

fn error(number: i64) -> u64 {
    (-number) as u64
}

fn kind_of(table: &Table, index: usize, descriptor: u64) -> Kind {
    table.processes[index]
        .as_ref()
        .and_then(|process| process.descriptors.get(descriptor as usize))
        .map_or(Kind::Free, |descriptor| descriptor.kind)
}

/// The filesystem service's status word as an errno the process sees.
fn service_error(outcome: Result<(u64, [u64; 3]), ServiceError>) -> Result<[u64; 3], i64> {
    match outcome {
        Ok((0, values)) => Ok(values),
        Ok((status, _)) => Err(status as i64),
        Err(ServiceError::Stale) => Err(ESTALE),
        Err(_) => Err(EIO),
    }
}

/// The lowest free descriptor from 3 up; 0 to 2 keep their meanings.
fn free_descriptor(descriptors: &[Descriptor; process::DESCRIPTORS]) -> Option<usize> {
    descriptors
        .iter()
        .enumerate()
        .skip(3)
        .find_map(|(number, descriptor)| (descriptor.kind == Kind::Free).then_some(number))
}

#[inline(never)]
fn open(
    table: &mut Table,
    index: usize,
    services: &mut Services<'_>,
    flags: u64,
    length: u64,
) -> u64 {
    let Some(filesystem) = services.filesystem.as_deref_mut() else {
        return error(ENOSYS);
    };
    let Some(process) = table.processes[index].as_mut() else {
        return error(EBADF);
    };
    let namespace = process.namespace;
    let wants_write = flags & (process::O_WRONLY | process::O_RDWR) != 0;
    let wants_create = flags & process::O_CREAT != 0;
    // The namespace's rights bound the request before the service sees it:
    // a process without `write` cannot open for writing, without `create`
    // cannot create, and nothing a path spells changes that.
    if (wants_write && !namespace.write) || (wants_create && !namespace.create) {
        return error(EACCES);
    }
    if !wants_write && !namespace.read {
        return error(EACCES);
    }
    let Some(number) = free_descriptor(&process.descriptors) else {
        return error(EMFILE);
    };
    let length = (length as usize).min(PAYLOAD_BYTES);
    let mut path = [0_u8; PAYLOAD_BYTES];
    for (offset, byte) in path.iter_mut().enumerate().take(length) {
        *byte = process.domain.core().read_payload(offset);
    }
    filesystem.write_payload(path.get(..length).unwrap_or(&[]));
    let handle = filesystem.handle();
    let outcome = filesystem.filesystem_request(
        handle,
        services.storage,
        fs::COMMAND_OPEN,
        [u64::from(namespace.root), flags, length as u64],
    );
    match service_error(outcome) {
        Ok([entry, length, kind]) => {
            if kind == fs::KIND_DIRECTORY && flags & process::O_DIRECTORY == 0 {
                return error(fs::EISDIR as i64);
            }
            if let Some(slot) = process.descriptors.get_mut(number) {
                *slot = Descriptor {
                    kind: Kind::File,
                    entry: entry as u16,
                    pipe: 0,
                    offset: if flags & process::O_APPEND != 0 {
                        length
                    } else {
                        0
                    },
                    length,
                    readable: !wants_write || flags & process::O_RDWR != 0,
                    writable: wants_write,
                    handle,
                };
            }
            number as u64
        }
        Err(number) => error(number),
    }
}

/// Move a file descriptor's offset. The end is the length the supervisor
/// last saw for this descriptor: at open, and after its own writes.
fn seek(table: &mut Table, index: usize, descriptor: u64, offset: u64, whence: u64) -> u64 {
    let Some(process) = table.processes[index].as_mut() else {
        return error(EBADF);
    };
    let Some(slot) = process.descriptors.get_mut(descriptor as usize) else {
        return error(EBADF);
    };
    match slot.kind {
        Kind::File => {}
        Kind::Free => return error(EBADF),
        _ => return error(ESPIPE),
    }
    let base = match whence {
        0 => 0_i64,
        1 => slot.offset as i64,
        2 => slot.length as i64,
        _ => return error(EINVAL),
    };
    let Some(target) = base.checked_add(offset as i64) else {
        return error(EINVAL);
    };
    if target < 0 || target > fs::FILE_BYTES as i64 {
        return error(EINVAL);
    }
    slot.offset = target as u64;
    slot.offset
}

/// Close a descriptor: a pipe end gives its count back, and the last write
/// end closing is the end of the stream for whoever reads it.
fn close(table: &mut Table, index: usize, descriptor: u64) -> u64 {
    let Some(process) = table.processes[index].as_mut() else {
        return error(EBADF);
    };
    let Some(slot) = process.descriptors.get_mut(descriptor as usize) else {
        return error(EBADF);
    };
    let closed = *slot;
    if closed.kind == Kind::Free {
        return error(EBADF);
    }
    *slot = Descriptor::FREE;
    if let Some(pipe) = table.pipes.get_mut(usize::from(closed.pipe)) {
        match closed.kind {
            Kind::PipeRead => pipe.readers = pipe.readers.saturating_sub(1),
            Kind::PipeWrite => pipe.writers = pipe.writers.saturating_sub(1),
            _ => {}
        }
        if pipe.readers == 0 && pipe.writers == 0 {
            *pipe = Pipe::EMPTY;
        }
    }
    0
}

/// A new pipe with one read end and one write end in this process's table.
fn pipe(table: &mut Table, index: usize) -> u64 {
    let Some(number) = table.pipes.iter().position(|pipe| !pipe.used) else {
        return error(ENFILE);
    };
    let Some(process) = table.processes[index].as_mut() else {
        return error(EBADF);
    };
    let Some(read_end) = free_descriptor(&process.descriptors) else {
        return error(EMFILE);
    };
    process.descriptors[read_end] = Descriptor {
        kind: Kind::PipeRead,
        pipe: number as u8,
        readable: true,
        ..Descriptor::FREE
    };
    let Some(write_end) = free_descriptor(&process.descriptors) else {
        process.descriptors[read_end] = Descriptor::FREE;
        return error(EMFILE);
    };
    process.descriptors[write_end] = Descriptor {
        kind: Kind::PipeWrite,
        pipe: number as u8,
        writable: true,
        ..Descriptor::FREE
    };
    table.pipes[number] = Pipe {
        used: true,
        readers: 1,
        writers: 1,
        ..Pipe::EMPTY
    };
    (read_end as u64) | ((write_end as u64) << 16)
}

/// Read from a pipe into the block area: `None` when the pipe is empty and
/// a writer remains, so the caller blocks; zero when it is empty for good.
fn pipe_read(table: &mut Table, index: usize, descriptor: u64, length: u64) -> Option<u64> {
    let Some(process) = table.processes[index].as_mut() else {
        return Some(error(EBADF));
    };
    let Some(slot) = process.descriptors.get(descriptor as usize).copied() else {
        return Some(error(EBADF));
    };
    if slot.kind != Kind::PipeRead {
        return Some(error(EBADF));
    }
    let Some(pipe) = table.pipes.get_mut(usize::from(slot.pipe)) else {
        return Some(error(EBADF));
    };
    if pipe.length == 0 {
        return (pipe.writers == 0).then_some(0);
    }
    let take = (length as usize).min(BLOCK_BYTES).min(pipe.length);
    for offset in 0..take {
        let byte = pipe.buffer[(pipe.head + offset) % process::PIPE_BYTES];
        process.domain.core().write_block(offset, byte);
    }
    pipe.head = (pipe.head + take) % process::PIPE_BYTES;
    pipe.length -= take;
    Some(take as u64)
}

/// Write the block area into a pipe: `None` when it is full and a reader
/// remains, so the caller blocks; `EPIPE` when no reader is left.
fn pipe_write(table: &mut Table, index: usize, descriptor: u64, length: u64) -> Option<u64> {
    let Some(process) = table.processes[index].as_mut() else {
        return Some(error(EBADF));
    };
    let Some(slot) = process.descriptors.get(descriptor as usize).copied() else {
        return Some(error(EBADF));
    };
    if slot.kind != Kind::PipeWrite {
        return Some(error(EBADF));
    }
    let Some(pipe) = table.pipes.get_mut(usize::from(slot.pipe)) else {
        return Some(error(EBADF));
    };
    if pipe.readers == 0 {
        return Some(error(EPIPE));
    }
    let room = process::PIPE_BYTES - pipe.length;
    if room == 0 {
        return None;
    }
    let take = (length as usize).min(BLOCK_BYTES).min(room);
    for offset in 0..take {
        let byte = process.domain.core().read_block(offset);
        pipe.buffer[(pipe.head + pipe.length + offset) % process::PIPE_BYTES] = byte;
    }
    pipe.length += take;
    Some(take as u64)
}

/// Start a child: the program named in the payload, the descriptors the
/// parent names for its 0 and 1, the console for its 2, and the parent's
/// namespace or a read-only view of it. Nothing else crosses.
#[inline(never)]
fn spawn(
    machine: &mut arch::Machine,
    services: &mut Services<'_>,
    table: &mut Table,
    index: usize,
    arguments: [u64; 4],
) -> u64 {
    let Some(free) = table.processes.iter().position(|slot| slot.is_none()) else {
        return error(EAGAIN);
    };
    let (name, name_length, block, block_length, namespace, stdin, stdout) = {
        let Some(parent) = table.processes[index].as_mut() else {
            return error(EBADF);
        };
        let length = (arguments[0] as usize).min(NAME_BYTES);
        let mut name = [0_u8; NAME_BYTES];
        for (offset, byte) in name.iter_mut().enumerate().take(length) {
            *byte = parent.domain.core().read_payload(offset);
        }
        // The child's arguments follow the name in the parent's payload:
        // `arguments[3]`'s upper bits say how many bytes. Without any, the
        // child's one argument is its name.
        let block_length = ((arguments[3] >> 16) as usize).min(PAYLOAD_BYTES - length - 1);
        let mut block = [0_u8; PAYLOAD_BYTES];
        if block_length == 0 {
            block[..length].copy_from_slice(&name[..length]);
        }
        for (offset, byte) in block.iter_mut().enumerate().take(block_length) {
            *byte = parent.domain.core().read_payload(length + 1 + offset);
        }
        let block_length = if block_length == 0 {
            length + 1
        } else {
            block_length
        };
        let namespace = if arguments[3] & process::SPAWN_READ_ONLY != 0 {
            Namespace::read_only(parent.namespace.root)
        } else {
            parent.namespace
        };
        let mut given = [Descriptor::FREE; 2];
        for (slot, wanted) in given.iter_mut().zip([arguments[1], arguments[2]]) {
            if wanted == process::NO_DESCRIPTOR {
                continue;
            }
            match parent.descriptors.get(wanted as usize) {
                Some(descriptor) if descriptor.kind != Kind::Free => *slot = *descriptor,
                _ => return error(EBADF),
            }
        }
        (
            name,
            length,
            block,
            block_length,
            namespace,
            given[0],
            given[1],
        )
    };
    let program = match find(services.storage, &name[..name_length]) {
        Ok(Some(program)) => program,
        Ok(None) => return error(fs::ENOENT as i64),
        Err(_) => return error(EIO),
    };
    let mut domain = match load(machine, services.storage, program) {
        Ok(domain) => domain,
        Err(_) => return error(EIO),
    };
    place_arguments(&mut domain, &block[..block_length]);
    let mut descriptors = [Descriptor::FREE; process::DESCRIPTORS];
    descriptors[0] = stdin;
    descriptors[1] = stdout;
    descriptors[2] = Descriptor::CONSOLE;
    // A pipe end handed on is one more end naming the pipe.
    for descriptor in [stdin, stdout] {
        if let Some(pipe) = table.pipes.get_mut(usize::from(descriptor.pipe)) {
            match descriptor.kind {
                Kind::PipeRead => pipe.readers = pipe.readers.saturating_add(1),
                Kind::PipeWrite => pipe.writers = pipe.writers.saturating_add(1),
                _ => {}
            }
        }
    }
    table.processes[free] = Some(Process {
        domain,
        descriptors,
        namespace,
        parent: Some(index),
        state: State::Runnable,
        name,
        name_length,
    });
    free as u64
}

#[inline(never)]
fn file_read(
    table: &mut Table,
    index: usize,
    services: &mut Services<'_>,
    descriptor: u64,
    length: u64,
) -> u64 {
    let Some(process) = table.processes[index].as_mut() else {
        return error(EBADF);
    };
    let Some(slot) = process.descriptors.get_mut(descriptor as usize) else {
        return error(EBADF);
    };
    if slot.kind != Kind::File || !slot.readable {
        return error(EBADF);
    }
    let Some(filesystem) = services.filesystem.as_deref_mut() else {
        return error(ENOSYS);
    };
    let length = length.min(BLOCK_BYTES as u64);
    let outcome = filesystem.filesystem_request(
        slot.handle,
        services.storage,
        fs::COMMAND_READ,
        [u64::from(slot.entry), slot.offset, length],
    );
    match service_error(outcome) {
        Ok([count, _, _]) => {
            for offset in 0..(count as usize).min(BLOCK_BYTES) {
                let byte = filesystem.read_block(offset);
                process.domain.core().write_block(offset, byte);
            }
            slot.offset += count;
            count
        }
        Err(number) => error(number),
    }
}

#[inline(never)]
fn file_write(
    table: &mut Table,
    index: usize,
    services: &mut Services<'_>,
    descriptor: u64,
    length: u64,
) -> u64 {
    let Some(process) = table.processes[index].as_mut() else {
        return error(EBADF);
    };
    let Some(slot) = process.descriptors.get_mut(descriptor as usize) else {
        return error(EBADF);
    };
    if slot.kind != Kind::File || !slot.writable {
        return error(EBADF);
    }
    let Some(filesystem) = services.filesystem.as_deref_mut() else {
        return error(ENOSYS);
    };
    let length = length.min(BLOCK_BYTES as u64);
    for offset in 0..length as usize {
        let byte = process.domain.core().read_block(offset);
        filesystem.write_block(offset, byte);
    }
    let outcome = filesystem.filesystem_request(
        slot.handle,
        services.storage,
        fs::COMMAND_WRITE,
        [u64::from(slot.entry), slot.offset, length],
    );
    match service_error(outcome) {
        Ok([count, _, _]) => {
            slot.offset += count;
            slot.length = slot.length.max(slot.offset);
            count
        }
        Err(number) => error(number),
    }
}

/// The console, through whatever the workshop gave, with the terminal's
/// line discipline applied here: a newline becomes a carriage return and a
/// newline, so the process may write text as text.
#[inline(never)]
fn write_console(table: &mut Table, index: usize, console: &mut dyn Console, length: u64) -> u64 {
    let Some(process) = table.processes[index].as_mut() else {
        return error(EBADF);
    };
    let length = (length as usize).min(BLOCK_BYTES);
    let mut bytes = [0_u8; BLOCK_BYTES * 2];
    let mut count = 0;
    for offset in 0..length {
        let byte = process.domain.core().read_block(offset);
        if byte == b'\n' {
            bytes[count] = b'\r';
            count += 1;
        }
        bytes[count] = byte;
        count += 1;
    }
    console.write(&bytes[..count]);
    length as u64
}
