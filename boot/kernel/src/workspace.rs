//! Crash-tolerant native source-cell images.
//!
//! v0.1.7 persists language source rather than Rust memory layout. Two fixed raw
//! disk slots retain the newest and previous committed workspace. A save first
//! invalidates the older slot, writes its payload, and publishes its header
//! last; boot accepts only a bounded, checksummed, canonically decoded image.

use crate::service::{ServiceDomain, ServiceError};
use crate::user::storage_status;

pub const MAX_CELLS: usize = 16;
pub const MAX_CELL_NAME: usize = 24;
pub const MAX_CELL_SOURCE: usize = crate::world::PAYLOAD_BYTES;

const SLOT_A: u32 = 1024;
const SLOT_B: u32 = 1040;
const SLOT_SECTORS: u32 = 16;
const PAYLOAD_SECTORS: usize = SLOT_SECTORS as usize - 1;
const PAYLOAD_BYTES: usize = PAYLOAD_SECTORS * 512;
/// The recovery record follows the two workspace slots. It names which
/// generation is trusted and which is a candidate still earning that trust.
pub const RECOVERY_SECTOR: u32 = 1056;
const RECOVERY_MAGIC: &[u8; 8] = b"AGELRC1\0";
const RECOVERY_VERSION: u16 = 1;
#[cfg(target_arch = "x86_64")]
/// The kernel slot selector follows the recovery record. The BIOS stage reads
/// and updates it before any kernel runs, so its first ten bytes are plain
/// values 16-bit code can parse without a checksum: magic, version, trusted
/// slot, candidate slot (`NO_CANDIDATE` for none), boot attempts, verified
/// flag, admitted flag. Bytes 12-15 hold the candidate's signed length and
/// bytes 64-127 its Ed25519 signature over the SHA-512 of those bytes; the
/// stage never reads them, the running kernel checks them before admitting.
pub const KERNEL_SELECTOR_SECTOR: u32 = 1057;
#[cfg(target_arch = "x86_64")]
const KERNEL_SELECTOR_MAGIC: &[u8; 4] = b"AGKS";
#[cfg(target_arch = "x86_64")]
const KERNEL_SELECTOR_VERSION: u8 = 2;
#[cfg(target_arch = "x86_64")]
/// First sector of each kernel slot and the sectors a slot holds.
pub const KERNEL_SLOT_BASE: [u32; 2] = [1, 512];
#[cfg(target_arch = "x86_64")]
pub const KERNEL_SLOT_SECTORS: u32 = 508;
#[cfg(target_arch = "x86_64")]
pub const NO_CANDIDATE: u8 = 0xff;
const MAGIC: &[u8; 8] = b"AGELWS1\0";
const FORMAT_VERSION: u16 = 1;

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    name_length: u8,
    source_length: u16,
    name: [u8; MAX_CELL_NAME],
    source: [u8; MAX_CELL_SOURCE],
}

impl Cell {
    const EMPTY: Self = Self {
        name_length: 0,
        source_length: 0,
        name: [0; MAX_CELL_NAME],
        source: [0; MAX_CELL_SOURCE],
    };

    pub fn name(&self) -> &[u8] {
        &self.name[..self.name_length as usize]
    }

    pub fn source(&self) -> &[u8] {
        &self.source[..self.source_length as usize]
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Workspace {
    cells: [Cell; MAX_CELLS],
}

impl Workspace {
    pub const fn new() -> Self {
        Self {
            cells: [Cell::EMPTY; MAX_CELLS],
        }
    }

    pub fn count(&self) -> usize {
        self.cells
            .iter()
            .filter(|cell| cell.name_length != 0)
            .count()
    }

    pub fn cell(&self, ordinal: usize) -> Option<&Cell> {
        self.cells
            .iter()
            .filter(|cell| cell.name_length != 0)
            .nth(ordinal)
    }

    pub fn find(&self, name: &[u8]) -> Option<&Cell> {
        self.cells
            .iter()
            .find(|cell| cell.name_length != 0 && cell.name() == name)
    }

    pub fn upsert(&mut self, name: &[u8], source: &[u8]) -> Result<(), &'static str> {
        validate_name(name)?;
        if source.is_empty() {
            return Err("cell source is empty");
        }
        if source.len() > MAX_CELL_SOURCE {
            return Err("cell source exceeds native limit");
        }
        let index = self
            .cells
            .iter()
            .position(|cell| cell.name_length != 0 && cell.name() == name)
            .or_else(|| self.cells.iter().position(|cell| cell.name_length == 0))
            .ok_or("workspace cell table is full")?;
        let mut cell = Cell::EMPTY;
        cell.name[..name.len()].copy_from_slice(name);
        cell.name_length = name.len() as u8;
        cell.source[..source.len()].copy_from_slice(source);
        cell.source_length = source.len() as u16;
        self.cells[index] = cell;
        Ok(())
    }

    pub fn delete(&mut self, name: &[u8]) -> Result<(), &'static str> {
        let index = self
            .cells
            .iter()
            .position(|cell| cell.name_length != 0 && cell.name() == name)
            .ok_or("no such workspace cell")?;
        for cursor in index..MAX_CELLS - 1 {
            self.cells[cursor] = self.cells[cursor + 1];
        }
        self.cells[MAX_CELLS - 1] = Cell::EMPTY;
        Ok(())
    }

    fn encode(&self, bytes: &mut [u8; PAYLOAD_BYTES]) -> Result<usize, &'static str> {
        bytes.fill(0);
        let mut cursor = 0;
        put_u16(bytes, &mut cursor, self.count() as u16)?;
        for ordinal in 0..self.count() {
            let cell = self.cell(ordinal).ok_or("workspace cell order changed")?;
            put_u8(bytes, &mut cursor, cell.name_length)?;
            put_u16(bytes, &mut cursor, cell.source_length)?;
            put_bytes(bytes, &mut cursor, cell.name())?;
            put_bytes(bytes, &mut cursor, cell.source())?;
        }
        Ok(cursor)
    }

    fn decode(bytes: &[u8]) -> Result<Self, &'static str> {
        let mut cursor = 0;
        let count = take_u16(bytes, &mut cursor)? as usize;
        if count > MAX_CELLS {
            return Err("workspace image has too many cells");
        }
        let mut workspace = Self::new();
        for _ in 0..count {
            let name_length = take_u8(bytes, &mut cursor)? as usize;
            let source_length = take_u16(bytes, &mut cursor)? as usize;
            if name_length == 0 || name_length > MAX_CELL_NAME {
                return Err("workspace image has invalid cell name length");
            }
            if source_length == 0 || source_length > MAX_CELL_SOURCE {
                return Err("workspace image has invalid cell source length");
            }
            let name = take_bytes(bytes, &mut cursor, name_length)?;
            let source = take_bytes(bytes, &mut cursor, source_length)?;
            if workspace.find(name).is_some() {
                return Err("workspace image repeats a cell name");
            }
            workspace.upsert(name, source)?;
        }
        if cursor != bytes.len() {
            return Err("workspace image has trailing bytes");
        }
        Ok(workspace)
    }
}

// Constructed only by the x86-64 disk path; the workshop's result type names
// it on every machine.
#[derive(Clone, Copy)]
pub struct LoadedWorkspace {
    pub workspace: Workspace,
    pub generation: u64,
}

/// Translate a storage service failure into the workspace's own vocabulary.
/// The driver carries codes; the supervisor decides what they mean.
fn storage_message(error: ServiceError) -> &'static str {
    match error {
        ServiceError::Stale => "storage driver handle is stale",
        ServiceError::Stopped | ServiceError::Faulted => "storage driver stopped; restart required",
        ServiceError::Device(storage_status::ABSENT) => "workspace disk is absent",
        ServiceError::Device(storage_status::BUSY) => "workspace disk remained busy",
        ServiceError::Device(storage_status::DEVICE_ERROR) => "workspace disk reported an error",
        ServiceError::Device(storage_status::DATA_TIMEOUT) => {
            "workspace disk data request timed out"
        }
        ServiceError::Device(storage_status::OUT_OF_RANGE) => "workspace sector is outside LBA28",
        ServiceError::Device(_) => "storage driver refused the request",
    }
}

pub(crate) fn read_sector(
    storage: &mut ServiceDomain,
    lba: u32,
    sector: &mut [u8; 512],
) -> Result<(), &'static str> {
    let handle = storage.handle();
    storage
        .read_sector(handle, lba, sector)
        .map_err(storage_message)
}

fn write_sector(
    storage: &mut ServiceDomain,
    lba: u32,
    sector: &[u8; 512],
) -> Result<(), &'static str> {
    let handle = storage.handle();
    storage
        .write_sector(handle, lba, sector)
        .map_err(storage_message)
}

fn flush(storage: &mut ServiceDomain) -> Result<(), &'static str> {
    let handle = storage.handle();
    storage.flush(handle).map_err(storage_message)
}

/// Read both slots independently and return valid candidates newest-first.
/// A localized read failure cannot hide a valid twin slot.
pub fn load(storage: &mut ServiceDomain) -> Result<[Option<LoadedWorkspace>; 2], &'static str> {
    let a = load_slot(storage, SLOT_A);
    let b = load_slot(storage, SLOT_B);
    match (a, b) {
        (Ok(Some(left)), Ok(Some(right))) if right.generation > left.generation => {
            Ok([Some(right), Some(left)])
        }
        (Ok(Some(left)), Ok(Some(right))) => Ok([Some(left), Some(right)]),
        (Ok(Some(image)), Ok(None) | Err(_)) | (Ok(None) | Err(_), Ok(Some(image))) => {
            Ok([Some(image), None])
        }
        (Ok(None), Ok(None)) => Ok([None, None]),
        (Err(reason), Ok(None) | Err(_)) | (Ok(None), Err(reason)) => Err(reason),
    }
}

/// The recovery plane's durable state: which generation is trusted, which is
/// a candidate, how many boots the candidate has been given, and whether one
/// of them reached a healthy state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecoveryRecord {
    pub trusted: u64,
    pub candidate: u64,
    pub attempts: u32,
    pub verified: bool,
}

impl RecoveryRecord {
    pub const EMPTY: Self = Self {
        trusted: 0,
        candidate: 0,
        attempts: 0,
        verified: false,
    };
}

/// Read the recovery record. An absent or corrupt record is the empty
/// record, so a fresh or damaged disk starts with nothing trusted; a device
/// failure is reported rather than treated as an empty record.
pub fn load_record(storage: &mut ServiceDomain) -> Result<RecoveryRecord, &'static str> {
    let mut sector = [0_u8; 512];
    read_sector(storage, RECOVERY_SECTOR, &mut sector)?;
    if sector[..8] != RECOVERY_MAGIC[..]
        || u16::from_be_bytes([sector[8], sector[9]]) != RECOVERY_VERSION
    {
        return Ok(RecoveryRecord::EMPTY);
    }
    let expected = u32::from_be_bytes(
        sector[40..44]
            .try_into()
            .map_err(|_| "recovery record checksum is malformed")?,
    );
    if checksum_parts(&sector[..40], &[]) != expected {
        return Ok(RecoveryRecord::EMPTY);
    }
    Ok(RecoveryRecord {
        trusted: u64::from_be_bytes(sector[16..24].try_into().map_err(|_| "malformed")?),
        candidate: u64::from_be_bytes(sector[24..32].try_into().map_err(|_| "malformed")?),
        attempts: u32::from_be_bytes(sector[32..36].try_into().map_err(|_| "malformed")?),
        verified: sector[36] == 1,
    })
}

/// Write and flush the recovery record, then read it back.
pub fn save_record(
    storage: &mut ServiceDomain,
    record: &RecoveryRecord,
) -> Result<(), &'static str> {
    let mut sector = [0_u8; 512];
    sector[..8].copy_from_slice(RECOVERY_MAGIC);
    sector[8..10].copy_from_slice(&RECOVERY_VERSION.to_be_bytes());
    sector[16..24].copy_from_slice(&record.trusted.to_be_bytes());
    sector[24..32].copy_from_slice(&record.candidate.to_be_bytes());
    sector[32..36].copy_from_slice(&record.attempts.to_be_bytes());
    sector[36] = u8::from(record.verified);
    let checksum = checksum_parts(&sector[..40], &[]);
    sector[40..44].copy_from_slice(&checksum.to_be_bytes());
    write_sector(storage, RECOVERY_SECTOR, &sector)?;
    flush(storage)?;
    if load_record(storage)? != *record {
        return Err("recovery record verification failed");
    }
    Ok(())
}

#[cfg(target_arch = "x86_64")]
/// Which kernel slot the boot stage loads: `trusted` unless a `candidate` is
/// present and either verified or still within its boot budget.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KernelSelector {
    pub trusted: u8,
    pub candidate: u8,
    pub attempts: u8,
    pub verified: bool,
    /// The running kernel checked the candidate's signature; until then the
    /// boot stage does not load it.
    pub admitted: bool,
    /// Bytes of the candidate slot the signature covers.
    pub length: u32,
    pub signature: [u8; 64],
}

#[cfg(target_arch = "x86_64")]
impl KernelSelector {
    /// No selector on disk: slot A is trusted and nothing is proposed, which
    /// is also what a boot stage that finds no selector does.
    pub const DEFAULT: Self = Self {
        trusted: 0,
        candidate: NO_CANDIDATE,
        attempts: 0,
        verified: false,
        admitted: false,
        length: 0,
        signature: [0; 64],
    };

    /// Forget the candidate entirely.
    pub fn clear_candidate(&mut self) {
        self.candidate = NO_CANDIDATE;
        self.attempts = 0;
        self.verified = false;
        self.admitted = false;
        self.length = 0;
        self.signature = [0; 64];
    }
}

#[cfg(target_arch = "x86_64")]
/// Read the kernel slot selector. Anything the boot stage would not act on
/// (absent, wrong version, a slot number that is not A or B) reads as the
/// default, which is what the boot stage did with it.
pub fn load_selector(storage: &mut ServiceDomain) -> Result<KernelSelector, &'static str> {
    let mut sector = [0_u8; 512];
    read_sector(storage, KERNEL_SELECTOR_SECTOR, &mut sector)?;
    if sector[..4] != KERNEL_SELECTOR_MAGIC[..] || sector[4] != KERNEL_SELECTOR_VERSION {
        return Ok(KernelSelector::DEFAULT);
    }
    let (trusted, candidate) = (sector[5], sector[6]);
    if trusted > 1 || (candidate > 1 && candidate != NO_CANDIDATE) {
        return Ok(KernelSelector::DEFAULT);
    }
    let mut signature = [0_u8; 64];
    signature.copy_from_slice(&sector[64..128]);
    Ok(KernelSelector {
        trusted,
        candidate,
        attempts: sector[7],
        verified: sector[8] != 0,
        admitted: sector[9] != 0,
        length: u32::from_le_bytes([sector[12], sector[13], sector[14], sector[15]]),
        signature,
    })
}

#[cfg(target_arch = "x86_64")]
/// One sector of a kernel slot, for hashing a candidate before admission.
pub fn read_slot_sector(
    storage: &mut ServiceDomain,
    slot: u8,
    index: u32,
    sector: &mut [u8; 512],
) -> Result<(), &'static str> {
    if index >= KERNEL_SLOT_SECTORS {
        return Err("sector is outside the kernel slot");
    }
    read_sector(
        storage,
        KERNEL_SLOT_BASE[usize::from(slot & 1)] + index,
        sector,
    )
}

#[cfg(target_arch = "x86_64")]
/// Write and flush the selector, then read it back.
pub fn save_selector(
    storage: &mut ServiceDomain,
    selector: &KernelSelector,
) -> Result<(), &'static str> {
    let mut sector = [0_u8; 512];
    sector[..4].copy_from_slice(KERNEL_SELECTOR_MAGIC);
    sector[4] = KERNEL_SELECTOR_VERSION;
    sector[5] = selector.trusted;
    sector[6] = selector.candidate;
    sector[7] = selector.attempts;
    sector[8] = u8::from(selector.verified);
    sector[9] = u8::from(selector.admitted);
    sector[12..16].copy_from_slice(&selector.length.to_le_bytes());
    sector[64..128].copy_from_slice(&selector.signature);
    write_sector(storage, KERNEL_SELECTOR_SECTOR, &sector)?;
    flush(storage)?;
    if load_selector(storage)? != *selector {
        return Err("kernel selector verification failed");
    }
    Ok(())
}

/// Save a new generation. The slot is chosen by parity unless that slot holds
/// `protect` (the trusted generation), in which case the other slot is used:
/// a save may supersede an unpromoted candidate but never the rollback point.
pub fn save(
    storage: &mut ServiceDomain,
    workspace: &Workspace,
    generation: u64,
    protect: u64,
) -> Result<u64, &'static str> {
    let next = generation
        .checked_add(1)
        .ok_or("workspace generation exhausted")?;
    let mut slot = if next & 1 == 0 { SLOT_A } else { SLOT_B };
    if protect != 0 {
        if let Ok(Some(occupant)) = load_slot(storage, slot) {
            if occupant.generation == protect {
                slot = if slot == SLOT_A { SLOT_B } else { SLOT_A };
            }
        }
    }
    let mut payload = [0_u8; PAYLOAD_BYTES];
    let length = workspace.encode(&mut payload)?;

    // Invalidate this slot before changing its payload. The other slot remains
    // a complete rollback point until the final header and cache flush land.
    let empty = [0_u8; 512];
    write_sector(storage, slot, &empty)?;
    flush(storage)?;
    for index in 0..PAYLOAD_SECTORS {
        let mut sector = [0_u8; 512];
        let start = index * 512;
        sector.copy_from_slice(&payload[start..start + 512]);
        write_sector(storage, slot + 1 + index as u32, &sector)?;
    }
    flush(storage)?;

    let mut header = [0_u8; 512];
    header[..MAGIC.len()].copy_from_slice(MAGIC);
    header[8..10].copy_from_slice(&FORMAT_VERSION.to_be_bytes());
    header[16..24].copy_from_slice(&next.to_be_bytes());
    header[24..28].copy_from_slice(&(length as u32).to_be_bytes());
    let image_checksum = checksum_parts(&header[..28], &payload[..length]);
    header[28..32].copy_from_slice(&image_checksum.to_be_bytes());
    write_sector(storage, slot, &header)?;
    flush(storage)?;

    let verified = load_slot(storage, slot);
    if !matches!(
        verified,
        Ok(Some(image)) if image.generation == next && image.workspace == *workspace
    ) {
        // A caller must never be told that a durable generation failed while a
        // bootable header for it remains. Best-effort invalidation converts a
        // failed verification into an unpublished slot; if that also fails,
        // report the only honest outcome.
        if write_sector(storage, slot, &empty).is_err() || flush(storage).is_err() {
            return Err("workspace save outcome is indeterminate");
        }
        return Err("workspace save verification failed; generation unpublished");
    }
    Ok(next)
}

fn load_slot(
    storage: &mut ServiceDomain,
    slot: u32,
) -> Result<Option<LoadedWorkspace>, &'static str> {
    let mut header = [0_u8; 512];
    read_sector(storage, slot, &mut header)?;
    if header[..MAGIC.len()] != MAGIC[..] {
        return Ok(None);
    }
    if u16::from_be_bytes([header[8], header[9]]) != FORMAT_VERSION {
        return Ok(None);
    }
    if header[10..16].iter().any(|byte| *byte != 0) || header[32..].iter().any(|byte| *byte != 0) {
        return Ok(None);
    }
    let generation = u64::from_be_bytes(
        header[16..24]
            .try_into()
            .map_err(|_| "workspace generation header is malformed")?,
    );
    let length = u32::from_be_bytes(
        header[24..28]
            .try_into()
            .map_err(|_| "workspace length header is malformed")?,
    ) as usize;
    let expected = u32::from_be_bytes(
        header[28..32]
            .try_into()
            .map_err(|_| "workspace checksum header is malformed")?,
    );
    if generation == 0 || !(2..=PAYLOAD_BYTES).contains(&length) {
        return Ok(None);
    }
    let mut payload = [0_u8; PAYLOAD_BYTES];
    for index in 0..PAYLOAD_SECTORS {
        let mut sector = [0_u8; 512];
        read_sector(storage, slot + 1 + index as u32, &mut sector)?;
        let start = index * 512;
        payload[start..start + 512].copy_from_slice(&sector);
    }
    if checksum_parts(&header[..28], &payload[..length]) != expected {
        return Ok(None);
    }
    let workspace = match Workspace::decode(&payload[..length]) {
        Ok(workspace) => workspace,
        Err(_) => return Ok(None),
    };
    Ok(Some(LoadedWorkspace {
        workspace,
        generation,
    }))
}

fn validate_name(name: &[u8]) -> Result<(), &'static str> {
    if name.is_empty() || name.len() > MAX_CELL_NAME {
        return Err("cell name must contain 1..24 bytes");
    }
    if !name.iter().all(|byte| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'/' | b'?' | b'!')
    }) {
        return Err("cell name contains an unsupported byte");
    }
    Ok(())
}

/// CRC-32 over data supplied in pieces: the loader checks a program image
/// sector by sector without holding it.
#[cfg(feature = "process")]
pub struct Crc(u32);

#[cfg(feature = "process")]
impl Crc {
    pub fn new() -> Self {
        Self(0xffff_ffff)
    }

    pub fn update(&mut self, bytes: &[u8]) {
        self.0 = checksum_update(self.0, bytes);
    }

    pub fn finish(self) -> u32 {
        !self.0
    }
}

fn checksum_parts(first: &[u8], second: &[u8]) -> u32 {
    let crc = checksum_update(0xffff_ffff_u32, first);
    !checksum_update(crc, second)
}

fn checksum_update(mut crc: u32, bytes: &[u8]) -> u32 {
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0_u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    crc
}

fn put_u8(bytes: &mut [u8], cursor: &mut usize, value: u8) -> Result<(), &'static str> {
    put_bytes(bytes, cursor, &[value])
}

fn put_u16(bytes: &mut [u8], cursor: &mut usize, value: u16) -> Result<(), &'static str> {
    put_bytes(bytes, cursor, &value.to_be_bytes())
}

fn put_bytes(bytes: &mut [u8], cursor: &mut usize, value: &[u8]) -> Result<(), &'static str> {
    let end = cursor
        .checked_add(value.len())
        .ok_or("workspace image length overflow")?;
    let target = bytes
        .get_mut(*cursor..end)
        .ok_or("workspace image exceeds its fixed slot")?;
    target.copy_from_slice(value);
    *cursor = end;
    Ok(())
}

fn take_u8(bytes: &[u8], cursor: &mut usize) -> Result<u8, &'static str> {
    Ok(take_bytes(bytes, cursor, 1)?[0])
}

fn take_u16(bytes: &[u8], cursor: &mut usize) -> Result<u16, &'static str> {
    let value = take_bytes(bytes, cursor, 2)?;
    Ok(u16::from_be_bytes([value[0], value[1]]))
}

fn take_bytes<'a>(
    bytes: &'a [u8],
    cursor: &mut usize,
    length: usize,
) -> Result<&'a [u8], &'static str> {
    let end = cursor
        .checked_add(length)
        .ok_or("workspace image offset overflow")?;
    let value = bytes
        .get(*cursor..end)
        .ok_or("workspace image is truncated")?;
    *cursor = end;
    Ok(value)
}
