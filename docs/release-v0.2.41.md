# Agel v0.2.41 — Processes: the first POSIX stratum

Every world Agel has run so far was code the kernel was built with. This
release loads code from the disk into a protection domain: the first stratum
of the POSIX personality, with the plan for the rest written down in
[`docs/posix-personality.md`](posix-personality.md).

```text
agel-native[0]> :exec hello
hello from a loaded process
process hello exited with status 42
agel-native[0]> :exec hostile
hostile process about to write where it may not
process hostile faulted: page-fault at 0x9000002c touching 0x10; contained
```

## What changed

- **A program region on the disk.** Sectors 1024 through 2047 hold a table
  and static ELF images; `scripts/install-program.py` writes them and
  `scripts/build-program.sh` builds the programs under `boot/posix` for any
  of the three machines.
- **A loader that trusts nothing.** The image's CRC is checked against its
  table row; the ELF must be a static executable for this machine whose
  segments lie inside the process window, are page-congruent, share no
  page and are never writable and executable; each page is a frame recorded
  with the domain and filled before the domain runs.
- **A process protocol.** A process is entered with its shared page in the
  first argument register and asks for things by writing a request block
  and yielding exactly as a driver does. `write` to the console and `exit`
  exist; everything else is `-ENOSYS`.
- **Proved on three machines.** `scripts/test-process.sh [arch]` runs the
  hello program twice, so its frames must have come back, contains the
  hostile one, refuses an absent name, and checks the workshop still
  evaluates afterwards. CI runs it on x86-64, AArch64 and RISC-V.

## Verification

```sh
./scripts/test-process.sh
./scripts/test-process.sh aarch64
./scripts/test-process.sh riscv64
```

## What this does not claim

No C library, no files, no namespaces, no arguments, no processes that make
processes: those are the next strata. A process can write to the console
because the supervisor's policy says so, not because it holds a capability.
The program region is unsigned. The graphics image does not carry the
loader.
