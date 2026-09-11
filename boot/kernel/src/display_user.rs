//! The unprivileged native vector compositor.
//!
//! This code runs in ring 3 with one private stack, one shared command page,
//! immutable program data, and only the VBE framebuffer pages as its device
//! grant. It owns no scene or desktop policy. Every command is checked again
//! here even though the build adapter and supervisor already checked it.

use crate::world::{shared, PAYLOAD_OFFSET};

const RECORD_BYTES: usize = 64;
const MAX_DIMENSION: u32 = 4096;
const STATUS_OK: u64 = 0;
const STATUS_REJECTED: u64 = 1;

#[derive(Clone, Copy)]
struct Surface {
    address: usize,
    width: u32,
    height: u32,
    pitch: u32,
    logical_width: u32,
    logical_height: u32,
    /// Pixels outside this rectangle are left alone: the supervisor asks
    /// for a region when only part of the frame changed.
    clip: Bounds,
}

#[derive(Clone, Copy)]
struct Bounds {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

impl Surface {
    #[inline(always)]
    unsafe fn from_page(page: *mut u64) -> Option<Self> {
        let width = unsafe { page.add(shared::DISPLAY_WIDTH).read_volatile() } as u32;
        let height = unsafe { page.add(shared::DISPLAY_HEIGHT).read_volatile() } as u32;
        let clip_width = unsafe { page.add(shared::CLIP_WIDTH).read_volatile() } as u32;
        let clip = if clip_width == 0 {
            Bounds {
                x: 0,
                y: 0,
                width,
                height,
            }
        } else {
            let x = (unsafe { page.add(shared::CLIP_X).read_volatile() } as u32).min(width);
            let y = (unsafe { page.add(shared::CLIP_Y).read_volatile() } as u32).min(height);
            Bounds {
                x,
                y,
                width: clip_width.min(width - x),
                height: (unsafe { page.add(shared::CLIP_HEIGHT).read_volatile() } as u32)
                    .min(height - y),
            }
        };
        let surface = Self {
            address: unsafe { page.add(shared::DISPLAY_ADDRESS).read_volatile() } as usize,
            width,
            height,
            pitch: unsafe { page.add(shared::DISPLAY_PITCH).read_volatile() } as u32,
            logical_width: unsafe { page.add(shared::DISPLAY_LOGICAL_WIDTH).read_volatile() }
                as u32,
            logical_height: unsafe { page.add(shared::DISPLAY_LOGICAL_HEIGHT).read_volatile() }
                as u32,
            clip,
        };
        if surface.address == 0
            || surface.width == 0
            || surface.height == 0
            || surface.width > MAX_DIMENSION
            || surface.height > MAX_DIMENSION
            || surface.logical_width == 0
            || surface.logical_height == 0
            || surface.pitch < surface.width.saturating_mul(4)
        {
            None
        } else {
            Some(surface)
        }
    }

    #[inline(always)]
    fn x(self, logical: u32) -> u32 {
        ((u64::from(logical) * u64::from(self.width)) / u64::from(self.logical_width)) as u32
    }

    #[inline(always)]
    fn y(self, logical: u32) -> u32 {
        ((u64::from(logical) * u64::from(self.height)) / u64::from(self.logical_height)) as u32
    }

    /// The columns of [x, right) inside the clip.
    #[inline(always)]
    fn columns(self, x: u32, right: u32) -> (u32, u32) {
        (x.max(self.clip.x), right.min(self.clip.x + self.clip.width))
    }

    /// The rows of [y, bottom) inside the clip.
    #[inline(always)]
    fn rows(self, y: u32, bottom: u32) -> (u32, u32) {
        (
            y.max(self.clip.y),
            bottom.min(self.clip.y + self.clip.height),
        )
    }

    #[inline(always)]
    unsafe fn pixel(self, x: u32, y: u32, color: u32) {
        if x < self.width && y < self.height {
            let offset = (y as usize) * (self.pitch as usize) + (x as usize) * 4;
            unsafe { (self.address.wrapping_add(offset) as *mut u32).write_volatile(color) };
        }
    }
}

#[inline(always)]
unsafe fn record_word(record: *const u8, word: usize) -> u32 {
    let offset = word * 4;
    let b0 = unsafe { record.add(offset).read_volatile() };
    let b1 = unsafe { record.add(offset + 1).read_volatile() };
    let b2 = unsafe { record.add(offset + 2).read_volatile() };
    let b3 = unsafe { record.add(offset + 3).read_volatile() };
    u32::from_le_bytes([b0, b1, b2, b3])
}

#[inline(always)]
fn valid_color(color: u32) -> bool {
    color <= 0x00ff_ffff
}

#[inline(always)]
fn channel(color: u32, shift: u32) -> u32 {
    (color >> shift) & 0xff
}

#[inline(always)]
fn gradient(start: u32, end: u32, step: u32, steps: u32) -> u32 {
    if steps == 0 {
        return start;
    }
    let mix = |shift| {
        let left = channel(start, shift);
        let right = channel(end, shift);
        if right >= left {
            left + ((right - left) * step / steps)
        } else {
            left - ((left - right) * step / steps)
        }
    };
    (mix(16) << 16) | (mix(8) << 8) | mix(0)
}

#[inline(always)]
unsafe fn fill_rect(surface: Surface, x: u32, y: u32, width: u32, height: u32, color: u32) {
    let (start_y, bottom) = surface.rows(y, y.saturating_add(height).min(surface.height));
    let (start_x, right) = surface.columns(x, x.saturating_add(width).min(surface.width));
    let mut py = start_y;
    while py < bottom {
        let mut px = start_x;
        while px < right {
            unsafe { surface.pixel(px, py, color) };
            px += 1;
        }
        py += 1;
    }
}

#[inline(always)]
fn inside_round(px: u32, py: u32, width: u32, height: u32, radius: u32) -> bool {
    let radius = radius.min(width / 2).min(height / 2);
    if radius == 0 {
        return true;
    }
    let (dx, dy) = if px < radius && py < radius {
        (radius - px, radius - py)
    } else if px >= width - radius && py < radius {
        (px - (width - radius - 1), radius - py)
    } else if px < radius && py >= height - radius {
        (radius - px, py - (height - radius - 1))
    } else if px >= width - radius && py >= height - radius {
        (px - (width - radius - 1), py - (height - radius - 1))
    } else {
        return true;
    };
    u64::from(dx) * u64::from(dx) + u64::from(dy) * u64::from(dy)
        <= u64::from(radius) * u64::from(radius)
}

#[inline(always)]
unsafe fn rounded(surface: Surface, bounds: Bounds, radius: u32, start: u32, end: u32) {
    let Bounds {
        x,
        y,
        width,
        height,
    } = bounds;
    if width == 0 || height == 0 {
        return;
    }
    let (start_y, bottom) = surface.rows(y, y.saturating_add(height).min(surface.height));
    let (start_x, right) = surface.columns(x, x.saturating_add(width).min(surface.width));
    let mut py = start_y;
    while py < bottom {
        let color = gradient(start, end, py - y, height.saturating_sub(1));
        let mut px = start_x;
        while px < right {
            if inside_round(px - x, py - y, width, height, radius) {
                unsafe { surface.pixel(px, py, color) };
            }
            px += 1;
        }
        py += 1;
    }
}

#[inline(always)]
unsafe fn ellipse(surface: Surface, cx: u32, cy: u32, rx: u32, ry: u32, color: u32) {
    if rx == 0 || ry == 0 {
        return;
    }
    let (top, bottom) = surface.rows(
        cy.saturating_sub(ry),
        cy.saturating_add(ry).min(surface.height),
    );
    let (left, right) = surface.columns(
        cx.saturating_sub(rx),
        cx.saturating_add(rx).min(surface.width),
    );
    let rx2 = u64::from(rx) * u64::from(rx);
    let ry2 = u64::from(ry) * u64::from(ry);
    let limit = rx2 * ry2;
    let mut y = top;
    while y < bottom {
        let dy = y.abs_diff(cy);
        let mut x = left;
        while x < right {
            let dx = x.abs_diff(cx);
            if u64::from(dx) * u64::from(dx) * ry2 + u64::from(dy) * u64::from(dy) * rx2 <= limit {
                unsafe { surface.pixel(x, y, color) };
            }
            x += 1;
        }
        y += 1;
    }
}

#[inline(always)]
fn glyph(character: u8) -> [u8; 7] {
    match character.to_ascii_uppercase() {
        b'A' => [14, 17, 17, 31, 17, 17, 17],
        b'B' => [30, 17, 17, 30, 17, 17, 30],
        b'C' => [14, 17, 16, 16, 16, 17, 14],
        b'D' => [30, 17, 17, 17, 17, 17, 30],
        b'E' => [31, 16, 16, 30, 16, 16, 31],
        b'F' => [31, 16, 16, 30, 16, 16, 16],
        b'G' => [14, 17, 16, 23, 17, 17, 14],
        b'H' => [17, 17, 17, 31, 17, 17, 17],
        b'I' => [31, 4, 4, 4, 4, 4, 31],
        b'J' => [7, 2, 2, 2, 18, 18, 12],
        b'K' => [17, 18, 20, 24, 20, 18, 17],
        b'L' => [16, 16, 16, 16, 16, 16, 31],
        b'M' => [17, 27, 21, 21, 17, 17, 17],
        b'N' => [17, 25, 21, 19, 17, 17, 17],
        b'O' => [14, 17, 17, 17, 17, 17, 14],
        b'P' => [30, 17, 17, 30, 16, 16, 16],
        b'Q' => [14, 17, 17, 17, 21, 18, 13],
        b'R' => [30, 17, 17, 30, 20, 18, 17],
        b'S' => [15, 16, 16, 14, 1, 1, 30],
        b'T' => [31, 4, 4, 4, 4, 4, 4],
        b'U' => [17, 17, 17, 17, 17, 17, 14],
        b'V' => [17, 17, 17, 17, 17, 10, 4],
        b'W' => [17, 17, 17, 21, 21, 21, 10],
        b'X' => [17, 17, 10, 4, 10, 17, 17],
        b'Y' => [17, 17, 10, 4, 4, 4, 4],
        b'Z' => [31, 1, 2, 4, 8, 16, 31],
        b'0' => [14, 17, 19, 21, 25, 17, 14],
        b'1' => [4, 12, 4, 4, 4, 4, 14],
        b'2' => [14, 17, 1, 2, 4, 8, 31],
        b'3' => [30, 1, 1, 14, 1, 1, 30],
        b'4' => [2, 6, 10, 18, 31, 2, 2],
        b'5' => [31, 16, 16, 30, 1, 1, 30],
        b'6' => [14, 16, 16, 30, 17, 17, 14],
        b'7' => [31, 1, 2, 4, 8, 8, 8],
        b'8' => [14, 17, 17, 14, 17, 17, 14],
        b'9' => [14, 17, 17, 15, 1, 1, 14],
        b'-' => [0, 0, 0, 31, 0, 0, 0],
        b'+' => [0, 4, 4, 31, 4, 4, 0],
        b'*' => [0, 21, 14, 31, 14, 21, 0],
        b'/' => [1, 2, 2, 4, 8, 8, 16],
        b'=' => [0, 0, 31, 0, 31, 0, 0],
        b'<' => [1, 2, 4, 8, 4, 2, 1],
        b':' => [0, 4, 4, 0, 4, 4, 0],
        b';' => [0, 4, 4, 0, 4, 4, 8],
        b'#' => [10, 31, 10, 10, 31, 10, 0],
        b'?' => [14, 17, 1, 2, 4, 0, 4],
        b'!' => [4, 4, 4, 4, 4, 0, 4],
        b'\'' => [4, 4, 8, 0, 0, 0, 0],
        b'[' => [14, 8, 8, 8, 8, 8, 14],
        b']' => [14, 2, 2, 2, 2, 2, 14],
        b'_' => [0, 0, 0, 0, 0, 0, 31],
        b'.' => [0, 0, 0, 0, 0, 12, 12],
        b',' => [0, 0, 0, 0, 4, 4, 8],
        b'(' => [2, 4, 8, 8, 8, 4, 2],
        b')' => [8, 4, 2, 2, 2, 4, 8],
        b'"' => [10, 10, 0, 0, 0, 0, 0],
        b'>' => [16, 8, 4, 2, 4, 8, 16],
        b' ' => [0; 7],
        _ => [31, 17, 21, 21, 17, 17, 31],
    }
}

#[inline(always)]
unsafe fn text(
    surface: Surface,
    record: *const u8,
    x: u32,
    y: u32,
    scale: u32,
    color: u32,
    length: usize,
) {
    let mut index = 0;
    while index < length {
        let character = unsafe { record.add(36 + index).read_volatile() };
        let rows = glyph(character);
        let mut row = 0;
        while row < 7 {
            let mut column = 0;
            while column < 5 {
                if rows[row] & (1 << (4 - column)) != 0 {
                    unsafe {
                        fill_rect(
                            surface,
                            x + (index as u32 * 6 + column as u32) * scale,
                            y + row as u32 * scale,
                            scale,
                            scale,
                            color,
                        )
                    };
                }
                column += 1;
            }
            row += 1;
        }
        index += 1;
    }
}

/// Blend `color` over the pixel at (x, y) with `alpha` in 0..=256.
#[inline(always)]
unsafe fn blend(surface: Surface, x: u32, y: u32, color: u32, alpha: u32) {
    let clip = surface.clip;
    if x >= surface.width
        || y >= surface.height
        || alpha == 0
        || x < clip.x
        || y < clip.y
        || x >= clip.x + clip.width
        || y >= clip.y + clip.height
    {
        return;
    }
    let offset = (y as usize) * (surface.pitch as usize) + (x as usize) * 4;
    let address = surface.address.wrapping_add(offset) as *mut u32;
    if alpha >= 256 {
        unsafe { address.write_volatile(color) };
        return;
    }
    let below = unsafe { address.read_volatile() };
    let mix = |shift| {
        let over = channel(color, shift);
        let under = channel(below, shift);
        (over * alpha + under * (256 - alpha)) >> 8
    };
    unsafe { address.write_volatile((mix(16) << 16) | (mix(8) << 8) | mix(0)) };
}

#[inline(always)]
fn isqrt(value: u64) -> u64 {
    if value < 2 {
        return value;
    }
    let mut low = 1_u64;
    let mut high = (value >> 1).min(0xffff_ffff) + 1;
    while low < high {
        let mid = low + (high - low) / 2;
        if mid * mid <= value {
            low = mid + 1;
        } else {
            high = mid;
        }
    }
    low - 1
}

/// How much of the pixel at (px, py) inside a width x height box with
/// rounded corners of `radius` is covered, in 0..=256: full inside, a
/// one-pixel ramp along the arcs, nothing outside them.
#[inline(always)]
fn coverage(px: u32, py: u32, width: u32, height: u32, radius: u32) -> u32 {
    let radius = radius.min(width / 2).min(height / 2);
    if radius == 0 {
        return 256;
    }
    // Distances in half pixels from the pixel's centre to the arc's centre.
    let (dx, dy) = if px < radius && py < radius {
        (2 * radius - (2 * px + 1), 2 * radius - (2 * py + 1))
    } else if px >= width - radius && py < radius {
        (
            (2 * px + 1) - 2 * (width - radius),
            2 * radius - (2 * py + 1),
        )
    } else if px < radius && py >= height - radius {
        (
            2 * radius - (2 * px + 1),
            (2 * py + 1) - 2 * (height - radius),
        )
    } else if px >= width - radius && py >= height - radius {
        (
            (2 * px + 1) - 2 * (width - radius),
            (2 * py + 1) - 2 * (height - radius),
        )
    } else {
        return 256;
    };
    let squared = u64::from(dx) * u64::from(dx) + u64::from(dy) * u64::from(dy);
    let edge = u64::from(2 * radius) + 1;
    // Only the two-half-pixel ramp along the arc needs the root; inside it
    // the pixel is covered and outside it is not, which a comparison of
    // squares decides. The root would otherwise run for every pixel of a
    // large rounded surface, most of which are nowhere near an arc.
    if squared >= edge * edge {
        return 0;
    }
    if edge >= 2 && squared <= (edge - 2) * (edge - 2) {
        return 256;
    }
    let distance = isqrt(squared);
    (((edge - distance) * 128).min(256)) as u32
}

/// A rounded box blended over the framebuffer with anti-aliased corners.
#[inline(always)]
unsafe fn surface_box(surface: Surface, bounds: Bounds, radius: u32, color: u32, alpha: u32) {
    let Bounds {
        x,
        y,
        width,
        height,
    } = bounds;
    let (start_y, bottom) = surface.rows(y, y.saturating_add(height).min(surface.height));
    let (start_x, right) = surface.columns(x, x.saturating_add(width).min(surface.width));
    let mut py = start_y;
    while py < bottom {
        let mut px = start_x;
        while px < right {
            let cover = coverage(px - x, py - y, width, height, radius);
            unsafe { blend(surface, px, py, color, alpha * cover / 256) };
            px += 1;
        }
        py += 1;
    }
}

/// The one-pixel ring between a rounded box and the box one pixel inside
/// it: the rows above and below the inner box in full, and for the rows the
/// inner box spans only the columns outside it. A shadow is many of these.
#[inline(always)]
unsafe fn surface_ring(surface: Surface, outer: Bounds, radius: u32, color: u32, alpha: u32) {
    let Bounds {
        x,
        y,
        width,
        height,
    } = outer;
    if width < 3 || height < 3 {
        unsafe { surface_box(surface, outer, radius, color, alpha) };
        return;
    }
    let (start_y, bottom) = surface.rows(y, y.saturating_add(height).min(surface.height));
    let (start_x, right) = surface.columns(x, x.saturating_add(width).min(surface.width));
    let inner_radius = radius.saturating_sub(1);
    let mut py = start_y;
    while py < bottom {
        let row = py - y;
        let full = row == 0 || row + 1 == height;
        let mut px = start_x;
        while px < right {
            let column = px - x;
            let edge = full || column == 0 || column + 1 == width;
            // Inside the inner box's straight span nothing is drawn; near
            // its arcs the inner coverage says how much this ring adds.
            let cover = if edge {
                coverage(column, row, width, height, radius)
            } else {
                let inner = coverage(column - 1, row - 1, width - 2, height - 2, inner_radius);
                if inner == 256 {
                    px += 1;
                    continue;
                }
                coverage(column, row, width, height, radius).saturating_sub(inner)
            };
            unsafe { blend(surface, px, py, color, alpha * cover / 256) };
            px += 1;
        }
        py += 1;
    }
}

#[inline(always)]
unsafe fn asset_byte(address: usize, bytes: usize, offset: usize) -> Option<u8> {
    if offset >= bytes {
        return None;
    }
    Some(unsafe { ((address + offset) as *const u8).read_volatile() })
}

#[inline(always)]
unsafe fn asset_u16(address: usize, bytes: usize, offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes([
        unsafe { asset_byte(address, bytes, offset)? },
        unsafe { asset_byte(address, bytes, offset + 1)? },
    ]))
}

#[inline(always)]
unsafe fn asset_u32(address: usize, bytes: usize, offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes([
        unsafe { asset_byte(address, bytes, offset)? },
        unsafe { asset_byte(address, bytes, offset + 1)? },
        unsafe { asset_byte(address, bytes, offset + 2)? },
        unsafe { asset_byte(address, bytes, offset + 3)? },
    ]))
}

/// Text from a font atlas the supervisor mapped: every offset the atlas
/// names is checked against the length the supervisor gave, so a broken or
/// hostile atlas draws nothing and cannot read outside itself.
/// A label's placement and style, from its record.
#[derive(Clone, Copy)]
struct Label {
    x: u32,
    y: u32,
    face: u32,
    size: u32,
    color: u32,
    alpha: u32,
    length: usize,
}

#[inline(always)]
unsafe fn label(surface: Surface, page: *mut u64, record: *const u8, style: Label) -> bool {
    let Label {
        x,
        y,
        face,
        size,
        color,
        alpha,
        length,
    } = style;
    if face as usize >= shared::SPRITE_SLOT {
        return false;
    }
    let slot = shared::ASSET_WORDS + 2 * face as usize;
    let address = unsafe { page.add(slot).read_volatile() } as usize;
    let bytes = unsafe { page.add(slot + 1).read_volatile() } as usize;
    if address == 0 || bytes < 12 {
        return false;
    }
    let magic = [
        unsafe { asset_byte(address, bytes, 0) },
        unsafe { asset_byte(address, bytes, 1) },
        unsafe { asset_byte(address, bytes, 2) },
        unsafe { asset_byte(address, bytes, 3) },
    ];
    if magic != [Some(b'A'), Some(b'G'), Some(b'F'), Some(b'1')] {
        return false;
    }
    let Some(size_count) = (unsafe { asset_u16(address, bytes, 4) }) else {
        return false;
    };
    let mut found = None;
    let mut index = 0;
    while index < usize::from(size_count).min(8) {
        let at = 12 + index * 16;
        if unsafe { asset_u16(address, bytes, at) } == Some(size as u16) {
            found = Some(at);
            break;
        }
        index += 1;
    }
    let Some(at) = found else {
        return false;
    };
    let ascent = unsafe { asset_u16(address, bytes, at + 2) }.unwrap_or(0) as i16 as i32;
    let Some(table) = (unsafe { asset_u32(address, bytes, at + 8) }) else {
        return false;
    };
    let baseline = y as i32 + ascent;
    let mut pen = x as i32;
    let mut index = 0;
    while index < length {
        let character = unsafe { record.add(36 + index).read_volatile() };
        let glyph = if (32..127).contains(&character) {
            usize::from(character - 32)
        } else {
            95
        };
        let row = table as usize + glyph * 12;
        let (
            Some(advance),
            Some(bearing_x),
            Some(bearing_y),
            Some(width),
            Some(height),
            Some(offset),
        ) = (
            unsafe { asset_byte(address, bytes, row) },
            unsafe { asset_byte(address, bytes, row + 1) },
            unsafe { asset_byte(address, bytes, row + 2) },
            unsafe { asset_byte(address, bytes, row + 3) },
            unsafe { asset_byte(address, bytes, row + 4) },
            unsafe { asset_u32(address, bytes, row + 8) },
        )
        else {
            return false;
        };
        let (width, height) = (usize::from(width), usize::from(height));
        let offset = offset as usize;
        if offset.saturating_add(width * height) > bytes {
            return false;
        }
        let left = pen + i32::from(bearing_x as i8);
        let top = baseline - i32::from(bearing_y as i8);
        let clip = surface.clip;
        let outside = left >= (clip.x + clip.width) as i32
            || top >= (clip.y + clip.height) as i32
            || left + width as i32 <= clip.x as i32
            || top + height as i32 <= clip.y as i32;
        if outside {
            pen += i32::from(advance);
            index += 1;
            continue;
        }
        let mut gy = 0;
        while gy < height {
            let mut gx = 0;
            while gx < width {
                let cover =
                    unsafe { ((address + offset + gy * width + gx) as *const u8).read_volatile() };
                if cover != 0 {
                    let px = left + gx as i32;
                    let py = top + gy as i32;
                    if px >= 0 && py >= 0 {
                        let mixed = (u32::from(cover) + 1) * alpha / 256;
                        unsafe { blend(surface, px as u32, py as u32, color, mixed) };
                    }
                }
                gx += 1;
            }
            gy += 1;
        }
        pen += i32::from(advance);
        index += 1;
    }
    true
}

/// A sprite from the sheet the supervisor mapped, blended by its own alpha
/// and, when `tint` is not black, drawn in that colour rather than its own:
/// how a white glyph becomes a grey control or an accent icon. Every
/// offset is checked against the sheet's length.
#[inline(always)]
unsafe fn sprite(
    surface: Surface,
    page: *mut u64,
    x: u32,
    y: u32,
    index: u32,
    tint: u32,
    alpha: u32,
) -> bool {
    let slot = shared::ASSET_WORDS + 2 * shared::SPRITE_SLOT;
    let address = unsafe { page.add(slot).read_volatile() } as usize;
    let bytes = unsafe { page.add(slot + 1).read_volatile() } as usize;
    if address == 0 || bytes < 8 {
        return false;
    }
    let magic = [
        unsafe { asset_byte(address, bytes, 0) },
        unsafe { asset_byte(address, bytes, 1) },
        unsafe { asset_byte(address, bytes, 2) },
        unsafe { asset_byte(address, bytes, 3) },
    ];
    if magic != [Some(b'A'), Some(b'G'), Some(b'I'), Some(b'1')] {
        return false;
    }
    let Some(count) = (unsafe { asset_u16(address, bytes, 4) }) else {
        return false;
    };
    if index >= u32::from(count) {
        return false;
    }
    let row = 8 + index as usize * 16;
    let (Some(width), Some(height), Some(offset)) = (
        unsafe { asset_u16(address, bytes, row) },
        unsafe { asset_u16(address, bytes, row + 2) },
        unsafe { asset_u32(address, bytes, row + 4) },
    ) else {
        return false;
    };
    let (width, height, offset) = (usize::from(width), usize::from(height), offset as usize);
    if offset.saturating_add(width * height * 4) > bytes {
        return false;
    }
    let clip = surface.clip;
    if x >= clip.x + clip.width
        || y >= clip.y + clip.height
        || x + width as u32 <= clip.x
        || y + height as u32 <= clip.y
    {
        return true;
    }
    let mut sy = 0;
    while sy < height {
        let mut sx = 0;
        while sx < width {
            let at = address + offset + (sy * width + sx) * 4;
            let r = unsafe { (at as *const u8).read_volatile() };
            let g = unsafe { ((at + 1) as *const u8).read_volatile() };
            let b = unsafe { ((at + 2) as *const u8).read_volatile() };
            let a = unsafe { ((at + 3) as *const u8).read_volatile() };
            if a != 0 {
                let color = if tint != 0 {
                    tint
                } else {
                    (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)
                };
                let mixed = (u32::from(a) + 1) * alpha / 256;
                unsafe { blend(surface, x + sx as u32, y + sy as u32, color, mixed) };
            }
            sx += 1;
        }
        sy += 1;
    }
    true
}

#[inline(always)]
unsafe fn draw(surface: Surface, page: *mut u64, record: *const u8, bytes: usize) -> bool {
    if bytes != RECORD_BYTES {
        return false;
    }
    let operation = unsafe { record_word(record, 0) };
    if operation == 1 {
        let start = unsafe { record_word(record, 1) };
        let end = unsafe { record_word(record, 2) };
        if !valid_color(start) || !valid_color(end) {
            return false;
        }
        let (mut y, bottom) = surface.rows(0, surface.height);
        while y < bottom {
            let color = gradient(start, end, y, surface.height.saturating_sub(1));
            unsafe { fill_rect(surface, 0, y, surface.width, 1, color) };
            y += 1;
        }
        return true;
    }
    let lx = unsafe { record_word(record, 1) };
    let ly = unsafe { record_word(record, 2) };
    if lx > surface.logical_width || ly > surface.logical_height {
        return false;
    }
    if operation == 2 || operation == 3 {
        let lw = unsafe { record_word(record, 3) };
        let lh = unsafe { record_word(record, 4) };
        let lr = unsafe { record_word(record, 5) };
        let start = unsafe { record_word(record, 6) };
        let end = if operation == 3 {
            unsafe { record_word(record, 7) }
        } else {
            start
        };
        if lw == 0
            || lh == 0
            || lw > surface.logical_width
            || lh > surface.logical_height
            || lr > lw.min(lh) / 2
            || !valid_color(start)
            || !valid_color(end)
        {
            return false;
        }
        unsafe {
            rounded(
                surface,
                Bounds {
                    x: surface.x(lx),
                    y: surface.y(ly),
                    width: surface.x(lw).max(1),
                    height: surface.y(lh).max(1),
                },
                surface.x(lr),
                start,
                end,
            )
        };
        true
    } else if operation == 4 {
        let lrx = unsafe { record_word(record, 3) };
        let lry = unsafe { record_word(record, 4) };
        let color = unsafe { record_word(record, 5) };
        if lrx == 0 || lry == 0 || !valid_color(color) {
            return false;
        }
        unsafe {
            ellipse(
                surface,
                surface.x(lx),
                surface.y(ly),
                surface.x(lrx).max(1),
                surface.y(lry).max(1),
                color,
            )
        };
        true
    } else if operation == 6 {
        let face = unsafe { record_word(record, 3) };
        let size = unsafe { record_word(record, 4) };
        let color = unsafe { record_word(record, 5) };
        let alpha = unsafe { record_word(record, 6) };
        let length = unsafe { record_word(record, 8) } as usize;
        if length > 28 || !valid_color(color) || alpha > 255 || size == 0 || size > 255 {
            return false;
        }
        let alpha = if alpha == 0 { 256 } else { alpha + 1 };
        unsafe {
            label(
                surface,
                page,
                record,
                Label {
                    x: surface.x(lx),
                    y: surface.y(ly),
                    face,
                    size,
                    color,
                    alpha,
                    length,
                },
            )
        }
    } else if operation == 7 || operation == 8 {
        let lw = unsafe { record_word(record, 3) };
        let lh = unsafe { record_word(record, 4) };
        let lr = unsafe { record_word(record, 5) };
        let sixth = unsafe { record_word(record, 6) };
        let alpha = unsafe { record_word(record, 7) };
        if lw == 0
            || lh == 0
            || lw > surface.logical_width
            || lh > surface.logical_height
            || lr > lw.min(lh) / 2
            || alpha > 255
        {
            return false;
        }
        let alpha = if alpha == 0 { 256 } else { alpha + 1 };
        let bounds = Bounds {
            x: surface.x(lx),
            y: surface.y(ly),
            width: surface.x(lw).max(1),
            height: surface.y(lh).max(1),
        };
        let radius = surface.x(lr);
        if operation == 7 {
            if !valid_color(sixth) {
                return false;
            }
            unsafe { surface_box(surface, bounds, radius, sixth, alpha) };
        } else {
            // A shadow: rings of a black box growing outward, each fainter
            // by the square of its distance, so a pixel `d` out of `blur`
            // carries the sum of the rings beyond it: dense at the box,
            // tailing off softly, as a Gaussian blur of the box would.
            // The box itself is left for whatever is drawn on top.
            let blur = sixth;
            if blur == 0 || blur > 64 {
                return false;
            }
            let total = blur * (blur + 1) * (2 * blur + 1) / 6 + 1;
            let mut step = blur;
            while step > 0 {
                let outer = Bounds {
                    x: bounds.x.saturating_sub(step),
                    y: bounds.y.saturating_sub(step),
                    width: bounds.width + 2 * step,
                    height: bounds.height + 2 * step,
                };
                let weight = (blur + 1 - step) * (blur + 1 - step);
                let ring_alpha = alpha * weight / total;
                unsafe { surface_ring(surface, outer, radius + step, 0, ring_alpha.max(1)) };
                step -= 1;
            }
        }
        true
    } else if operation == 9 {
        let index = unsafe { record_word(record, 3) };
        let tint = unsafe { record_word(record, 4) };
        let alpha = unsafe { record_word(record, 5) };
        if !valid_color(tint) || alpha > 255 {
            return false;
        }
        let alpha = if alpha == 0 { 256 } else { alpha + 1 };
        unsafe {
            sprite(
                surface,
                page,
                surface.x(lx),
                surface.y(ly),
                index,
                tint,
                alpha,
            )
        }
    } else if operation == 5 {
        let scale = unsafe { record_word(record, 3) };
        let color = unsafe { record_word(record, 4) };
        let length = unsafe { record_word(record, 8) } as usize;
        if scale == 0 || scale > 8 || length > 28 || !valid_color(color) {
            return false;
        }
        let physical_scale = surface.x(scale).max(1);
        unsafe {
            text(
                surface,
                record,
                surface.x(lx),
                surface.y(ly),
                physical_scale,
                color,
                length,
            )
        };
        true
    } else {
        false
    }
}

#[inline(always)]
unsafe fn checksum(surface: Surface) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    let mut y = 0;
    while y < surface.height {
        let mut x = 0;
        while x < surface.width {
            let offset = (y as usize) * (surface.pitch as usize) + (x as usize) * 4;
            let value =
                unsafe { (surface.address.wrapping_add(offset) as *const u32).read_volatile() };
            hash ^= u64::from(value);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            x += 1;
        }
        y += 1;
    }
    hash
}

/// Persistent ring-3 compositor entry point.
///
/// # Safety
/// Entered with the private mappings established by `Domain::new_display`.
#[no_mangle]
#[link_section = ".user_text"]
pub unsafe extern "C" fn agel_compositor_main(shared_page: u64) -> ! {
    let page = shared_page as *mut u64;
    loop {
        let command = unsafe { page.add(shared::COMMAND).read_volatile() };
        let Some(surface) = (unsafe { Surface::from_page(page) }) else {
            unsafe { page.add(shared::STATUS).write_volatile(STATUS_REJECTED) };
            unsafe { crate::user::yield_to_supervisor() };
            continue;
        };
        if command == shared::COMMAND_DISPLAY_DRAW {
            let bytes = unsafe { page.add(shared::ARGUMENTS).read_volatile() } as usize;
            let record = (shared_page as usize + PAYLOAD_OFFSET) as *const u8;
            let accepted = unsafe { draw(surface, page, record, bytes) };
            unsafe {
                page.add(shared::STATUS).write_volatile(if accepted {
                    STATUS_OK
                } else {
                    STATUS_REJECTED
                })
            };
        } else if command == shared::COMMAND_DISPLAY_CHECKSUM {
            let value = unsafe { checksum(surface) };
            unsafe {
                page.add(shared::STATUS).write_volatile(STATUS_OK);
                page.add(shared::VALUES).write_volatile(value);
            }
        } else if command == shared::COMMAND_DISPLAY_FAULT {
            unsafe { (crate::arch::KERNEL_PROBE_ADDRESS as *mut u64).write_volatile(0xdead) };
        } else {
            unsafe { page.add(shared::STATUS).write_volatile(STATUS_REJECTED) };
        }
        unsafe { crate::user::yield_to_supervisor() };
    }
}
