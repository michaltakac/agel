# The POSIX personality

Status: started at v0.2.41. This document is the plan, the process ABI, and
an honest account of what exists.

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
| 1. Files and namespaces | an unprivileged filesystem service; a namespace capability per process; `open`, `read`, `write`, `close` on descriptors derived from it | next |
| 2. The C library | `agel-libc`, a `no_std` Rust library with a C ABI, so C and Rust programs build for Agel from source | after 1 |
| 3. Processes that make processes | `spawn` with an explicit capability set, never `fork`; pipes; `wait` | after 2 |
| 4. Breadth | the growing subset of the standard that real programs need: `stdio`, `malloc`, `string`, `errno`, time | ongoing |

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
| `2` write | descriptor, length ≤ 512 | bytes written; the data is the first `length` bytes of the block area at byte 1024 of the page; descriptors 1 and 2 are the console, through the console driver domain, with `\n` written as `\r\n`; any other descriptor is `-EBADF` |

Any other kind answers `-ENOSYS`. Nothing here is a contract operation, and
nothing here is a path: the program region is a table of names the
supervisor owns, and a process reaches the console because the supervisor's
policy for descriptors 1 and 2 says so.

`boot/posix/hello` and `boot/posix/hostile` are the two programs; each
carries the process side of the protocol in its own `abi.rs` and depends on
nothing in the kernel. `scripts/test-process.sh [arch]` installs both on a
temporary disk and requires the hello program to run twice (the second run
proves the first's frames came back), the hostile one to be contained, an
absent name to be refused, and the workshop to still evaluate afterwards.

### What this stratum does not claim

There is no C library yet: the programs are Rust. There are no files,
descriptors other than the console, namespaces, arguments, environment, or
processes that make processes. The program table names programs but grants
nothing; a process has no capability set beyond the console policy above.
The graphics image does not carry the loader; it has no command that would
reach it. Reclamation of a process's frames is the frame pool's, which the
serial workshop now compiles in.
