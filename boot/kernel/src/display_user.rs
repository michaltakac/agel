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
        let surface = Self {
            address: unsafe { page.add(shared::DISPLAY_ADDRESS).read_volatile() } as usize,
            width: unsafe { page.add(shared::DISPLAY_WIDTH).read_volatile() } as u32,
            height: unsafe { page.add(shared::DISPLAY_HEIGHT).read_volatile() } as u32,
            pitch: unsafe { page.add(shared::DISPLAY_PITCH).read_volatile() } as u32,
            logical_width: unsafe { page.add(shared::DISPLAY_LOGICAL_WIDTH).read_volatile() }
                as u32,
            logical_height: unsafe { page.add(shared::DISPLAY_LOGICAL_HEIGHT).read_volatile() }
                as u32,
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
    let right = x.saturating_add(width).min(surface.width);
    let bottom = y.saturating_add(height).min(surface.height);
    let mut py = y.min(surface.height);
    while py < bottom {
        let mut px = x.min(surface.width);
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
    let right = x.saturating_add(width).min(surface.width);
    let bottom = y.saturating_add(height).min(surface.height);
    let mut py = y.min(surface.height);
    while py < bottom {
        let color = gradient(start, end, py - y, height.saturating_sub(1));
        let mut px = x.min(surface.width);
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
    let left = cx.saturating_sub(rx);
    let top = cy.saturating_sub(ry);
    let right = cx.saturating_add(rx).min(surface.width);
    let bottom = cy.saturating_add(ry).min(surface.height);
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
    match character {
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
        b' ' => [0; 7],
        _ => [0, 0, 0, 31, 0, 0, 0],
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

#[inline(always)]
unsafe fn draw(surface: Surface, record: *const u8, bytes: usize) -> bool {
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
        let mut y = 0;
        while y < surface.height {
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
            let accepted = unsafe { draw(surface, record, bytes) };
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
