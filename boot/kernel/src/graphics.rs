//! Native graphical boot orchestration.
//!
//! The BIOS selects a linear VBE mode. The supervisor validates that descriptor,
//! maps only its pages into a ring-3 compositor, and feeds the build-validated
//! Agel vector stream one bounded record at a time. It never draws pixels.

use crate::arch;
use crate::console;
use crate::kprint;
use crate::native_session::{replay as replay_workspace, request as evaluator_request};
use crate::workspace::Workspace;
use crate::world::{shared, Stop, PAYLOAD_BYTES};

const BOOT_GRAPHICS_MARKER: *const u32 = 0x6ff0 as *const u32;
const MODE_INFO: usize = 0x7000;
const BOOT_GRAPHICS_MAGIC: u32 = 0xa6e1_0fb0;
const MAX_FRAMEBUFFER_BYTES: u64 = 16 * 1024 * 1024;
const RECORD_BYTES: usize = 64;
const STREAM_HEADER_BYTES: usize = 16;
const STREAM_MAGIC: &[u8; 4] = b"AGV1";
const VECTOR_STREAM: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/native-desktop.agv"));
const MAX_SCENE_COMMANDS: usize = 48;
const INPUT_BYTES: usize = PAYLOAD_BYTES;
const DISPLAY_LINE_BYTES: usize = 22;

const VIOLET: [u32; 3] = [0x92_85_ff, 0x62_54_e7, 0x9b_8c_ff];
const CYAN: [u32; 3] = [0x57_d8_ff, 0x17_8bb8, 0x71_e6_ff];
const AMBER: [u32; 3] = [0xff_c857, 0xc77b18, 0xff_d580];

#[derive(Clone, Copy)]
struct Scene {
    accent: u8,
    workspace: u8,
    title: [u8; 28],
    title_len: u8,
}

impl Scene {
    fn initial() -> Self {
        let mut title = [0; 28];
        let text = b"MOLD THE SYSTEM AS IT RUNS";
        title[..text.len()].copy_from_slice(text);
        Self {
            accent: 0,
            workspace: 1,
            title,
            title_len: text.len() as u8,
        }
    }
}

#[derive(Clone, Copy)]
struct Frame {
    records: [[u8; RECORD_BYTES]; MAX_SCENE_COMMANDS],
    count: usize,
}

impl Frame {
    fn empty() -> Self {
        Self {
            records: [[0; RECORD_BYTES]; MAX_SCENE_COMMANDS],
            count: 0,
        }
    }

    fn push(&mut self, record: [u8; RECORD_BYTES]) -> Result<(), &'static str> {
        let slot = self
            .records
            .get_mut(self.count)
            .ok_or("live scene command budget exceeded")?;
        *slot = record;
        self.count += 1;
        Ok(())
    }
}

enum Intent {
    Accent(u8),
    Workspace(u8),
    Title([u8; 28], u8),
    Rollback,
    Inspect,
    Help,
}

struct Keyboard {
    shift: bool,
    extended: bool,
}

impl Keyboard {
    const fn new() -> Self {
        Self {
            shift: false,
            extended: false,
        }
    }

    fn decode(&mut self, scan: u8) -> Option<u8> {
        if scan == 0xe0 {
            self.extended = true;
            return None;
        }
        if self.extended {
            self.extended = false;
            return None;
        }
        let released = scan & 0x80 != 0;
        let code = scan & 0x7f;
        if code == 0x2a || code == 0x36 {
            self.shift = !released;
            return None;
        }
        if released {
            return None;
        }
        Some(match code {
            0x01 => 0x1b,
            0x0e => 0x08,
            0x1c => b'\n',
            0x39 => b' ',
            0x02..=0x0b => {
                const PLAIN: &[u8; 10] = b"1234567890";
                const SHIFTED: &[u8; 10] = b"!@#$%^&*()";
                let table = if self.shift { SHIFTED } else { PLAIN };
                table[(code - 0x02) as usize]
            }
            0x10..=0x19 => b"qwertyuiop"[(code - 0x10) as usize],
            0x1e..=0x26 => b"asdfghjkl"[(code - 0x1e) as usize],
            0x2c..=0x32 => b"zxcvbnm"[(code - 0x2c) as usize],
            0x0c => b'-',
            0x28 if self.shift => b'"',
            _ => return None,
        })
    }
}

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

fn record_u32(record: &[u8; RECORD_BYTES], word: usize) -> u32 {
    let offset = word * 4;
    u32::from_le_bytes(record[offset..offset + 4].try_into().unwrap_or([0; 4]))
}

fn put_u32(record: &mut [u8; RECORD_BYTES], word: usize, value: u32) {
    let offset = word * 4;
    record[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn text_record(x: u32, y: u32, scale: u32, color: u32, text: &[u8]) -> [u8; RECORD_BYTES] {
    let mut record = [0; RECORD_BYTES];
    put_u32(&mut record, 0, 5);
    put_u32(&mut record, 1, x);
    put_u32(&mut record, 2, y);
    put_u32(&mut record, 3, scale);
    put_u32(&mut record, 4, color);
    let length = text.len().min(28);
    put_u32(&mut record, 8, length as u32);
    record[36..36 + length].copy_from_slice(&text[..length]);
    record
}

fn panel_record() -> [u8; RECORD_BYTES] {
    let mut record = [0; RECORD_BYTES];
    put_u32(&mut record, 0, 2);
    put_u32(&mut record, 1, 246);
    put_u32(&mut record, 2, 694);
    put_u32(&mut record, 3, 758);
    put_u32(&mut record, 4, 62);
    put_u32(&mut record, 5, 12);
    put_u32(&mut record, 6, 0x18_1c_2a);
    record
}

fn palette(accent: u8) -> [u32; 3] {
    match accent {
        1 => CYAN,
        2 => AMBER,
        _ => VIOLET,
    }
}

fn replace_accent(record: &mut [u8; RECORD_BYTES], accent: [u32; 3]) {
    let fields: &[usize] = match record_u32(record, 0) {
        1 => &[1, 2],
        2 => &[6],
        3 => &[6, 7],
        4 => &[5],
        5 => &[4],
        _ => &[],
    };
    for field in fields {
        let color = record_u32(record, *field);
        if let Some(index) = VIOLET.iter().position(|candidate| *candidate == color) {
            put_u32(record, *field, accent[index]);
        }
    }
}

fn record_text(record: &[u8; RECORD_BYTES]) -> &[u8] {
    if record_u32(record, 0) != 5 {
        return &[];
    }
    let length = (record_u32(record, 8) as usize).min(28);
    &record[36..36 + length]
}

fn replace_text(record: &mut [u8; RECORD_BYTES], text: &[u8]) {
    record[36..64].fill(0);
    let length = text.len().min(28);
    record[36..36 + length].copy_from_slice(&text[..length]);
    put_u32(record, 8, length as u32);
}

fn materialize(scene: Scene, line: Option<&[u8]>, status: &[u8]) -> Result<Frame, &'static str> {
    let mut frame = Frame::empty();
    let (commands, remainder) = VECTOR_STREAM[STREAM_HEADER_BYTES..].as_chunks::<RECORD_BYTES>();
    if !remainder.is_empty() {
        return Err("compiled vector stream has a partial record");
    }
    let accent = palette(scene.accent);
    for source in commands {
        let mut record = *source;
        replace_accent(&mut record, accent);
        if record_text(&record) == b"WORKSPACE 1" {
            record[46] = b'0' + scene.workspace;
        } else if record_text(&record) == b"MOLD THE SYSTEM AS IT RUNS" {
            replace_text(&mut record, &scene.title[..scene.title_len as usize]);
        }
        frame.push(record)?;
    }
    if let Some(line) = line {
        let mut prompt = [b' '; 28];
        prompt[..6].copy_from_slice(b"AGEL> ");
        let shown = if line.len() > DISPLAY_LINE_BYTES {
            &line[line.len() - DISPLAY_LINE_BYTES..]
        } else {
            line
        };
        let length = shown.len();
        prompt[6..6 + length].copy_from_slice(shown);
        frame.push(panel_record())?;
        frame.push(text_record(270, 706, 2, accent[2], &prompt[..6 + length]))?;
        frame.push(text_record(270, 732, 2, 0xae_b6_d0, status))?;
    }
    Ok(frame)
}

fn parse_intent(line: &[u8]) -> Result<Intent, &'static str> {
    let line = trim(line);
    if line.eq_ignore_ascii_case(b"(help)") {
        return Ok(Intent::Help);
    }
    if line.eq_ignore_ascii_case(b"(inspect)") {
        return Ok(Intent::Inspect);
    }
    if line.eq_ignore_ascii_case(b"(rollback)") {
        return Ok(Intent::Rollback);
    }
    if let Some(value) = argument(line, b"(accent ", b")") {
        if value.eq_ignore_ascii_case(b"violet") {
            return Ok(Intent::Accent(0));
        }
        if value.eq_ignore_ascii_case(b"cyan") {
            return Ok(Intent::Accent(1));
        }
        if value.eq_ignore_ascii_case(b"amber") {
            return Ok(Intent::Accent(2));
        }
        return Err("ACCENT VIOLET CYAN AMBER");
    }
    if let Some(value) = argument(line, b"(workspace ", b")") {
        return match value {
            b"1" => Ok(Intent::Workspace(1)),
            b"2" => Ok(Intent::Workspace(2)),
            b"3" => Ok(Intent::Workspace(3)),
            _ => Err("WORKSPACE MUST BE 1 2 OR 3"),
        };
    }
    if let Some(value) = argument(line, b"(title \"", b"\")") {
        if value.is_empty() || value.len() > 28 {
            return Err("TITLE NEEDS 1 TO 28 CHARS");
        }
        let mut title = [0; 28];
        for (slot, byte) in title.iter_mut().zip(value) {
            let byte = byte.to_ascii_uppercase();
            if !(byte.is_ascii_alphanumeric() || byte == b' ' || byte == b'-') {
                return Err("TITLE USES LETTERS NUMBERS -");
            }
            *slot = byte;
        }
        return Ok(Intent::Title(title, value.len() as u8));
    }
    Err("TRY (HELP)")
}

fn trim(mut value: &[u8]) -> &[u8] {
    while value.first().is_some_and(u8::is_ascii_whitespace) {
        value = &value[1..];
    }
    while value.last().is_some_and(u8::is_ascii_whitespace) {
        value = &value[..value.len() - 1];
    }
    value
}

fn argument<'a>(line: &'a [u8], prefix: &[u8], suffix: &[u8]) -> Option<&'a [u8]> {
    if line.len() < prefix.len() + suffix.len()
        || !line[..prefix.len()].eq_ignore_ascii_case(prefix)
        || !line.ends_with(suffix)
    {
        return None;
    }
    Some(&line[prefix.len()..line.len() - suffix.len()])
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

fn render(domain: &mut arch::Domain, frame: &Frame) -> Result<(), &'static str> {
    for command in &frame.records[..frame.count] {
        for (offset, byte) in command.iter().enumerate() {
            domain.core().write_payload(offset, *byte);
        }
        domain
            .core()
            .write_shared(shared::ARGUMENTS, RECORD_BYTES as u64);
        request(domain, shared::COMMAND_DISPLAY_DRAW)?;
    }
    Ok(())
}

fn render_overlay(domain: &mut arch::Domain, frame: &Frame) -> Result<(), &'static str> {
    let start = frame.count.saturating_sub(3);
    for command in &frame.records[start..frame.count] {
        for (offset, byte) in command.iter().enumerate() {
            domain.core().write_payload(offset, *byte);
        }
        domain
            .core()
            .write_shared(shared::ARGUMENTS, RECORD_BYTES as u64);
        request(domain, shared::COMMAND_DISPLAY_DRAW)?;
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct StatusLine {
    bytes: [u8; PAYLOAD_BYTES],
    len: usize,
}

impl StatusLine {
    fn new(text: &[u8]) -> Self {
        let mut status = Self {
            bytes: [0; PAYLOAD_BYTES],
            len: 0,
        };
        status.push(text);
        status
    }

    fn push(&mut self, text: &[u8]) {
        let count = text.len().min(self.bytes.len() - self.len);
        self.bytes[self.len..self.len + count].copy_from_slice(&text[..count]);
        self.len += count;
    }

    fn number(&mut self, value: u8) {
        if self.len < self.bytes.len() {
            self.bytes[self.len] = b'0' + value.min(9);
            self.len += 1;
        }
    }

    fn number_u64(&mut self, value: u64) {
        let mut digits = [0_u8; 20];
        let mut cursor = digits.len();
        let mut remaining = value;
        loop {
            cursor -= 1;
            digits[cursor] = b'0' + (remaining % 10) as u8;
            remaining /= 10;
            if remaining == 0 {
                break;
            }
        }
        self.push(&digits[cursor..]);
    }

    fn get(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}

fn restore_from_disk(evaluator: &mut arch::Domain) -> Result<(Workspace, u64, u64), &'static str> {
    let candidates = crate::workspace::load()?;
    let mut highest_generation = 0;
    for loaded in candidates.into_iter().flatten() {
        highest_generation = highest_generation.max(loaded.generation);
        if let Ok(revision) = replay_workspace(evaluator, &loaded.workspace) {
            return Ok((loaded.workspace, loaded.generation, revision));
        }
    }
    let empty = Workspace::new();
    let revision = replay_workspace(evaluator, &empty).map_err(|failure| failure.message())?;
    Ok((empty, highest_generation, revision))
}

fn command_argument<'a>(line: &'a [u8], prefix: &[u8]) -> Option<&'a [u8]> {
    line.strip_prefix(prefix).map(trim)
}

fn cell_definition(line: &[u8]) -> Option<(&[u8], &[u8])> {
    let body = line.strip_prefix(b":cell ")?;
    let split = body.iter().position(u8::is_ascii_whitespace)?;
    let name = trim(&body[..split]);
    let source = trim(&body[split + 1..]);
    Some((name, source))
}

fn scene_command(line: &[u8]) -> bool {
    let line = trim(line);
    line.eq_ignore_ascii_case(b"(help)")
        || line.eq_ignore_ascii_case(b"(inspect)")
        || line.eq_ignore_ascii_case(b"(rollback)")
        || line
            .get(..8)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(b"(accent "))
        || line
            .get(..11)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(b"(workspace "))
        || line
            .get(..8)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(b"(title \""))
}

#[allow(clippy::too_many_arguments)]
fn execute_workshop(
    compositor: &mut arch::Domain,
    evaluator: &mut arch::Domain,
    current: &mut Scene,
    previous: &mut Scene,
    scene_revision: &mut u8,
    evaluator_revision: &mut u64,
    workspace: &mut Workspace,
    committed_workspace: &mut Workspace,
    generation: &mut u64,
    dirty: &mut bool,
    line: &[u8],
) -> StatusLine {
    let line = trim(line);
    if line.is_empty() {
        return StatusLine::new(b"READY");
    }
    if scene_command(line) {
        return execute(compositor, current, previous, scene_revision, line);
    }
    if line == b":shutdown" {
        arch::exit(true);
    }
    if line == b":help" {
        return StatusLine::new(b"forms: Lisp evaluates | UI: (help) | cells: :cell NAME FORM :run NAME :show NAME :delete NAME :cells :workspace :save :reload | evaluator: :revision :rollback :defs :limits | :shutdown");
    }
    if line == b":revision" {
        let mut status = StatusLine::new(b"EVAL REV ");
        status.number_u64(*evaluator_revision);
        return status;
    }
    if line == b":cells" {
        let mut status = StatusLine::new(b"CELLS ");
        status.number_u64(workspace.count() as u64);
        for ordinal in 0..workspace.count() {
            if let Some(cell) = workspace.cell(ordinal) {
                status.push(b" ");
                status.push(cell.name());
            }
        }
        return status;
    }
    if line == b":workspace" {
        let mut status = StatusLine::new(b"GEN ");
        status.number_u64(*generation);
        status.push(b" CELLS ");
        status.number_u64(workspace.count() as u64);
        status.push(if *dirty { b" DIRTY" } else { b" CLEAN" });
        return status;
    }
    if line == b":save" {
        return match replay_workspace(evaluator, workspace) {
            Ok(revision) => match crate::workspace::save(workspace, *generation) {
                Ok(next) => {
                    *generation = next;
                    *committed_workspace = *workspace;
                    *evaluator_revision = revision;
                    *dirty = false;
                    let mut status = StatusLine::new(b"SAVED GENERATION ");
                    status.number_u64(next);
                    status
                }
                Err(reason) => {
                    let _ = replay_workspace(evaluator, committed_workspace);
                    StatusLine::new(reason.as_bytes())
                }
            },
            Err(failure) => {
                *evaluator_revision =
                    replay_workspace(evaluator, committed_workspace).unwrap_or(*evaluator_revision);
                StatusLine::new(failure.message().as_bytes())
            }
        };
    }
    if line == b":reload" {
        return match restore_from_disk(evaluator) {
            Ok((restored, restored_generation, revision)) => {
                *workspace = restored;
                *committed_workspace = restored;
                *generation = restored_generation;
                *evaluator_revision = revision;
                *dirty = false;
                StatusLine::new(b"WORKSPACE RELOADED")
            }
            Err(reason) => StatusLine::new(reason.as_bytes()),
        };
    }
    if let Some((name, source)) = cell_definition(line) {
        return match workspace.upsert(name, source) {
            Ok(()) => {
                *dirty = *workspace != *committed_workspace;
                StatusLine::new(b"CELL STAGED - RUN OR SAVE")
            }
            Err(reason) => StatusLine::new(reason.as_bytes()),
        };
    }
    if let Some(name) = command_argument(line, b":run ") {
        return match workspace.find(name) {
            Some(cell) => evaluator_status(
                evaluator,
                evaluator_revision,
                shared::COMMAND_EVALUATE,
                cell.source(),
            ),
            None => StatusLine::new(b"NO SUCH CELL"),
        };
    }
    if let Some(name) = command_argument(line, b":show ") {
        return workspace
            .find(name)
            .map(|cell| StatusLine::new(cell.source()))
            .unwrap_or_else(|| StatusLine::new(b"NO SUCH CELL"));
    }
    if let Some(name) = command_argument(line, b":delete ") {
        return match workspace.delete(name) {
            Ok(()) => {
                *dirty = *workspace != *committed_workspace;
                StatusLine::new(b"CELL DELETED - SAVE")
            }
            Err(reason) => StatusLine::new(reason.as_bytes()),
        };
    }
    let command = match line {
        b":rollback" => shared::COMMAND_EVALUATOR_ROLLBACK,
        b":defs" => shared::COMMAND_EVALUATOR_DEFS,
        b":limits" => shared::COMMAND_EVALUATOR_LIMITS,
        _ if line.starts_with(b":") => return StatusLine::new(b"UNKNOWN COMMAND - :HELP"),
        _ => shared::COMMAND_EVALUATE,
    };
    evaluator_status(
        evaluator,
        evaluator_revision,
        command,
        if command == shared::COMMAND_EVALUATE {
            line
        } else {
            b""
        },
    )
}

fn evaluator_status(
    evaluator: &mut arch::Domain,
    revision: &mut u64,
    command: u64,
    source: &[u8],
) -> StatusLine {
    match evaluator_request(evaluator, command, source) {
        Ok(reply) => {
            *revision = reply.revision;
            let status = StatusLine::new(&reply.bytes[..reply.length]);
            if reply.error {
                console::write("evaluator transaction rolled back\n");
            }
            status
        }
        Err(reason) => StatusLine::new(reason.as_bytes()),
    }
}

fn inspect_status(scene: Scene, revision: u8) -> StatusLine {
    let mut status = StatusLine::new(b"REV ");
    status.number(revision);
    status.push(b" WS ");
    status.number(scene.workspace);
    status.push(b" ");
    status.push(match scene.accent {
        1 => b"CYAN",
        2 => b"AMBER",
        _ => b"VIOLET",
    });
    status
}

fn commit_scene(
    compositor: &mut arch::Domain,
    current: &mut Scene,
    previous: &mut Scene,
    revision: &mut u8,
    candidate: Scene,
) -> Result<u64, &'static str> {
    let frame = materialize(candidate, None, b"")?;
    render(compositor, &frame)?;
    let digest = checksum(compositor)?;
    if digest == 0 {
        return Err("candidate rendered an empty digest");
    }
    *previous = *current;
    *current = candidate;
    *revision = revision.saturating_add(1);
    Ok(digest)
}

fn execute(
    compositor: &mut arch::Domain,
    current: &mut Scene,
    previous: &mut Scene,
    revision: &mut u8,
    line: &[u8],
) -> StatusLine {
    let intent = match parse_intent(line) {
        Ok(intent) => intent,
        Err(reason) => return StatusLine::new(reason.as_bytes()),
    };
    if matches!(intent, Intent::Help) {
        return StatusLine::new(b"ACCENT WORKSPACE TITLE ROLLBACK");
    }
    if matches!(intent, Intent::Inspect) {
        return inspect_status(*current, *revision);
    }
    let mut candidate = *current;
    match intent {
        Intent::Accent(accent) => candidate.accent = accent,
        Intent::Workspace(workspace) => candidate.workspace = workspace,
        Intent::Title(title, length) => {
            candidate.title = title;
            candidate.title_len = length;
        }
        Intent::Rollback => candidate = *previous,
        Intent::Inspect | Intent::Help => unreachable!(),
    }
    match commit_scene(compositor, current, previous, revision, candidate) {
        Ok(_) => {
            let mut status = StatusLine::new(b"COMMITTED REV ");
            status.number(*revision);
            status
        }
        Err(_) => StatusLine::new(b"TRANSACTION ABORTED"),
    }
}

fn next_input(keyboard: &mut Keyboard) -> Option<u8> {
    if let Some(byte) = arch::console_try_read_byte() {
        return Some(byte);
    }
    arch::keyboard_try_read_scancode().and_then(|scan| keyboard.decode(scan))
}

fn interactive(
    machine: &mut arch::Machine,
    compositor: &mut arch::Domain,
    mut current: Scene,
    mut previous: Scene,
) -> ! {
    let evaluator_entry = crate::user::agel_evaluator_main as *const () as usize as u64;
    let mut evaluator = machine
        .create_evaluator_world(evaluator_entry, 20)
        .unwrap_or_else(|reason| failed(reason));
    let (mut workspace, mut generation, mut evaluator_revision) =
        restore_from_disk(&mut evaluator).unwrap_or_else(|reason| failed(reason));
    let mut committed_workspace = workspace;
    let mut dirty = false;
    let mut scene_revision = 0_u8;
    let mut line = [0; INPUT_BYTES];
    let mut length = 0;
    let mut keyboard = Keyboard::new();
    let mut status = if generation == 0 {
        StatusLine::new(b"AGEL READY - TYPE :HELP")
    } else {
        let mut status = StatusLine::new(b"RESTORED GEN ");
        status.number_u64(generation);
        status
    };
    let frame = materialize(current, Some(&line[..length]), status.get())
        .unwrap_or_else(|reason| failed(reason));
    render_overlay(compositor, &frame).unwrap_or_else(|reason| failed(reason));
    console::write("live-desktop> ");

    loop {
        let Some(byte) = next_input(&mut keyboard) else {
            core::hint::spin_loop();
            continue;
        };
        match byte {
            b'\r' | b'\n' => {
                console::write("\n");
                status = execute_workshop(
                    compositor,
                    &mut evaluator,
                    &mut current,
                    &mut previous,
                    &mut scene_revision,
                    &mut evaluator_revision,
                    &mut workspace,
                    &mut committed_workspace,
                    &mut generation,
                    &mut dirty,
                    &line[..length],
                );
                console::write_bytes(status.get());
                console::write("\nlive-desktop> ");
                length = 0;
            }
            0x08 | 0x7f if length > 0 => {
                length -= 1;
                console::write("\x08 \x08");
            }
            0x1b => {
                length = 0;
                status = StatusLine::new(b"INPUT CLEARED");
            }
            byte if byte.is_ascii_graphic() || byte == b' ' => {
                if length < line.len() {
                    line[length] = byte;
                    length += 1;
                    console::write_byte(byte);
                } else {
                    status = StatusLine::new(b"INPUT LIMIT 256 BYTES");
                }
            }
            _ => {}
        }
        let frame = materialize(current, Some(&line[..length]), status.get())
            .unwrap_or_else(|reason| failed(reason));
        render_overlay(compositor, &frame).unwrap_or_else(|reason| failed(reason));
    }
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
    let initial = Scene::initial();
    let frame = materialize(initial, None, b"").unwrap_or_else(|reason| failed(reason));
    if frame.count != count {
        failed("compiled vector command count disagrees");
    }
    render(&mut compositor, &frame).unwrap_or_else(|reason| failed(reason));
    let stable = checksum(&mut compositor).unwrap_or_else(|reason| failed(reason));
    if stable == 0 {
        failed("compositor produced an empty framebuffer digest");
    }

    // Prove that semantic Lisp input creates a new complete frame, rejection
    // leaves it unchanged, and rollback returns to the exact original pixels.
    let mut current = initial;
    let mut previous = initial;
    let mut revision = 0;
    let mut candidate = initial;
    candidate.accent = 1;
    candidate.workspace = 2;
    let changed = commit_scene(
        &mut compositor,
        &mut current,
        &mut previous,
        &mut revision,
        candidate,
    )
    .unwrap_or_else(|reason| failed(reason));
    if changed == stable || revision != 1 {
        failed("live scene transaction did not change the frame");
    }
    if parse_intent(b"(workspace 99)").is_ok()
        || checksum(&mut compositor).unwrap_or_else(|reason| failed(reason)) != changed
    {
        failed("rejected live scene transaction changed the frame");
    }
    let rollback = execute(
        &mut compositor,
        &mut current,
        &mut previous,
        &mut revision,
        b"(rollback)",
    );
    if !rollback.get().starts_with(b"COMMITTED")
        || revision != 2
        || checksum(&mut compositor).unwrap_or_else(|reason| failed(reason)) != stable
    {
        failed("live scene rollback did not restore the stable frame");
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
    console::write("graphics[x86_64]: live Lisp scene commit/reject/rollback [ok]\n");
    console::write("AGEL_GRAPHICS_OK\n");

    #[cfg(feature = "graphics-selftest")]
    arch::exit(true);

    #[cfg(not(feature = "graphics-selftest"))]
    interactive(&mut machine, &mut replacement, initial, initial)
}
