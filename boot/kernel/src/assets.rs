//! Assets the compositor draws with: font atlases read from the disk's
//! asset region and mapped read-only into the compositor's address space.
//! The supervisor keeps each face's metrics, so it can lay text out; the
//! compositor reads the bitmaps, checking every offset against the length
//! it was told, because a region on the disk is data and nothing more.

use crate::arch;
use crate::memory::Access;
use crate::region::Region;
use crate::service::ServiceDomain;
use crate::world::shared;

/// The asset region: sectors 3072 through 6143.
pub const REGION: Region = Region {
    table: 3072,
    last: 6143,
    magic: b"AGELAS1\0",
};
pub const MAX_SIZES: usize = 8;
/// Printable ASCII, 32 to 127; the last is the replacement glyph.
pub const GLYPHS: usize = 96;
const PAGE: u64 = 4096;
const MAX_PAGES: usize = 128;
const HEADER_BYTES: usize = 12;
const SIZE_BYTES: usize = 16;
const GLYPH_BYTES: usize = 12;

/// One pixel size of a face, as the supervisor needs it for layout.
#[derive(Clone, Copy)]
pub struct SizeMetrics {
    pub px: u16,
    pub ascent: i16,
    pub line_height: i16,
}

/// A loaded face: which sizes it has and every glyph's advance at each.
#[derive(Clone, Copy)]
pub struct Face {
    pub loaded: bool,
    pub bytes: u32,
    pub size_count: usize,
    pub sizes: [SizeMetrics; MAX_SIZES],
    pub advances: [[u8; GLYPHS]; MAX_SIZES],
}

impl Face {
    pub const EMPTY: Self = Self {
        loaded: false,
        bytes: 0,
        size_count: 0,
        sizes: [SizeMetrics {
            px: 0,
            ascent: 0,
            line_height: 0,
        }; MAX_SIZES],
        advances: [[0; GLYPHS]; MAX_SIZES],
    };

    /// The size table's index for `px`, if the face has that size.
    pub fn size_index(&self, px: u16) -> Option<usize> {
        self.sizes[..self.size_count]
            .iter()
            .position(|size| size.px == px)
    }

    /// The line height at `px`: what one line of text advances the next.
    pub fn line_height(&self, px: u16) -> u32 {
        self.size_index(px)
            .map_or(0, |index| self.sizes[index].line_height.max(0) as u32)
    }

    /// The ascent at `px`: the baseline's distance below the line's top.
    pub fn ascent(&self, px: u16) -> u32 {
        self.size_index(px)
            .map_or(0, |index| self.sizes[index].ascent.max(0) as u32)
    }

    /// The width of `text` at `px`, in pixels; zero for a size not held.
    pub fn measure(&self, px: u16, text: &[u8]) -> u32 {
        if !self.loaded {
            return 0;
        }
        let Some(index) = self.size_index(px) else {
            return 0;
        };
        text.iter()
            .map(|byte| {
                let glyph = glyph_index(*byte);
                u32::from(self.advances[index][glyph])
            })
            .sum()
    }
}

/// Which glyph row a byte uses: printable ASCII, or the replacement.
pub fn glyph_index(byte: u8) -> usize {
    if (32..127).contains(&byte) {
        usize::from(byte - 32)
    } else {
        GLYPHS - 1
    }
}

/// Read one byte of a loaded asset through the identity mapping of its
/// frames, which the supervisor filled.
fn byte_at(frames: &[u64; MAX_PAGES], offset: usize) -> u8 {
    let frame = frames[offset / PAGE as usize];
    // Safety: the frame came from the pool, is identity mapped for the
    // kernel, and the offset within it is below PAGE.
    unsafe { ((frame as usize + offset % PAGE as usize) as *const u8).read_volatile() }
}

fn u16_at(frames: &[u64; MAX_PAGES], offset: usize) -> u16 {
    u16::from_le_bytes([byte_at(frames, offset), byte_at(frames, offset + 1)])
}

fn u32_at(frames: &[u64; MAX_PAGES], offset: usize) -> u32 {
    u32::from_le_bytes([
        byte_at(frames, offset),
        byte_at(frames, offset + 1),
        byte_at(frames, offset + 2),
        byte_at(frames, offset + 3),
    ])
}

/// Load the named atlas into slot `slot` of the compositor's asset window,
/// tell the compositor where it is, and return the face's metrics.
pub fn load_face(
    machine: &mut arch::Machine,
    storage: &mut ServiceDomain,
    domain: &mut arch::Domain,
    name: &[u8],
    slot: usize,
) -> Result<Face, &'static str> {
    let entry = REGION
        .find(storage, name)?
        .ok_or("asset is not in the asset region")?;
    let length = entry.length as usize;
    let pages = length.div_ceil(PAGE as usize);
    if pages == 0 || pages > MAX_PAGES || (pages as u64) * PAGE > arch::ASSET_SLOT_BYTES {
        return Err("asset is larger than its slot");
    }
    let base = arch::ASSET_BASE + slot as u64 * arch::ASSET_SLOT_BYTES;
    let mut frames = [0_u64; MAX_PAGES];
    for (page, frame) in frames.iter_mut().enumerate().take(pages) {
        *frame =
            machine.map_process_page(domain, base + page as u64 * PAGE, Access::UserReadOnly)?;
    }
    entry.read_with(storage, |offset, bytes| {
        let frame = frames[offset / PAGE as usize];
        let inside = offset % PAGE as usize;
        for (index, byte) in bytes.iter().enumerate() {
            // Safety: as in `byte_at`, and a sector never crosses a page.
            unsafe { ((frame as usize + inside + index) as *mut u8).write_volatile(*byte) };
        }
    })?;
    // The header, checked here so layout never trusts a broken atlas; the
    // compositor checks it again for itself.
    if length < HEADER_BYTES
        || byte_at(&frames, 0) != b'A'
        || byte_at(&frames, 1) != b'G'
        || byte_at(&frames, 2) != b'F'
        || byte_at(&frames, 3) != b'1'
    {
        return Err("asset is not a font atlas");
    }
    let size_count = usize::from(u16_at(&frames, 4));
    let glyph_count = usize::from(u16_at(&frames, 6));
    if size_count == 0 || size_count > MAX_SIZES || glyph_count != GLYPHS {
        return Err("font atlas has an unsupported shape");
    }
    let mut face = Face {
        loaded: true,
        bytes: entry.length,
        size_count,
        ..Face::EMPTY
    };
    for index in 0..size_count {
        let at = HEADER_BYTES + index * SIZE_BYTES;
        if at + SIZE_BYTES > length {
            return Err("font atlas size table is truncated");
        }
        let table = u32_at(&frames, at + 8) as usize;
        if table + GLYPHS * GLYPH_BYTES > length {
            return Err("font atlas glyph table is truncated");
        }
        face.sizes[index] = SizeMetrics {
            px: u16_at(&frames, at),
            ascent: u16_at(&frames, at + 2) as i16,
            line_height: u16_at(&frames, at + 6) as i16,
        };
        for glyph in 0..GLYPHS {
            face.advances[index][glyph] = byte_at(&frames, table + glyph * GLYPH_BYTES);
        }
    }
    let core = domain.core();
    core.write_shared(shared::FACE_WORDS + slot * 2, base);
    core.write_shared(shared::FACE_WORDS + slot * 2 + 1, u64::from(entry.length));
    Ok(face)
}
