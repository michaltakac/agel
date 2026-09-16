//! The Agel language in a protection domain.
//!
//! This is the hosted runtime (`agel-core` without `std`) and the standard
//! library, loaded by the supervisor as a process like any other: `:exec
//! agel ROOT -- FILE` reads `FILE` from the namespace it was given, installs
//! the standard library, evaluates the file as one transaction and prints
//! each form's value on its console descriptor. It holds no authority the
//! supervisor did not hand it: a namespace, descriptors 1 and 2, pages at
//! its break. Its effect words reach the namespace, the console, the clock
//! and the program table through the process protocol, each behind a
//! capability the evaluator checks. Its heap is those pages; its stack is 4 MiB of them, which it
//! gives itself before the runtime runs, since the sixteen pages a process
//! is built with hold no deep evaluation.
//!
//! The runtime yields every so many evaluation steps by asking the clock,
//! so a long evaluation stays inside the tick budget a process has per
//! entry: the supervisor stops a domain that computes for a second without
//! a request.
#![no_std]
#![no_main]

extern crate alloc;

use agel_core::{Budget, EvaluationOptions, HostError, HostWord, Pulse, Value, World};
use agel_process_abi as abi;
use alloc::{format, string::String, vec::Vec};
use core::alloc::{GlobalAlloc, Layout};
use core::sync::atomic::{AtomicUsize, Ordering};

/// Pages of stack the process gives itself at its break before it runs
/// the runtime: 4 MiB, for the evaluator's recursion up to the language's
/// call-depth budget.
const STACK_PAGES: u64 = 1024;
/// Evaluation steps between two yields to the supervisor.
const PULSE_STEPS: u64 = 4096;
/// Fuel for one file: the standard library alone takes tens of thousands
/// of steps, and a program is expected to do real work after it.
const FUEL: u64 = 50_000_000;
/// The most bytes read from the source file: the filesystem's files are
/// smaller, and the runtime's own source limit is larger.
const SOURCE_BYTES: usize = 256 * 1024;

static SHARED_PAGE: AtomicUsize = AtomicUsize::new(0);

fn process() -> abi::Process {
    // Safety: `_start` stored the page the supervisor entered this domain
    // with, and nothing else ever runs here.
    unsafe { abi::Process::new(SHARED_PAGE.load(Ordering::Relaxed) as u64) }
}

/// Map `pages` at the break, in the pieces one `brk` maps: the address
/// they start at, or the negated error.
fn map_pages(pages: u64) -> i64 {
    let process = process();
    let start = process.brk(0);
    if start < 0 {
        return start;
    }
    let mut mapped = 0;
    while mapped < pages {
        let take = (pages - mapped).min(abi::BRK_PAGES);
        let result = process.brk(take);
        if result < 0 {
            return result;
        }
        mapped += take;
    }
    start
}

// ---------------------------------------------------------------------------
// The heap
// ---------------------------------------------------------------------------

/// Pages at the break, handed out in power-of-two classes from 16 bytes to
/// 64 KiB with a free list each, and as whole page runs for anything
/// larger. A freed block goes back to its class; a freed run to a small
/// table of free runs. The evaluator's churn of small nodes is therefore
/// reused at once and never walks a list; a run a class cannot hold is
/// rare (a long text, a large vector). Nothing here is thread-safe,
/// because nothing here is threaded.
const CLASS_FIRST: u32 = 4;
const CLASS_LAST: u32 = 16;
const CLASSES: usize = (CLASS_LAST - CLASS_FIRST + 1) as usize;
const PAGE: usize = 4096;
const RUNS: usize = 64;

struct Heap {
    /// The first free block of each class, or 0; a free block's first word
    /// is the next.
    free: [usize; CLASSES],
    /// The pages not yet carved: `[bump, end)`, contiguous with the break.
    bump: usize,
    end: usize,
    /// Free page runs: address and page count, `(0, 0)` for an empty slot.
    runs: [(usize, usize); RUNS],
}

struct Allocator;

static mut HEAP: Heap = Heap {
    free: [0; CLASSES],
    bump: 0,
    end: 0,
    runs: [(0, 0); RUNS],
};

/// The class of a request, or `None` for one that needs whole pages.
fn class_of(layout: &Layout) -> Option<(usize, usize)> {
    let wanted = layout.size().max(layout.align()).max(1 << CLASS_FIRST);
    let bits = usize::BITS - (wanted - 1).leading_zeros();
    if bits > CLASS_LAST {
        return None;
    }
    Some(((bits - CLASS_FIRST) as usize, 1 << bits))
}

/// `bytes` from the uncarved pages, aligned to `align`, growing the heap
/// by whole pieces of the break when they run out.
unsafe fn carve(heap: &mut Heap, bytes: usize, align: usize) -> *mut u8 {
    loop {
        let start = (heap.bump + align - 1) & !(align - 1);
        if heap.bump != 0 && start + bytes <= heap.end {
            heap.bump = start + bytes;
            return start as *mut u8;
        }
        let missing = (start + bytes).saturating_sub(heap.end).max(1);
        let pieces = missing.div_ceil(PAGE * abi::BRK_PAGES as usize) as u64;
        let mapped = map_pages(pieces * abi::BRK_PAGES);
        if mapped < 0 {
            return core::ptr::null_mut();
        }
        if heap.bump == 0 {
            heap.bump = mapped as usize;
            heap.end = mapped as usize;
        }
        heap.end += pieces as usize * abi::BRK_PAGES as usize * PAGE;
    }
}

unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let heap = &mut *core::ptr::addr_of_mut!(HEAP);
        match class_of(&layout) {
            Some((class, bytes)) => {
                let head = heap.free[class];
                if head != 0 {
                    heap.free[class] = *(head as *const usize);
                    return head as *mut u8;
                }
                carve(heap, bytes, bytes.min(PAGE).max(layout.align()))
            }
            None => {
                let pages = layout.size().div_ceil(PAGE);
                for slot in heap.runs.iter_mut() {
                    if slot.1 >= pages && layout.align() <= PAGE {
                        let address = slot.0;
                        *slot = if slot.1 == pages {
                            (0, 0)
                        } else {
                            (slot.0 + pages * PAGE, slot.1 - pages)
                        };
                        return address as *mut u8;
                    }
                }
                carve(heap, pages * PAGE, PAGE.max(layout.align()))
            }
        }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        let heap = &mut *core::ptr::addr_of_mut!(HEAP);
        match class_of(&layout) {
            Some((class, _)) => {
                *(pointer as *mut usize) = heap.free[class];
                heap.free[class] = pointer as usize;
            }
            None => {
                let pages = layout.size().div_ceil(PAGE);
                if let Some(slot) = heap.runs.iter_mut().find(|slot| slot.1 == 0) {
                    *slot = (pointer as usize, pages);
                }
                // With the table full the run is left mapped and unused.
            }
        }
    }
}

#[global_allocator]
static ALLOCATOR: Allocator = Allocator;

// ---------------------------------------------------------------------------
// Entry
// ---------------------------------------------------------------------------

#[no_mangle]
#[link_section = ".text.entry"]
pub extern "C" fn _start(shared_page: u64) -> ! {
    SHARED_PAGE.store(shared_page as usize, Ordering::Relaxed);
    let base = map_pages(STACK_PAGES);
    if base < 0 {
        process().report(2, b"agel: no pages for a stack: error ", -base, b"\n");
        process().exit((-base) as u64);
    }
    let top = base as u64 + STACK_PAGES * PAGE as u64;
    // Safety: the pages are mapped and nothing is on the old stack that
    // `main` needs; `main` never returns.
    unsafe {
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!("mov rsp, {top}", "call {main}", top = in(reg) top, main = sym main, options(noreturn));
        #[cfg(target_arch = "aarch64")]
        core::arch::asm!("mov sp, {top}", "b {main}", top = in(reg) top, main = sym main, options(noreturn));
        #[cfg(target_arch = "riscv64")]
        core::arch::asm!("mv sp, {top}", "j {main}", top = in(reg) top, main = sym main, options(noreturn));
    }
}

/// The arguments the supervisor placed: the program's name first.
fn arguments() -> Vec<Vec<u8>> {
    let (count, payload) = process().arguments();
    let mut arguments = Vec::new();
    let mut at = 0;
    for _ in 0..count.min(32) {
        let end = payload[at..]
            .iter()
            .position(|byte| *byte == 0)
            .map_or(payload.len(), |offset| at + offset);
        arguments.push(payload[at..end].to_vec());
        at = (end + 1).min(payload.len());
    }
    arguments
}

fn read_file(path: &[u8]) -> Result<Vec<u8>, i64> {
    let process = process();
    let descriptor = process.open(path, abi::O_RDONLY);
    if descriptor < 0 {
        return Err(descriptor);
    }
    let mut bytes = Vec::new();
    let mut block = [0_u8; abi::BLOCK_BYTES];
    loop {
        let count = process.read(descriptor as u64, &mut block);
        if count < 0 {
            process.close(descriptor as u64);
            return Err(count);
        }
        if count == 0 || bytes.len() >= SOURCE_BYTES {
            break;
        }
        bytes.extend_from_slice(&block[..count as usize]);
    }
    process.close(descriptor as u64);
    Ok(bytes)
}

/// The evaluator's yield: a request the supervisor answers at once, which
/// ends one entry of the domain and starts the next with a fresh budget.
fn pulse() {
    process().clock();
}

// ---------------------------------------------------------------------------
// Effect words
// ---------------------------------------------------------------------------

/// The words the process gives the language: the desktop's vocabulary for
/// its own evaluator (`file-read`, `file-write`, `file-append`,
/// `file-list`, `clock`, `console-log`, `exec`), each over the process
/// protocol and each behind a capability kind the evaluator checks before
/// the word runs — the evaluation's own set holds every kind, an agent
/// holds what it was spawned with — and `print-form`, the printed form of
/// a value as text, pure, so a program can write source the reader reads.
static HOST: [HostWord; 8] = [
    HostWord {
        name: "file-read",
        capability: "file/read",
        call: file_read,
    },
    HostWord {
        name: "file-write",
        capability: "file/write",
        call: file_write,
    },
    HostWord {
        name: "file-append",
        capability: "file/write",
        call: file_append,
    },
    HostWord {
        name: "file-list",
        capability: "file/read",
        call: file_list,
    },
    HostWord {
        name: "clock",
        capability: "clock/read",
        call: clock,
    },
    HostWord {
        name: "console-log",
        capability: "console/write",
        call: console_log,
    },
    HostWord {
        name: "exec",
        capability: "process/run",
        call: exec,
    },
    HostWord {
        name: "print-form",
        capability: "",
        call: print_form,
    },
];

/// The capability kinds the words need, issued to the evaluation for
/// every scope: the namespace `:exec` granted is the bound on what they
/// can name, and an agent gets only what it is spawned with.
const CAPABILITY_KINDS: [&str; 5] = [
    "file/read",
    "file/write",
    "clock/read",
    "console/write",
    "process/run",
];

/// The most bytes `file-read` answers.
const FILE_READ_BYTES: usize = 65536;

fn fail(kind: &str, message: String) -> HostError {
    HostError {
        kind: String::from(kind),
        message,
    }
}

fn text_argument<'a>(
    word: &str,
    arguments: &'a [Value],
    index: usize,
) -> Result<&'a str, HostError> {
    match arguments.get(index) {
        Some(Value::String(text)) => Ok(text),
        _ => Err(fail("type", format!("{word} expects text"))),
    }
}

fn expect_arguments(word: &str, arguments: &[Value], count: usize) -> Result<(), HostError> {
    if arguments.len() == count {
        Ok(())
    } else {
        Err(fail(
            "arity",
            format!("{word} expects {count} arguments, got {}", arguments.len()),
        ))
    }
}

/// A path as the namespace names it: from its root, without the leading
/// slash the language's paths carry; the root itself is `.`.
fn namespace_path(path: &str) -> &[u8] {
    let path = path.trim_start_matches('/');
    if path.is_empty() {
        b"."
    } else {
        path.as_bytes()
    }
}

fn file_error(word: &str, path: &str, error: i64) -> HostError {
    let kind = match -error {
        2 => "file/not-found",
        13 => "file/denied",
        28 => "file/full",
        _ => "file/error",
    };
    fail(kind, format!("{word}: {path}: error {}", -error))
}

fn file_read(arguments: &[Value]) -> Result<Value, HostError> {
    expect_arguments("file-read", arguments, 1)?;
    let path = text_argument("file-read", arguments, 0)?;
    let process = process();
    let descriptor = process.open(namespace_path(path), abi::O_RDONLY);
    if descriptor < 0 {
        return Err(file_error("file-read", path, descriptor));
    }
    let mut bytes = Vec::new();
    let mut block = [0_u8; abi::BLOCK_BYTES];
    let outcome = loop {
        let count = process.read(descriptor as u64, &mut block);
        if count < 0 {
            break Err(file_error("file-read", path, count));
        }
        if count == 0 || bytes.len() >= FILE_READ_BYTES {
            break Ok(());
        }
        bytes.extend_from_slice(&block[..count as usize]);
    };
    process.close(descriptor as u64);
    outcome?;
    Ok(Value::String(String::from_utf8_lossy(&bytes).into_owned()))
}

fn file_put(word: &str, arguments: &[Value], append: bool) -> Result<Value, HostError> {
    expect_arguments(word, arguments, 2)?;
    let path = text_argument(word, arguments, 0)?;
    let text = text_argument(word, arguments, 1)?;
    let process = process();
    let flags = abi::O_WRONLY | abi::O_CREAT | if append { abi::O_APPEND } else { 0 };
    let descriptor = process.open(namespace_path(path), flags);
    if descriptor < 0 {
        return Err(file_error(word, path, descriptor));
    }
    if !append {
        let result = process.ftruncate(descriptor as u64, 0);
        if result < 0 {
            process.close(descriptor as u64);
            return Err(file_error(word, path, result));
        }
    }
    let written = process.write(descriptor as u64, text.as_bytes());
    process.close(descriptor as u64);
    if (written as i64) < 0 || written as usize != text.len() {
        return Err(fail("file/error", format!("{word}: {path}: short write")));
    }
    Ok(Value::Int(written as i64))
}

fn file_write(arguments: &[Value]) -> Result<Value, HostError> {
    file_put("file-write", arguments, false)
}

fn file_append(arguments: &[Value]) -> Result<Value, HostError> {
    file_put("file-append", arguments, true)
}

fn file_list(arguments: &[Value]) -> Result<Value, HostError> {
    expect_arguments("file-list", arguments, 1)?;
    let path = text_argument("file-list", arguments, 0)?;
    let process = process();
    let descriptor = process.open(namespace_path(path), abi::O_RDONLY | abi::O_DIRECTORY);
    if descriptor < 0 {
        return Err(file_error("file-list", path, descriptor));
    }
    let mut names = Vec::new();
    let mut name = [0_u8; 64];
    let outcome = loop {
        match process.readdir(descriptor as u64, &mut name) {
            Ok(Some((length, _, _))) => {
                names.push(Value::String(
                    String::from_utf8_lossy(&name[..length]).into_owned(),
                ));
            }
            Ok(None) => break Ok(()),
            Err(error) => break Err(file_error("file-list", path, error)),
        }
    };
    process.close(descriptor as u64);
    outcome?;
    Ok(Value::List(names))
}

/// Seconds since the machine came up, whole.
fn clock(arguments: &[Value]) -> Result<Value, HostError> {
    expect_arguments("clock", arguments, 0)?;
    Ok(Value::Int((process().clock() / 1_000_000) as i64))
}

fn console_log(arguments: &[Value]) -> Result<Value, HostError> {
    expect_arguments("console-log", arguments, 1)?;
    let process = process();
    let line = match &arguments[0] {
        Value::String(text) => text.clone(),
        other => format!("{other}"),
    };
    process.write(1, line.as_bytes());
    process.write(1, b"\n");
    Ok(Value::Nil)
}

/// The printed form of a value: what the reader reads back.
fn print_form(arguments: &[Value]) -> Result<Value, HostError> {
    expect_arguments("print-form", arguments, 1)?;
    Ok(Value::String(format!("{}", arguments[0])))
}

/// Run a program from the table as a child with this process's namespace,
/// no standard input, this process's console as its output, and wait for
/// it: its status.
fn exec(arguments: &[Value]) -> Result<Value, HostError> {
    expect_arguments("exec", arguments, 1)?;
    let name = text_argument("exec", arguments, 0)?;
    let process = process();
    let child = process.spawn(name.as_bytes(), b"", abi::NO_DESCRIPTOR, 1, 0);
    if child < 0 {
        return Err(fail(
            "process/error",
            format!("exec: {name}: error {}", -child),
        ));
    }
    Ok(Value::Int(process.wait(child as u64)))
}

fn say(descriptor: u64, text: &str) {
    process().write(descriptor, text.as_bytes());
}

extern "C" fn main() -> ! {
    let process = process();
    let arguments = arguments();
    let mut stdlib = true;
    let mut file = None;
    for argument in arguments.iter().skip(1) {
        match argument.as_slice() {
            b"--no-stdlib" => stdlib = false,
            _ => file = Some(argument.clone()),
        }
    }
    let Some(path) = file else {
        say(2, "usage: agel [--no-stdlib] FILE\n");
        process.exit(2);
    };
    let source = match read_file(&path) {
        Ok(bytes) => bytes,
        Err(error) => {
            process.write(2, b"agel: ");
            process.write(2, &path);
            process.report(2, b": error ", -error, b"\n");
            process.exit((-error) as u64);
        }
    };
    let Ok(source) = String::from_utf8(source) else {
        say(2, "agel: the file is not UTF-8\n");
        process.exit(1);
    };
    let mut world = World::default();
    world.install_host(&HOST);
    let mut capabilities = Vec::new();
    for kind in CAPABILITY_KINDS {
        match world.issue_capability(kind, "*") {
            Ok(capability) => capabilities.push(capability),
            Err(error) => {
                say(2, &format!("agel: {error}\n"));
                process.exit(1);
            }
        }
    }
    let options = EvaluationOptions {
        budget: Budget {
            fuel: FUEL,
            ..Budget::default()
        },
        capabilities,
        pulse: Some(Pulse {
            every: PULSE_STEPS,
            hook: pulse,
        }),
        host: &HOST,
    };
    if stdlib {
        match agel_stdlib::install(&mut world, &options) {
            Ok(commit) => say(
                1,
                &format!(
                    "agel: standard library installed, {} steps\n",
                    commit.steps_used
                ),
            ),
            Err(error) => {
                say(2, &format!("agel: standard library: {error}\n"));
                process.exit(1);
            }
        }
    }
    match world.evaluate_with(&source, &options) {
        Ok(commit) => {
            for value in &commit.values {
                say(1, &format!("=> {value}\n"));
            }
            say(
                1,
                &format!(
                    "agel: {} forms, {} steps, revision {}\n",
                    commit.values.len(),
                    commit.steps_used,
                    commit.revision
                ),
            );
            process.exit(0)
        }
        Err(error) => {
            say(2, &format!("agel: error: {error}\n"));
            process.exit(1)
        }
    }
}
