# Native boot seed and recovery monitor

Agel v1.0 adds a small, reproducible path from a raw disk image to freestanding
Rust on x86-64. It exists to make the trust boundary executable early; it is not
yet the hosted Agel runtime transplanted into a VM.

## Boot path

1. The 512-byte BIOS stage loads the remaining 127 sectors at physical
   `0x10000`.
2. It creates identity-mapped four-level page tables for the first GiB.
3. It enables A20, PAE, long mode, protected mode, and paging.
4. It jumps through a 64-bit GDT entry and calls the fixed kernel entry at
   `0x10000`.
5. The `no_std`, `no_main` Rust seed initializes COM1 and starts the recovery
   monitor.

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

Build and enter it with `./scripts/run-qemu.sh`. `./scripts/test-boot.sh`
rebuilds the disk twice, requires byte equality, boots it, and checks a serial
success token. `./scripts/test-monitor.sh` boots a deterministic monitor scenario
and asserts denial, verification, promotion, and rollback.

## Trust boundary

`boot/kernel` is intentionally a separate Cargo workspace. Its only unsafe
operations are x86 port I/O and `cli; hlt`; BIOS transition assembly lives in
`boot/bios`. The main hosted workspace still has `unsafe_code = "forbid"`.

The v1.0 monitor does not persist slots or verify signatures, and there is no
IDT, allocator, driver model, Agel evaluator, or hardware watchdog in the VM.
Those are subsequent native rungs. The important v1 invariant is already real:
mutable agent code cannot be the only component capable of recovering it.
