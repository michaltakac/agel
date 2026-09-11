//! Native graphical boot orchestration.
//!
//! The BIOS selects a linear VBE mode. The supervisor validates that descriptor,
//! maps only its pages into a ring-3 compositor, and feeds the build-validated
//! Agel vector stream one bounded record at a time. It never draws pixels.

use crate::arch;
use crate::console;
use crate::kprint;
use crate::native_session::{replay as replay_workspace, request as evaluator_request};
#[cfg(target_arch = "x86_64")]
use crate::recovery::{slot_name, Admission, KernelRecovery};
use crate::recovery::{BootPlan, LiveRecovery};

/// Kernel slots need the BIOS stage: a board has none, and the selector's
/// place in the workshop stays empty.
#[cfg(not(target_arch = "x86_64"))]
#[allow(dead_code)]
struct KernelRecovery;
use crate::service::{ServiceDomain, ServiceKind};
use crate::workspace::Workspace;
use crate::world::{shared, Stop, PAYLOAD_BYTES};
use core::fmt::Write;

#[cfg(target_arch = "x86_64")]
const BOOT_GRAPHICS_MARKER: *const u32 = 0x6ff0 as *const u32;
#[cfg(target_arch = "x86_64")]
const MODE_INFO: usize = 0x7000;
#[cfg(target_arch = "x86_64")]
const BOOT_GRAPHICS_MAGIC: u32 = 0xa6e1_0fb0;
const MAX_FRAMEBUFFER_BYTES: u64 = 16 * 1024 * 1024;
const RECORD_BYTES: usize = 64;
const STREAM_HEADER_BYTES: usize = 16;
const STREAM_MAGIC: &[u8; 4] = b"AGV1";
const VECTOR_STREAM: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/native-desktop.agv"));
const MAX_SCENE_COMMANDS: usize = 224;
const INPUT_BYTES: usize = PAYLOAD_BYTES;
/// The self-documenting command postcard. It must fit one status line, and a
/// longer postcard is a build error rather than a silently truncated `:help`.
const HELP_POSTCARD: &[u8] = b":workbench | :preview FORM :promote :discard :source ID | quote if begin let def fn | spawn send step run | scene-* | :cell :run :show :delete :cells :save :reload | :exec NAME [ROOT] :close :minimize :maximize N :fs-format :fs-ls | :rollback :shutdown";
const _: () = assert!(HELP_POSTCARD.len() <= PAYLOAD_BYTES);
const DISPLAY_LINE_BYTES: usize = 26;

/// The faces the compositor was given, by slot: regular sans, medium sans,
/// mono. The supervisor lays text out from their metrics.
const FACE_COUNT: usize = shared::SPRITE_SLOT;
static mut FACES: [crate::assets::Face; FACE_COUNT] = [crate::assets::Face::EMPTY; FACE_COUNT];
const FACE_NAMES: [&[u8]; FACE_COUNT] = [b"fira-sans", b"fira-sans-medium", b"fira-mono"];
const SPRITE_SHEET: &[u8] = b"sprites";
/// Sprite ids in the sheet `scripts/build-sprites.py` draws.
const SPRITE_CURSOR: u32 = 0;
const SPRITE_CLOSE: u32 = 10;
const SPRITE_MAXIMIZE: u32 = 9;
const SPRITE_MINIMIZE: u32 = 8;
const SPRITE_COUNT: u32 = 12;
const SPRITE_SIZE: u32 = 32;
pub const FACE_SANS: u32 = 0;
pub const FACE_SANS_MEDIUM: u32 = 1;
pub const FACE_MONO: u32 = 2;

fn faces() -> &'static [crate::assets::Face; FACE_COUNT] {
    // Safety: the supervisor is single-threaded; the faces are written once
    // at boot before any layout reads them.
    unsafe { &*core::ptr::addr_of!(FACES) }
}

/// Map every face and the sprite sheet into `compositor` and remember the
/// faces' metrics. An absent or broken asset is a boot failure: the
/// desktop's text is set in the faces and its icons drawn from the sheet.
fn load_assets(
    machine: &mut arch::Machine,
    storage: &mut ServiceDomain,
    compositor: &mut arch::Domain,
) {
    for (slot, name) in FACE_NAMES.iter().enumerate() {
        match crate::assets::load_face(machine, storage, compositor, name, slot) {
            Ok(face) => {
                // Safety: as in `faces`.
                unsafe { (*core::ptr::addr_of_mut!(FACES))[slot] = face };
                kprint!(
                    "assets: {} {} sizes, {} bytes\n",
                    core::str::from_utf8(name).unwrap_or("?"),
                    face.size_count,
                    face.bytes
                );
            }
            Err(reason) => {
                kprint!(
                    "assets: {} unavailable: {reason}\n",
                    core::str::from_utf8(name).unwrap_or("?")
                );
                failed("a font atlas the desktop needs is missing from the asset region");
            }
        }
    }
    match crate::assets::load_sprites(machine, storage, compositor, SPRITE_SHEET) {
        Ok(count) => kprint!("assets: sprites {count} sprites\n"),
        Err(reason) => {
            kprint!("assets: sprites unavailable: {reason}\n");
            failed("the sprite sheet the desktop needs is missing from the asset region");
        }
    }
}

/// A sprite from the sheet at (x, y), in its own colours when `tint` is
/// zero.
fn sprite_record(x: u32, y: u32, index: u32, tint: u32) -> [u8; RECORD_BYTES] {
    let mut record = [0; RECORD_BYTES];
    for (field, word) in [9, x, y, index, tint, 255].iter().enumerate() {
        put_u32(&mut record, field, *word);
    }
    record
}

/// The width of `text` in `face` at `size`, for layout.
fn measure(face: u32, size: u32, text: &[u8]) -> u32 {
    faces()
        .get(face as usize)
        .map_or(0, |face| face.measure(size as u16, text))
}

/// One line's advance in `face` at `size`.
fn line_height(face: u32, size: u32) -> u32 {
    faces().get(face as usize).map_or(size + size / 4, |face| {
        face.line_height(size as u16).max(size)
    })
}

/// Where a line box of `height` puts a label's top so its baseline sits
/// centred: half the space the ascent leaves.
fn centred_top(face: u32, size: u32, top: u32, height: u32) -> u32 {
    let ascent = faces()
        .get(face as usize)
        .map_or(size, |face| face.ascent(size as u16).max(1));
    top + height.saturating_sub(ascent + size / 4) / 2
}

/// A text record set in a font face: anti-aliased, blended, at most 28 bytes.
fn label_record(
    x: u32,
    y: u32,
    face: u32,
    size: u32,
    color: u32,
    text: &[u8],
) -> [u8; RECORD_BYTES] {
    let mut record = [0; RECORD_BYTES];
    put_u32(&mut record, 0, 6);
    put_u32(&mut record, 1, x);
    put_u32(&mut record, 2, y);
    put_u32(&mut record, 3, face);
    put_u32(&mut record, 4, size);
    put_u32(&mut record, 5, color);
    put_u32(&mut record, 6, 255);
    let length = text.len().min(28);
    put_u32(&mut record, 8, length as u32);
    record[36..36 + length].copy_from_slice(&text[..length]);
    record
}

/// An opaque rectangle with square corners.
fn rect_record(x: u32, y: u32, width: u32, height: u32, color: u32) -> [u8; RECORD_BYTES] {
    let mut record = [0; RECORD_BYTES];
    for (index, word) in [2, x, y, width, height, 0, color].iter().enumerate() {
        put_u32(&mut record, index, *word);
    }
    record
}

/// A rounded box blended over what is below it.
fn surface_record(
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    radius: u32,
    color: u32,
    alpha: u32,
) -> [u8; RECORD_BYTES] {
    let mut record = [0; RECORD_BYTES];
    for (index, word) in [7, x, y, width, height, radius, color, alpha]
        .iter()
        .enumerate()
    {
        put_u32(&mut record, index, *word);
    }
    record
}

const VIOLET: [u32; 3] = [0xe7_9c_fe, 0xcf_7d_ff, 0xe7_9c_fe];
const CYAN: [u32; 3] = [0x63_d0_df, 0x3e_88_ff, 0x63_d0_df];
const AMBER: [u32; 3] = [0xff_ad_00, 0xfe_db_40, 0xff_ad_00];

#[derive(Clone, Copy)]
struct Scene {
    accent: u8,
    workspace: u8,
    title: [u8; 28],
    title_len: u8,
    rectangles: [[u32; 7]; 12],
    rectangle_count: usize,
    previewing: bool,
    inspector: Option<StatusLine>,
    pointer: Option<(u32, u32)>,
    /// What processes wrote, shown in the workshop window.
    terminal: Terminal,
    /// The clock driver's last answer, for the panel.
    clock: Option<Clock>,
    /// What the pointer is over, for the surface under it to say so.
    hover: Hover,
    /// The Applications launcher, when open: the program table's names.
    launcher: Option<Launcher>,
    /// Windows processes asked for, kept after their process ended until
    /// closed; drawn after the workshop and under the launcher.
    windows: [Option<Window>; crate::world::process::WINDOWS],
    /// The window keys go to, while a live process owns it.
    focus: Option<u8>,
    /// The windows back to front: every slot once, the front last.
    order: [u8; crate::world::process::WINDOWS],
    /// A window being moved by its header: the slot, and where in the
    /// window the pointer took hold.
    drag: Option<Drag>,
    /// The window whose content took a press, until the button is released:
    /// motion and the release are its.
    grab: Option<u8>,
    /// The control under a held button, drawn pressed until the release.
    pressed: Hover,
    /// A window being resized by its corner.
    resize: Option<u8>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Drag {
    slot: u8,
    dx: i32,
    dy: i32,
}

/// Bring `slot` to the front of the order.
fn raise(order: &mut [u8; crate::world::process::WINDOWS], slot: u8) {
    if let Some(at) = order.iter().position(|entry| *entry == slot) {
        order[at..].rotate_left(1);
    }
}

const WINDOW_HEADER: u32 = 40;
const WINDOW_RADIUS: u32 = 8;
const WINDOW_SHADOW: u32 = 32;
/// The one-pixel lighter edge around a window and the launcher.
const OUTLINE: u32 = 0x3d_3d_3d;
const EBADF: i64 = 9;
const EBUSY: i64 = 16;
const EINVAL: i64 = 22;
const ENOSPC: i64 = 28;

/// A window a process asked for: its content box on the screen (the
/// header sits above it), its title, and the records the supervisor
/// accepted into it, kept relative to the content so the desktop repaints
/// them with everything else.
#[derive(Clone, Copy)]
struct Window {
    /// Which of the scene's windows this is.
    slot: u8,
    title: [u8; 28],
    title_len: u8,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    records: [[u8; RECORD_BYTES]; crate::world::process::WINDOW_RECORDS],
    count: u8,
    /// The process slot that may draw into it, while its process runs.
    owner: Option<u8>,
    /// Presses in the content and keys while focused, oldest first,
    /// waiting for the owner's `EVENT`; the oldest is dropped when full.
    events: [u64; crate::world::process::WINDOW_EVENTS],
    event_head: u8,
    event_count: u8,
    /// Minimized: kept, not painted, a pill in the panel.
    hidden: bool,
    /// The box a maximized window returns to.
    restore: Option<(u32, u32, u32, u32)>,
}

/// Where the panel's pills for minimized windows start, and their height.
const PILL_X: u32 = 380;
const PILL_Y: u32 = 6;
const PILL_HEIGHT: u32 = 28;
/// The corner a resize takes hold of: this many pixels inside the content.
const CORNER: u32 = 16;

impl Window {
    /// The header's three controls, left to right: minimize, maximize,
    /// close; each a sprite in a square.
    fn control_bounds(&self, control: u8) -> (u32, u32, u32, u32) {
        (
            self.x + self.width - 40 - 32 * u32::from(control),
            self.y - WINDOW_HEADER + 4,
            SPRITE_SIZE,
            SPRITE_SIZE,
        )
    }

    /// The bottom-right corner of the content, for a resize.
    fn corner_bounds(&self) -> (u32, u32, u32, u32) {
        (
            self.x + self.width - CORNER,
            self.y + self.height - CORNER,
            CORNER,
            CORNER,
        )
    }

    /// The pill a minimized window shows in the panel: its left edge and
    /// width, given the pills before it.
    fn pill_width(&self) -> u32 {
        measure(FACE_SANS, 14, self.title()) + 24
    }

    /// Tell the owner the content's size.
    fn announce_size(&mut self) {
        if self.listens() {
            self.queue(
                crate::world::process::EVENT_RESIZE
                    | (u64::from(self.width) << 32)
                    | (u64::from(self.height) << 16),
            );
        }
    }

    /// Queue an event for the owner; a full queue loses its oldest.
    fn queue(&mut self, event: u64) {
        let capacity = self.events.len();
        if usize::from(self.event_count) == capacity {
            self.event_head = ((usize::from(self.event_head) + 1) % capacity) as u8;
            self.event_count -= 1;
        }
        let at = (usize::from(self.event_head) + usize::from(self.event_count)) % capacity;
        self.events[at] = event;
        self.event_count += 1;
    }

    /// Motion is coalesced: a motion behind another motion replaces it,
    /// so a slow reader sees where the pointer is, not where it was.
    fn queue_motion(&mut self, event: u64) {
        if self.event_count > 0 {
            let capacity = self.events.len();
            let last =
                (usize::from(self.event_head) + usize::from(self.event_count) - 1) % capacity;
            if self.events[last] & (0xff << 56) == crate::world::process::EVENT_MOTION {
                self.events[last] = event;
                return;
            }
        }
        self.queue(event);
    }

    /// A content coordinate of the pointer at (px, py), which may lie
    /// outside the content while the button is held: clamped to sixteen
    /// bits and packed like a press.
    fn packed_position(&self, px: u32, py: u32) -> u64 {
        let x = i64::from(px) - i64::from(self.x);
        let y = i64::from(py) - i64::from(self.y);
        ((x.clamp(0, 0xffff) as u64) << 32) | ((y.clamp(0, 0xffff) as u64) << 16)
    }

    fn take(&mut self) -> Option<u64> {
        if self.event_count == 0 {
            return None;
        }
        let event = self.events[usize::from(self.event_head)];
        self.event_head = ((usize::from(self.event_head) + 1) % self.events.len()) as u8;
        self.event_count -= 1;
        Some(event)
    }

    /// Whether a live process may receive events here.
    fn listens(&self) -> bool {
        self.owner.is_some()
    }

    /// The box the window occupies: header and content.
    fn outer(&self) -> (u32, u32, u32, u32) {
        (
            self.x,
            self.y - WINDOW_HEADER,
            self.width,
            self.height + WINDOW_HEADER,
        )
    }

    /// The close control at the header's right.
    fn title(&self) -> &[u8] {
        &self.title[..usize::from(self.title_len)]
    }

    /// Whether a process may draw `record` here: one of the operations a
    /// window admits, lying wholly inside the content. Colours are 24-bit,
    /// alphas at most 255, as the compositor requires, so a refused record
    /// never reaches it.
    fn permits(&self, record: &[u8; RECORD_BYTES]) -> bool {
        let word = |index: usize| record_u32(record, index);
        let colour = |value: u32| value <= 0xff_ff_ff;
        let inside = |x: u32, y: u32, width: u32, height: u32| {
            width > 0
                && height > 0
                && x.checked_add(width)
                    .is_some_and(|right| right <= self.width)
                && y.checked_add(height)
                    .is_some_and(|bottom| bottom <= self.height)
        };
        let (x, y) = (word(1), word(2));
        match word(0) {
            2 | 3 | 7 => {
                let (width, height, radius) = (word(3), word(4), word(5));
                let second = match word(0) {
                    2 => colour(word(6)),
                    3 => colour(word(6)) && colour(word(7)),
                    _ => colour(word(6)) && word(7) <= 255,
                };
                inside(x, y, width, height) && radius <= width.min(height) / 2 && second
            }
            4 => {
                let (rx, ry) = (word(3), word(4));
                rx > 0
                    && ry > 0
                    && x >= rx
                    && y >= ry
                    && inside(x - rx, y - ry, 2 * rx, 2 * ry)
                    && colour(word(5))
            }
            6 => {
                let (face, size, length) = (word(3), word(4), word(8) as usize);
                face < FACE_COUNT as u32
                    && (1..=64).contains(&size)
                    && colour(word(5))
                    && word(6) <= 255
                    && length <= 28
                    && inside(
                        x,
                        y,
                        measure(face, size, &record[36..36 + length]).max(1),
                        line_height(face, size),
                    )
            }
            9 => {
                word(3) < SPRITE_COUNT
                    && colour(word(4))
                    && word(5) <= 255
                    && inside(x, y, SPRITE_SIZE, SPRITE_SIZE)
            }
            _ => false,
        }
    }
}

/// A window's records for the frame: its shadow when asked, its surface
/// and header, the title, the close control, then what the process drew,
/// moved to the content's origin.
fn window_records(
    frame: &mut Frame,
    window: &Window,
    hover: Hover,
    shadow: bool,
) -> Result<(), &'static str> {
    let (x, y, width, height) = window.outer();
    if shadow {
        let mut record = [0; RECORD_BYTES];
        for (field, word) in [8, x, y, width, height, WINDOW_RADIUS, WINDOW_SHADOW, 170]
            .iter()
            .enumerate()
        {
            put_u32(&mut record, field, *word);
        }
        frame.push(record)?;
    }
    // The edge: a lighter box one pixel larger, under the surface, kept
    // on the screen for a window at its edge.
    let left = x.saturating_sub(1);
    let top = y.saturating_sub(1);
    let right = (x + width + 1).min(crate::world::SCENE_WIDTH);
    let bottom = (y + height + 1).min(crate::world::SCENE_HEIGHT);
    frame.push(surface_record(
        left,
        top,
        right - left,
        bottom - top,
        WINDOW_RADIUS + 1,
        OUTLINE,
        255,
    ))?;
    frame.push(surface_record(
        x,
        y,
        width,
        height,
        WINDOW_RADIUS,
        0x1b_1b_1b,
        255,
    ))?;
    frame.push(surface_record(
        x,
        y,
        width,
        WINDOW_HEADER,
        WINDOW_RADIUS,
        0x26_26_26,
        255,
    ))?;
    frame.push(rect_record(
        x,
        y + WINDOW_RADIUS,
        width,
        WINDOW_HEADER - WINDOW_RADIUS,
        0x26_26_26,
    ))?;
    let title_width = measure(FACE_SANS_MEDIUM, 16, window.title());
    frame.push(label_record(
        x + width.saturating_sub(title_width) / 2,
        centred_top(FACE_SANS_MEDIUM, 16, y, WINDOW_HEADER),
        FACE_SANS_MEDIUM,
        16,
        0xde_de_de,
        window.title(),
    ))?;
    // The controls: minimize, maximize, close, the hovered one lit.
    let slot = window.slot;
    let controls = [
        (0, SPRITE_CLOSE, Hover::WindowClose(slot)),
        (1, SPRITE_MAXIMIZE, Hover::WindowMaximize(slot)),
        (2, SPRITE_MINIMIZE, Hover::WindowMinimize(slot)),
    ];
    for (control, sprite, lit) in controls {
        let (cx, cy, _, _) = window.control_bounds(control);
        let hovered = hover == lit;
        if hovered {
            frame.push(surface_record(
                cx,
                cy,
                SPRITE_SIZE,
                SPRITE_SIZE,
                16,
                0xff_ff_ff,
                40,
            ))?;
        }
        frame.push(sprite_record(
            cx,
            cy,
            sprite,
            if hovered { 0xde_de_de } else { 0x9e_9e_9e },
        ))?;
    }
    // What the process drew, where it still fits: a window made smaller
    // keeps its records, and shows those inside its content.
    for record in &window.records[..usize::from(window.count)] {
        if !window.permits(record) {
            continue;
        }
        let mut moved = *record;
        put_u32(&mut moved, 1, record_u32(record, 1) + window.x);
        put_u32(&mut moved, 2, record_u32(record, 2) + window.y);
        frame.push(moved)?;
    }
    Ok(())
}

/// The desktop as a process sees it through `WINDOW` and `DRAW`: the
/// windows of the scene, painted as they change. What a process may draw
/// is decided here and nowhere else.
struct Desk<'a, 'b> {
    compositor: &'a mut arch::Domain,
    inputs: Option<&'a mut Inputs<'b>>,
    windows: &'a mut [Option<Window>; crate::world::process::WINDOWS],
    focus: &'a mut Option<u8>,
    order: &'a mut [u8; crate::world::process::WINDOWS],
    pointer: Option<(u32, u32)>,
}

impl Desk<'_, '_> {
    /// Repaint one window where it is: with its shadow when it is new,
    /// otherwise only its box, so the shadow is never blended twice.
    fn paint(&mut self, slot: usize, shadow: bool) {
        let Some(window) = self.windows[slot] else {
            return;
        };
        let mut frame = Frame::empty();
        if window_records(&mut frame, &window, Hover::Nothing, shadow).is_err() {
            return;
        }
        let (x, y, width, height) = window.outer();
        let region = if shadow {
            (
                x.saturating_sub(WINDOW_SHADOW),
                y.saturating_sub(WINDOW_SHADOW),
                width + 2 * WINDOW_SHADOW,
                height + 2 * WINDOW_SHADOW,
            )
        } else {
            (x, y, width, height)
        };
        if let Some((px, py)) = self.pointer {
            let _ = frame.push(sprite_record(px, py, SPRITE_CURSOR, 0));
        }
        let _ = render_region(self.compositor, self.inputs.as_deref_mut(), &frame, region);
    }
}

impl crate::process::Display for Desk<'_, '_> {
    fn open(&mut self, owner: usize, width: u32, height: u32, title: &[u8]) -> i64 {
        use crate::world::process::{WINDOW_MAX, WINDOW_MIN};
        if width < WINDOW_MIN.0
            || height < WINDOW_MIN.1
            || width > WINDOW_MAX.0
            || height > WINDOW_MAX.1
        {
            return -EINVAL;
        }
        if self
            .windows
            .iter()
            .flatten()
            .any(|window| window.owner == Some(owner as u8))
        {
            return -EBUSY;
        }
        let Some(slot) = self.windows.iter().position(Option::is_none) else {
            return -EBUSY;
        };
        // Cascaded from the workshop's upper left, kept on the screen.
        let step = 64 * slot as u32;
        let x = (560 + step).min(crate::world::SCENE_WIDTH - width - WINDOW_SHADOW);
        let y = (120 + WINDOW_HEADER + step)
            .min(crate::world::SCENE_DRAWABLE_HEIGHT - height - WINDOW_SHADOW)
            .max(WINDOW_HEADER + 48);
        let mut window = Window {
            slot: slot as u8,
            title: [0; 28],
            title_len: title.len().min(28) as u8,
            x,
            y,
            width,
            height,
            records: [[0; RECORD_BYTES]; crate::world::process::WINDOW_RECORDS],
            count: 0,
            owner: Some(owner as u8),
            events: [0; crate::world::process::WINDOW_EVENTS],
            event_head: 0,
            event_count: 0,
            hidden: false,
            restore: None,
        };
        window.title[..usize::from(window.title_len)]
            .copy_from_slice(&title[..usize::from(window.title_len)]);
        self.windows[slot] = Some(window);
        // A new window is in front and has the keyboard.
        raise(self.order, slot as u8);
        *self.focus = Some(slot as u8);
        self.paint(slot, true);
        slot as i64
    }

    fn draw(
        &mut self,
        owner: usize,
        window: u64,
        flags: u64,
        records: &[[u8; RECORD_BYTES]],
    ) -> i64 {
        let slot = window as usize;
        let Some(Some(target)) = self.windows.get_mut(slot) else {
            return -EBADF;
        };
        if target.owner != Some(owner as u8) {
            return -EBADF;
        }
        if records.iter().any(|record| !target.permits(record)) {
            return -EINVAL;
        }
        let kept = if flags & crate::world::process::DRAW_CLEAR != 0 {
            0
        } else {
            usize::from(target.count)
        };
        if kept + records.len() > target.records.len() {
            return -ENOSPC;
        }
        target.records[kept..kept + records.len()].copy_from_slice(records);
        target.count = (kept + records.len()) as u8;
        self.paint(slot, false);
        i64::from(self.windows[slot].map_or(0, |window| window.count))
    }

    fn event(&mut self, owner: usize, window: u64) -> Result<Option<u64>, i64> {
        match self.windows.get_mut(window as usize) {
            Some(Some(target)) if target.owner == Some(owner as u8) => Ok(target.take()),
            _ => Err(EBADF),
        }
    }

    fn release(&mut self, owner: usize) {
        for window in self.windows.iter_mut().flatten() {
            if window.owner == Some(owner as u8) {
                window.owner = None;
            }
        }
    }
}

/// The terminal panel's box on the screen, for repainting it alone.
const TERMINAL_REGION: (u32, u32, u32, u32) = (452, 292, 1360, 508);

/// Where the desktop's one run lives: a run is four domains and four
/// pipes, too large for the supervisor's stack beside the frames, and it
/// outlasts the command that started it when the program listens.
static mut RUN_SLOT: crate::process::RunSlot = crate::process::RunSlot::UNINIT;

/// A fresh, empty run in the desktop's slot. Called only while no run is
/// in flight: the reference the last run held is gone by then.
fn prepare_run() -> &'static mut crate::process::Run {
    // Safety: a single supervisor, and the caller holds no run.
    unsafe { (*core::ptr::addr_of_mut!(RUN_SLOT)).prepare() }
}

/// Passes over a running process's table between two inputs: enough to
/// keep it moving, few enough that the next key is not kept waiting.
const PASSES_PER_IDLE: usize = 16;

/// Minutes and the date, as the panel shows them.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Clock {
    hours: u8,
    minutes: u8,
    day: u8,
    month: u8,
}

impl Clock {
    #[cfg(target_arch = "x86_64")]
    fn from_packed(packed: u64) -> Self {
        Self {
            minutes: (packed >> 8) as u8,
            hours: (packed >> 16) as u8,
            day: (packed >> 24) as u8,
            month: (packed >> 32) as u8,
        }
    }
}

const MONTHS: [&[u8]; 12] = [
    b"January",
    b"February",
    b"March",
    b"April",
    b"May",
    b"June",
    b"July",
    b"August",
    b"September",
    b"October",
    b"November",
    b"December",
];

/// What the pointer is over. Each has bounds, for the hover highlight and
/// for the click that does what the surface says.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Hover {
    Nothing,
    Applications,
    Dock(u8),
    Launcher(u8),
    /// A window's body: a click there is the window's, not the desktop's.
    Window(u8),
    /// A window's header: a press there takes hold of the window.
    WindowHeader(u8),
    WindowClose(u8),
    WindowMaximize(u8),
    WindowMinimize(u8),
    /// The content's bottom-right corner: a press there resizes.
    WindowCorner(u8),
    /// A minimized window's pill in the panel.
    Pill(u8),
}

const DOCK_TILES: u32 = 7;

impl Hover {
    /// The rectangle a hover covers, for redrawing when it changes.
    fn bounds(self, scene: &Scene) -> Option<(u32, u32, u32, u32)> {
        match self {
            Hover::Nothing => None,
            Hover::Applications => Some((8, 0, 120, 40)),
            Hover::Dock(tile) => Some((712 + u32::from(tile) * 72, 932, 64, 64)),
            Hover::Launcher(entry) => scene.launcher.map(|_| {
                (
                    LAUNCHER_X,
                    LAUNCHER_Y + 56 + u32::from(entry) * 48,
                    LAUNCHER_WIDTH,
                    48,
                )
            }),
            Hover::Window(_) | Hover::WindowHeader(_) | Hover::WindowCorner(_) => None,
            Hover::WindowClose(slot) => scene.windows[usize::from(slot)]
                .as_ref()
                .map(|window| window.control_bounds(0)),
            Hover::WindowMaximize(slot) => scene.windows[usize::from(slot)]
                .as_ref()
                .map(|window| window.control_bounds(1)),
            Hover::WindowMinimize(slot) => scene.windows[usize::from(slot)]
                .as_ref()
                .map(|window| window.control_bounds(2)),
            Hover::Pill(slot) => scene.pill(slot),
        }
    }

    /// What is under (x, y) in this scene.
    fn at(scene: &Scene, x: u32, y: u32) -> Self {
        if let Some(launcher) = scene.launcher {
            if (LAUNCHER_X..LAUNCHER_X + LAUNCHER_WIDTH).contains(&x)
                && (LAUNCHER_Y + 56..LAUNCHER_Y + 56 + launcher.count as u32 * 48).contains(&y)
            {
                return Hover::Launcher(((y - LAUNCHER_Y - 56) / 48) as u8);
            }
        }
        let inside = |bounds: (u32, u32, u32, u32)| {
            (bounds.0..bounds.0 + bounds.2).contains(&x)
                && (bounds.1..bounds.1 + bounds.3).contains(&y)
        };
        // Front to back.
        for slot in scene.order.iter().rev() {
            let Some(window) = scene.windows[usize::from(*slot)] else {
                continue;
            };
            if window.hidden {
                continue;
            }
            if inside(window.control_bounds(0)) {
                return Hover::WindowClose(*slot);
            }
            if inside(window.control_bounds(1)) {
                return Hover::WindowMaximize(*slot);
            }
            if inside(window.control_bounds(2)) {
                return Hover::WindowMinimize(*slot);
            }
            if inside(window.corner_bounds()) {
                return Hover::WindowCorner(*slot);
            }
            if inside(window.outer()) {
                return if y < window.y {
                    Hover::WindowHeader(*slot)
                } else {
                    Hover::Window(*slot)
                };
            }
        }
        for slot in 0..crate::world::process::WINDOWS as u8 {
            if scene.pill(slot).is_some_and(inside) {
                return Hover::Pill(slot);
            }
        }
        if y < 40 && (8..128).contains(&x) {
            return Hover::Applications;
        }
        if (932..996).contains(&y) && x >= 712 {
            let tile = (x - 712) / 72;
            if tile < DOCK_TILES && (x - 712) % 72 < 64 {
                return Hover::Dock(tile as u8);
            }
        }
        Hover::Nothing
    }
}

const LAUNCHER_X: u32 = 24;
const LAUNCHER_Y: u32 = 48;
const LAUNCHER_WIDTH: u32 = 360;
const LAUNCHER_ENTRIES: usize = 8;

/// The launcher's contents: the program region's names at the moment it
/// was opened.
#[derive(Clone, Copy)]
struct Launcher {
    names: [[u8; crate::region::NAME_BYTES]; LAUNCHER_ENTRIES],
    lengths: [u8; LAUNCHER_ENTRIES],
    count: usize,
}

impl Launcher {
    fn name(&self, entry: usize) -> &[u8] {
        &self.names[entry][..usize::from(self.lengths[entry])]
    }

    fn height(&self) -> u32 {
        56 + self.count.max(1) as u32 * 48 + 16
    }
}

/// The terminal panel: a bounded scrollback of what processes and the file
/// commands wrote, one process's console on the desktop. Carriage returns
/// are ignored, newlines end a row, and the oldest row scrolls away.
#[derive(Clone, Copy)]
struct Terminal {
    rows: [[u8; Terminal::COLUMNS]; Terminal::ROWS],
    lengths: [u8; Terminal::ROWS],
    /// Rows holding text, oldest first; the last is the one being written.
    used: usize,
    /// The last byte was a newline: the next byte opens a row.
    line_ended: bool,
    /// Written since the panel was last painted.
    dirty: bool,
}

impl Terminal {
    const ROWS: usize = 16;
    const COLUMNS: usize = 84;

    const EMPTY: Self = Self {
        rows: [[0; Self::COLUMNS]; Self::ROWS],
        lengths: [0; Self::ROWS],
        used: 0,
        line_ended: false,
        dirty: false,
    };

    fn is_empty(&self) -> bool {
        self.used == 0
    }

    fn newline(&mut self) {
        if self.used < Self::ROWS {
            self.used += 1;
        } else {
            self.rows.copy_within(1.., 0);
            self.lengths.copy_within(1.., 0);
        }
        let last = self.used - 1;
        self.rows[last] = [0; Self::COLUMNS];
        self.lengths[last] = 0;
    }

    /// A newline ends the row; the next row opens when something is written
    /// on it, so a line a process finished does not leave a blank one.
    fn push(&mut self, bytes: &[u8]) {
        self.dirty = true;
        for byte in bytes {
            match *byte {
                b'\r' => {}
                b'\n' => self.line_ended = true,
                byte => {
                    if self.used == 0 || self.line_ended {
                        self.newline();
                        self.line_ended = false;
                    }
                    let last = self.used - 1;
                    let length = usize::from(self.lengths[last]);
                    if length >= Self::COLUMNS {
                        self.newline();
                    }
                    let last = self.used - 1;
                    let length = usize::from(self.lengths[last]);
                    self.rows[last][length] = if byte.is_ascii_graphic() || byte == b' ' {
                        byte
                    } else {
                        b'?'
                    };
                    self.lengths[last] = (length + 1) as u8;
                }
            }
        }
    }
}

/// A console that writes to the serial console driver and to the terminal
/// panel: what the harness reads, and what the desktop shows.
struct Tee<'a> {
    serial: &'a mut ServiceDomain,
    terminal: &'a mut Terminal,
}

impl crate::process::Console for Tee<'_> {
    fn write(&mut self, bytes: &[u8]) {
        crate::process::Console::write(self.serial, bytes);
        self.terminal.push(bytes);
    }
}

impl Scene {
    /// The panel pill of minimized window `slot`, if it is minimized: the
    /// pills sit left to right in slot order.
    fn pill(&self, slot: u8) -> Option<(u32, u32, u32, u32)> {
        let mut x = PILL_X;
        for (index, window) in self.windows.iter().enumerate() {
            let Some(window) = window else {
                continue;
            };
            if !window.hidden {
                continue;
            }
            let width = window.pill_width();
            if index == usize::from(slot) {
                return Some((x, PILL_Y, width, PILL_HEIGHT));
            }
            x += width + 8;
        }
        None
    }

    fn initial() -> Self {
        let mut title = [0; 28];
        let text = b"MOLD THE SYSTEM AS IT RUNS";
        title[..text.len()].copy_from_slice(text);
        Self {
            accent: 0,
            workspace: 1,
            title,
            title_len: text.len() as u8,
            rectangles: [[0; 7]; 12],
            rectangle_count: 0,
            previewing: false,
            inspector: None,
            pointer: None,
            terminal: Terminal::EMPTY,
            clock: None,
            hover: Hover::Nothing,
            launcher: None,
            windows: [None; crate::world::process::WINDOWS],
            focus: None,
            order: [0, 1],
            drag: None,
            grab: None,
            pressed: Hover::Nothing,
            resize: None,
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
    shifts: u8,
    controls: u8,
    caps: bool,
    extended: bool,
}

impl Keyboard {
    const fn new() -> Self {
        Self {
            shifts: 0,
            controls: 0,
            caps: false,
            extended: false,
        }
    }

    fn decode(&mut self, scan: u8) -> Option<u8> {
        if scan == 0xe0 {
            self.extended = true;
            return None;
        }
        let extended = self.extended;
        self.extended = false;
        let released = scan & 0x80 != 0;
        let code = scan & 0x7f;
        if code == 0x1d {
            let mask = if extended { 2 } else { 1 };
            if released {
                self.controls &= !mask;
            } else {
                self.controls |= mask;
            }
            return None;
        }
        if extended {
            return if !released && code == 0x1c {
                Some(b'\n')
            } else {
                None
            };
        }
        if code == 0x2a || code == 0x36 {
            let mask = if code == 0x2a { 1 } else { 2 };
            if released {
                self.shifts &= !mask;
            } else {
                self.shifts |= mask;
            }
            return None;
        }
        if released {
            return None;
        }
        if code == 0x3a {
            self.caps = !self.caps;
            return None;
        }
        if self.controls != 0 {
            return match code {
                0x16 | 0x2e => Some(0x1b), // Ctrl-U/C: clear input
                0x23 => Some(0x08),        // Ctrl-H: backspace
                _ => None,
            };
        }
        let shift = self.shifts != 0;
        let letter = match code {
            0x10..=0x19 => Some(b"qwertyuiop"[(code - 0x10) as usize]),
            0x1e..=0x26 => Some(b"asdfghjkl"[(code - 0x1e) as usize]),
            0x2c..=0x32 => Some(b"zxcvbnm"[(code - 0x2c) as usize]),
            _ => None,
        };
        if let Some(letter) = letter {
            return Some(if shift ^ self.caps {
                letter.to_ascii_uppercase()
            } else {
                letter
            });
        }
        Some(match code {
            0x01 => 0x1b,
            0x0f => b'\t',
            0x0e => 0x08,
            0x1c => b'\n',
            0x39 => b' ',
            0x02..=0x0b => {
                const PLAIN: &[u8; 10] = b"1234567890";
                const SHIFTED: &[u8; 10] = b"!@#$%^&*()";
                let table = if shift { SHIFTED } else { PLAIN };
                table[(code - 0x02) as usize]
            }
            0x0c => {
                if shift {
                    b'_'
                } else {
                    b'-'
                }
            }
            0x0d => {
                if shift {
                    b'+'
                } else {
                    b'='
                }
            }
            0x1a => {
                if shift {
                    b'{'
                } else {
                    b'['
                }
            }
            0x1b => {
                if shift {
                    b'}'
                } else {
                    b']'
                }
            }
            0x27 => {
                if shift {
                    b':'
                } else {
                    b';'
                }
            }
            0x28 => {
                if shift {
                    b'"'
                } else {
                    b'\''
                }
            }
            0x29 => {
                if shift {
                    b'~'
                } else {
                    b'`'
                }
            }
            0x2b => {
                if shift {
                    b'|'
                } else {
                    b'\\'
                }
            }
            0x33 => {
                if shift {
                    b'<'
                } else {
                    b','
                }
            }
            0x34 => {
                if shift {
                    b'>'
                } else {
                    b'.'
                }
            }
            0x35 => {
                if shift {
                    b'?'
                } else {
                    b'/'
                }
            }
            0x56 => {
                if shift {
                    b'>'
                } else {
                    b'<'
                }
            }
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

/// The Bochs display interface QEMU's standard VGA exposes: an index port
/// and a data port through which the resolution can be set directly, with
/// the linear framebuffer staying where the BIOS mode put it.
#[cfg(target_arch = "x86_64")]
const DISPI_INDEX: u16 = 0x1ce;
#[cfg(target_arch = "x86_64")]
const DISPI_DATA: u16 = 0x1cf;
#[cfg(target_arch = "x86_64")]
const DISPI_ID: u16 = 0;
#[cfg(target_arch = "x86_64")]
const DISPI_XRES: u16 = 1;
#[cfg(target_arch = "x86_64")]
const DISPI_YRES: u16 = 2;
#[cfg(target_arch = "x86_64")]
const DISPI_BPP: u16 = 3;
#[cfg(target_arch = "x86_64")]
const DISPI_ENABLE: u16 = 4;
#[cfg(target_arch = "x86_64")]
const DISPI_VIRT_WIDTH: u16 = 6;
#[cfg(target_arch = "x86_64")]
const DISPI_ENABLED_LFB: u16 = 0x41;

#[cfg(target_arch = "x86_64")]
fn dispi_write(index: u16, value: u16) {
    // Safety: the Bochs interface's two ports; writing them configures the
    // emulated display and nothing else.
    unsafe {
        arch::hal::out16(DISPI_INDEX, index);
        arch::hal::out16(DISPI_DATA, value);
    }
}

#[cfg(target_arch = "x86_64")]
fn dispi_read(index: u16) -> u16 {
    // Safety: as in `dispi_write`; reading has no side effect.
    unsafe {
        arch::hal::out16(DISPI_INDEX, index);
        arch::hal::in16(DISPI_DATA)
    }
}

impl Framebuffer {
    /// The framebuffer this machine gives: on x86-64 the BIOS mode's
    /// linear framebuffer, switched to the scene's size through the display
    /// interface when there is one; on a board, what the firmware's mailbox
    /// answers for the scene's size.
    #[cfg(target_arch = "x86_64")]
    fn acquire() -> Option<Self> {
        let framebuffer = Self::discover()?;
        Some(framebuffer.native().unwrap_or(framebuffer))
    }

    #[cfg(target_arch = "aarch64")]
    fn acquire() -> Option<Self> {
        let width = crate::world::SCENE_WIDTH;
        let height = crate::world::SCENE_HEIGHT;
        let (physical, pitch, bytes) = arch::framebuffer(width, height)?;
        if bytes > MAX_FRAMEBUFFER_BYTES {
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

#[cfg(target_arch = "x86_64")]
impl Framebuffer {
    /// The display at the scene's native size, when the Bochs interface is
    /// there to set it: the mode the BIOS stage chose is replaced by
    /// 1920×1080×32 at the same linear framebuffer. Without the interface
    /// (real firmware, another card) the BIOS mode stays and the scene is
    /// scaled into it.
    fn native(self) -> Option<Self> {
        if !(0xb0c0..=0xb0cf).contains(&dispi_read(DISPI_ID)) {
            return None;
        }
        let width = crate::world::SCENE_WIDTH;
        let height = crate::world::SCENE_HEIGHT;
        dispi_write(DISPI_ENABLE, 0);
        dispi_write(DISPI_XRES, width as u16);
        dispi_write(DISPI_YRES, height as u16);
        dispi_write(DISPI_BPP, 32);
        dispi_write(DISPI_VIRT_WIDTH, width as u16);
        dispi_write(DISPI_ENABLE, DISPI_ENABLED_LFB);
        if dispi_read(DISPI_XRES) != width as u16 || dispi_read(DISPI_YRES) != height as u16 {
            return None;
        }
        let pitch = width * 4;
        let bytes = u64::from(pitch) * u64::from(height);
        if bytes > MAX_FRAMEBUFFER_BYTES {
            return None;
        }
        Some(Self {
            physical: self.physical,
            width,
            height,
            pitch,
            bytes,
        })
    }

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

#[cfg(target_arch = "x86_64")]
fn read_u8(address: usize) -> u8 {
    // Safety: the VBE mode block is a fixed BIOS handoff in low memory.
    unsafe { (address as *const u8).read_volatile() }
}

#[cfg(target_arch = "x86_64")]
fn read_u16(address: usize) -> u16 {
    u16::from_le_bytes([read_u8(address), read_u8(address + 1)])
}

#[cfg(target_arch = "x86_64")]
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

fn panel_record() -> [u8; RECORD_BYTES] {
    // Opaque: keystrokes redraw only this field and what is on it.
    surface_record(60, 1012, 1800, 60, 12, 0x26_26_26, 255)
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
        6 => &[5],
        7 => &[6],
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
        if record_text(&record) == b"Workspace 1" {
            record[46] = b'0' + scene.workspace;
        } else if record_text(&record) == b"MOLD THE SYSTEM AS IT RUNS" {
            replace_text(&mut record, &scene.title[..scene.title_len as usize]);
        }
        frame.push(record)?;
    }
    for rectangle in &scene.rectangles[..scene.rectangle_count] {
        let mut record = [0; RECORD_BYTES];
        for (index, word) in rectangle.iter().enumerate() {
            put_u32(&mut record, index, *word);
        }
        frame.push(record)?;
    }
    // The panel's centre: the clock, or the workspace's name until the
    // clock driver has answered.
    let mut text = crate::process::Line::new();
    if let Some(clock) = scene.clock {
        let month = MONTHS
            .get(usize::from(clock.month).wrapping_sub(1))
            .copied()
            .unwrap_or(b"?");
        let _ = write!(
            text,
            "{} {}, {:02}:{:02}",
            core::str::from_utf8(month).unwrap_or("?"),
            clock.day,
            clock.hours,
            clock.minutes
        );
    } else {
        let _ = write!(text, "Workspace {}", scene.workspace);
    }
    let width = measure(FACE_SANS_MEDIUM, 16, text.get());
    frame.push(label_record(
        (crate::world::SCENE_WIDTH.saturating_sub(width)) / 2,
        10,
        FACE_SANS_MEDIUM,
        16,
        0xde_de_de,
        text.get(),
    ))?;
    // The surface under the pointer says so.
    match scene.hover {
        Hover::Applications => {
            frame.push(surface_record(8, 4, 120, 32, 8, 0xff_ff_ff, 28))?;
        }
        Hover::Dock(tile) => {
            let x = 716 + u32::from(tile) * 72;
            frame.push(surface_record(x, 936, 56, 56, 14, 0xff_ff_ff, 60))?;
        }
        Hover::Pill(slot) => {
            if let Some((x, y, width, height)) = scene.pill(slot) {
                frame.push(surface_record(x, y, width, height, 8, 0xff_ff_ff, 28))?;
            }
        }
        Hover::Launcher(_)
        | Hover::Window(_)
        | Hover::WindowHeader(_)
        | Hover::WindowClose(_)
        | Hover::WindowMaximize(_)
        | Hover::WindowMinimize(_)
        | Hover::WindowCorner(_)
        | Hover::Nothing => {}
    }
    // Minimized windows: pills in the panel, their titles.
    for slot in 0..crate::world::process::WINDOWS as u8 {
        let Some((x, y, width, height)) = scene.pill(slot) else {
            continue;
        };
        let Some(window) = &scene.windows[usize::from(slot)] else {
            continue;
        };
        frame.push(surface_record(x, y, width, height, 8, 0x33_33_33, 255))?;
        frame.push(label_record(
            x + 12,
            centred_top(FACE_SANS, 14, y, height),
            FACE_SANS,
            14,
            0xde_de_de,
            window.title(),
        ))?;
    }
    // The control under a held button darkens, as COSMIC's do.
    match scene.pressed {
        Hover::Applications => {
            frame.push(surface_record(8, 4, 120, 32, 8, 0x00_00_00, 80))?;
        }
        Hover::Dock(tile) => {
            let x = 716 + u32::from(tile) * 72;
            frame.push(surface_record(x, 936, 56, 56, 14, 0x00_00_00, 80))?;
        }
        _ => {}
    }
    // The terminal panel: processes' output, or a hint when nothing ran.
    frame.push(surface_record(452, 292, 1360, 508, 16, 0x1b_1b_1b, 255))?;
    frame.push(label_record(
        476,
        308,
        FACE_SANS_MEDIUM,
        14,
        0x80_80_80,
        b"TERMINAL",
    ))?;
    if scene.terminal.is_empty() {
        frame.push(label_record(
            476,
            340,
            FACE_MONO,
            16,
            0x63_63_63,
            b":exec NAME runs a program here",
        ))?;
    } else {
        let spacing = line_height(FACE_MONO, 16).max(24);
        for row in 0..scene.terminal.used {
            let text = &scene.terminal.rows[row][..usize::from(scene.terminal.lengths[row])];
            let y = 340 + row as u32 * spacing;
            for (piece, chunk) in text.chunks(28).enumerate() {
                let x = 476 + measure(FACE_MONO, 16, &text[..piece * 28]);
                frame.push(label_record(x, y, FACE_MONO, 16, 0xde_de_de, chunk))?;
            }
        }
    }
    // Back to front, so the front window covers the others.
    for slot in scene.order {
        if let Some(window) = &scene.windows[usize::from(slot)] {
            if window.hidden {
                continue;
            }
            window_records(&mut frame, window, scene.hover, true)?;
        }
    }
    if let Some(launcher) = scene.launcher {
        let height = launcher.height();
        let mut shadow = [0; RECORD_BYTES];
        for (field, word) in [
            8,
            LAUNCHER_X,
            LAUNCHER_Y,
            LAUNCHER_WIDTH,
            height,
            16,
            WINDOW_SHADOW,
            170,
        ]
        .iter()
        .enumerate()
        {
            put_u32(&mut shadow, field, *word);
        }
        frame.push(shadow)?;
        frame.push(surface_record(
            LAUNCHER_X - 1,
            LAUNCHER_Y - 1,
            LAUNCHER_WIDTH + 2,
            height + 2,
            17,
            OUTLINE,
            255,
        ))?;
        frame.push(surface_record(
            LAUNCHER_X,
            LAUNCHER_Y,
            LAUNCHER_WIDTH,
            height,
            16,
            0x1b_1b_1b,
            250,
        ))?;
        frame.push(label_record(
            LAUNCHER_X + 24,
            LAUNCHER_Y + 18,
            FACE_SANS_MEDIUM,
            16,
            0xde_de_de,
            b"Applications",
        ))?;
        if launcher.count == 0 {
            frame.push(label_record(
                LAUNCHER_X + 24,
                LAUNCHER_Y + 70,
                FACE_SANS,
                14,
                0x80_80_80,
                b"no programs installed",
            ))?;
        }
        for entry in 0..launcher.count {
            let y = LAUNCHER_Y + 56 + entry as u32 * 48;
            if scene.hover == Hover::Launcher(entry as u8) {
                let pressed = scene.pressed == scene.hover;
                frame.push(surface_record(
                    LAUNCHER_X + 8,
                    y,
                    LAUNCHER_WIDTH - 16,
                    48,
                    8,
                    if pressed { 0x11_11_11 } else { 0x33_33_33 },
                    255,
                ))?;
            }
            frame.push(sprite_record(LAUNCHER_X + 24, y + 8, 2, 0x9e_9e_9e))?;
            frame.push(label_record(
                LAUNCHER_X + 72,
                y + 14,
                FACE_SANS_MEDIUM,
                16,
                0xde_de_de,
                launcher.name(entry),
            ))?;
        }
    }
    if let Some(text) = scene.inspector {
        frame.push(surface_record(560, 300, 800, 400, 16, 0x1b_1b_1b, 250))?;
        frame.push(label_record(
            588,
            320,
            FACE_SANS_MEDIUM,
            20,
            0xde_de_de,
            b"Agent source",
        ))?;
        frame.push(label_record(
            740,
            325,
            FACE_SANS,
            14,
            0x80_80_80,
            b"Esc closes",
        ))?;
        let spacing = line_height(FACE_MONO, 16);
        for (index, chunk) in text.get().chunks(28).enumerate() {
            frame.push(label_record(
                588,
                364 + index as u32 * spacing,
                FACE_MONO,
                16,
                0x9e_9e_9e,
                chunk,
            ))?;
        }
    }
    if let Some(line) = line {
        let mut prompt = [b' '; 28];
        prompt[..2].copy_from_slice(b"> ");
        let shown = if line.len() > DISPLAY_LINE_BYTES {
            &line[line.len() - DISPLAY_LINE_BYTES..]
        } else {
            line
        };
        let length = shown.len();
        prompt[2..2 + length].copy_from_slice(shown);
        frame.push(panel_record())?;
        frame.push(label_record(
            84,
            centred_top(FACE_MONO, 16, 1012, 32),
            FACE_MONO,
            16,
            accent[2],
            &prompt[..2 + length],
        ))?;
        frame.push(label_record(
            84,
            centred_top(FACE_SANS, 14, 1042, 28),
            FACE_SANS,
            14,
            0x9e_9e_9e,
            status,
        ))?;
    }
    if let Some((x, y)) = scene.pointer {
        frame.push(sprite_record(
            x.min(crate::world::SCENE_WIDTH - 1),
            y.min(crate::world::SCENE_HEIGHT - 1),
            SPRITE_CURSOR,
            0,
        ))?;
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

fn render(
    domain: &mut arch::Domain,
    mut inputs: Option<&mut Inputs<'_>>,
    frame: &Frame,
) -> Result<(), &'static str> {
    for command in &frame.records[..frame.count] {
        for (offset, byte) in command.iter().enumerate() {
            domain.core().write_payload(offset, *byte);
        }
        domain
            .core()
            .write_shared(shared::ARGUMENTS, RECORD_BYTES as u64);
        request(domain, shared::COMMAND_DISPLAY_DRAW)?;
        if let Some(inputs) = inputs.as_deref_mut() {
            inputs.drain();
        }
    }
    Ok(())
}

/// Redraw the whole frame, but only the pixels inside `region`: what a
/// pointer that moved or a widget that changed needs, at the cost of that
/// area rather than the screen.
fn render_region(
    domain: &mut arch::Domain,
    inputs: Option<&mut Inputs<'_>>,
    frame: &Frame,
    region: (u32, u32, u32, u32),
) -> Result<(), &'static str> {
    let (x, y, width, height) = region;
    let core = domain.core();
    core.write_shared(shared::CLIP_X, u64::from(x));
    core.write_shared(shared::CLIP_Y, u64::from(y));
    core.write_shared(shared::CLIP_WIDTH, u64::from(width.max(1)));
    core.write_shared(shared::CLIP_HEIGHT, u64::from(height.max(1)));
    let outcome = render(domain, inputs, frame);
    domain.core().write_shared(shared::CLIP_WIDTH, 0);
    outcome
}

fn render_overlay(
    domain: &mut arch::Domain,
    mut inputs: Option<&mut Inputs<'_>>,
    frame: &Frame,
) -> Result<(), &'static str> {
    // A pointer, when visible, follows the three command-bar records.
    let has_pointer = frame.count > 0 && record_u32(&frame.records[frame.count - 1], 0) == 9;
    let start = frame.count.saturating_sub(if has_pointer { 4 } else { 3 });
    for command in &frame.records[start..frame.count] {
        for (offset, byte) in command.iter().enumerate() {
            domain.core().write_payload(offset, *byte);
        }
        domain
            .core()
            .write_shared(shared::ARGUMENTS, RECORD_BYTES as u64);
        request(domain, shared::COMMAND_DISPLAY_DRAW)?;
        if let Some(inputs) = inputs.as_deref_mut() {
            inputs.drain();
        }
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

/// The kernel's own A/B state on the serial log at boot.
#[cfg(target_arch = "x86_64")]
fn report_kernel_slot(kernel: &KernelRecovery) {
    let Some(booted) = kernel.booted() else {
        kprint!("kernel: boot stage has no slot selector; slot A loaded\n");
        return;
    };
    let selector = kernel.selector();
    if kernel.rolled_back() {
        kprint!(
            "watchdog fault: candidate kernel slot {} failed {} boots; booted trusted slot {}\n",
            slot_name(selector.candidate),
            selector.attempts,
            slot_name(selector.trusted)
        );
    }
    kprint!(
        "kernel: running slot {}; trusted slot {}",
        slot_name(booted),
        slot_name(selector.trusted)
    );
    if selector.candidate == crate::workspace::NO_CANDIDATE {
        kprint!("; no candidate\n");
    } else if !selector.admitted {
        kprint!(
            "; candidate slot {} (staged, not admitted)\n",
            slot_name(selector.candidate)
        );
    } else {
        kprint!(
            "; candidate slot {} ({}, boots {})\n",
            slot_name(selector.candidate),
            if selector.verified {
                "verified"
            } else {
                "unverified"
            },
            selector.attempts
        );
    }
}

fn restore_from_disk(
    evaluator: &mut arch::Domain,
    storage: &mut ServiceDomain,
    recovery: Option<&mut LiveRecovery>,
) -> Result<(Workspace, u64, u64), &'static str> {
    let candidates = crate::workspace::load(storage)?;
    let newest = candidates
        .iter()
        .flatten()
        .map(|loaded| loaded.generation)
        .max()
        .unwrap_or(0);
    if let Some(recovery) = recovery {
        if let BootPlan::Rollback {
            trusted,
            candidate,
            attempts,
        } = recovery.plan_boot(storage, newest)?
        {
            kprint!(
                "watchdog fault: candidate generation {} failed {} boots; rolling back to generation {}\n",
                candidate,
                attempts,
                trusted
            );
            if let Some(loaded) = candidates
                .iter()
                .flatten()
                .find(|loaded| loaded.generation == trusted)
            {
                if let Ok(revision) = replay_workspace(evaluator, &loaded.workspace) {
                    recovery.note_rollback();
                    return Ok((loaded.workspace, loaded.generation, revision));
                }
            }
            kprint!("trusted generation unavailable; trying the newest generation instead\n");
        }
    }
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
    machine: &mut arch::Machine,
    compositor: &mut arch::Domain,
    inputs: Option<&mut Inputs<'_>>,
    evaluator: &mut arch::Domain,
    storage: &mut ServiceDomain,
    filesystem: Option<&mut ServiceDomain>,
    serial: &mut ServiceDomain,
    recovery: &mut Option<LiveRecovery>,
    kernel: &mut Option<KernelRecovery>,
    current: &mut Scene,
    previous: &mut Scene,
    scene_revision: &mut u8,
    evaluator_revision: &mut u64,
    workspace: &mut Workspace,
    committed_workspace: &mut Workspace,
    generation: &mut u64,
    dirty: &mut bool,
    running: &mut Option<&'static mut crate::process::Run>,
    line: &[u8],
) -> StatusLine {
    let line = trim(line);
    if line.is_empty() {
        return StatusLine::new(b"READY");
    }
    if scene_command(line) {
        return execute(compositor, inputs, current, previous, scene_revision, line);
    }
    if line == b":shutdown" {
        arch::exit(true);
    }
    if line == b":workbench" {
        if workspace.count() != 0 || *evaluator_revision != 0 {
            return StatusLine::new(b"WORKBENCH NEEDS A FRESH EMPTY WORLD");
        }
        let mut candidate = *workspace;
        for (index, source) in include_bytes!("../../desktop/workbench.agel")
            .split(|byte| *byte == b'\n')
            .filter(|line| line.starts_with(b"("))
            .enumerate()
        {
            let mut name = StatusLine::new(b"wb-");
            name.number_u64(index as u64);
            if let Err(reason) = candidate.upsert(name.get(), source) {
                return StatusLine::new(reason.as_bytes());
            }
        }
        return match replay_workspace(evaluator, &candidate) {
            Ok(revision) => {
                *workspace = candidate;
                *evaluator_revision = revision;
                *dirty = true;
                StatusLine::new(b"WORKBENCH READY - CLICK OR TAB")
            }
            Err(failure) => {
                let _ = replay_workspace(evaluator, workspace);
                StatusLine::new(failure.message().as_bytes())
            }
        };
    }
    // Programs and files, answered on the serial console and in the
    // terminal panel; the frame is repainted after, as after any command.
    // A program runs to its end here, as in the serial workshop, unless it
    // waits for an event in a window: then the desktop takes the input
    // back and runs it between inputs, until it ends.
    if let Some(rest) = line.strip_prefix(b":exec ") {
        if running.is_some() {
            return StatusLine::new(b"A PROCESS IS RUNNING");
        }
        let mut filesystem = filesystem;
        let mut tee = Tee {
            serial,
            terminal: &mut current.terminal,
        };
        let mut desk = Desk {
            compositor,
            inputs,
            windows: &mut current.windows,
            focus: &mut current.focus,
            order: &mut current.order,
            pointer: current.pointer,
        };
        let run = prepare_run();
        if crate::workshop::start_program(
            machine,
            storage,
            filesystem.as_deref_mut(),
            &mut tee,
            Some(&mut desk),
            rest,
            run,
        )
        .is_none()
        {
            return StatusLine::new(b"PROCESS NOT STARTED");
        }
        let mut services = crate::process::Services {
            storage,
            console: &mut tee,
            filesystem,
            display: Some(&mut desk as &mut dyn crate::process::Display),
        };
        loop {
            match crate::process::step_run(machine, &mut services, run) {
                crate::process::Progress::Running => {}
                crate::process::Progress::Listening => {
                    *running = Some(run);
                    return StatusLine::new(b"PROCESS LISTENING");
                }
                crate::process::Progress::Sleeping => {
                    *running = Some(run);
                    return StatusLine::new(b"PROCESS SLEEPING");
                }
                crate::process::Progress::Ended(exit) => {
                    crate::workshop::finish_program(machine, services.console, run, exit);
                    return StatusLine::new(b"PROCESS ENDED");
                }
            }
        }
    }
    // The header's other controls, and the pills, as typed commands.
    for (prefix, action) in [
        (&b":maximize "[..], 0_u8),
        (&b":minimize "[..], 1),
        (&b":restore "[..], 2),
    ] {
        let Some(rest) = line.strip_prefix(prefix) else {
            continue;
        };
        let slot = match trim(rest) {
            [digit @ b'0'..=b'9'] => usize::from(digit - b'0'),
            _ => return StatusLine::new(b"USAGE :maximize|:minimize|:restore N"),
        };
        let Some(Some(window)) = current.windows.get_mut(slot) else {
            return StatusLine::new(b"NO SUCH WINDOW");
        };
        match action {
            0 => {
                if let Some((x, y, width, height)) = window.restore.take() {
                    window.x = x;
                    window.y = y;
                    window.width = width;
                    window.height = height;
                } else {
                    window.restore = Some((window.x, window.y, window.width, window.height));
                    window.x = 0;
                    window.y = 40 + WINDOW_HEADER;
                    window.width = crate::world::SCENE_WIDTH;
                    window.height = crate::world::SCENE_DRAWABLE_HEIGHT - window.y;
                }
                window.hidden = false;
                window.announce_size();
            }
            1 => {
                window.hidden = true;
                if current.focus == Some(slot as u8) {
                    current.focus = None;
                }
            }
            _ => {
                window.hidden = false;
                raise(&mut current.order, slot as u8);
                current.focus = Some(slot as u8);
            }
        }
        let mut status = StatusLine::new(match action {
            0 if current.windows[slot].is_some_and(|window| window.restore.is_some()) => {
                b"WINDOW MAXIMIZED "
            }
            0 => b"WINDOW RESTORED ",
            1 => b"WINDOW MINIMIZED ",
            _ => b"WINDOW RESTORED ",
        });
        status.number(slot as u8);
        return status;
    }
    if let Some(rest) = line.strip_prefix(b":close ") {
        let slot = match trim(rest) {
            [digit @ b'0'..=b'9'] => usize::from(digit - b'0'),
            _ => return StatusLine::new(b"USAGE :close N"),
        };
        return match current.windows.get_mut(slot) {
            Some(window) if window.is_some() => {
                *window = None;
                if current.focus == Some(slot as u8) {
                    current.focus = None;
                }
                if current.grab == Some(slot as u8) {
                    current.grab = None;
                }
                if current.drag.is_some_and(|drag| drag.slot == slot as u8) {
                    current.drag = None;
                }
                if current.resize == Some(slot as u8) {
                    current.resize = None;
                }
                let mut status = StatusLine::new(b"WINDOW CLOSED ");
                status.number(slot as u8);
                status
            }
            _ => StatusLine::new(b"NO SUCH WINDOW"),
        };
    }
    if line == b":fs-format"
        || line == b":fs-ls"
        || line.starts_with(b":fs-ls ")
        || line.starts_with(b":fs-mkdir ")
    {
        let command = if line == b":fs-format" {
            crate::workshop::FilesystemCommand::Format
        } else if let Some(path) = line.strip_prefix(b":fs-mkdir ") {
            crate::workshop::FilesystemCommand::MakeDirectory(path)
        } else {
            crate::workshop::FilesystemCommand::List(line.strip_prefix(b":fs-ls ").unwrap_or(b"/"))
        };
        let mut tee = Tee {
            serial,
            terminal: &mut current.terminal,
        };
        crate::workshop::filesystem_command(Some(storage), filesystem, &mut tee, command);
        return StatusLine::new(b"FILESYSTEM");
    }
    if line == b":fs-restart" {
        let mut tee = Tee {
            serial,
            terminal: &mut current.terminal,
        };
        let Some(service) = filesystem else {
            crate::workshop::line(&mut tee, b"denied: no filesystem service");
            return StatusLine::new(b"FILESYSTEM");
        };
        match service.restart(machine) {
            Ok(()) => {
                let mut out = crate::process::Line::new();
                let _ = write!(
                    out,
                    "filesystem restarted: generation {}",
                    service.generation()
                );
                crate::workshop::line(&mut tee, out.get());
            }
            Err(reason) => {
                crate::workshop::text(&mut tee, b"filesystem restart failed: ", reason.as_bytes())
            }
        }
        return StatusLine::new(b"FILESYSTEM");
    }
    if line == b":help" {
        return StatusLine::new(HELP_POSTCARD);
    }
    if line == b":revision" {
        let mut status = StatusLine::new(b"EVAL REV ");
        status.number_u64(*evaluator_revision);
        return status;
    }
    if let Some(source) = line.strip_prefix(b":preview ") {
        return evaluator_status(
            evaluator,
            evaluator_revision,
            shared::COMMAND_EVALUATOR_PREVIEW,
            source,
        );
    }
    if let Some(id) = line.strip_prefix(b":source ") {
        if let Some(id) = core::str::from_utf8(id)
            .ok()
            .and_then(|s| s.parse::<u8>().ok())
        {
            return evaluator_status(
                evaluator,
                evaluator_revision,
                shared::COMMAND_EVALUATOR_SOURCE,
                &[id],
            );
        }
        return StatusLine::new(b"SOURCE EXPECTS AGENT ID");
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
        let protect = recovery
            .as_ref()
            .map_or(0, |recovery| recovery.record().trusted);
        return match crate::native_session::save(
            evaluator,
            Some(storage),
            workspace,
            *generation,
            protect,
        ) {
            Ok((next, revision)) => {
                *generation = next;
                *committed_workspace = *workspace;
                *evaluator_revision = revision;
                *dirty = false;
                if let Some(recovery) = recovery.as_mut() {
                    if recovery.on_saved(storage, next).is_err() {
                        console::write("recovery record not updated\n");
                    }
                }
                let mut status = StatusLine::new(b"SAVED GENERATION ");
                status.number_u64(next);
                status
            }
            Err(reason) => StatusLine::new(reason.as_bytes()),
        };
    }
    if line == b":recovery" {
        let Some(recovery) = recovery.as_ref() else {
            return StatusLine::new(b"RECOVERY RECORD UNAVAILABLE");
        };
        let record = recovery.record();
        let mut status = StatusLine::new(b"TRUSTED GEN ");
        status.number_u64(record.trusted);
        if record.candidate != 0 {
            status.push(b" CANDIDATE GEN ");
            status.number_u64(record.candidate);
            status.push(b" BOOTS ");
            status.number_u64(u64::from(record.attempts));
            status.push(if record.verified {
                b" VERIFIED"
            } else {
                b" UNVERIFIED"
            });
        }
        if recovery.rolled_back() {
            status.push(b" ROLLED BACK");
        }
        return status;
    }
    #[cfg(target_arch = "x86_64")]
    if line == b":kernel" {
        let Some(kernel) = kernel.as_ref() else {
            return StatusLine::new(b"KERNEL SELECTOR UNAVAILABLE");
        };
        let Some(booted) = kernel.booted() else {
            return StatusLine::new(b"KERNEL SLOT A NO SELECTOR IN BOOT STAGE");
        };
        let selector = kernel.selector();
        let mut status = StatusLine::new(b"KERNEL SLOT ");
        status.push(slot_name(booted).as_bytes());
        status.push(b" TRUSTED ");
        status.push(slot_name(selector.trusted).as_bytes());
        if selector.candidate != crate::workspace::NO_CANDIDATE && !selector.admitted {
            status.push(b" STAGED ");
            status.push(slot_name(selector.candidate).as_bytes());
        } else if selector.candidate != crate::workspace::NO_CANDIDATE {
            status.push(b" CANDIDATE ");
            status.push(slot_name(selector.candidate).as_bytes());
            status.push(b" BOOTS ");
            status.number_u64(u64::from(selector.attempts));
            status.push(if selector.verified {
                b" VERIFIED"
            } else {
                b" UNVERIFIED"
            });
        }
        if kernel.rolled_back() {
            status.push(b" ROLLED BACK");
        }
        return status;
    }
    if line == b":reload" {
        return match restore_from_disk(evaluator, storage, None) {
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
        b":promote" => shared::COMMAND_EVALUATOR_PROMOTE,
        b":discard" => shared::COMMAND_EVALUATOR_DISCARD,
        b":rollback" => shared::COMMAND_EVALUATOR_ROLLBACK,
        b":defs" => shared::COMMAND_EVALUATOR_DEFS,
        b":limits" => shared::COMMAND_EVALUATOR_LIMITS,
        _ if line.starts_with(b":") => return StatusLine::new(b"UNKNOWN COMMAND - :HELP"),
        _ => shared::COMMAND_EVALUATE,
    };
    let before = *evaluator_revision;
    let status = evaluator_status(
        evaluator,
        evaluator_revision,
        command,
        if command == shared::COMMAND_EVALUATE {
            line
        } else {
            b""
        },
    );
    // A form evaluated after boot is the health oracle every generation gets
    // for free: the desktop reached an interactive, working state.
    if command == shared::COMMAND_EVALUATE && *evaluator_revision > before {
        if let Some(recovery) = recovery.as_mut() {
            if let Ok(Some(verified)) = recovery.healthy(storage, *generation) {
                kprint!(
                    "candidate generation {} verified by a healthy boot\n",
                    verified
                );
            }
        }
        #[cfg(target_arch = "x86_64")]
        if let Some(kernel) = kernel.as_mut() {
            if let Ok(Some(slot)) = kernel.healthy(storage) {
                kprint!(
                    "kernel slot {} verified by a healthy boot\n",
                    slot_name(slot)
                );
            }
        }
        #[cfg(not(target_arch = "x86_64"))]
        let _ = &kernel;
    }
    status
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
    inputs: Option<&mut Inputs<'_>>,
    current: &mut Scene,
    previous: &mut Scene,
    revision: &mut u8,
    candidate: Scene,
) -> Result<u64, &'static str> {
    let frame = materialize(candidate, None, b"")?;
    render(compositor, inputs, &frame)?;
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
    inputs: Option<&mut Inputs<'_>>,
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
        Intent::Rollback => {
            candidate = *previous;
            // What processes made is not part of the scene transaction.
            candidate.windows = current.windows;
            candidate.terminal = current.terminal;
        }
        Intent::Inspect | Intent::Help => unreachable!(),
    }
    match commit_scene(compositor, inputs, current, previous, revision, candidate) {
        Ok(_) => {
            let mut status = StatusLine::new(b"COMMITTED REV ");
            status.number(*revision);
            status
        }
        Err(_) => StatusLine::new(b"TRANSACTION ABORTED"),
    }
}

enum Input {
    Byte(u8),
    Pointer(bool),
}

/// The input driver, and a queue of what it produced while the compositor
/// was painting. A frame takes long enough under emulation that keys typed
/// meanwhile would overrun the controller's buffer if nobody read them, so
/// painting drains the driver between records and the session reads the
/// queue first.
const INPUT_QUEUE: usize = 256;

struct Inputs<'a> {
    /// The 8042 driver domain; a board without one has an empty queue.
    driver: Option<&'a mut ServiceDomain>,
    queue: [(bool, u8); INPUT_QUEUE],
    head: usize,
    length: usize,
}

impl<'a> Inputs<'a> {
    fn new(driver: Option<&'a mut ServiceDomain>) -> Self {
        Self {
            driver,
            queue: [(false, 0); INPUT_QUEUE],
            head: 0,
            length: 0,
        }
    }

    /// Read whatever the driver has, up to a bounded burst, into the queue.
    fn drain(&mut self) {
        #[cfg(not(target_arch = "x86_64"))]
        let _ = &self.driver;
        #[cfg(target_arch = "x86_64")]
        let Some(driver) = self.driver.as_deref_mut() else {
            return;
        };
        #[cfg(target_arch = "x86_64")]
        for _ in 0..32 {
            let handle = driver.handle();
            match driver.read_input(handle) {
                Ok(Some(pair)) => {
                    if self.length == INPUT_QUEUE {
                        return;
                    }
                    self.queue[(self.head + self.length) % INPUT_QUEUE] = pair;
                    self.length += 1;
                }
                _ => return,
            }
        }
    }

    /// The next queued byte, without taking it.
    fn peek(&mut self) -> Option<(bool, u8)> {
        if self.length == 0 {
            self.drain();
        }
        (self.length > 0).then(|| self.queue[self.head])
    }

    fn pop(&mut self) -> Option<(bool, u8)> {
        if self.length == 0 {
            return None;
        }
        let pair = self.queue[self.head];
        self.head = (self.head + 1) % INPUT_QUEUE;
        self.length -= 1;
        Some(pair)
    }
}

fn next_input(
    console: &mut ServiceDomain,
    inputs: &mut Inputs<'_>,
    keyboard: &mut Keyboard,
    pointer: &mut crate::pointer::Pointer,
) -> Option<Input> {
    if let Ok(Some(byte)) = console.read_console(console.handle()) {
        return Some(Input::Byte(byte));
    }
    if inputs.length == 0 {
        inputs.drain();
    }
    let (auxiliary, byte) = inputs.pop()?;
    if auxiliary {
        pointer.feed(byte).map(Input::Pointer)
    } else {
        keyboard.decode(byte).map(Input::Byte)
    }
}

/// Pull committed language data through the bounded evaluator page. Validate
/// the complete candidate before any command reaches the display domain.
fn synchronize_language_scene(
    evaluator: &mut arch::Domain,
    compositor: &mut arch::Domain,
    mut inputs: Option<&mut Inputs<'_>>,
    current: &mut Scene,
) -> Result<(), &'static str> {
    let mut candidate = *current;
    candidate.rectangles = [[0; 7]; 12];
    let mut count = 0;
    let mut revision = 0;
    for index in 0..12 {
        let reply = evaluator_request(
            evaluator,
            shared::COMMAND_EVALUATOR_SCENE,
            &[index as u8 | if current.previewing { 128 } else { 0 }],
        )?;
        if index == 0 {
            count = reply.bytes[0] as usize;
            revision = reply.revision;
        }
        if reply.error
            || count > 12
            || reply.bytes[0] as usize != count
            || reply.revision != revision
        {
            return Err("invalid native scene envelope");
        }
        if count == 0 && reply.length == 1 {
            break;
        }
        if reply.length != 29 {
            return Err("invalid native scene record length");
        }
        let mut rect = [0_u32; 7];
        for (field, value) in rect.iter_mut().enumerate() {
            let start = 1 + field * 4;
            *value = u32::from_le_bytes(reply.bytes[start..start + 4].try_into().unwrap());
        }
        let [opcode, x, y, width, height, radius, rgb] = rect;
        if opcode != 2
            || x > crate::world::SCENE_WIDTH
            || y > crate::world::SCENE_DRAWABLE_HEIGHT
            || width == 0
            || height == 0
            || width > crate::world::SCENE_WIDTH - x
            || height > crate::world::SCENE_DRAWABLE_HEIGHT - y
            || radius > width / 2
            || radius > height / 2
            || rgb > 0xffffff
        {
            return Err("native scene rectangle rejected");
        }
        candidate.rectangles[index] = rect;
        if index + 1 == count {
            break;
        }
    }
    candidate.rectangle_count = count;
    if current.rectangle_count == count && current.rectangles == candidate.rectangles {
        return Ok(());
    }
    let frame = materialize(candidate, None, b"")?;
    if let Err(reason) = render(compositor, inputs.as_deref_mut(), &frame) {
        let old = materialize(*current, None, b"")?;
        let _ = render(compositor, inputs, &old);
        return Err(reason);
    }
    *current = candidate;
    Ok(())
}

fn interactive(
    machine: &mut arch::Machine,
    compositor: &mut arch::Domain,
    mut storage: ServiceDomain,
    mut current: Scene,
    mut previous: Scene,
) -> ! {
    let evaluator_entry = crate::user::agel_evaluator_main as *const () as usize as u64;
    let mut evaluator = machine
        .create_evaluator_world(evaluator_entry, 20)
        .unwrap_or_else(|reason| failed(reason));
    let mut recovery = match LiveRecovery::load(&mut storage) {
        Ok(recovery) => Some(recovery),
        Err(reason) => {
            kprint!("recovery record unavailable: {}\n", reason);
            None
        }
    };
    #[cfg(not(target_arch = "x86_64"))]
    let mut kernel: Option<KernelRecovery> = None;
    #[cfg(target_arch = "x86_64")]
    let mut kernel = match KernelRecovery::load(&mut storage) {
        Ok(mut kernel) => {
            match kernel.admit(&mut storage) {
                Ok(Admission::Nothing) => {}
                Ok(Admission::Admitted(slot)) => kprint!(
                    "candidate kernel slot {} admitted: signature verified against the kernel's trust key; next boot tries it\n",
                    slot_name(slot)
                ),
                Ok(Admission::Refused(slot, reason)) => kprint!(
                    "candidate kernel slot {} refused: {}; slot cleared\n",
                    slot_name(slot),
                    reason
                ),
                Err(reason) => kprint!("candidate kernel could not be checked: {}\n", reason),
            }
            report_kernel_slot(&kernel);
            Some(kernel)
        }
        Err(reason) => {
            kprint!("kernel selector unavailable: {}\n", reason);
            None
        }
    };
    let (mut workspace, mut generation, mut evaluator_revision) =
        restore_from_disk(&mut evaluator, &mut storage, recovery.as_mut())
            .unwrap_or_else(|reason| failed(reason));
    let mut committed_workspace = workspace;
    let mut dirty = false;
    let mut scene_revision = 0_u8;
    // Input leaves the supervisor: serial bytes come through the console
    // driver domain and keyboard/pointer bytes through the 8042 driver domain,
    // each granted only its own ports. The input driver comes before the
    // first frame is painted, so painting can drain it.
    let console_entry = crate::user::agel_world_main as *const () as usize as u64;
    let mut console_driver = machine
        .create_console_world(console_entry, 8)
        .map(|domain| ServiceDomain::new(domain, ServiceKind::Console, console_entry, 8))
        .unwrap_or_else(|reason| failed(reason));
    // The keyboard and pointer: an 8042 driver domain on x86-64; a board
    // has no such controller, and its keyboard is the serial console.
    #[cfg(target_arch = "x86_64")]
    let input_entry = crate::user::agel_input_main as *const () as usize as u64;
    #[cfg(target_arch = "x86_64")]
    let mut input_driver = machine
        .create_input_world(input_entry, 50)
        .map(|domain| ServiceDomain::new(domain, ServiceKind::Input, input_entry, 50))
        .unwrap_or_else(|reason| failed(reason));
    #[cfg(target_arch = "x86_64")]
    match input_driver.enable_pointer(input_driver.handle()) {
        Ok(true) => {}
        Ok(false) => console::write("pointer unavailable; keyboard remains active\n"),
        Err(_) => failed("the input driver domain stopped during pointer enable"),
    }
    // The filesystem service, as the serial workshop has it: an unprivileged
    // world owning a region of the disk through relayed sector requests.
    let mut filesystem = {
        let entry = crate::user::agel_fs_main as *const () as usize as u64;
        match machine.create_filesystem_world(entry, crate::world::fs::TICKS) {
            Ok(domain) => Some(ServiceDomain::new(
                domain,
                ServiceKind::Filesystem,
                entry,
                crate::world::fs::TICKS,
            )),
            Err(reason) => {
                kprint!("filesystem service unavailable: {reason}\n");
                None
            }
        }
    };
    // The clock driver: the CMOS clock behind two ports, read at boot and
    // then while idle, so the panel's time is the machine's.
    #[cfg(target_arch = "x86_64")]
    let clock_entry = crate::user::agel_clock_main as *const () as usize as u64;
    #[cfg(target_arch = "x86_64")]
    let mut clock_driver = machine
        .create_clock_world(clock_entry, 8)
        .map(|domain| ServiceDomain::new(domain, ServiceKind::Clock, clock_entry, 8))
        .ok();
    #[cfg(not(target_arch = "x86_64"))]
    kprint!("clock: none on this board; the panel shows the workspace\n");
    #[cfg(target_arch = "x86_64")]
    if let Some(driver) = clock_driver.as_mut() {
        let handle = driver.handle();
        match driver.read_clock(handle) {
            Ok(Some(packed)) => {
                let clock = Clock::from_packed(packed);
                current.clock = Some(clock);
                previous.clock = Some(clock);
                kprint!(
                    "clock: 20{:02}-{:02}-{:02} {:02}:{:02}\n",
                    (packed >> 40) as u8,
                    clock.month,
                    clock.day,
                    clock.hours,
                    clock.minutes
                );
            }
            _ => kprint!("clock: unavailable\n"),
        }
    }
    #[cfg(target_arch = "x86_64")]
    let mut inputs = Inputs::new(Some(&mut input_driver));
    #[cfg(not(target_arch = "x86_64"))]
    let mut inputs = Inputs::new(None);
    synchronize_language_scene(&mut evaluator, compositor, Some(&mut inputs), &mut current)
        .unwrap_or_else(|reason| failed(reason));
    let mut line = [0; INPUT_BYTES];
    let mut length = 0;
    let mut keyboard = Keyboard::new();
    let mut pointer = crate::pointer::Pointer::new(
        crate::world::SCENE_WIDTH as i32,
        crate::world::SCENE_HEIGHT as i32,
    );
    let mut status = if generation == 0 {
        StatusLine::new(b"AGEL READY - TYPE :HELP")
    } else {
        let mut status = StatusLine::new(b"RESTORED GEN ");
        status.number_u64(generation);
        status
    };
    let frame = materialize(current, Some(&line[..length]), status.get())
        .unwrap_or_else(|reason| failed(reason));
    render_overlay(compositor, Some(&mut inputs), &frame).unwrap_or_else(|reason| failed(reason));
    console::write("live-desktop> ");

    let mut idle: u32 = 0;
    // A process that waits for its window's events, run between inputs.
    let mut running: Option<&'static mut crate::process::Run> = None;
    // The pointer's button as of the last packet, for releases.
    let mut held = false;
    loop {
        let Some(input) = next_input(
            &mut console_driver,
            &mut inputs,
            &mut keyboard,
            &mut pointer,
        ) else {
            if let Some(run) = running.as_mut() {
                let ended = {
                    let mut tee = Tee {
                        serial: &mut console_driver,
                        terminal: &mut current.terminal,
                    };
                    let mut desk = Desk {
                        compositor,
                        inputs: Some(&mut inputs),
                        windows: &mut current.windows,
                        focus: &mut current.focus,
                        order: &mut current.order,
                        pointer: current.pointer,
                    };
                    let mut services = crate::process::Services {
                        storage: &mut storage,
                        console: &mut tee,
                        filesystem: filesystem.as_mut(),
                        display: Some(&mut desk as &mut dyn crate::process::Display),
                    };
                    let mut ended = None;
                    for _ in 0..PASSES_PER_IDLE {
                        match crate::process::step_run(machine, &mut services, run) {
                            crate::process::Progress::Running => {}
                            crate::process::Progress::Listening
                            | crate::process::Progress::Sleeping => break,
                            crate::process::Progress::Ended(exit) => {
                                crate::workshop::finish_program(
                                    machine,
                                    services.console,
                                    run,
                                    exit,
                                );
                                ended = Some(exit);
                                break;
                            }
                        }
                    }
                    ended
                };
                if ended.is_some() {
                    running = None;
                    status = StatusLine::new(b"PROCESS ENDED");
                    current.terminal.dirty = false;
                    let frame = materialize(current, Some(&line[..length]), status.get())
                        .unwrap_or_else(|reason| failed(reason));
                    render(compositor, Some(&mut inputs), &frame)
                        .unwrap_or_else(|reason| failed(reason));
                    console::write_bytes(status.get());
                    console::write("\nlive-desktop> ");
                } else if current.terminal.dirty {
                    current.terminal.dirty = false;
                    let frame = materialize(current, Some(&line[..length]), status.get())
                        .unwrap_or_else(|reason| failed(reason));
                    render_region(compositor, Some(&mut inputs), &frame, TERMINAL_REGION)
                        .unwrap_or_else(|reason| failed(reason));
                }
            }
            idle = idle.wrapping_add(1);
            #[cfg(target_arch = "x86_64")]
            if idle.is_multiple_of(200_000) {
                // While idle, the clock: repainted only when its minute turns.
                if let Some(driver) = clock_driver.as_mut() {
                    let handle = driver.handle();
                    if let Ok(Some(packed)) = driver.read_clock(handle) {
                        let clock = Clock::from_packed(packed);
                        if current.clock != Some(clock) {
                            current.clock = Some(clock);
                            previous.clock = Some(clock);
                            let frame = materialize(current, Some(&line[..length]), status.get())
                                .unwrap_or_else(|reason| failed(reason));
                            render_region(compositor, Some(&mut inputs), &frame, (600, 0, 720, 40))
                                .unwrap_or_else(|reason| failed(reason));
                        }
                    }
                }
            }
            core::hint::spin_loop();
            continue;
        };
        let byte = match input {
            Input::Byte(byte) => {
                // A window with the keyboard and a live owner takes the key.
                if let Some(window) = current
                    .focus
                    .and_then(|slot| current.windows[usize::from(slot)].as_mut())
                    .filter(|window| window.listens() && running.is_some())
                {
                    window.queue(crate::world::process::EVENT_KEY | u64::from(byte));
                    continue;
                }
                byte
            }
            Input::Pointer(pressed) => {
                let before = current.pointer;
                // Motion queued behind this packet moves the pointer before
                // anything is painted: one redraw for the whole path, so a
                // burst of packets never waits on a paint per packet. A
                // press ends the run and is handled where it landed.
                let mut pressed = pressed;
                while !pressed {
                    match inputs.peek() {
                        Some((true, byte)) => {
                            inputs.pop();
                            if let Some(press) = pointer.feed(byte) {
                                pressed = press;
                            }
                        }
                        _ => break,
                    }
                }
                let (px, py) = (pointer.x as u32, pointer.y as u32);
                current.pointer = Some((px, py));
                let released = held && !pointer.down();
                held = pointer.down();
                let pressed_before = current.pressed;
                if released {
                    current.pressed = Hover::Nothing;
                }
                // A window taken hold of by its header follows the pointer
                // while the button is held; the old and new places are
                // repainted together.
                if let Some(drag) = current.drag {
                    if let Some(window) = current.windows[usize::from(drag.slot)].as_mut() {
                        let (ox, oy, width, height) = window.outer();
                        let x = (i64::from(px) - i64::from(drag.dx))
                            .clamp(0, i64::from(crate::world::SCENE_WIDTH - width));
                        let y = (i64::from(py) - i64::from(drag.dy)).clamp(
                            i64::from(WINDOW_HEADER + 40),
                            i64::from(crate::world::SCENE_DRAWABLE_HEIGHT - window.height),
                        );
                        window.x = x as u32;
                        window.y = y as u32;
                        let (nx, ny, _, _) = window.outer();
                        if released {
                            current.drag = None;
                        }
                        let left = ox.min(nx).saturating_sub(WINDOW_SHADOW);
                        let top = oy.min(ny).saturating_sub(WINDOW_SHADOW);
                        let right = (ox.max(nx) + width + WINDOW_SHADOW + 32)
                            .min(crate::world::SCENE_WIDTH);
                        let bottom = (oy.max(ny) + height + WINDOW_SHADOW + 40)
                            .min(crate::world::SCENE_HEIGHT);
                        current.hover = Hover::at(&current, px, py);
                        let frame = materialize(current, Some(&line[..length]), status.get())
                            .unwrap_or_else(|reason| failed(reason));
                        render_region(
                            compositor,
                            Some(&mut inputs),
                            &frame,
                            (left, top, right - left, bottom - top),
                        )
                        .unwrap_or_else(|reason| failed(reason));
                        continue;
                    }
                    current.drag = None;
                }
                // A window taken hold of by its corner grows and shrinks
                // with the pointer; the release tells its owner the size.
                if let Some(slot) = current.resize {
                    if let Some(window) = current.windows[usize::from(slot)].as_mut() {
                        let (ox, oy, ow, oh) = window.outer();
                        let (min_width, min_height) = crate::world::process::WINDOW_MIN;
                        let width = (i64::from(px) - i64::from(window.x) + i64::from(CORNER) / 2)
                            .clamp(
                                i64::from(min_width),
                                i64::from(crate::world::SCENE_WIDTH - window.x),
                            );
                        let height = (i64::from(py) - i64::from(window.y) + i64::from(CORNER) / 2)
                            .clamp(
                                i64::from(min_height),
                                i64::from(crate::world::SCENE_DRAWABLE_HEIGHT - window.y),
                            );
                        window.width = width as u32;
                        window.height = height as u32;
                        window.restore = None;
                        if released {
                            current.resize = None;
                            window.announce_size();
                        }
                        let left = ox.saturating_sub(WINDOW_SHADOW);
                        let top = oy.saturating_sub(WINDOW_SHADOW);
                        let right = (ox + ow.max(window.width) + WINDOW_SHADOW + 32)
                            .min(crate::world::SCENE_WIDTH);
                        let bottom =
                            (oy + oh.max(window.height + WINDOW_HEADER) + WINDOW_SHADOW + 40)
                                .min(crate::world::SCENE_HEIGHT);
                        current.hover = Hover::at(&current, px, py);
                        let frame = materialize(current, Some(&line[..length]), status.get())
                            .unwrap_or_else(|reason| failed(reason));
                        render_region(
                            compositor,
                            Some(&mut inputs),
                            &frame,
                            (left, top, right - left, bottom - top),
                        )
                        .unwrap_or_else(|reason| failed(reason));
                        continue;
                    }
                    current.resize = None;
                }
                // A window whose content took the press has the pointer
                // until the release: motion and the release are its.
                if let Some(slot) = current.grab {
                    if let Some(window) = current.windows[usize::from(slot)].as_mut() {
                        let position = window.packed_position(px, py);
                        if released {
                            window.queue(crate::world::process::EVENT_RELEASE | position);
                        } else if !pressed {
                            window.queue_motion(crate::world::process::EVENT_MOTION | position);
                        }
                    }
                    if released {
                        current.grab = None;
                    }
                }
                let over = Hover::at(&current, px, py);
                let hovered_before = current.hover;
                current.hover = over;
                if pressed && over != Hover::Nothing && length == 0 {
                    // A click on something the desktop itself owns: it runs
                    // as a typed command, so the console shows it too.
                    let mut command = StatusLine::new(b"");
                    let mut close_launcher = true;
                    if matches!(
                        over,
                        Hover::Applications | Hover::Dock(_) | Hover::Launcher(_)
                    ) {
                        current.pressed = over;
                    }
                    match over {
                        Hover::Applications | Hover::Dock(0) | Hover::Dock(5) => {
                            if current.launcher.is_none() {
                                let listing = crate::region::Region {
                                    table: crate::process::TABLE_SECTOR,
                                    last: crate::process::LAST_SECTOR,
                                    magic: b"AGELPR1\0",
                                }
                                .list::<LAUNCHER_ENTRIES>(&mut storage);
                                current.launcher = listing.ok().map(|listing| Launcher {
                                    names: listing.names,
                                    lengths: listing.lengths,
                                    count: listing.count,
                                });
                                close_launcher = false;
                            }
                        }
                        Hover::Launcher(entry) => {
                            if let Some(launcher) = current.launcher {
                                command.push(b":exec ");
                                command.push(launcher.name(usize::from(entry)));
                            }
                        }
                        Hover::Dock(1) => current.terminal = Terminal::EMPTY,
                        Hover::Dock(2) => command.push(b":fs-ls /"),
                        Hover::Dock(4) => command.push(match current.accent {
                            0 => b"(accent cyan)",
                            1 => b"(accent amber)",
                            _ => b"(accent violet)",
                        }),
                        Hover::Dock(6) => command.push(b":help"),
                        Hover::WindowClose(slot) => {
                            command.push(b":close ");
                            command.number(slot);
                        }
                        Hover::WindowMaximize(slot) => {
                            command.push(b":maximize ");
                            command.number(slot);
                        }
                        Hover::WindowMinimize(slot) => {
                            command.push(b":minimize ");
                            command.number(slot);
                        }
                        Hover::Pill(slot) => {
                            command.push(b":restore ");
                            command.number(slot);
                        }
                        Hover::WindowCorner(slot) => {
                            raise(&mut current.order, slot);
                            current.focus = Some(slot);
                            current.resize = Some(slot);
                            continue;
                        }
                        Hover::Window(slot) | Hover::WindowHeader(slot) => {
                            // The window's: it comes to the front and takes
                            // the keyboard; a press in its content is an
                            // event for its owner, and the pointer is the
                            // window's until the release; a press in its
                            // header takes hold of it.
                            let was_front = current.order[current.order.len() - 1] == slot;
                            raise(&mut current.order, slot);
                            current.focus = Some(slot);
                            if let Some(window) = current.windows[usize::from(slot)].as_mut() {
                                if over == Hover::WindowHeader(slot) {
                                    current.drag = Some(Drag {
                                        slot,
                                        dx: px as i32 - window.x as i32,
                                        dy: py as i32 - window.y as i32,
                                    });
                                } else if window.listens() && px >= window.x && py >= window.y {
                                    let (x, y) = (px - window.x, py - window.y);
                                    window.queue(
                                        crate::world::process::EVENT_PRESS
                                            | (u64::from(x) << 32)
                                            | (u64::from(y) << 16),
                                    );
                                    current.grab = Some(slot);
                                }
                                if !was_front {
                                    let (x, y, width, height) = window.outer();
                                    let frame =
                                        materialize(current, Some(&line[..length]), status.get())
                                            .unwrap_or_else(|reason| failed(reason));
                                    render_region(
                                        compositor,
                                        Some(&mut inputs),
                                        &frame,
                                        (
                                            x.saturating_sub(WINDOW_SHADOW),
                                            y.saturating_sub(WINDOW_SHADOW),
                                            width + 2 * WINDOW_SHADOW,
                                            height + 2 * WINDOW_SHADOW,
                                        ),
                                    )
                                    .unwrap_or_else(|reason| failed(reason));
                                }
                            }
                            continue;
                        }
                        Hover::Dock(_) | Hover::Nothing => {}
                    }
                    if close_launcher {
                        current.launcher = None;
                        current.hover = Hover::Nothing;
                    }
                    if command.len > 0 {
                        // Echoed as if typed, so the console shows the
                        // command a click became.
                        console::write_bytes(command.get());
                        line[..command.len].copy_from_slice(command.get());
                        length = command.len;
                        b'\n'
                    } else {
                        let frame = materialize(current, Some(&line[..length]), status.get())
                            .unwrap_or_else(|reason| failed(reason));
                        render(compositor, Some(&mut inputs), &frame)
                            .unwrap_or_else(|reason| failed(reason));
                        continue;
                    }
                } else if pressed
                    && (pointer.y as u32) < crate::world::SCENE_DRAWABLE_HEIGHT
                    && current.inspector.is_none()
                    && length == 0
                {
                    // The workshop has the keyboard again.
                    current.focus = None;
                    let mut command = StatusLine::new(b"(point ");
                    command.number_u64(pointer.x as u64);
                    command.push(b" ");
                    command.number_u64(pointer.y as u64);
                    command.push(b")");
                    line[..command.len].copy_from_slice(command.get());
                    length = command.len;
                    b'\n'
                } else {
                    let frame = materialize(current, Some(&line[..length]), status.get())
                        .unwrap_or_else(|reason| failed(reason));
                    // Only where the pointer was and where it is now, and
                    // the surfaces whose hover changed.
                    let (nx, ny) = (pointer.x as u32, pointer.y as u32);
                    let (ox, oy) = before.unwrap_or((nx, ny));
                    let mut left = nx.min(ox).saturating_sub(2);
                    let mut top = ny.min(oy).saturating_sub(2);
                    let mut right = nx.max(ox) + 26;
                    let mut bottom = ny.max(oy) + 34;
                    if hovered_before != over || pressed_before != current.pressed {
                        for hover in [hovered_before, over, pressed_before] {
                            if let Some((x, y, width, height)) = hover.bounds(&current) {
                                left = left.min(x);
                                top = top.min(y);
                                right = right.max(x + width);
                                bottom = bottom.max(y + height);
                            }
                        }
                    }
                    render_region(
                        compositor,
                        Some(&mut inputs),
                        &frame,
                        (left, top, right - left, bottom - top),
                    )
                    .unwrap_or_else(|reason| failed(reason));
                    continue;
                }
            }
        };
        let mut prompt_pending = false;
        match byte {
            b'\t' => {
                status = evaluator_status(
                    &mut evaluator,
                    &mut evaluator_revision,
                    shared::COMMAND_EVALUATE,
                    b"(focus-next)",
                );
                current.previewing = false;
                if let Err(reason) = synchronize_language_scene(
                    &mut evaluator,
                    compositor,
                    Some(&mut inputs),
                    &mut current,
                ) {
                    status = StatusLine::new(reason.as_bytes());
                }
            }
            b'\r' | b'\n' => {
                if length == 0 && workspace.find(b"wb-0").is_some() {
                    line[..10].copy_from_slice(b"(activate)");
                    length = 10;
                }
                console::write("\n");
                if core::str::from_utf8(&line[..length]).is_err() {
                    length = 0;
                    console::write("INVALID UTF-8\nlive-desktop> ");
                    continue;
                }
                status = execute_workshop(
                    machine,
                    compositor,
                    Some(&mut inputs),
                    &mut evaluator,
                    &mut storage,
                    filesystem.as_mut(),
                    &mut console_driver,
                    &mut recovery,
                    &mut kernel,
                    &mut current,
                    &mut previous,
                    &mut scene_revision,
                    &mut evaluator_revision,
                    &mut workspace,
                    &mut committed_workspace,
                    &mut generation,
                    &mut dirty,
                    &mut running,
                    &line[..length],
                );
                current.terminal.dirty = false;
                current.previewing = trim(&line[..length]).starts_with(b":preview ")
                    && status.get().starts_with(b"CANDIDATE VALIDATED");
                if trim(&line[..length]).starts_with(b":source ") {
                    current.inspector = Some(status);
                } else {
                    // Source is a snapshot; never label it as current after an edit.
                    current.inspector = None;
                }
                if let Err(reason) = synchronize_language_scene(
                    &mut evaluator,
                    compositor,
                    Some(&mut inputs),
                    &mut current,
                ) {
                    status = StatusLine::new(reason.as_bytes());
                }
                prompt_pending = true;
                length = 0;
            }
            0x08 | 0x7f if length > 0 => {
                length -= 1;
                while length > 0 && line[length] & 0xc0 == 0x80 {
                    length -= 1;
                }
                console::write("\x08 \x08");
            }
            0x1b => {
                length = 0;
                current.inspector = None;
                status = StatusLine::new(b"INPUT CLEARED");
            }
            byte if byte.is_ascii_graphic() || byte == b' ' || byte >= 0x80 => {
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
        if prompt_pending || byte == 0x1b || byte == b'\t' {
            render(compositor, Some(&mut inputs), &frame).unwrap_or_else(|reason| failed(reason));
        } else {
            render_overlay(compositor, Some(&mut inputs), &frame)
                .unwrap_or_else(|reason| failed(reason));
        }
        if prompt_pending {
            console::write_bytes(status.get());
            console::write("\nlive-desktop> ");
        }
    }
}

/// Draw the Agel-authored desktop in a contained compositor domain.
pub fn run() -> ! {
    let framebuffer =
        Framebuffer::acquire().unwrap_or_else(|| failed("no framebuffer on this machine"));
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
    // The storage driver comes first: the compositor's fonts are on the disk.
    let storage_entry = crate::user::agel_storage_main as *const () as usize as u64;
    let mut storage = machine
        .create_storage_world(storage_entry, crate::world::STORAGE_TICKS)
        .map(|domain| {
            ServiceDomain::new(
                domain,
                ServiceKind::Storage,
                storage_entry,
                crate::world::STORAGE_TICKS,
            )
        })
        .unwrap_or_else(|reason| failed(reason));
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
    load_assets(&mut machine, &mut storage, &mut compositor);
    let initial = Scene::initial();
    let frame = materialize(initial, None, b"").unwrap_or_else(|reason| failed(reason));
    // The frame is the compiled scene plus what the supervisor adds for the
    // terminal panel; the compiled records must all be there.
    if frame.count < count {
        failed("compiled vector command count disagrees");
    }
    render(&mut compositor, None, &frame).unwrap_or_else(|reason| failed(reason));
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
        None,
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
        None,
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
    load_assets(&mut machine, &mut storage, &mut replacement);
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

    // Keep the production entry type-checked in the selftest build as well.
    if cfg!(feature = "graphics-selftest") {
        arch::exit(true);
    }
    interactive(&mut machine, &mut replacement, storage, initial, initial)
}
