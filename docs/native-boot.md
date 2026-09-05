# Native boot seed and recovery monitor

Agel v1.0 added a small, reproducible path from a raw disk image to freestanding
Rust on x86-64. v1.1 places a fixed-memory Agel evaluator and transactional REPL
on that substrate while retaining the recovery boundary.

## Boot path, x86-64

AArch64 and RISC-V need none of this: QEMU's `virt` machine loads an ELF by its
program headers, so those images state where they want to live and start there.
x86-64 keeps the BIOS seed because that is where the project's native work
began, and because a reproducible 128 KiB boot seed is a useful thing to have.

1. The 512-byte BIOS stage loads 254 kernel sectors in two conservative
   127-sector requests beginning at physical `0x10000`.
2. It creates identity-mapped four-level page tables for the first GiB.
3. It enables A20, PAE, long mode, protected mode, and paging.
4. It jumps through a 64-bit GDT entry and calls the fixed kernel entry at
   `0x10000`.
5. The `no_std`, `no_main` Rust seed zeroes `.bss`, initializes COM1, and starts
   the native Agel workshop. Recovery policy remains separate from the
   evaluator's world banks.

The linker keeps `.text.entry` first so helper-function reordering cannot move
the address called by the BIOS stage. The complete raw image is 2,048 sectors
(1 MiB). Sectors 0 through 255 are the replaceable boot seed; the build rejects
an oversized kernel. Sectors 256 through 287 are the two v1.7 workspace slots,
and rebuilding deliberately preserves them.

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

## Three machines, one contract

v1.3 builds the isolation backend for **x86-64, AArch64, and RISC-V from one
source**. The shared half — the capability space, the handshake page, the tick
budget, the conformance driver, the containment driver, and the unprivileged
world program — is architecture-neutral. Address spaces, register frames, trap
entry, and the privilege transition are per-architecture, and that is the whole
of what differs.

```sh
./scripts/build-kernel.sh aarch64    # or x86_64, or riscv64
./scripts/test-isolation.sh          # all three
./scripts/test-isolation.sh riscv64  # or just one
```

| | x86-64 | AArch64 | RISC-V |
|---|---|---|---|
| Platform | BIOS seed, raw 1 MiB disk | QEMU `virt`, ELF | QEMU `virt`, ELF over OpenSBI |
| Supervisor level | ring 0 | EL1 | S-mode |
| Unprivileged level | ring 3 | EL0 | U-mode |
| Trap gate | `int 0x80` | `svc #0` | `ecall` |
| Translation | 4-level, 4 KiB pages | 3-level, 39-bit, 4 KiB | Sv39, 4 KiB |
| Preemption | 8259-routed PIT, 100 Hz | EL1 physical timer via GICv2 | SBI timer, 100 Hz |
| Domain window | 512 GiB | 2 GiB | 4 GiB |

Each architecture's domain window sits in a different top-level table entry from
the kernel and device windows, so two domains do not merely fail to reach each
other's memory — they have no translation for it.

RISC-V is the one backend that is not alone on its machine: OpenSBI runs in
machine mode beneath it, owns the timer, and constrains what S-mode may touch
through physical memory protection. That is a useful reminder of what the whole
exercise is about, with the kernel on the receiving end of the arrangement.

## Protection domains

v1.2 added the isolation layer the roadmap's Phase 1 calls for, built and tested
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

`./scripts/test-isolation.sh` boots each architecture and requires that an
unprivileged world answers all 81 steps of the kernel-contract corpus with a
transcript byte-identical to `bootstrap/kernel-contract.trace`; that a world
writing to kernel memory, a world executing something it is not allowed to, and
a world that never yields are each contained with the fault that machine
actually produces; that the native evaluator performs persistent definitions,
recursion, and transactional rollback in the lowest privilege level; and that
the recovery monitor still denies, verifies, promotes, and rolls back
afterwards. The linker isolates evaluator code in `.user_text`, immutable data
is user-readable but non-writable/non-executable, and the live corpus fails on
any call that escapes those mappings.

The fault vocabulary is shared but the mapping is not flattened. x86-64
distinguishes four causes and is provoked four ways. AArch64 has no integer
divide exception, and its privileged-instruction case reads the physical timer's
control register — the first move a world would make towards disabling its own
preemption — which `CNTKCTL_EL1` denies EL0. RISC-V genuinely cannot tell a
privileged instruction from an undefined one; both raise *illegal instruction*,
and the test says so rather than inventing a distinction the architecture does
not make.

## Trust boundary

`boot/kernel` is intentionally a separate Cargo workspace, and the main hosted
workspace still has `unsafe_code = "forbid"`. Privileged instructions are
confined to `boot/kernel/src/hal.rs`; BIOS transition assembly lives in
`boot/bios`.

Since v1.6, `./scripts/run-qemu.sh` boots an x86-64 interactive workshop whose
evaluator lives on a private 512 KiB bounded domain stack and whose output goes
through the v1.5 console domain. The same evaluator path is tested on AArch64
and RISC-V. Serial input still terminates in the supervisor, and seL4 still runs
only the frozen contract. v1.7 adds alternating, checksummed native source-image
slots and boot-time replay, but the ATA mechanism remains supervisor code and
the images are not signed. There is no allocator, hardware watchdog, full agent
runtime, or frame reclamation in the VM. Mutable language state is nevertheless
no longer the component responsible for recovering itself.
