//! Named regions of the disk with the same shape: a table sector naming
//! entries by a short ASCII name with a start sector, a byte length and a
//! CRC-32, followed by the entries themselves. The program region holds
//! static ELF images; the asset region holds what the compositor draws
//! with. Nothing in a region is trusted beyond its checksum: whoever writes
//! the disk chooses what is here, and every reader checks what it reads.

use crate::service::ServiceDomain;
use crate::workspace::read_sector;
#[cfg(feature = "native-graphics")]
use crate::workspace::Crc;

/// What installing an entry needs: the region's sectors through the
/// storage driver, and the bytes to install, read from wherever they are
/// (a file through the filesystem service, which relays through the same
/// driver between sectors).
#[cfg(feature = "native-graphics")]
pub trait InstallIo {
    fn read_sector(&mut self, lba: u32, sector: &mut [u8; 512]) -> Result<(), &'static str>;
    fn write_sector(&mut self, lba: u32, sector: &[u8; 512]) -> Result<(), &'static str>;
    fn flush(&mut self) -> Result<(), &'static str>;
    /// Bytes of the source from `offset`: how many, 0 at its end.
    fn read_source(&mut self, offset: usize, out: &mut [u8]) -> Result<usize, &'static str>;
}

const ENTRY_BYTES: usize = 32;
const MAX_ENTRIES: usize = (512 - 16) / ENTRY_BYTES;
/// Names are short and ASCII; the table pads them with zeros.
pub const NAME_BYTES: usize = 16;

/// A region: its table sector, its last sector, and its magic.
#[derive(Clone, Copy)]
pub struct Region {
    pub table: u32,
    pub last: u32,
    pub magic: &'static [u8; 8],
}

/// One row of a region's table.
#[derive(Clone, Copy)]
pub struct Entry {
    pub start: u32,
    pub length: u32,
    pub checksum: u32,
}

impl Region {
    /// The most an entry may occupy: the region minus its table.
    fn max_length(self) -> u32 {
        (self.last - self.table) * 512
    }

    /// Look an entry up by name.
    pub fn find(
        self,
        storage: &mut ServiceDomain,
        name: &[u8],
    ) -> Result<Option<Entry>, &'static str> {
        if name.is_empty() || name.len() > NAME_BYTES {
            return Ok(None);
        }
        let mut table = [0_u8; 512];
        read_sector(storage, self.table, &mut table)?;
        if !table.starts_with(self.magic) {
            return Ok(None);
        }
        let count = (u32::from_le_bytes([table[8], table[9], table[10], table[11]]) as usize)
            .min(MAX_ENTRIES);
        let (rows, _) = table[16..].as_chunks::<ENTRY_BYTES>();
        for entry in rows.iter().take(count) {
            let stored_len = entry[..NAME_BYTES]
                .iter()
                .position(|byte| *byte == 0)
                .unwrap_or(NAME_BYTES);
            if &entry[..stored_len] != name {
                continue;
            }
            let start = u32::from_le_bytes([entry[16], entry[17], entry[18], entry[19]]);
            let length = u32::from_le_bytes([entry[20], entry[21], entry[22], entry[23]]);
            let checksum = u32::from_le_bytes([entry[24], entry[25], entry[26], entry[27]]);
            if start <= self.table
                || length == 0
                || length > self.max_length()
                || u64::from(start) + u64::from(length).div_ceil(512) > u64::from(self.last) + 1
            {
                return Err("table entry is outside its region");
            }
            return Ok(Some(Entry {
                start,
                length,
                checksum,
            }));
        }
        Ok(None)
    }
}

/// The names a region's table holds, in table order, at most `N`.
#[cfg(feature = "native-graphics")]
pub struct Listing<const N: usize> {
    pub names: [[u8; NAME_BYTES]; N],
    pub lengths: [u8; N],
    pub count: usize,
}

impl Region {
    /// Install an entry named `name` holding the bytes `source` yields as
    /// hex text (whitespace ignored), at the first sectors past every
    /// entry the table holds. An entry of that name is replaced in the
    /// table and its old sectors are left as a hole, which the host's
    /// installer compacts; a table that is not this region's is begun.
    /// Written sector by sector as the text is decoded, so nothing is held
    /// whole; the table is written last, so a failure leaves the old one.
    #[cfg(feature = "native-graphics")]
    pub fn install(self, io: &mut dyn InstallIo, name: &[u8]) -> Result<Entry, &'static str> {
        if name.is_empty() || name.len() > NAME_BYTES || !name.iter().all(u8::is_ascii_graphic) {
            return Err("a program name is 1 to 16 printable ASCII bytes");
        }
        let mut table = [0_u8; 512];
        io.read_sector(self.table, &mut table)?;
        let count = if table.starts_with(self.magic) {
            (u32::from_le_bytes([table[8], table[9], table[10], table[11]]) as usize)
                .min(MAX_ENTRIES)
        } else {
            table = [0; 512];
            table[..8].copy_from_slice(self.magic);
            0
        };
        let mut first_free = self.table + 1;
        let mut existing = None;
        for index in 0..count {
            let row = &table[16 + index * ENTRY_BYTES..16 + (index + 1) * ENTRY_BYTES];
            let start = u32::from_le_bytes([row[16], row[17], row[18], row[19]]);
            let length = u32::from_le_bytes([row[20], row[21], row[22], row[23]]);
            let end = start.saturating_add(length.div_ceil(512));
            first_free = first_free.max(end);
            let stored = row[..NAME_BYTES]
                .iter()
                .position(|byte| *byte == 0)
                .unwrap_or(NAME_BYTES);
            if &row[..stored] == name {
                existing = Some(index);
            }
        }
        if existing.is_none() && count == MAX_ENTRIES {
            return Err("the program table is full");
        }
        let mut crc = Crc::new();
        let mut sector = [0_u8; 512];
        let mut filled = 0;
        let mut length = 0_u32;
        let mut lba = first_free;
        let mut chunk = [0_u8; 1024];
        let mut offset = 0;
        let mut high: Option<u8> = None;
        loop {
            let got = io.read_source(offset, &mut chunk)?;
            if got == 0 {
                break;
            }
            offset += got;
            for byte in &chunk[..got] {
                let digit = match byte {
                    b'0'..=b'9' => byte - b'0',
                    b'a'..=b'f' => byte - b'a' + 10,
                    b'A'..=b'F' => byte - b'A' + 10,
                    b' ' | b'\n' | b'\r' | b'\t' => continue,
                    _ => return Err("the file is not hex text"),
                };
                let Some(upper) = high.take() else {
                    high = Some(digit);
                    continue;
                };
                let value = (upper << 4) | digit;
                sector[filled] = value;
                filled += 1;
                crc.update(&[value]);
                length += 1;
                if filled == 512 {
                    if lba > self.last {
                        return Err("the program region is full");
                    }
                    io.write_sector(lba, &sector)?;
                    lba += 1;
                    filled = 0;
                    sector = [0; 512];
                }
            }
        }
        if high.is_some() {
            return Err("the hex text has an odd digit");
        }
        if length == 0 {
            return Err("the file holds no bytes");
        }
        if filled > 0 {
            if lba > self.last {
                return Err("the program region is full");
            }
            io.write_sector(lba, &sector)?;
        }
        let checksum = crc.finish();
        let index = existing.unwrap_or(count);
        let row = &mut table[16 + index * ENTRY_BYTES..16 + (index + 1) * ENTRY_BYTES];
        row.fill(0);
        row[..name.len()].copy_from_slice(name);
        row[16..20].copy_from_slice(&first_free.to_le_bytes());
        row[20..24].copy_from_slice(&length.to_le_bytes());
        row[24..28].copy_from_slice(&checksum.to_le_bytes());
        let rows = if existing.is_some() { count } else { count + 1 } as u32;
        table[8..12].copy_from_slice(&rows.to_le_bytes());
        io.write_sector(self.table, &table)?;
        io.flush()?;
        Ok(Entry {
            start: first_free,
            length,
            checksum,
        })
    }

    /// Every name in the table, for a launcher to show.
    #[cfg(feature = "native-graphics")]
    pub fn list<const N: usize>(
        self,
        storage: &mut ServiceDomain,
    ) -> Result<Listing<N>, &'static str> {
        let mut listing = Listing {
            names: [[0; NAME_BYTES]; N],
            lengths: [0; N],
            count: 0,
        };
        let mut table = [0_u8; 512];
        read_sector(storage, self.table, &mut table)?;
        if !table.starts_with(self.magic) {
            return Ok(listing);
        }
        let count = (u32::from_le_bytes([table[8], table[9], table[10], table[11]]) as usize)
            .min(MAX_ENTRIES)
            .min(N);
        let (rows, _) = table[16..].as_chunks::<ENTRY_BYTES>();
        for (index, entry) in rows.iter().take(count).enumerate() {
            let length = entry[..NAME_BYTES]
                .iter()
                .position(|byte| *byte == 0)
                .unwrap_or(NAME_BYTES);
            listing.names[index][..length].copy_from_slice(&entry[..length]);
            listing.lengths[index] = length as u8;
            listing.count = index + 1;
        }
        Ok(listing)
    }
}

impl Entry {
    /// Read the entry sector by sector, handing each 512-byte piece and its
    /// offset to `sink`, checking the CRC-32 at the end. For readers that
    /// place the bytes straight into a domain's pages: the asset loader.
    #[cfg(feature = "native-graphics")]
    pub fn read_with(
        self,
        storage: &mut ServiceDomain,
        mut sink: impl FnMut(usize, &[u8]),
    ) -> Result<(), &'static str> {
        let length = self.length as usize;
        let sectors = length.div_ceil(512) as u32;
        let mut crc = Crc::new();
        let mut done = 0;
        let mut sector = [0_u8; 512];
        for index in 0..sectors {
            read_sector(storage, self.start + index, &mut sector)?;
            let take = (length - done).min(512);
            crc.update(&sector[..take]);
            sink(done, &sector[..take]);
            done += take;
        }
        if crc.finish() != self.checksum {
            return Err("image checksum does not match its table entry");
        }
        Ok(())
    }
}
