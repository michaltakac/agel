//! Crash-tolerant native source-cell images.
//!
//! v1.7 persists language source rather than Rust memory layout. Two fixed raw
//! disk slots retain the newest and previous committed workspace. A save first
//! invalidates the older slot, writes its payload, and publishes its header
//! last; boot accepts only a bounded, checksummed, canonically decoded image.

pub const MAX_CELLS: usize = 16;
pub const MAX_CELL_NAME: usize = 24;
pub const MAX_CELL_SOURCE: usize = crate::world::PAYLOAD_BYTES;

const SLOT_A: u32 = 256;
const SLOT_B: u32 = 272;
const SLOT_SECTORS: u32 = 16;
const PAYLOAD_SECTORS: usize = SLOT_SECTORS as usize - 1;
const PAYLOAD_BYTES: usize = PAYLOAD_SECTORS * 512;
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

#[derive(Clone, Copy)]
pub struct LoadedWorkspace {
    pub workspace: Workspace,
    pub generation: u64,
}

/// Read both slots independently and return valid candidates newest-first.
/// A localized read failure cannot hide a valid twin slot.
pub fn load() -> Result<[Option<LoadedWorkspace>; 2], &'static str> {
    let a = load_slot(SLOT_A);
    let b = load_slot(SLOT_B);
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

pub fn save(workspace: &Workspace, generation: u64) -> Result<u64, &'static str> {
    let next = generation
        .checked_add(1)
        .ok_or("workspace generation exhausted")?;
    let slot = if next & 1 == 0 { SLOT_A } else { SLOT_B };
    let mut payload = [0_u8; PAYLOAD_BYTES];
    let length = workspace.encode(&mut payload)?;

    // Invalidate this slot before changing its payload. The other slot remains
    // a complete rollback point until the final header and cache flush land.
    let empty = [0_u8; 512];
    crate::arch::write_disk_sector(slot, &empty)?;
    crate::arch::flush_disk()?;
    for index in 0..PAYLOAD_SECTORS {
        let mut sector = [0_u8; 512];
        let start = index * 512;
        sector.copy_from_slice(&payload[start..start + 512]);
        crate::arch::write_disk_sector(slot + 1 + index as u32, &sector)?;
    }
    crate::arch::flush_disk()?;

    let mut header = [0_u8; 512];
    header[..MAGIC.len()].copy_from_slice(MAGIC);
    header[8..10].copy_from_slice(&FORMAT_VERSION.to_be_bytes());
    header[16..24].copy_from_slice(&next.to_be_bytes());
    header[24..28].copy_from_slice(&(length as u32).to_be_bytes());
    let image_checksum = checksum_parts(&header[..28], &payload[..length]);
    header[28..32].copy_from_slice(&image_checksum.to_be_bytes());
    crate::arch::write_disk_sector(slot, &header)?;
    crate::arch::flush_disk()?;

    let verified = load_slot(slot);
    if !matches!(
        verified,
        Ok(Some(image)) if image.generation == next && image.workspace == *workspace
    ) {
        // A caller must never be told that a durable generation failed while a
        // bootable header for it remains. Best-effort invalidation converts a
        // failed verification into an unpublished slot; if that also fails,
        // report the only honest outcome.
        if crate::arch::write_disk_sector(slot, &empty).is_err()
            || crate::arch::flush_disk().is_err()
        {
            return Err("workspace save outcome is indeterminate");
        }
        return Err("workspace save verification failed; generation unpublished");
    }
    Ok(next)
}

fn load_slot(slot: u32) -> Result<Option<LoadedWorkspace>, &'static str> {
    let mut header = [0_u8; 512];
    crate::arch::read_disk_sector(slot, &mut header)?;
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
        crate::arch::read_disk_sector(slot + 1 + index as u32, &mut sector)?;
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
