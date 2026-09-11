//! `agel-libc`: the C library of the POSIX personality, in Rust with a C
//! ABI. C programs build against the headers in `include/` and link this
//! archive; every function here translates a POSIX call into the process
//! protocol in `docs/posix-personality.md`, through `agel-process-abi`.
//!
//! The `unsafe` in this file is the C boundary: reading a C string, filling
//! a caller's buffer, the process entry. Nothing here has ambient
//! authority; a path resolves through the namespace the process was given.
#![no_std]

use core::ffi::{c_char, c_int, c_void};
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
    fn main() -> c_int;
}

/// The process entry: the supervisor arrives here with the shared page in
/// the first argument register. Runs `main` and exits with its answer.
#[no_mangle]
#[link_section = ".text.entry"]
pub extern "C" fn _start(shared_page: u64) -> ! {
    PAGE.store(shared_page as usize, Ordering::Relaxed);
    // Safety: `main` is the program's, with the C prototype `int main(void)`.
    let status = unsafe { main() };
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
/// `program` must be NUL-terminated.
#[no_mangle]
pub unsafe extern "C" fn agel_spawn(
    program: *const c_char,
    stdin_fd: c_int,
    stdout_fd: c_int,
    flags: c_int,
) -> c_int {
    let length = unsafe { strlen(program) };
    let name = unsafe { core::slice::from_raw_parts(program as *const u8, length) };
    let descriptor = |number: c_int| {
        if number < 0 {
            agel_process_abi::NO_DESCRIPTOR
        } else {
            number as u64
        }
    };
    outcome(process().spawn(
        name,
        descriptor(stdin_fd),
        descriptor(stdout_fd),
        flags as u64,
    )) as c_int
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
    _exit(status)
}

#[no_mangle]
pub extern "C" fn abort() -> ! {
    _exit(134)
}

/// The heap: a fixed arena in the process's own `.bss`, handed out by a
/// bump pointer. `free` returns nothing to it; a process that needs more
/// than this or needs reuse is what a later stratum's allocator is for.
const ARENA_BYTES: usize = 64 * 1024;
static mut ARENA: [u8; ARENA_BYTES] = [0; ARENA_BYTES];
static NEXT: AtomicUsize = AtomicUsize::new(0);

#[no_mangle]
pub extern "C" fn malloc(size: usize) -> *mut c_void {
    let size = size.max(1);
    let start = (NEXT.load(Ordering::Relaxed) + 15) & !15;
    let Some(end) = start.checked_add(size) else {
        return core::ptr::null_mut();
    };
    if end > ARENA_BYTES {
        // Safety: as in `outcome`.
        unsafe { ERRNO = 12 };
        return core::ptr::null_mut();
    }
    NEXT.store(end, Ordering::Relaxed);
    // Safety: the range lies inside the arena and was never handed out.
    unsafe { core::ptr::addr_of_mut!(ARENA).cast::<u8>().add(start) as *mut c_void }
}

#[no_mangle]
pub extern "C" fn calloc(count: usize, size: usize) -> *mut c_void {
    let Some(total) = count.checked_mul(size) else {
        return core::ptr::null_mut();
    };
    // The arena is zero and never reused, so a fresh block is already clear.
    malloc(total)
}

/// # Safety
/// `pointer` is null or came from `malloc`; the old block's size is not
/// known, so the copy is bounded by the new size.
#[no_mangle]
pub unsafe extern "C" fn realloc(pointer: *mut c_void, size: usize) -> *mut c_void {
    let replacement = malloc(size);
    if !pointer.is_null() && !replacement.is_null() {
        unsafe { memcpy(replacement, pointer, size) };
    }
    replacement
}

#[no_mangle]
pub extern "C" fn free(_pointer: *mut c_void) {}

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

// ---------------------------------------------------------------------------
// stdio.h, the part that is not variadic. `printf` is in `c/stdio.c`.
// ---------------------------------------------------------------------------

#[no_mangle]
pub extern "C" fn putchar(character: c_int) -> c_int {
    let byte = [character as u8];
    if process().write(1, &byte) == 1 {
        c_int::from(byte[0])
    } else {
        -1
    }
}

/// # Safety
/// `text` must be NUL-terminated.
#[no_mangle]
pub unsafe extern "C" fn puts(text: *const c_char) -> c_int {
    let length = unsafe { strlen(text) };
    let bytes = unsafe { core::slice::from_raw_parts(text as *const u8, length) };
    let process = process();
    if process.write(1, bytes) as i64 != length as i64 || process.write(1, b"\n") != 1 {
        return -1;
    }
    (length + 1) as c_int
}
