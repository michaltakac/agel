# The POSIX personality

Status: started at v0.2.41; stratum 1 at v0.2.43; stratum 2 at v0.2.44;
stratum 3 at v0.2.45; stratum 4 begun at v0.2.46. This document is the
plan, the process ABI, and an honest account of what exists.

The requirements are in [`deployment-targets.md`](deployment-targets.md):
the kernel contract contains no POSIX concept; the personality runs
unprivileged, in protection domains, above the contract; a process reaches
exactly what its capability set covers and a name outside it is unreachable
however it is spelled; a descriptor is a derived handle that fails closed
across a service restart; and POSIX software builds against Agel's C library
from source. Everything below is measured against those.

## The strata, in order

| Stratum | What it adds | Status |
|---|---|---|
| 0. Processes | a static ELF loaded from disk into a fresh domain; a request protocol on the shared page; `write` to the console and `exit` | **v0.2.41**, all three research machines |
| 1. Files and namespaces | an unprivileged filesystem service; a namespace capability per process; `open`, `read`, `write`, `close` on descriptors derived from it | **v0.2.43**, all three research machines |
| 2. The C library | `agel-libc`, a `no_std` Rust library with a C ABI, so C and Rust programs build for Agel from source | **v0.2.44**, all three research machines |
| 3. Processes that make processes | `spawn` with an explicit capability set, never `fork`; pipes; `wait` | **v0.2.45**, all three research machines |
| 4. Breadth | the growing subset of the standard that real programs need: `stdio`, `malloc`, `string`, `errno`, time | **begun at v0.2.46**: arguments, a free-list heap, streams, seek and append, `ctype`, the wider `string` and `stdlib`; an unmodified third-party source builds and runs |

Binary compatibility with Linux ELFs is not planned; see the requirements.

## Stratum 0: processes

A process is an ordinary protection domain whose code did not come from the
kernel image. The supervisor reads a static ELF from the disk's **program
region**, sectors 2048 through 3071 (1024 through 2047 before v0.2.42): a
table sector (`AGELPR1`, a count, then
32-byte rows of name, start sector, length and CRC-32) followed by the
images. `scripts/install-program.py IMAGE NAME ELF` writes a row;
`scripts/build-program.sh NAME [arch]` builds one of the programs under
`boot/posix` for a research machine. The serial workshop's `:exec NAME`
loads and runs it:

```text
agel-native[0]> :exec hello
hello from a loaded process
process hello exited with status 42
agel-native[0]> :exec hostile
hostile process about to write where it may not
process hostile faulted: page-fault at 0x9000002c touching 0x10; contained
```

The loader checks the image's CRC-32 against its table row, then requires a
little-endian ELF64 static executable for this machine whose `PT_LOAD`
segments lie inside the **process window**, 16 MiB at a fixed address in
the domain's private region (`PROCESS_BASE`, per machine), are
page-congruent with their file offsets, do not share pages, and are never
writable and executable together. Each page is a frame allocated with the
domain and recorded with its frames, filled from the image before the domain
runs, and mapped with the segment's rights. The entry point must lie in the
window. A process has sixteen stack pages and a tick budget of one second
per entry, like a driver; a process that computes for longer without
yielding is stopped and reported.

### The process protocol

A process is entered like every world: the address of its shared page in
the first argument register (`rdi`, `x0`, `a0`). It asks for things by
writing a request block into that page and yielding, which is the
contract's `endpoint.send` on the supervisor's well-known slot 31, the same
instruction every driver domain uses. The supervisor answers in the block
and resumes the process.

| Word | Meaning |
|---|---|
| 64 | request kind |
| 65–68 | four argument words |
| 69 | the answer: a result, or a negated error number |

| Kind | Arguments | Answer |
|---|---|---|
| `1` exit | status | never returns; the domain is not resumed and its frames go back to the pool |
| `2` write | descriptor, length ≤ 512 | bytes written; the data is the first `length` bytes of the block area at byte 1024 of the page; descriptors 1 and 2 are the console, through the console driver domain, with `\n` written as `\r\n`; a file descriptor from stratum 1 writes at its offset; anything else is `-EBADF` |
| `3` open | flags, path length ≤ 256 | a descriptor from 3 up, or a negated error; the path is the first `length` bytes of the payload area at byte 128; stratum 1 |
| `4` read | descriptor, length ≤ 512 | bytes read into the block area, 0 at the end of the file, or a negated error; stratum 1 |
| `5` close | descriptor | 0, or `-EBADF`; stratum 1 |
| `6` spawn | name length, stdin descriptor, stdout descriptor, flags | the child's id; the name is in the payload area; `0xffff` names no descriptor; flag `1` gives the child a read-only namespace; stratum 3 |
| `7` pipe | none | read descriptor in the low sixteen bits, write descriptor in the next sixteen; stratum 3 |
| `8` wait | child id | the child's exit status, or `0x100` with a signal number in the low byte when the machine stopped it (`11` a fault, `9` a budget or a deadlock); blocks until the child ends; `-ECHILD` for a child that is not the caller's; stratum 3 |
| `9` seek | descriptor, offset, whence (0 start, 1 current, 2 end) | the new offset; `-ESPIPE` for a pipe or the console; stratum 4 |
| `10` window | width, height, title length ≤ 28 | a window's number, or `-ENODEV` where there is no display, `-EBUSY` when every window is taken or the process has one, `-EINVAL` for a size outside 64×48..1280×720; the title is in the payload area; not POSIX, see [`native-graphics.md`](native-graphics.md) |
| `11` draw | window, record count ≤ 8, flags (`1` clears first) | the records the window holds after this; the records are 64-byte compositor records in the block area, relative to the window's content; `-EINVAL` and nothing drawn when any is not permitted or reaches outside the content, `-ENOSPC` past 24 records, `-EBADF` for a window the process does not own; not POSIX |

Before a process first runs, the supervisor places its arguments in the
payload area as NUL-terminated strings, the program's name first, and
their count in word 70. `:exec NAME [ROOT] [ro] [-- ARG...]` supplies them
from the workshop; `spawn` supplies a child's from the bytes after the
name in the parent's payload, whose length rides in the upper bits of the
flags word. `open` honours `O_APPEND` (`0o2000`): the descriptor starts at
the file's end.

Any other kind answers `-ENOSYS`. Nothing here is a contract operation, and
nothing here is a path the kernel interprets: the program region is a table
of names the supervisor owns, a process reaches the console because the
supervisor's policy for descriptors 1 and 2 says so, and a path in `open`
is resolved by an unprivileged service inside the namespace the process
was given.

The programs under `boot/posix` share the process side of the protocol as
the `agel-process-abi` crate, which depends on nothing in the kernel.
`scripts/test-process.sh [arch]` installs `hello` and `hostile` on a
temporary disk and requires the hello program to run twice (the second run
proves the first's frames came back), the hostile one to be contained, an
absent name to be refused, and the workshop to still evaluate afterwards.

### What stratum 0 does not claim

There is no C library yet: the programs are Rust. There are no arguments,
environment, or processes that make processes. The program table names
programs but grants nothing; what a process may reach is the namespace
stratum 1 gives it at `:exec`. Since v0.2.49 the graphics image carries the
loader and the filesystem service too: the graphical workshop's `:exec`
runs a program with its console tee'd to a terminal panel on the desktop.
Reclamation of a process's frames is the frame pool's, which every workshop
compiles in.

## Stratum 1: files through namespaces

A file is reached through a **namespace**, never through an ambient root.
The operator grants the namespace at `:exec NAME [ROOT] [ro]`: the directory
the process sees as `/`, and whether it may write and create. Nothing a
process does widens it. No `ROOT` means the filesystem's root, which is
entry 0 by construction and is not looked up, so a program that never opens
a file runs whether or not the region is formatted; a named `ROOT` is
resolved before the program is loaded and a bad one refuses the `:exec`.

```text
agel-native[0]> :fs-format
formatted
agel-native[0]> :fs-mkdir app
directory ready: app
agel-native[0]> :fs-mkdir etc
directory ready: etc
agel-native[0]> :exec writer
writer: wrote etc/secret and app/notes
process writer exited with status 0
agel-native[0]> :exec reader /app
reader: notes: notes for the app
reader: etc/secret: error 2
reader: ../etc/secret: error 13
process reader exited with status 0
agel-native[0]> :fs-restart
filesystem restarted: generation 2
agel-native[0]> :exec writer /app ro
writer: open etc/secret: error 13
process writer exited with status 13
```

The reader, rooted at `app`, reads `notes` and cannot name `etc/secret`:
the file exists on the disk and is `ENOENT` to this process, because the
namespace is the whole of what it can name. `..` at the namespace root is
`EACCES`, not a step up. The writer in a read-only namespace is refused at
its first `open` for writing, by the supervisor, before the service sees
the path.

### The filesystem service

The filesystem is an ordinary unprivileged world, `agel_fs_main`, built like
a driver: its own address space, a private stack, a shared page, no device.
It owns disk sectors 1536 through 2047 and reaches them only by asking:
to read or write a sector it fills three words of its shared page (an
operation, a sector, and a status the supervisor fills) and yields; the
supervisor relays the sector through the storage driver domain, refusing
any sector outside the region with `EACCES`, and resumes the service. The
service never touches the device and cannot reach a sector it does not own,
however it computes the number.

The on-disk shape, `agelfs`, is deliberately small: a superblock
(`AGELFS1\0`) at sector 1536, four directory sectors of 32 entries (a
32-byte name, a kind, a parent, a length), and one 8-sector extent per
entry, so a file holds at most 4,096 bytes. Entry 0 is the root directory.
An unformatted region is `EIO`, not an empty filesystem the service
invents; `:fs-format` writes one. The service keeps the directory in its
stack and writes an entry's sector back on every change, so the files are
on the disk and a new boot or a restarted service reads them.

The supervisor speaks to the service by command: format, open (a root
entry, flags, a path in the payload area; answers entry, length, kind),
read and write (entry, offset, length; the data crosses in the block area),
and list (a directory and a position; answers the entry and leaves its name
in the payload). The service resolves a path from the root it was given,
component by component, refusing `..` at that root; the supervisor passes a
process's namespace root, so the service never sees a path the process
could not have named.

### Descriptors

A descriptor is a supervisor-side record: the entry, an offset, whether it
may read and write, and the filesystem service's generation when it was
opened. Sixteen per process, numbered from 3. `open` is bounded by the
namespace before the service is asked: a namespace without `write` cannot
open for writing, without `create` cannot create, so the refusal costs no
sector. `read` and `write` carry the descriptor's entry and offset to the
service and advance the offset by what it answered. `close` frees the slot;
a closed or unknown descriptor is `EBADF`.

The generation is what makes a descriptor fail closed: `:fs-restart`
replaces the service world with a fresh one, and a request through a
descriptor from before answers `ESTALE` (116), the way a stale driver
handle answers `stale-generation`. The files survive the restart, because
they are on the disk; the descriptors do not, because their authority came
from a service that no longer exists.

### What is proved

`scripts/test-files.sh [arch]` runs on all three research machines:
format, two directories, the writer at the root, the reader in a namespace
rooted below it (its file readable, the sibling directory unnameable, `..`
refused), the service restarted to a new generation and the reader run
again, the writer in a read-only namespace refused with `EACCES`, `:fs-ls`
of the root and a directory, `:exec` with a root that does not exist
refused, and a reboot after which the reader still finds its file. The
reader also requires a read past the end of the file to answer 0 and a
read on a closed descriptor to answer `EBADF`.

### What stratum 1 does not claim (before stratum 2)

The stale-descriptor path is implemented and not exercised: a process runs
to its end within one `:exec`, and there is no way yet to restart the
service while one holds a descriptor, so `ESTALE` is code the tests have
not reached. Files hold one extent; there is no `unlink`, `rename`, `seek`,
`stat`, no timestamps, no free-space accounting beyond the fixed table, and
no integrity beyond the superblock magic: a damaged directory sector is
read as it is. The namespace is a root and three rights, not a general
capability tree: two processes cannot be given disjoint rights on the same
directory except by `ro`. The filesystem service is one world for the
machine and serves one request at a time. The supervisor still holds the
descriptor table; a stratum with a C library moves what it can into the
process.

## Stratum 2: the C library

`boot/posix/libc` is `agel-libc`: a `no_std` Rust crate built as a static
archive with a C ABI, and the headers in `boot/posix/libc/include`. A C
program compiles with any clang that has the three backends, links the
archive with lld, and runs as a process like the Rust ones:

```text
agel-native[0]> :exec c-hello
hello from C on Agel: a heap string of 13 bytes, 100% sure, ff hex
process c-hello exited with status 7
agel-native[0]> :exec c-cat /app
notes for the app
process c-cat exited with status 0
agel-native[0]> :exec c-cat
cat: notes: errno 2
process c-cat exited with status 2
```

`scripts/build-c-program.sh NAME [arch]` builds `boot/posix/c/NAME.c`:
clang with `-ffreestanding -nostdlib -fno-builtin` and the machine's flags
(position-independent code without SSE on x86-64, general registers only
on AArch64, `rv64imac` on RISC-V, since a process gets no floating-point
state), then `-fuse-ld=lld` with the same linker script as the Rust
programs. Apple's clang has no RISC-V backend and no lld; Homebrew's `llvm`
and `lld` do, and `AGEL_CLANG` names a compiler explicitly.

### What the library provides

| Header | Functions | Notes |
|---|---|---|
| `unistd.h` | `read`, `write`, `close`, `_exit` | one process request each; `read` and `write` move at most one block per call, as the protocol does |
| `fcntl.h` | `open` | through the namespace; the C prototype is variadic, the definition takes the two arguments every call passes |
| `errno.h` | `errno`, the numbers the protocol answers | `errno` is `*__errno_location()`, set from a negated answer |
| `stdlib.h` | `malloc`, `calloc`, `realloc`, `free`, `exit`, `abort` | a 64 KiB bump arena in the process's own `.bss`; `free` returns nothing |
| `string.h` | `memcpy`, `memmove`, `memset`, `memcmp`, `strlen`, `strcmp`, `strncmp`, `strcpy`, `strchr` | volatile byte loops, so the compiler cannot turn them into calls to themselves |
| `stdio.h` | `printf`, `puts`, `putchar` | `printf` handles `%s %d %i %u %x %c %p %%` with `l` and `z`; it is C, in `libc/c/stdio.c`, because a C-variadic definition is not stable Rust |

The process entry `_start` is the library's: it keeps the shared page,
calls `int main(void)` and exits with what it returns. The `unsafe` in the
library is the C boundary, reading C strings and filling callers' buffers;
nothing in it holds authority the process was not given.

### What is proved

`scripts/test-libc.sh [arch]` builds `hello.c` and `cat.c` from source for
the machine and runs them: `printf` with every conversion the program uses,
`malloc`, `strcpy` and `strlen`, `main`'s return as the exit status; `cat`
in a namespace rooted at `app` reading `notes` to the end through `open`,
`read`, `write` and `close`, and at the root finding no `notes`, reporting
`errno` and exiting with it. CI runs all three machines.

### What stratum 2 does not claim

This is the foundation of source compatibility, not its breadth: no
`stdio` streams, no `fopen`, no `scanf`, no `sprintf`, no `time`, no
`signal`, no `math`, no environment, no `argv` (a process still gets no
arguments), no `errno` strings, no locale. `free` frees nothing. `printf`
is a subset; a format it does not know is copied through. The library is
single-threaded, like the process. Nothing here is POSIX-certified or
tested against a conformance suite; a real program will find the first
missing function quickly, and stratum 4 is the answer to that, one function
at a time, with a test each.

## Stratum 3: processes that make processes

There is no `fork`. A process starts another by name with `spawn`, and the
child receives exactly what the parent names: one descriptor as its
standard input, one as its standard output (either may be none), the
console as its standard error, and the parent's namespace, or a read-only
view of it. Nothing else crosses; a child holds no descriptor its parent
did not choose to give it, and cannot reach a wider namespace than its
parent's.

```text
agel-native[0]> :exec c-pipeline
HELLO FROM THE PARENT
child 1 exited with 22
hostile process about to write where it may not
process hostile faulted: page-fault at 0x9000002c touching 0x10; contained
child 1 stopped by signal 11
spawn nothing: errno 2
wait for no child: errno 10
process c-pipeline exited with status 0
```

The parent made a pipe, spawned `c-shout` with the pipe's read end as its
standard input, closed its own copy of that end, wrote a line, closed the
write end, and waited; the child read to the end of the stream, uppercased
what it read, and exited with the byte count. Then the parent spawned the
hostile program, which the machine stopped, and saw a signal; then a name
the program region does not hold, and a wait for a child it never had.

### The process table

`:exec` now serves a table of up to four processes, the one the operator
started and every descendant, with a round-robin scheduler: each pass gives
every runnable process one entry (it runs until it yields a request or is
stopped) and retries every blocked one. A process blocks on `wait` for a
child that has not ended, on a `read` of an empty pipe that still has a
writer, and on a `write` to a full pipe that still has a reader; the
request is answered on the pass where it can be. When no live process can
make progress, every process that is left is blocked on something no other
will do, and they are stopped as blocked forever. `:exec` returns when
every process has ended, reporting the first one's end; a child the
machine stops is reported on the console when it happens, since no `wait`
need ever hear of it.

A process's frames go back to the pool the moment it ends. Its descriptors
close then too, so the last write end of a pipe closing is the end of the
stream for whoever reads it. An ended child stays in the table, with its
exit, until its parent waits for it or the `:exec` ends.

### Pipes

A pipe is a 512-byte queue in the supervisor with a count of the read ends
and write ends that name it; there are four. `pipe` returns two descriptors
of the calling process; passing an end to a child at `spawn` is one more end
naming the pipe, and `close` gives one back. `read` on an empty pipe with no
writer left answers 0; `write` on a pipe with no reader left answers
`-EPIPE`. Reads and writes move at most a block, and may move less than
asked.

### The C library

`<unistd.h>` gains `pipe`, `<sys/wait.h>` has `waitpid` with `WIFEXITED`,
`WEXITSTATUS`, `WIFSIGNALED` and `WTERMSIG` encoded as POSIX reads them, and
`<spawn.h>` has Agel's own `agel_spawn(program, stdin_fd, stdout_fd, flags)`
with `AGEL_SPAWN_READ_ONLY`. It is named for what it is: not
`posix_spawn`, whose file actions and attributes assume an inherited
environment this system does not have.

### What is proved

`scripts/test-spawn.sh [arch]` runs the pipeline above on all three
machines, requires the child's output, the byte count as its status, the
fault report and the signal the parent sees, `ENOENT` for a missing
program, `ECHILD` for a child that is not the caller's, and then runs the
whole thing again to show every process's frames came back.

### What stratum 3 does not claim

A child gets two descriptors and a namespace, never arguments or an
environment; `main` is still `int main(void)`. The table holds four
processes and four pipes, and the scheduler is a round robin without
priorities. A parent cannot give a child a narrower root than its own,
only a read-only view. There is no `kill`, no signals a process can catch,
no process groups, no `exec` that replaces a running image, and no
blocking `read` of the console. Deadlock is detected only when nothing at
all can run; a process spinning on a pipe it will never fill is stopped by
its tick budget, not by the detector.

## Stratum 4: breadth, begun

Breadth is not a release but a direction: each step adds what a real
program needs next, with a test. v0.2.46 is the first step, chosen so
that a C source written for other systems builds and runs unmodified.

```text
agel-native[0]> :exec c-cat /app -- notes
notes for the app
process c-cat exited with status 0
agel-native[0]> :exec c-digest /app -- notes
968fdb320ca48afd0557fe96b6416246e0b266031146ab2a707a119c0107b684  notes
process c-digest exited with status 0
agel-native[0]> :exec c-breadth /etc -- one two
arguments: 3 [c-breadth] [one] [two]
heap: reuse, join, realloc, ENOMEM
formatter: widths, flags, precision, truncation
strings: strtol, strstr, strrchr, ctype, strcat, qsort
streams: fopen, fprintf, append, fgets, feof, lseek
breadth: 23 checks passed
process c-breadth exited with status 0
```

What this step adds:

- **Arguments.** `main(int argc, char **argv)` receives what `:exec`'s
  `--` or the parent's `agel_spawn(program, argv, ...)` supplied, at most
  256 bytes and 32 arguments; `argv[0]` is the program's name.
- **A heap that gives memory back.** `malloc` is a first-fit free list
  over a 256 KiB arena in the process's `.bss` with 16-byte headers;
  `free` joins a freed block with free neighbours; `realloc` grows in place
  when the block already suffices and copies otherwise; `calloc` clears.
- **Streams.** `FILE` is a descriptor with a line-buffered output buffer
  and a block-buffered input buffer: `fopen` (`r`, `w`, `a`, with `+`),
  `fclose`, `fflush`, `fileno`, `feof`, `ferror`, `clearerr`, `fputc`,
  `putc`, `putchar`, `fputs`, `puts`, `fwrite`, `fgetc`, `getc`,
  `getchar`, `fgets`, `fread`, `printf`, `fprintf`, `sprintf`,
  `snprintf`, `vprintf`, `vfprintf`, `vsprintf`, `vsnprintf`, `perror`.
  `exit` flushes every stream. The formatter handles `%s %c %d %i %u %x
  %X %o %p %%` with `-`, `0`, `+` and space, a width, a precision, `*`,
  and the `l`, `ll`, `z` and `h` modifiers; no floating point.
- **Seek and append.** `lseek` with `SEEK_SET`, `SEEK_CUR` and `SEEK_END`
  on file descriptors, `O_APPEND`, and `fopen`'s `a`.
- **`string.h`, `stdlib.h`, `ctype.h`, `assert.h`, `memory.h`.**
  `memchr`, `strncpy`, `strcat`, `strncat`, `strrchr`, `strstr`, `strdup`,
  `strerror`; `atoi`, `atol`, `strtol`, `strtoul`, `abs`, `labs`, `qsort`
  (an insertion sort), `EXIT_SUCCESS`; the C-locale character classes;
  `assert`; and `memory.h` as the old name of `string.h`, which the
  third-party source includes.
- **A third-party source, unmodified.** `boot/posix/c/third-party/sha256.c`
  is Brad Conte's public-domain SHA-256 as published; `digest.c` is the
  program around it, and the test compares its digest of a file with the
  host's.

**Windows, which are not POSIX.** `<agel/window.h>` declares
`agel_window(width, height, title)` and `agel_draw(window, records, count,
flags)` over the `10` and `11` requests, with inline builders for the
records the desktop admits: `agel_rect`, `agel_gradient`, `agel_ellipse`,
`agel_label`, `agel_surface`, `agel_sprite`. `chart.c` draws a bar for
each number on its command line and, given the word `outside`, first asks
to draw past its window's edge and reports the refusal. On a machine with
no display the request answers `-ENODEV`, which `chart` reports and exits
with 3; `scripts/test-libc.sh` requires exactly that on all three
machines, and `scripts/test-desktop-process.sh` the windows themselves.
How the desktop keeps and checks what a process draws is in
[`native-graphics.md`](native-graphics.md).

What it does not add: no `scanf` family, no `time`, no `signal`, no
`setjmp`, no `math`, no environment, no `getopt`, no `stat`, no
`opendir`, no `unlink` or `rename` (the filesystem has none), no floating
point in the formatter, no locale, no threads. `qsort` is quadratic. The
heap is one arena and never grows. Each is a step this stratum takes when
a program needs it, with a test.
