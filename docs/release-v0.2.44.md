# Agel v0.2.44 — The C library: the third POSIX stratum

C programs now build for Agel from source and run as processes. `agel-libc`
is a `no_std` Rust crate built as a static archive with a C ABI, with the
headers a C program includes; clang compiles the program, lld links it with
the same script as the Rust programs, and the supervisor loads it into a
protection domain like any other.

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

## What changed

- **`agel-libc`** under `boot/posix/libc`: `read`, `write`, `open`,
  `close`, `_exit`, `exit`, `abort`, `errno`, a 64 KiB bump-arena `malloc`
  with `calloc`, `realloc` and a `free` that frees nothing, the `mem*` and
  `str*` routines written as volatile byte loops, `puts`, `putchar`, and a
  `printf` subset (`%s %d %i %u %x %c %p %%`, `l` and `z`) in C because a
  variadic definition is not stable Rust. Its `_start` keeps the shared
  page, calls `main` and exits with its answer. Headers: `unistd.h`,
  `fcntl.h`, `errno.h`, `stdlib.h`, `string.h`, `stdio.h`.
- **Programs in C.** `boot/posix/c/hello.c` and `cat.c`;
  `scripts/build-c-program.sh NAME [arch]` builds one with clang and lld for
  any of the three machines, without floating point or vector code, since a
  process has no such state.
- **Linker scripts that place the GOT.** Position-independent C on x86-64
  carries a GOT; the scripts put it and `.data.rel.ro` in the writable
  segment so the loader's no-shared-page rule holds for C as for Rust.
- **Toolchain.** CI installs `lld`; on macOS Homebrew's `llvm` and `lld`
  are needed because Apple's clang has neither the RISC-V backend nor lld.

## Proof

`scripts/test-libc.sh [arch]` builds both programs from source and runs
them on x86-64, AArch64 and RISC-V: `printf` with every conversion the
program uses, `malloc`, `strcpy`, `strlen`, `main`'s return as the exit
status; `cat` rooted at `app` reading `notes` to the end through `open`,
`read`, `write`, `close`, and at the root reporting `errno` 2 and exiting
with it. CI runs all three.

## Not claimed

This is the foundation of source compatibility, not its breadth: no
streams, `fopen`, `scanf`, `sprintf`, `time`, `signal`, `math`,
environment or `argv`; `free` frees nothing; `printf` is a subset. The
library is not audited against a C standard or a conformance suite. A
process still cannot make a process; that is stratum 3.
