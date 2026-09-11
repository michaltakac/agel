//! `agel-libc`: the C library of the POSIX personality, in Rust with a C
//! ABI. C programs build against the headers in `include/` and link this
//! archive; every function here translates a POSIX call into the process
//! protocol in `docs/posix-personality.md`, through `agel-process-abi`.
//!
//! The `unsafe` in this file is the C boundary: reading a C string, filling
//! a caller's buffer, the process entry. Nothing here has ambient
//! authority; a path resolves through the namespace the process was given.
#![no_std]

use core::ffi::{c_char, c_int, c_long, c_uint, c_ulong, c_void};
use core::sync::atomic::{AtomicUsize, Ordering};

use agel_process_abi::Process;

/// The shared page the supervisor entered this process with.
static PAGE: AtomicUsize = AtomicUsize::new(0);

/// The C `errno`: `<errno.h>` defines the name as `*__errno_location()`.
static mut ERRNO: c_int = 0;

fn process() -> Process {
    // Safety: `_start` stored the page the supervisor gave this process.
    unsafe { Process::new(PAGE.load(Ordering::Relaxed) as u64) }
}

/// Fold a negated error number from the protocol into `errno` and -1.
fn outcome(result: i64) -> isize {
    if result < 0 {
        // Safety: a process is single-threaded; errno is its own.
        unsafe { ERRNO = (-result) as c_int };
        -1
    } else {
        result as isize
    }
}

extern "C" {
    fn main(argc: c_int, argv: *mut *mut c_char) -> c_int;
    /// In `c/stdio.c`: write out every stream's buffer.
    fn __agel_flush_all();
}

/// The arguments, copied out of the shared page once, and the `argv`
/// pointers into that copy. `main` may keep them for its whole life.
const MAX_ARGUMENTS: usize = 32;
static mut ARGUMENT_BYTES: [u8; agel_process_abi::PAYLOAD_BYTES] =
    [0; agel_process_abi::PAYLOAD_BYTES];
static mut ARGV: [*mut c_char; MAX_ARGUMENTS + 1] = [core::ptr::null_mut(); MAX_ARGUMENTS + 1];

/// The process entry: the supervisor arrives here with the shared page in
/// the first argument register. Builds `argv` from the argument block the
/// supervisor left in the payload area, runs `main` and exits with its
/// answer, flushing the streams first.
#[no_mangle]
#[link_section = ".text.entry"]
pub extern "C" fn _start(shared_page: u64) -> ! {
    PAGE.store(shared_page as usize, Ordering::Relaxed);
    let (count, block) = process().arguments();
    let argc = count.min(MAX_ARGUMENTS);
    // Safety: the process is single-threaded and nothing has run yet.
    unsafe {
        ARGUMENT_BYTES = block;
        let bytes = core::ptr::addr_of_mut!(ARGUMENT_BYTES).cast::<u8>();
        // The last byte of the block is always a terminator for the last
        // argument, whatever the supervisor wrote.
        *bytes.add(agel_process_abi::PAYLOAD_BYTES - 1) = 0;
        let argv = core::ptr::addr_of_mut!(ARGV).cast::<*mut c_char>();
        let mut at = 0;
        for slot in 0..argc {
            argv.add(slot).write(bytes.add(at) as *mut c_char);
            while at < agel_process_abi::PAYLOAD_BYTES - 1 && *bytes.add(at) != 0 {
                at += 1;
            }
            at = (at + 1).min(agel_process_abi::PAYLOAD_BYTES - 1);
        }
    }
    // Safety: `main` is the program's, with either C prototype; the extra
    // arguments are ignored by `int main(void)` on every machine's ABI.
    let status = unsafe {
        main(
            argc as c_int,
            core::ptr::addr_of_mut!(ARGV).cast::<*mut c_char>(),
        )
    };
    exit(status)
}

#[no_mangle]
pub extern "C" fn __errno_location() -> *mut c_int {
    core::ptr::addr_of_mut!(ERRNO)
}

// ---------------------------------------------------------------------------
// unistd.h
// ---------------------------------------------------------------------------

/// # Safety
/// `buffer` must point to `count` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn write(descriptor: c_int, buffer: *const c_void, count: usize) -> isize {
    if descriptor < 0 {
        return outcome(-9);
    }
    let bytes = unsafe { core::slice::from_raw_parts(buffer as *const u8, count) };
    let written = process().write(descriptor as u64, bytes);
    outcome(written as i64)
}

/// # Safety
/// `buffer` must point to `count` writable bytes.
#[no_mangle]
pub unsafe extern "C" fn read(descriptor: c_int, buffer: *mut c_void, count: usize) -> isize {
    if descriptor < 0 {
        return outcome(-9);
    }
    let bytes = unsafe { core::slice::from_raw_parts_mut(buffer as *mut u8, count) };
    outcome(process().read(descriptor as u64, bytes))
}

#[no_mangle]
pub extern "C" fn close(descriptor: c_int) -> c_int {
    if descriptor < 0 {
        return outcome(-9) as c_int;
    }
    outcome(process().close(descriptor as u64)) as c_int
}

#[no_mangle]
pub extern "C" fn _exit(status: c_int) -> ! {
    process().exit(status as u64)
}

/// # Safety
/// `descriptors` must point to two writable `int`s.
#[no_mangle]
pub unsafe extern "C" fn pipe(descriptors: *mut c_int) -> c_int {
    match process().pipe() {
        Ok((read_end, write_end)) => {
            unsafe {
                descriptors.write(read_end as c_int);
                descriptors.add(1).write(write_end as c_int);
            }
            0
        }
        Err(error) => outcome(error) as c_int,
    }
}

// ---------------------------------------------------------------------------
// sys/wait.h and spawn.h
// ---------------------------------------------------------------------------

/// `waitpid` for the one shape Agel has: a named child, no options. The
/// status is encoded as POSIX macros read it: an exit status in bits 8-15,
/// or a signal in the low seven bits when the machine stopped the child.
///
/// # Safety
/// `status` is null or points to a writable `int`.
#[no_mangle]
pub unsafe extern "C" fn waitpid(child: c_int, status: *mut c_int, _options: c_int) -> c_int {
    if child < 0 {
        return outcome(-10) as c_int; // ECHILD
    }
    let answer = process().wait(child as u64);
    if answer < 0 {
        return outcome(answer) as c_int;
    }
    let answer = answer as u64;
    let encoded = if answer & agel_process_abi::WAIT_SIGNALED != 0 {
        (answer & 0x7f) as c_int
    } else {
        ((answer & 0xff) << 8) as c_int
    };
    if !status.is_null() {
        unsafe { status.write(encoded) };
    }
    child
}

/// Agel's own: start `program` as a child that gets exactly `stdin_fd` and
/// `stdout_fd` (`-1` for none) as its descriptors 0 and 1, the console as
/// 2, and this process's namespace, read-only with `AGEL_SPAWN_READ_ONLY`.
/// There is no `fork`: a child never inherits what it was not given.
///
/// # Safety
/// `program` must be NUL-terminated; `argv` is null or a NULL-terminated
/// array of NUL-terminated strings.
#[no_mangle]
pub unsafe extern "C" fn agel_spawn(
    program: *const c_char,
    argv: *const *const c_char,
    stdin_fd: c_int,
    stdout_fd: c_int,
    flags: c_int,
) -> c_int {
    let length = unsafe { strlen(program) };
    let name = unsafe { core::slice::from_raw_parts(program as *const u8, length) };
    let mut block = [0_u8; agel_process_abi::PAYLOAD_BYTES];
    let mut used = 0;
    if !argv.is_null() {
        let mut index = 0;
        loop {
            let argument = unsafe { *argv.add(index) };
            if argument.is_null() {
                break;
            }
            let bytes =
                unsafe { core::slice::from_raw_parts(argument as *const u8, strlen(argument)) };
            for byte in bytes.iter().chain(&[0]) {
                if used < block.len() {
                    block[used] = *byte;
                    used += 1;
                }
            }
            index += 1;
        }
    }
    let descriptor = |number: c_int| {
        if number < 0 {
            agel_process_abi::NO_DESCRIPTOR
        } else {
            number as u64
        }
    };
    outcome(process().spawn(
        name,
        &block[..used],
        descriptor(stdin_fd),
        descriptor(stdout_fd),
        flags as u64,
    )) as c_int
}

// ---------------------------------------------------------------------------
// agel/window.h
// ---------------------------------------------------------------------------

/// A window on the desktop: its number, or -1 with `errno` (`ENODEV`
/// where there is no display).
///
/// # Safety
/// `title` must be NUL-terminated.
#[no_mangle]
pub unsafe extern "C" fn agel_window(width: c_uint, height: c_uint, title: *const c_char) -> c_int {
    let length = unsafe { strlen(title) };
    let bytes = unsafe { core::slice::from_raw_parts(title as *const u8, length) };
    outcome(process().window(width, height, bytes)) as c_int
}

/// Draw `count` records into `window`, eight per request, clearing it
/// first with `AGEL_DRAW_CLEAR`: the records the window holds, or -1 with
/// `errno` and the request's records undrawn (`EINVAL` for one the window
/// does not permit).
///
/// # Safety
/// `records` must point to `count` records of 64 bytes.
#[no_mangle]
pub unsafe extern "C" fn agel_draw(
    window: c_int,
    records: *const c_void,
    count: c_uint,
    flags: c_uint,
) -> c_int {
    if window < 0 {
        return outcome(-9) as c_int;
    }
    let bytes = unsafe {
        core::slice::from_raw_parts(
            records as *const u8,
            count as usize * agel_process_abi::RECORD_BYTES,
        )
    };
    let mut flags = u64::from(flags);
    let mut held = 0;
    let mut sent = 0;
    while sent < bytes.len() || (sent == 0 && flags != 0) {
        let take = (bytes.len() - sent)
            .min(agel_process_abi::DRAW_RECORDS * agel_process_abi::RECORD_BYTES);
        let result = process().draw(window as u64, &bytes[sent..sent + take], flags);
        if result < 0 {
            return outcome(result) as c_int;
        }
        held = result;
        flags &= !agel_process_abi::DRAW_CLEAR;
        sent += take;
        if take == 0 {
            break;
        }
    }
    held as c_int
}

/// The next event for `window`, waiting for one when `wait` is nonzero:
/// 1 with the event in `*event`, 0 when there is none, or -1 with `errno`.
///
/// # Safety
/// `event` must be a valid pointer.
#[no_mangle]
pub unsafe extern "C" fn agel_event(
    window: c_int,
    event: *mut agel_window_event,
    wait: c_int,
) -> c_int {
    if window < 0 {
        return outcome(-9) as c_int;
    }
    let packed = process().event(window as u64, wait != 0);
    if packed < 0 {
        return outcome(packed) as c_int;
    }
    if packed == 0 {
        return 0;
    }
    let packed = packed as u64;
    unsafe {
        (*event).kind = (packed >> 56) as c_int;
        (*event).x = ((packed >> 32) & 0xffff) as c_int;
        (*event).y = ((packed >> 16) & 0xffff) as c_int;
        (*event).key = (packed & 0xff) as c_int;
    }
    1
}

/// `<agel/window.h>`'s `agel_window_event`, as C lays it out.
#[repr(C)]
pub struct agel_window_event {
    pub kind: c_int,
    pub x: c_int,
    pub y: c_int,
    pub key: c_int,
}

// ---------------------------------------------------------------------------
// fcntl.h
// ---------------------------------------------------------------------------

/// The C prototype is `int open(const char *, int, ...)`; the mode a third
/// argument would carry has no meaning here yet, and a variadic definition
/// is not stable Rust, so this takes the two arguments every call passes.
/// On every machine's calling convention the fixed arguments arrive the same
/// way whether or not the prototype is variadic.
///
/// # Safety
/// `path` must be a NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn open(path: *const c_char, flags: c_int) -> c_int {
    let length = unsafe { strlen(path) };
    let bytes = unsafe { core::slice::from_raw_parts(path as *const u8, length) };
    outcome(process().open(bytes, flags as u64)) as c_int
}

// ---------------------------------------------------------------------------
// stdlib.h
// ---------------------------------------------------------------------------

#[no_mangle]
pub extern "C" fn exit(status: c_int) -> ! {
    // Safety: the stream table is the library's own.
    unsafe { __agel_flush_all() };
    _exit(status)
}

#[no_mangle]
pub extern "C" fn lseek(descriptor: c_int, offset: isize, whence: c_int) -> isize {
    if descriptor < 0 {
        return outcome(-9);
    }
    outcome(process().seek(descriptor as u64, offset as i64, whence as u64))
}

#[no_mangle]
pub extern "C" fn abort() -> ! {
    _exit(134)
}

/// The heap: a fixed arena in the process's own `.bss`, managed as a list
/// of blocks with a header each. `malloc` takes the first free block that
/// fits, splitting what is left; `free` marks a block free and joins it
/// with a free neighbour on either side, so memory is reused and does not
/// fragment into unusable slivers for the patterns small programs have.
/// Nothing here is thread-safe, because nothing here is threaded.
const ARENA_BYTES: usize = 256 * 1024;
const HEADER_BYTES: usize = 16;
const ALIGN: usize = 16;
static mut ARENA: [u8; ARENA_BYTES] = [0; ARENA_BYTES];
static HEAP_READY: AtomicUsize = AtomicUsize::new(0);

/// A block header: the payload size in bytes, and whether it is free.
#[repr(C)]
struct Header {
    size: usize,
    free: usize,
}

fn arena() -> *mut u8 {
    // The arena is the library's static; every pointer derived stays
    // inside it, checked against ARENA_BYTES.
    core::ptr::addr_of_mut!(ARENA).cast::<u8>()
}

fn header_at(offset: usize) -> *mut Header {
    // Safety: `offset` is a block boundary inside the arena.
    unsafe { arena().add(offset).cast::<Header>() }
}

fn heap_init() {
    if HEAP_READY.swap(1, Ordering::Relaxed) == 0 {
        // Safety: one block spanning the arena, free.
        unsafe {
            header_at(0).write(Header {
                size: ARENA_BYTES - HEADER_BYTES,
                free: 1,
            })
        };
    }
}

#[no_mangle]
pub extern "C" fn malloc(size: usize) -> *mut c_void {
    heap_init();
    let wanted = (size.max(1) + ALIGN - 1) & !(ALIGN - 1);
    let mut offset = 0;
    while offset + HEADER_BYTES <= ARENA_BYTES {
        // Safety: `offset` walks block boundaries from the first header.
        let header = unsafe { &mut *header_at(offset) };
        if header.free == 1 && header.size >= wanted {
            let rest = header.size - wanted;
            if rest >= HEADER_BYTES + ALIGN {
                header.size = wanted;
                // Safety: the split block lies inside the block just cut.
                unsafe {
                    header_at(offset + HEADER_BYTES + wanted).write(Header {
                        size: rest - HEADER_BYTES,
                        free: 1,
                    })
                };
            }
            header.free = 0;
            // Safety: the payload follows the header inside the arena.
            return unsafe { arena().add(offset + HEADER_BYTES) } as *mut c_void;
        }
        offset += HEADER_BYTES + header.size;
    }
    // Safety: as in `outcome`.
    unsafe { ERRNO = 12 };
    core::ptr::null_mut()
}

/// The header offset of a block `malloc` handed out, if `pointer` is one.
fn block_of(pointer: *mut c_void) -> Option<usize> {
    let address = pointer as usize;
    let base = arena() as usize;
    if address < base + HEADER_BYTES || address >= base + ARENA_BYTES {
        return None;
    }
    let offset = address - base - HEADER_BYTES;
    offset.is_multiple_of(ALIGN).then_some(offset)
}

#[no_mangle]
pub extern "C" fn free(pointer: *mut c_void) {
    let Some(offset) = block_of(pointer) else {
        return;
    };
    heap_init();
    // Walk from the start so the previous block is known; the walk also
    // confirms `offset` is a boundary before anything is written.
    let mut previous: Option<usize> = None;
    let mut at = 0;
    while at + HEADER_BYTES <= ARENA_BYTES {
        // Safety: `at` walks block boundaries.
        let header = unsafe { &mut *header_at(at) };
        if at == offset {
            if header.free == 1 {
                return;
            }
            header.free = 1;
            let next = at + HEADER_BYTES + header.size;
            if next + HEADER_BYTES <= ARENA_BYTES {
                // Safety: `next` is the following block's boundary.
                let following = unsafe { &*header_at(next) };
                if following.free == 1 {
                    header.size += HEADER_BYTES + following.size;
                }
            }
            if let Some(before) = previous {
                // Safety: `before` is the preceding block's boundary.
                let preceding = unsafe { &mut *header_at(before) };
                if preceding.free == 1 {
                    preceding.size += HEADER_BYTES + header.size;
                }
            }
            return;
        }
        previous = Some(at);
        at += HEADER_BYTES + header.size;
    }
}

#[no_mangle]
pub extern "C" fn calloc(count: usize, size: usize) -> *mut c_void {
    let Some(total) = count.checked_mul(size) else {
        return core::ptr::null_mut();
    };
    let block = malloc(total);
    if !block.is_null() {
        // Safety: the block holds at least `total` bytes.
        unsafe { memset(block, 0, total) };
    }
    block
}

/// # Safety
/// `pointer` is null or came from `malloc`.
#[no_mangle]
pub unsafe extern "C" fn realloc(pointer: *mut c_void, size: usize) -> *mut c_void {
    let Some(offset) = block_of(pointer) else {
        return malloc(size);
    };
    // Safety: `offset` is the block's boundary.
    let current = unsafe { (*header_at(offset)).size };
    if current >= size {
        return pointer;
    }
    let replacement = malloc(size);
    if !replacement.is_null() {
        unsafe { memcpy(replacement, pointer, current) };
        free(pointer);
    }
    replacement
}

// ---------------------------------------------------------------------------
// string.h
// ---------------------------------------------------------------------------
// Written with volatile byte loops so the compiler cannot recognise them
// as the very calls they implement.

/// # Safety
/// `destination` and `source` must each cover `count` bytes; they may not overlap.
#[no_mangle]
pub unsafe extern "C" fn memcpy(
    destination: *mut c_void,
    source: *const c_void,
    count: usize,
) -> *mut c_void {
    let to = destination as *mut u8;
    let from = source as *const u8;
    for offset in 0..count {
        unsafe {
            to.add(offset)
                .write_volatile(from.add(offset).read_volatile())
        };
    }
    destination
}

/// # Safety
/// `destination` and `source` must each cover `count` bytes.
#[no_mangle]
pub unsafe extern "C" fn memmove(
    destination: *mut c_void,
    source: *const c_void,
    count: usize,
) -> *mut c_void {
    let to = destination as *mut u8;
    let from = source as *const u8;
    if (to as usize) <= (from as usize) {
        for offset in 0..count {
            unsafe {
                to.add(offset)
                    .write_volatile(from.add(offset).read_volatile())
            };
        }
    } else {
        for offset in (0..count).rev() {
            unsafe {
                to.add(offset)
                    .write_volatile(from.add(offset).read_volatile())
            };
        }
    }
    destination
}

/// # Safety
/// `destination` must cover `count` bytes.
#[no_mangle]
pub unsafe extern "C" fn memset(
    destination: *mut c_void,
    value: c_int,
    count: usize,
) -> *mut c_void {
    let to = destination as *mut u8;
    for offset in 0..count {
        unsafe { to.add(offset).write_volatile(value as u8) };
    }
    destination
}

/// # Safety
/// Both pointers must cover `count` bytes.
#[no_mangle]
pub unsafe extern "C" fn memcmp(left: *const c_void, right: *const c_void, count: usize) -> c_int {
    let a = left as *const u8;
    let b = right as *const u8;
    for offset in 0..count {
        let (x, y) = unsafe { (a.add(offset).read_volatile(), b.add(offset).read_volatile()) };
        if x != y {
            return c_int::from(x) - c_int::from(y);
        }
    }
    0
}

/// # Safety
/// `text` must be NUL-terminated.
#[no_mangle]
pub unsafe extern "C" fn strlen(text: *const c_char) -> usize {
    let mut length = 0;
    while unsafe { text.add(length).read_volatile() } != 0 {
        length += 1;
    }
    length
}

/// # Safety
/// Both strings must be NUL-terminated.
#[no_mangle]
pub unsafe extern "C" fn strcmp(left: *const c_char, right: *const c_char) -> c_int {
    let mut offset = 0;
    loop {
        let (x, y) = unsafe {
            (
                left.add(offset).read_volatile() as u8,
                right.add(offset).read_volatile() as u8,
            )
        };
        if x != y || x == 0 {
            return c_int::from(x) - c_int::from(y);
        }
        offset += 1;
    }
}

/// # Safety
/// Both strings must be NUL-terminated or cover `count` bytes.
#[no_mangle]
pub unsafe extern "C" fn strncmp(left: *const c_char, right: *const c_char, count: usize) -> c_int {
    for offset in 0..count {
        let (x, y) = unsafe {
            (
                left.add(offset).read_volatile() as u8,
                right.add(offset).read_volatile() as u8,
            )
        };
        if x != y || x == 0 {
            return c_int::from(x) - c_int::from(y);
        }
    }
    0
}

/// # Safety
/// `source` must be NUL-terminated and `destination` must have room for it.
#[no_mangle]
pub unsafe extern "C" fn strcpy(destination: *mut c_char, source: *const c_char) -> *mut c_char {
    let mut offset = 0;
    loop {
        let byte = unsafe { source.add(offset).read_volatile() };
        unsafe { destination.add(offset).write_volatile(byte) };
        if byte == 0 {
            return destination;
        }
        offset += 1;
    }
}

/// # Safety
/// `text` must be NUL-terminated.
#[no_mangle]
pub unsafe extern "C" fn strchr(text: *const c_char, wanted: c_int) -> *mut c_char {
    let wanted = wanted as c_char;
    let mut offset = 0;
    loop {
        let byte = unsafe { text.add(offset).read_volatile() };
        if byte == wanted {
            return unsafe { text.add(offset) } as *mut c_char;
        }
        if byte == 0 {
            return core::ptr::null_mut();
        }
        offset += 1;
    }
}

// stdio.h is in `c/stdio.c`: streams, and a formatter that is C because a
// C-variadic definition is not stable Rust.

// ---------------------------------------------------------------------------
// string.h, the rest
// ---------------------------------------------------------------------------

/// # Safety
/// `source` must be NUL-terminated or cover `count` bytes; `destination` must hold `count`.
#[no_mangle]
pub unsafe extern "C" fn strncpy(
    destination: *mut c_char,
    source: *const c_char,
    count: usize,
) -> *mut c_char {
    let mut offset = 0;
    while offset < count {
        let byte = unsafe { source.add(offset).read_volatile() };
        unsafe { destination.add(offset).write_volatile(byte) };
        offset += 1;
        if byte == 0 {
            break;
        }
    }
    while offset < count {
        unsafe { destination.add(offset).write_volatile(0) };
        offset += 1;
    }
    destination
}

/// # Safety
/// Both must be NUL-terminated and `destination` must have room.
#[no_mangle]
pub unsafe extern "C" fn strcat(destination: *mut c_char, source: *const c_char) -> *mut c_char {
    let end = unsafe { strlen(destination) };
    unsafe { strcpy(destination.add(end), source) };
    destination
}

/// # Safety
/// As `strcat`, copying at most `count` bytes of `source`.
#[no_mangle]
pub unsafe extern "C" fn strncat(
    destination: *mut c_char,
    source: *const c_char,
    count: usize,
) -> *mut c_char {
    let mut end = unsafe { strlen(destination) };
    for offset in 0..count {
        let byte = unsafe { source.add(offset).read_volatile() };
        if byte == 0 {
            break;
        }
        unsafe { destination.add(end).write_volatile(byte) };
        end += 1;
    }
    unsafe { destination.add(end).write_volatile(0) };
    destination
}

/// # Safety
/// `text` must be NUL-terminated.
#[no_mangle]
pub unsafe extern "C" fn strrchr(text: *const c_char, wanted: c_int) -> *mut c_char {
    let wanted = wanted as c_char;
    let mut found = core::ptr::null_mut();
    let mut offset = 0;
    loop {
        let byte = unsafe { text.add(offset).read_volatile() };
        if byte == wanted {
            found = unsafe { text.add(offset) } as *mut c_char;
        }
        if byte == 0 {
            return found;
        }
        offset += 1;
    }
}

/// # Safety
/// Both must be NUL-terminated.
#[no_mangle]
pub unsafe extern "C" fn strstr(haystack: *const c_char, needle: *const c_char) -> *mut c_char {
    let needle_length = unsafe { strlen(needle) };
    let haystack_length = unsafe { strlen(haystack) };
    if needle_length == 0 {
        return haystack as *mut c_char;
    }
    let mut start = 0;
    while start + needle_length <= haystack_length {
        if unsafe { strncmp(haystack.add(start), needle, needle_length) } == 0 {
            return unsafe { haystack.add(start) } as *mut c_char;
        }
        start += 1;
    }
    core::ptr::null_mut()
}

/// # Safety
/// `block` must cover `count` bytes.
#[no_mangle]
pub unsafe extern "C" fn memchr(block: *const c_void, wanted: c_int, count: usize) -> *mut c_void {
    let bytes = block as *const u8;
    for offset in 0..count {
        if unsafe { bytes.add(offset).read_volatile() } == wanted as u8 {
            return unsafe { bytes.add(offset) } as *mut c_void;
        }
    }
    core::ptr::null_mut()
}

/// # Safety
/// `text` must be NUL-terminated.
#[no_mangle]
pub unsafe extern "C" fn strdup(text: *const c_char) -> *mut c_char {
    let length = unsafe { strlen(text) };
    let copy = malloc(length + 1) as *mut c_char;
    if !copy.is_null() {
        unsafe { memcpy(copy as *mut c_void, text as *const c_void, length + 1) };
    }
    copy
}

/// The error numbers the protocol answers, as text.
#[no_mangle]
pub extern "C" fn strerror(number: c_int) -> *mut c_char {
    let text: &'static [u8] = match number {
        0 => b"Success\0",
        2 => b"No such file or directory\0",
        5 => b"Input/output error\0",
        9 => b"Bad file descriptor\0",
        10 => b"No child processes\0",
        11 => b"Resource temporarily unavailable\0",
        12 => b"Cannot allocate memory\0",
        13 => b"Permission denied\0",
        20 => b"Not a directory\0",
        21 => b"Is a directory\0",
        22 => b"Invalid argument\0",
        23 => b"Too many open files in system\0",
        24 => b"Too many open files\0",
        27 => b"File too large\0",
        28 => b"No space left on device\0",
        29 => b"Illegal seek\0",
        32 => b"Broken pipe\0",
        38 => b"Function not implemented\0",
        116 => b"Stale file handle\0",
        _ => b"Unknown error\0",
    };
    text.as_ptr() as *mut c_char
}

// ---------------------------------------------------------------------------
// stdlib.h: numbers and sorting
// ---------------------------------------------------------------------------

/// # Safety
/// `text` must be NUL-terminated; `end` is null or points to a writable pointer.
#[no_mangle]
pub unsafe extern "C" fn strtol(text: *const c_char, end: *mut *mut c_char, base: c_int) -> c_long {
    let (value, consumed) = unsafe { parse_integer(text, base) };
    if !end.is_null() {
        unsafe { end.write(text.add(consumed) as *mut c_char) };
    }
    value
}

/// # Safety
/// As `strtol`.
#[no_mangle]
pub unsafe extern "C" fn strtoul(
    text: *const c_char,
    end: *mut *mut c_char,
    base: c_int,
) -> c_ulong {
    unsafe { strtol(text, end, base) as c_ulong }
}

/// # Safety
/// `text` must be NUL-terminated.
#[no_mangle]
pub unsafe extern "C" fn atoi(text: *const c_char) -> c_int {
    unsafe { parse_integer(text, 10).0 as c_int }
}

/// # Safety
/// `text` must be NUL-terminated.
#[no_mangle]
pub unsafe extern "C" fn atol(text: *const c_char) -> c_long {
    unsafe { parse_integer(text, 10).0 }
}

/// Leading space, a sign, an optional `0x` for base 16 or 0, digits.
/// Returns the value and how many bytes were consumed; overflow saturates.
unsafe fn parse_integer(text: *const c_char, base: c_int) -> (c_long, usize) {
    let byte = |at: usize| unsafe { text.add(at).read_volatile() as u8 };
    let mut at = 0;
    while isspace(c_int::from(byte(at))) != 0 {
        at += 1;
    }
    let negative = match byte(at) {
        b'-' => {
            at += 1;
            true
        }
        b'+' => {
            at += 1;
            false
        }
        _ => false,
    };
    let mut base = base as u32;
    if (base == 0 || base == 16) && byte(at) == b'0' && (byte(at + 1) | 0x20) == b'x' {
        base = 16;
        at += 2;
    } else if base == 0 {
        base = if byte(at) == b'0' { 8 } else { 10 };
    }
    let mut value: c_long = 0;
    let start = at;
    loop {
        let digit = match byte(at) {
            digit @ b'0'..=b'9' => u32::from(digit - b'0'),
            letter @ b'a'..=b'z' => u32::from(letter - b'a') + 10,
            letter @ b'A'..=b'Z' => u32::from(letter - b'A') + 10,
            _ => break,
        };
        if digit >= base {
            break;
        }
        value = value
            .saturating_mul(c_long::from(base))
            .saturating_add(c_long::from(digit));
        at += 1;
    }
    if at == start {
        return (0, 0);
    }
    (if negative { -value } else { value }, at)
}

#[no_mangle]
pub extern "C" fn abs(value: c_int) -> c_int {
    value.wrapping_abs()
}

#[no_mangle]
pub extern "C" fn labs(value: c_long) -> c_long {
    value.wrapping_abs()
}

/// Insertion sort: quadratic, stable, and correct, for the sizes a process
/// here sorts.
///
/// # Safety
/// `base` must cover `count` elements of `size` bytes; `compare` must be a
/// C function that orders two of them.
#[no_mangle]
pub unsafe extern "C" fn qsort(
    base: *mut c_void,
    count: usize,
    size: usize,
    compare: Option<unsafe extern "C" fn(*const c_void, *const c_void) -> c_int>,
) {
    let Some(compare) = compare else {
        return;
    };
    let bytes = base as *mut u8;
    for index in 1..count {
        let mut at = index;
        while at > 0 {
            let current = unsafe { bytes.add(at * size) };
            let before = unsafe { bytes.add((at - 1) * size) };
            if unsafe { compare(before as *const c_void, current as *const c_void) } <= 0 {
                break;
            }
            for offset in 0..size {
                unsafe {
                    let a = before.add(offset).read_volatile();
                    let b = current.add(offset).read_volatile();
                    before.add(offset).write_volatile(b);
                    current.add(offset).write_volatile(a);
                }
            }
            at -= 1;
        }
    }
}

// ---------------------------------------------------------------------------
// ctype.h, for the C locale, which is the only one.
// ---------------------------------------------------------------------------

#[no_mangle]
pub extern "C" fn isspace(character: c_int) -> c_int {
    c_int::from(matches!(character, 0x20 | 0x09..=0x0d))
}

#[no_mangle]
pub extern "C" fn isdigit(character: c_int) -> c_int {
    c_int::from((0x30..=0x39).contains(&character))
}

#[no_mangle]
pub extern "C" fn isalpha(character: c_int) -> c_int {
    c_int::from((0x41..=0x5a).contains(&character) || (0x61..=0x7a).contains(&character))
}

#[no_mangle]
pub extern "C" fn isalnum(character: c_int) -> c_int {
    c_int::from(isalpha(character) != 0 || isdigit(character) != 0)
}

#[no_mangle]
pub extern "C" fn isupper(character: c_int) -> c_int {
    c_int::from((0x41..=0x5a).contains(&character))
}

#[no_mangle]
pub extern "C" fn islower(character: c_int) -> c_int {
    c_int::from((0x61..=0x7a).contains(&character))
}

#[no_mangle]
pub extern "C" fn isprint(character: c_int) -> c_int {
    c_int::from((0x20..=0x7e).contains(&character))
}

#[no_mangle]
pub extern "C" fn isxdigit(character: c_int) -> c_int {
    c_int::from(
        isdigit(character) != 0
            || (0x41..=0x46).contains(&character)
            || (0x61..=0x66).contains(&character),
    )
}

#[no_mangle]
pub extern "C" fn toupper(character: c_int) -> c_int {
    if islower(character) != 0 {
        character - 0x20
    } else {
        character
    }
}

#[no_mangle]
pub extern "C" fn tolower(character: c_int) -> c_int {
    if isupper(character) != 0 {
        character + 0x20
    } else {
        character
    }
}
