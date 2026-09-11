# Native boot seed and recovery monitor

Agel v0.1.0 added a small, reproducible path from a raw disk image to freestanding
Rust on x86-64. v0.1.1 places a fixed-memory Agel evaluator and transactional REPL
on that substrate while retaining the recovery boundary.

## Boot path, x86-64

AArch64 and RISC-V need none of this: QEMU's `virt` machine loads an ELF by its
program headers, so those images state where they want to live and start there.
x86-64 keeps the BIOS seed because that is where the project's native work
began, and because a reproducible 256 KiB boot seed is a useful thing to have.

1. The 512-byte BIOS stage reads the kernel slot selector at sector 1057,
   charges an unverified candidate one boot and writes the selector back, and
   loads 508 kernel sectors from the chosen slot in four conservative
   127-sector requests beginning at physical `0x10000`. It leaves the chosen
   slot at `0x6fec` behind a marker at `0x6fe8` for the kernel.
2. It creates identity-mapped four-level page tables for the first GiB.
3. It enables A20, PAE, long mode, protected mode, and paging.
4. It jumps through a 64-bit GDT entry and calls the fixed kernel entry at
   `0x10000`.
5. The `no_std`, `no_main` Rust seed zeroes `.bss`, initializes COM1, and starts
   the native Agel workshop. Recovery policy remains separate from the
   evaluator's world banks.

The linker keeps `.text.entry` first so helper-function reordering cannot move
the address called by the BIOS stage. The x86-64 image is 6,144 sectors
(3 MiB), laid out as follows since v0.2.42 with the asset region added at
v0.2.47; the virtio disks of the AArch64 and RISC-V machines are 3,072
sectors, have no BIOS stage, kernel slots or assets, and keep everything from
sector 1024 to 3071 in the same place.

| Sectors | Contents |
|---|---|
| 0 | the BIOS stage |
| 1–508 | kernel slot A; with sector 0 it is the replaceable boot seed, and the build rejects a kernel over 508 sectors (260,096 bytes) |
| 512–1019 | kernel slot B |
| 1024–1055 | the two v0.1.7 workspace slots, 16 sectors each |
| 1056 | the v0.2.29 recovery record |
| 1057 | the v0.2.30 kernel slot selector |
| 1536–2047 | reserved for the filesystem the POSIX personality's next stratum adds |
| 2048–3071 | the v0.2.41 program region: a table sector and static ELF images |
| 3072–6143 | the v0.2.47 asset region: a table sector, the compositor's font atlases and, since v0.2.48, its sprite sheet; the x86-64 image is 6,144 sectors (3 MiB) to hold it |

Rebuilding installs the new kernel as slot A, clears the selector so that
kernel is what boots, and preserves everything else. Before v0.2.42 a kernel
slot held 254 sectors, the workspace slots began at sector 256, the records
were sectors 288 and 289, slot B was 290–543 and the program region 1024–2047;
an image from before then is not read by this layout and has to be rebuilt,
which `./scripts/build-boot.sh` does by growing the file and installing the
new seed.

## Kernel slots

The selector's first ten bytes are what the 16-bit stage parses, without a
checksum: magic `AGKS`, version, trusted slot, candidate slot (`0xff` for
none), boot attempts, a verified flag and an admitted flag. The stage loads
the trusted slot unless a candidate is present, admitted, and either verified
or still within its budget of three boots; a boot of an unverified candidate
is counted and flushed to disk before the candidate's first instruction runs,
so a kernel that halts, faults or never reaches the serial console cannot
avoid the charge.

Admission is the running kernel's decision, since v0.2.32. The selector also
carries the candidate's signed length (bytes 12-15) and an Ed25519 signature
(bytes 64-127) over the SHA-512 of the slot's bytes. On every boot the kernel
hashes a staged, unadmitted candidate sector by sector through the storage
driver domain and verifies the signature against the public key it was built
with, `bootstrap/kernel-signing.pub`. A valid signature sets the admitted
flag; anything else clears the candidate, and the stage never loads it. The
key pair in `bootstrap/` is a development key checked into the repository:
anyone with the repository can sign for kernels built from it, so an operator
who means it generates their own with
`cargo run -p agel-integrity --example kernel-sign -- keygen KEY`, puts the
public half in `bootstrap/kernel-signing.pub`, and rebuilds.

```sh
./scripts/stage-kernel.py target/boot/agel-v1.img some-kernel.bin              # signed with bootstrap/kernel-signing.key
./scripts/stage-kernel.py target/boot/agel-v1.img some-kernel.bin --key KEY    # signed with KEY
./scripts/stage-kernel.py target/boot/agel-v1.img some-kernel.bin --unsigned   # refused at the next boot
./scripts/stage-kernel.py target/boot/agel-v1.img --status
```

The running kernel reads the selector through the storage driver domain. Its
first successful evaluation marks a booted candidate verified and prints
`kernel slot B verified by a healthy boot`; `:kernel-status` reports the
slots, `:kernel-promote` makes a verified candidate the trusted slot and names
the slot retained for rollback, and `:kernel-fault` gives the candidate up so
the next boot loads the trusted slot. When the stage has fallen back, the boot
log begins with `watchdog fault: candidate kernel slot A failed 3 boots;
booted trusted slot B`. `./scripts/test-kernel-rollback.sh` proves all of it,
including three boots of a kernel that halts at its entry point.

Staging is a host tool because the guest has no compiler for its own kernel.
The trusted slot and the selector's own bytes are not signed: a disk that
lies about slot A chooses the kernel, and nothing checks the kernel before
the BIOS stage runs it. What the signature settles is that no kernel reaches
slot B, or replaces a trusted kernel, without the key the running kernel
trusts.

## Recovery boundary

The policy model has stable A and candidate B states. `promote` is denied
until `verify` records isolated health evidence. Promotion retains A; `fault`
models a watchdog rollback. On x86-64 since v0.2.29 the same commands act on a
disk-backed record binding those states to workspace generations, with a boot
budget that rolls a failing candidate back automatically; see
[`native-workshop.md`](native-workshop.md). The normal serial shell supports:

```text
help status verify promote fault agents shutdown
```

Build and enter the Agel REPL with `./scripts/run-qemu.sh`; recovery operations
are colon commands such as `:recovery-status`, `:verify`, and `:fault`.
`./scripts/test-boot.sh`
rebuilds the disk twice, requires byte equality, boots it, and checks a serial
success token. `./scripts/test-power-cut.sh [aarch64|riscv64]` cuts the
power at every sector write of a workspace save and requires each reboot to
find a whole generation. `./scripts/test-monitor.sh` boots a deterministic monitor scenario
and asserts denial, verification, promotion, and rollback.

## Three machines, one contract

v0.1.3 builds the isolation backend for **x86-64, AArch64, and RISC-V from one
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
| Disk | primary ATA, nine I/O ports | virtio-blk, one MMIO page + one DMA frame | virtio-blk, one MMIO page + one DMA frame |
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

## Storage on the machines without a BIOS

The `virt` machines have no ATA controller. Since v0.2.33 their storage driver
domain drives a virtio block device: the supervisor scans the machine's
virtio-mmio transports for a block device behind a modern (version 2)
transport, maps that one page of registers and one freshly allocated DMA frame
into the driver domain, and tells the driver the frame's physical address
through the shared page. The driver acknowledges the device, negotiates the
modern feature bit and flush, places one four-entry queue in its DMA frame, and
moves single sectors through it by polling the used ring, bounded like every
other wait in a driver: the domain's three-second tick budget is the hard
bound, and the driver's own poll count is sized to report a timeout inside it. It holds no policy and reaches nothing else; a world
that was not granted the device page faults on it, and the isolation suite
asserts that on both machines. QEMU exposes legacy transports unless started
with `-global virtio-mmio.force-legacy=false`; the scripts pass it, and a
legacy device is reported as absent rather than driven wrongly. The
supervisor stack on these machines grew from 64 KiB to 512 KiB to hold the
workshop's bounded workspaces, matching x86-64.

## Protection domains

v0.1.2 added the isolation layer the roadmap's Phase 1 calls for, built and tested
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
unprivileged world answers all 118 steps of the kernel-contract corpus with a
transcript byte-identical to `bootstrap/kernel-contract.trace`, the v1.1
profile with a frame window the page tables make real; that a world
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

`boot/kernel` is intentionally a separate Cargo workspace. The main hosted
workspace forbids `unsafe` in every crate except the optional `agel-jit`
backend, which sets `unsafe_code = "deny"` and carries four audited
`#[allow(unsafe_code)]` sites for entering generated code and releasing
executable memory; see [`integer-jit.md`](integer-jit.md). Privileged
instructions are confined to the per-architecture
`boot/kernel/src/arch/{x86_64,aarch64,riscv64}/hal.rs`; BIOS transition
assembly lives in `boot/bios`.

Since v0.1.6, `./scripts/run-qemu.sh` boots an x86-64 interactive workshop whose
evaluator lives on a private 512 KiB bounded domain stack and whose output goes
through the v0.1.5 console domain, and since v0.2.27 reads its serial input
through that same domain. Since v0.2.28 `./scripts/run-qemu.sh aarch64` and
`riscv64` boot the same interactive workshop on those machines, and since
v0.2.33 with a disk: a virtio block device behind QEMU's virtio-mmio transport,
driven from an unprivileged domain. Since v0.2.38 the seL4 world domain also
runs the native evaluator, over the same forms the research kernels check. v0.1.7 adds alternating, checksummed native source-image
slots and boot-time replay; since v0.2.26 the ATA driver is an unprivileged,
restartable domain granted exactly the disk's ports, while the slot policy and
codec stay in the supervisor and the images are not signed. There is no general allocator, hardware watchdog or full agent
runtime in the VM; since v0.2.35 a replaced domain's frames are reclaimed. Mutable language state is nevertheless
no longer the component responsible for recovering itself.
