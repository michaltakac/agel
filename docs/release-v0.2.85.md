# Agel v0.2.85 — The backend in Agel

The last Rust piece of the toolchain has an Agel counterpart, and it runs
in the guest. `agel/native-x86`, a module of the standard library written
in Agel, turns the IR `native-compile` produces into x86-64 machine code
in a static ELF for the Agel supervisor, as hex text; the loaded runtime
runs it in a protection domain, writes the image to a file, and the
desktop installs and runs the result as a process. Agel read the source,
linked it, compiled it and assembled it, in the OS, and the OS ran what
it made. See [`native-backend.md`](native-backend.md).

## What changed

- **`agel/native-x86`**: an assembler and code generator in about 250
  lines of Agel. It compiles the IR's integer subset — integers, booleans,
  nil, `+ - * / = <`, `if`, `begin`, functions with static links so `let`
  and closures over enclosing frames work, calls through closure values
  with the explicit-self convention — into a process image whose entry
  calls the function on the constant arguments given, prints the result
  as a decimal line through the process protocol and exits with its low
  byte. Code is a binary tree of short leaves, so every pass recurses by
  the program's nesting and never by its length, under the evaluator's
  call-depth budget of 256 without tail-call elimination.
- **The process window is 64 MiB**, from 16, on every machine: the
  runtime with the library holds a few copies of a megabyte world through
  a simple allocator, and the kept-world run of v0.2.83 filled sixteen
  once the library grew by this module. The layout had the room (the
  asset slots begin 256 MiB above the window's base); frames are still
  mapped only as a process asks.
- **`World::from_canonical_over`** consumes its base's state instead of
  copying it: one world fewer in memory when a kept world is restored.

## Proof

Hosted, `crates/agel-stdlib/tests/backend.rs`: the fib IR becomes a
static ELF — magic, 64-bit, `ET_EXEC`, x86-64, the entry past the
headers, the code segment the file, the arena a megabyte above — and a
`cons` or mismatched arguments are refused. In the OS,
`scripts/test-agel-process.sh`: `backend.agel` compiles fib, a `let`
(`(* n (+ k j))` with n = 8), a tail-recursive sum to a hundred and a
division by zero, four images emitted in 4.9 seconds on QEMU's TCG; the
desktop installs each and runs it: `55` and status 55, `40` and 40,
`5050` and 186 (5050 modulo 256), and 111 for the division. The fib image
is 796 bytes, emitted in 209,865 steps. The full regression passes; the
kernel is unchanged in size.

## Not claimed

The backend compiles integer programs to a standalone process; it is not
the JIT, whose Cranelift backend compiles the whole IR with a managed
heap, lists and texts, on the host, and remains the toolchain's full
backend. A tail call is a call and a return, so a loop is bounded by the
process's stack. The self-compiled toolchain — the reader, linker and
compiler themselves compiled by this backend — needs lists and texts it
does not have. Nothing it emits is signed; the loader's ELF rules and the
protection domain are what stand between the emitted code and the
machine, as for any program.
