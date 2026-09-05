//! Native graphical boot orchestration.
//!
//! The BIOS selects a linear VBE mode. The supervisor validates that descriptor,
//! maps only its pages into a ring-3 compositor, and feeds the build-validated
//! Agel vector stream one bounded record at a time. It never draws pixels.

use crate::arch;
use crate::console;
use crate::kprint;
use crate::world::{shared, Stop};

const BOOT_GRAPHICS_MARKER: *const u32 = 0x6ff0 as *const u32;
const MODE_INFO: usize = 0x7000;
const BOOT_GRAPHICS_MAGIC: u32 = 0xa6e1_0fb0;
const MAX_FRAMEBUFFER_BYTES: u64 = 16 * 1024 * 1024;
const RECORD_BYTES: usize = 64;
const STREAM_HEADER_BYTES: usize = 16;
const STREAM_MAGIC: &[u8; 4] = b"AGV1";
const VECTOR_STREAM: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/native-desktop.agv"));

#[derive(Clone, Copy)]
struct Framebuffer {
    physical: u64,
    width: u32,
    height: u32,
    pitch: u32,
    bytes: u64,
}

impl Framebuffer {
    fn discover() -> Option<Self> {
        // Safety: the 512-byte BIOS stage owns this fixed low-memory handoff.
        if unsafe { BOOT_GRAPHICS_MARKER.read_volatile() } != BOOT_GRAPHICS_MAGIC {
            return None;
        }
        let attributes = read_u16(MODE_INFO);
        let pitch = u32::from(read_u16(MODE_INFO + 0x10));
        let width = u32::from(read_u16(MODE_INFO + 0x12));
        let height = u32::from(read_u16(MODE_INFO + 0x14));
        let bits_per_pixel = read_u8(MODE_INFO + 0x19);
        let memory_model = read_u8(MODE_INFO + 0x1b);
        let red = (read_u8(MODE_INFO + 0x1f), read_u8(MODE_INFO + 0x20));
        let green = (read_u8(MODE_INFO + 0x21), read_u8(MODE_INFO + 0x22));
        let blue = (read_u8(MODE_INFO + 0x23), read_u8(MODE_INFO + 0x24));
        let physical = u64::from(read_u32(MODE_INFO + 0x28));
        if attributes & 0x91 != 0x91
            || bits_per_pixel != 32
            || memory_model != 6
            || red != (8, 16)
            || green != (8, 8)
            || blue != (8, 0)
            || width == 0
            || height == 0
            || width > 4096
            || height > 4096
            || pitch < width.checked_mul(4)?
            || physical == 0
        {
            return None;
        }
        let bytes = u64::from(pitch).checked_mul(u64::from(height))?;
        if bytes == 0 || bytes > MAX_FRAMEBUFFER_BYTES {
            return None;
        }
        Some(Self {
            physical,
            width,
            height,
            pitch,
            bytes,
        })
    }
}

fn read_u8(address: usize) -> u8 {
    // Safety: the VBE mode block is a fixed BIOS handoff in low memory.
    unsafe { (address as *const u8).read_volatile() }
}

fn read_u16(address: usize) -> u16 {
    u16::from_le_bytes([read_u8(address), read_u8(address + 1)])
}

fn read_u32(address: usize) -> u32 {
    u32::from_le_bytes([
        read_u8(address),
        read_u8(address + 1),
        read_u8(address + 2),
        read_u8(address + 3),
    ])
}

fn stream_u32(offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        VECTOR_STREAM.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn failed(reason: &str) -> ! {
    console::write("AGEL_GRAPHICS_FAILED: ");
    console::write(reason);
    console::write("\n");
    arch::exit(false)
}

fn configure(
    domain: &mut arch::Domain,
    device_address: u64,
    framebuffer: Framebuffer,
    logical_width: u32,
    logical_height: u32,
) {
    let core = domain.core();
    core.write_shared(shared::DISPLAY_ADDRESS, device_address);
    core.write_shared(shared::DISPLAY_WIDTH, u64::from(framebuffer.width));
    core.write_shared(shared::DISPLAY_HEIGHT, u64::from(framebuffer.height));
    core.write_shared(shared::DISPLAY_PITCH, u64::from(framebuffer.pitch));
    core.write_shared(shared::DISPLAY_LOGICAL_WIDTH, u64::from(logical_width));
    core.write_shared(shared::DISPLAY_LOGICAL_HEIGHT, u64::from(logical_height));
}

fn request(domain: &mut arch::Domain, command: u64) -> Result<(), &'static str> {
    domain.core().stage_command(command);
    match domain.run() {
        Stop::Replied if domain.core().read_shared(shared::STATUS) == 0 => Ok(()),
        Stop::Replied => Err("compositor rejected a command"),
        Stop::Faulted(_) => Err("compositor faulted while drawing"),
        Stop::BudgetExhausted => Err("compositor exhausted its tick budget"),
    }
}

fn checksum(domain: &mut arch::Domain) -> Result<u64, &'static str> {
    request(domain, shared::COMMAND_DISPLAY_CHECKSUM)?;
    Ok(domain.core().read_shared(shared::VALUES))
}

fn render(domain: &mut arch::Domain, count: usize) -> Result<(), &'static str> {
    for command in VECTOR_STREAM[STREAM_HEADER_BYTES..].chunks_exact(RECORD_BYTES) {
        for (offset, byte) in command.iter().enumerate() {
            domain.core().write_payload(offset, *byte);
        }
        domain
            .core()
            .write_shared(shared::ARGUMENTS, RECORD_BYTES as u64);
        request(domain, shared::COMMAND_DISPLAY_DRAW)?;
    }
    if VECTOR_STREAM[STREAM_HEADER_BYTES..]
        .chunks_exact(RECORD_BYTES)
        .len()
        != count
    {
        return Err("compiled vector command count disagrees");
    }
    Ok(())
}

/// Draw the Agel-authored desktop in a contained compositor domain.
pub fn run() -> ! {
    let framebuffer = Framebuffer::discover().unwrap_or_else(|| failed("no valid VBE framebuffer"));
    if VECTOR_STREAM.get(0..4) != Some(STREAM_MAGIC) {
        failed("native vector stream has the wrong magic");
    }
    let logical_width = stream_u32(4).unwrap_or_else(|| failed("truncated vector viewport"));
    let logical_height = stream_u32(8).unwrap_or_else(|| failed("truncated vector viewport"));
    let count = stream_u32(12).unwrap_or_else(|| failed("truncated vector command count")) as usize;
    let expected = STREAM_HEADER_BYTES
        .checked_add(count.saturating_mul(RECORD_BYTES))
        .unwrap_or_else(|| failed("native vector stream size overflow"));
    if count == 0 || count > 256 || VECTOR_STREAM.len() != expected {
        failed("native vector stream violates its bounds");
    }

    let mut machine = arch::Machine::bring_up().unwrap_or_else(|reason| failed(reason));
    let entry = crate::display_user::agel_compositor_main as *const () as usize as u64;
    if !arch::user_text_range().contains(&entry) {
        failed("compositor entry is outside user-executable text");
    }
    let (mut compositor, device_address) = machine
        .create_display_world(entry, 250, framebuffer.physical, framebuffer.bytes)
        .unwrap_or_else(|reason| failed(reason));
    configure(
        &mut compositor,
        device_address,
        framebuffer,
        logical_width,
        logical_height,
    );
    render(&mut compositor, count).unwrap_or_else(|reason| failed(reason));
    let stable = checksum(&mut compositor).unwrap_or_else(|reason| failed(reason));
    if stable == 0 {
        failed("compositor produced an empty framebuffer digest");
    }

    // A malformed display record is rejected without changing the last frame.
    for offset in 0..RECORD_BYTES {
        compositor.core().write_payload(offset, 0);
    }
    compositor
        .core()
        .write_shared(shared::ARGUMENTS, RECORD_BYTES as u64);
    compositor
        .core()
        .stage_command(shared::COMMAND_DISPLAY_DRAW);
    if !matches!(compositor.run(), Stop::Replied)
        || compositor.core().read_shared(shared::STATUS) == 0
    {
        failed("compositor accepted a malformed vector command");
    }
    if checksum(&mut compositor).unwrap_or_else(|reason| failed(reason)) != stable {
        failed("rejected vector command changed the framebuffer");
    }

    // Lose the compositor deliberately. Its device pages are not supervisor
    // pages, so the fault is contained and the last good pixels remain.
    compositor
        .core()
        .stage_command(shared::COMMAND_DISPLAY_FAULT);
    match compositor.run() {
        Stop::Faulted(fault) if fault.name() == "page-fault" => {}
        _ => failed("display fault escaped containment"),
    }
    let (mut replacement, replacement_address) = machine
        .create_display_world(entry, 250, framebuffer.physical, framebuffer.bytes)
        .unwrap_or_else(|reason| failed(reason));
    configure(
        &mut replacement,
        replacement_address,
        framebuffer,
        logical_width,
        logical_height,
    );
    if checksum(&mut replacement).unwrap_or_else(|reason| failed(reason)) != stable {
        failed("replacement compositor did not inherit the last good frame");
    }

    kprint!(
        "graphics[x86_64]: {}x{}x32, {} Agel vector commands, digest {stable:#018x}\n",
        framebuffer.width,
        framebuffer.height,
        count
    );
    console::write("graphics[x86_64]: malformed frame rejected; last good frame retained\n");
    console::write("graphics[x86_64]: compositor fault contained and replaced\n");
    console::write("AGEL_GRAPHICS_OK\n");

    #[cfg(feature = "graphics-selftest")]
    arch::exit(true);

    #[cfg(not(feature = "graphics-selftest"))]
    arch::halt()
}
