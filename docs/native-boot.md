# Native boot seed and recovery monitor

Agel v1.0 added a small, reproducible path from a raw disk image to freestanding
Rust on x86-64. v1.1 places a fixed-memory Agel evaluator and transactional REPL
on that substrate while retaining the recovery boundary.

## Boot path

1. The 512-byte BIOS stage loads the remaining 127 sectors at physical
   `0x10000`.
2. It creates identity-mapped four-level page tables for the first GiB.
3. It enables A20, PAE, long mode, protected mode, and paging.
4. It jumps through a 64-bit GDT entry and calls the fixed kernel entry at
   `0x10000`.
5. The `no_std`, `no_main` Rust seed initializes COM1 and starts the native Agel
   workshop. Recovery policy remains separate from the evaluator's world banks.

The linker keeps `.text.entry` first so helper-function reordering cannot move
the address called by the BIOS stage. The raw image is always exactly 128
sectors; the build rejects an oversized kernel.

## Recovery boundary

The monitor has stable A and candidate B states. `promote` is denied until
`verify` records isolated health evidence. Promotion retains A; `fault` models a
watchdog rollback. The normal serial shell supports:

```text
help status verify promote fault agents shutdown
```

Build and enter the Agel REPL with `./scripts/run-qemu.sh`; recovery operations
are colon commands such as `:recovery-status`, `:verify`, and `:fault`.
`./scripts/test-boot.sh`
rebuilds the disk twice, requires byte equality, boots it, and checks a serial
success token. `./scripts/test-monitor.sh` boots a deterministic monitor scenario
and asserts denial, verification, promotion, and rollback.

## Trust boundary

`boot/kernel` is intentionally a separate Cargo workspace. Its only unsafe
operations are x86 port I/O and `cli; hlt`; BIOS transition assembly lives in
`boot/bios`. The main hosted workspace still has `unsafe_code = "forbid"`.

The monitor does not persist slots or verify signatures, and there is no IDT,
allocator, driver model, hardware watchdog, or full agent runtime in the VM.
The native evaluator is intentionally fixed-memory and session-only. The
important invariant remains: mutable language state is not the only component
capable of recovering it.
