//! The firmware's property mailbox on a Raspberry Pi: one message, built
//! in a static buffer the firmware reads by physical address, asking for
//! a framebuffer of a given size. The supervisor uses it once, before any
//! world runs; nothing else on the machine speaks to the firmware.

/// A property message: sixteen-byte aligned, as the mailbox requires.
#[repr(C, align(16))]
struct Message([u32; 40]);

static mut MESSAGE: Message = Message([0; 40]);

const CHANNEL_PROPERTY: u32 = 8;
const STATUS_FULL: u32 = 1 << 31;
const STATUS_EMPTY: u32 = 1 << 30;
/// Register offsets from the mailbox base.
const READ: u64 = 0x00;
const STATUS: u64 = 0x18;
const WRITE: u64 = 0x20;
const RESPONSE_OK: u32 = 0x8000_0000;
/// Polls of the status register before giving up on the firmware.
const POLL_LIMIT: u32 = 50_000_000;

const TAG_ALLOCATE: u32 = 0x4_0001;
const TAG_PITCH: u32 = 0x4_0008;
const TAG_PHYSICAL_SIZE: u32 = 0x4_8003;
const TAG_VIRTUAL_SIZE: u32 = 0x4_8004;
const TAG_DEPTH: u32 = 0x4_8005;
const TAG_PIXEL_ORDER: u32 = 0x4_8006;

/// A framebuffer of `width` by `height` at 32 bits per pixel, blue in
/// the low byte, as the compositor paints: its physical address, the
/// pitch and the size in bytes. `None` when the firmware does not answer
/// or answers with something else.
///
/// # Safety
/// Once, before any world runs, with the mailbox in the device window.
pub unsafe fn framebuffer(base: u64, width: u32, height: u32) -> Option<(u64, u32, u64)> {
    let message = core::ptr::addr_of_mut!(MESSAGE).cast::<u32>();
    let words: [u32; 30] = [
        30 * 4,
        0,
        TAG_PHYSICAL_SIZE,
        8,
        0,
        width,
        height,
        TAG_VIRTUAL_SIZE,
        8,
        0,
        width,
        height,
        TAG_DEPTH,
        4,
        0,
        32,
        TAG_PIXEL_ORDER,
        4,
        0,
        0,
        TAG_ALLOCATE,
        8,
        0,
        4096,
        0,
        TAG_PITCH,
        4,
        0,
        0,
        0,
    ];
    unsafe {
        for (index, word) in words.iter().enumerate() {
            message.add(index).write_volatile(*word);
        }
        clean(message as u64, words.len() * 4);
        let address = message as u64;
        if address >= 1 << 32 || address & 0xf != 0 {
            return None;
        }
        let mut polls = 0;
        while ((base + STATUS) as *const u32).read_volatile() & STATUS_FULL != 0 {
            polls += 1;
            if polls > POLL_LIMIT {
                return None;
            }
        }
        ((base + WRITE) as *mut u32).write_volatile(address as u32 | CHANNEL_PROPERTY);
        loop {
            while ((base + STATUS) as *const u32).read_volatile() & STATUS_EMPTY != 0 {
                polls += 1;
                if polls > POLL_LIMIT {
                    return None;
                }
            }
            let answer = ((base + READ) as *const u32).read_volatile();
            if answer & 0xf == CHANNEL_PROPERTY {
                break;
            }
        }
        clean(message as u64, words.len() * 4);
        let word = |index: usize| message.add(index).read_volatile();
        if word(1) != RESPONSE_OK
            || word(5) != width
            || word(6) != height
            || word(15) != 32
            || word(19) != 0
            || word(23) == 0
        {
            return None;
        }
        // The firmware answers a bus address; the top two bits name the
        // cache alias, and the rest is the physical address.
        let physical = u64::from(word(23) & 0x3fff_ffff);
        let bytes = u64::from(word(24));
        let pitch = word(28);
        if pitch < width.checked_mul(4)? || bytes < u64::from(pitch) * u64::from(height) {
            return None;
        }
        Some((physical, pitch, bytes))
    }
}

/// Clean and invalidate the message's cache lines, so the firmware reads
/// what was written and the supervisor reads what it answered.
///
/// # Safety
/// `address` and `bytes` must lie in mapped memory.
unsafe fn clean(address: u64, bytes: usize) {
    let mut line = address & !63;
    while line < address + bytes as u64 {
        unsafe { core::arch::asm!("dc civac, {}", in(reg) line, options(nostack)) };
        line += 64;
    }
    unsafe { core::arch::asm!("dsb sy", options(nostack)) };
}
