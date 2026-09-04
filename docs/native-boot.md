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
5. The `no_std`, `no_main` Rust seed zeroes `.bss`, initializes COM1, and starts
   the native Agel workshop. Recovery policy remains separate from the
   evaluator's world banks.

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

## Ring-3 protection domains

v1.2 adds the isolation layer the roadmap's Phase 1 calls for, built and tested
under `--features isolation-selftest`:

- the kernel replaces the BIOS's single supervisor mapping with page tables it
  owns, and gives every protection domain its own root at a distinct top-level
  slot, so two domains have no translation for each other's memory;
- `.user_text` is the only range of the image marked user-executable, and it is
  never writable; domain stacks and shared pages are writable and never
  executable, with `EFER.NXE` enabled so that promise is enforced;
- a GDT with ring-3 descriptors and a TSS, an IDT covering every architectural
  exception plus the timer and one ring-3-callable trap gate, and a second stack
  reached through IST for double faults;
- the 8259s are remapped off the exception vectors and the PIT runs at 100 Hz,
  so a domain that never yields is preempted; and
- ring 0 runs with interrupts masked throughout. They are only ever enabled by
  entering ring 3 with a frame whose flags set `IF`.

`./scripts/test-isolation.sh` boots the result and requires that an
unprivileged world answers all 81 steps of the kernel-contract corpus through
`int 0x80` with a transcript byte-identical to
`bootstrap/kernel-contract.trace`; that worlds writing to kernel memory,
dividing by zero, and masking interrupts are contained with the expected
architectural vector; that a world which never yields is preempted by budget;
and that the recovery monitor still denies, verifies, promotes, and rolls back
afterwards. It also rejects the image if the built `.user_text` contains a call
or indirect branch, since either would leave the user-executable range.

## Trust boundary

`boot/kernel` is intentionally a separate Cargo workspace, and the main hosted
workspace still has `unsafe_code = "forbid"`. Privileged instructions are
confined to `boot/kernel/src/hal.rs`; BIOS transition assembly lives in
`boot/bios`.

The default REPL image still runs the Agel evaluator in ring 0. The isolation
machinery exists and is tested, but moving the evaluator into a domain is the
next rung, not a claim this release makes. There is still no allocator, driver
model, hardware watchdog, persisted A/B slots, signature verification, or agent
runtime in the VM, and the frame allocator never reclaims. The important
invariant remains: mutable language state is not the only component capable of
recovering it.
