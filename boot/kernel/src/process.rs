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
use crate::service::ServiceDomain;
use crate::workspace::read_sector;
use crate::world::{process, Fault, Stop, BLOCK_BYTES};

/// The program region: a table sector followed by the programs it names.
pub const TABLE_SECTOR: u32 = 2048;
/// Last sector of the program region, inclusive.
pub const LAST_SECTOR: u32 = 3071;
const MAGIC: &[u8; 8] = b"AGELPR1\0";
const ENTRY_BYTES: usize = 32;
const MAX_PROGRAMS: usize = (512 - 16) / ENTRY_BYTES;
/// Program names are short and ASCII; the table pads them with zeros.
pub const NAME_BYTES: usize = 16;
/// The most a program image may occupy: the region minus its table.
const MAX_LENGTH: u32 = (LAST_SECTOR - TABLE_SECTOR) * 512;
const MAX_SEGMENTS: usize = 8;
/// Pages a process may be built from, code, data and zero fill together.
const MAX_PAGES: usize = 128;
const PAGE: u64 = 4096;

/// One row of the program table.
#[derive(Clone, Copy)]
pub struct Program {
    pub start: u32,
    pub length: u32,
    pub checksum: u32,
}

/// Look a program up by name.
pub fn find(storage: &mut ServiceDomain, name: &[u8]) -> Result<Option<Program>, &'static str> {
    if name.is_empty() || name.len() > NAME_BYTES {
        return Ok(None);
    }
    let mut table = [0_u8; 512];
    read_sector(storage, TABLE_SECTOR, &mut table)?;
    if table[..8] != MAGIC[..] {
        return Ok(None);
    }
    let count =
        (u32::from_le_bytes([table[8], table[9], table[10], table[11]]) as usize).min(MAX_PROGRAMS);
    for index in 0..count {
        let entry = &table[16 + index * ENTRY_BYTES..16 + (index + 1) * ENTRY_BYTES];
        let stored = &entry[..NAME_BYTES];
        let stored_len = stored
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(NAME_BYTES);
        if &stored[..stored_len] != name {
            continue;
        }
        let start = u32::from_le_bytes([entry[16], entry[17], entry[18], entry[19]]);
        let length = u32::from_le_bytes([entry[20], entry[21], entry[22], entry[23]]);
        let checksum = u32::from_le_bytes([entry[24], entry[25], entry[26], entry[27]]);
        if start <= TABLE_SECTOR
            || length == 0
            || length > MAX_LENGTH
            || u64::from(start) + u64::from(length).div_ceil(512) > u64::from(LAST_SECTOR) + 1
        {
            return Err("program table entry is outside the program region");
        }
        return Ok(Some(Program {
            start,
            length,
            checksum,
        }));
    }
    Ok(None)
}

/// How a process ended.
pub enum Exit {
    /// It asked to leave with this status.
    Status(u64),
    /// The machine stopped it.
    Faulted(Fault),
    /// It never yielded inside one entry's tick budget.
    BudgetExhausted,
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

/// Load and run the named program to its end, serving its requests through
/// `console`. Returns how it ended; the domain's frames go back to the pool.
pub fn exec(
    machine: &mut arch::Machine,
    storage: &mut ServiceDomain,
    console: &mut ServiceDomain,
    program: Program,
) -> Result<Exit, &'static str> {
    let mut domain = load(machine, storage, program)?;
    let outcome = serve(&mut domain, console);
    machine.reclaim(domain.frames());
    Ok(outcome)
}

fn serve(domain: &mut arch::Domain, console: &mut ServiceDomain) -> Exit {
    loop {
        match domain.run() {
            Stop::Replied => {}
            Stop::Faulted(fault) => return Exit::Faulted(fault),
            Stop::BudgetExhausted => return Exit::BudgetExhausted,
        }
        let kind = domain.core().read_shared(process::KIND);
        let arguments = [
            domain.core().read_shared(process::ARGUMENTS),
            domain.core().read_shared(process::ARGUMENTS + 1),
        ];
        let result = match kind {
            process::EXIT => return Exit::Status(arguments[0]),
            process::WRITE => write(domain, console, arguments[0], arguments[1]),
            _ => (-38_i64) as u64, // ENOSYS: the request has no meaning here.
        };
        domain.core().write_shared(process::RESULT, result);
    }
}

/// Descriptors 1 and 2 are the console, through the console driver domain,
/// with the terminal's line discipline applied here: a newline becomes a
/// carriage return and a newline, so the process may write text as text.
fn write(
    domain: &mut arch::Domain,
    console: &mut ServiceDomain,
    descriptor: u64,
    length: u64,
) -> u64 {
    if descriptor != 1 && descriptor != 2 {
        return (-9_i64) as u64; // EBADF
    }
    let length = (length as usize).min(BLOCK_BYTES);
    let mut bytes = [0_u8; BLOCK_BYTES * 2];
    let mut count = 0;
    for offset in 0..length {
        let byte = domain.core().read_block(offset);
        if byte == b'\n' {
            bytes[count] = b'\r';
            count += 1;
        }
        bytes[count] = byte;
        count += 1;
    }
    let handle = console.handle();
    let mut written = 0;
    while written < count {
        let take = (count - written).min(crate::world::PAYLOAD_BYTES);
        if console
            .write_console(handle, &bytes[written..written + take])
            .is_err()
        {
            return (-5_i64) as u64; // EIO
        }
        written += take;
    }
    length as u64
}
