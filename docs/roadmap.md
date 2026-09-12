# Roadmap

What Agel has, what it is in the middle of, and what it has not begun. No
dates and no ordering beyond dependence: an item is **done** only when the
code exists on every surface the line names and a test in this repository
proves it; **partial** names exactly what is missing; **open** means no
code. Each release moves lines here; the release-by-release history is in
[`versioning.md`](versioning.md) and the trust story in
[`threat-model.md`](threat-model.md).

## The language and the hosted runtime

| Line | Status | Proof and limits |
|---|---|---|
| Homoiconic reader, evaluator, atomic batches, versioned worlds with rollback | done | `cargo test --workspace`, `scripts/test-bootstrap.sh` |
| Lexical closures, hygienic template macros, modules, conditions and restarts | done | [`language-core.md`](language-core.md) |
| Capability-scoped effects with deterministic budgets | done | [`effect-sandbox.md`](effect-sandbox.md) |
| Agents with isolated heaps, typed protocols, supervision, event history, snapshots and replay | done | [`agent-runtime.md`](agent-runtime.md) |
| Transactional model requests to external providers with an audit log | done | [`model-agents.md`](model-agents.md); external providers only |
| Ed25519-signed portable images with a tamper-evident chain | done | `cargo run -q -p agel-image --example portable_image` |
| Evidence-carrying upgrades: a proposal runs in a zero-authority canary and is promoted or discarded atomically | partial | `cargo run -q -p agel-verify --example safe_upgrade`; the digest binding evidence to a world state is over a debug rendering of the state, not a canonical encoding |
| Standard library, metacircular evaluator, UI model, layout engine and vector renderer written in Agel | done | `scripts/test-kitchensink.sh`, `scripts/test-fixed-point.sh` |
| Managed JIT with an opt-in integer path | done | `cargo run --release -p agel-jit --example self_host` |
| The hosted agent runtime running inside a protection domain | open | the native evaluator runs unprivileged; the hosted runtime's effects, model adapters and rich protocols have not moved |
| The compiler and reader running in the guest | partial | the native reader exists; the compiler does not fit the native bounds, so compilation stays on the host; function-valued captures in native closures are refused |

## The kernel contract and its backends

| Line | Status | Proof and limits |
|---|---|---|
| Contract v1.1 (`core`, `capability`, `endpoint`, `notification`, `clock`, `memory`) with a reference model and a 118-step corpus | done | `scripts/test-kernel-contract.sh` |
| Three research kernels (x86-64, AArch64, RISC-V) answering the corpus byte-identically from ring 3 | done | `scripts/test-isolation.sh` |
| A second, independent implementation of the contract on an unmodified seL4 under Microkit | done | `scripts/test-sel4.sh`; publishes v1.0 (no `memory` group under Microkit's static mappings) |
| The `domain` and `interrupt` groups | open | declared in the contract, in no backend's profile |
| Timers, networking and model brokering as contract operations | open | today these are the supervisor's |
| Running a verified seL4 configuration | open | Microkit ships MCS kernels whose proofs are ongoing |

## Isolation, drivers and recovery

| Line | Status | Proof and limits |
|---|---|---|
| Per-domain address spaces, write-xor-execute, trap gate, preemption timer; containment of worlds that fault, run privileged instructions, touch ungranted devices or never yield | done | `scripts/test-isolation.sh` on three machines |
| Restartable driver domains with generations and fail-closed stale handles | done | console, serial, keyboard, disk, filesystem, clock, compositor |
| Storage drivers in domains: ATA, virtio-blk, SD host controller | done | `scripts/test-native-persistence.sh`, `scripts/test-raspi4.sh` |
| Workspace saved to alternating checksummed disk slots, replayed after reboot, surviving a power cut at any write | done | `scripts/test-power-cut.sh` on three machines |
| A/B kernel slots with signed candidates, a three-boot budget, watchdog rollback | partial | `scripts/test-kernel-rollback.sh`; x86-64 only, and only candidate kernels are signed: the trusted slot, the selector, workspace generations and the recovery record are not, and there is no root of trust before the BIOS stage |
| A signed seL4 manifest and program region | open | the manifest is regenerated and diffed in CI but unsigned; the program region has a CRC only |

## The workshop

| Line | Status | Proof and limits |
|---|---|---|
| Serial workshop on three machines: named cells, `:save`, replay after reboot | done | `scripts/test-native-repl.sh [ARCH]` |
| Graphical workshop with the host-side console for layout composition and paste | done | `scripts/test-graphical-workshop.sh`, `scripts/test-live-keyboard.sh` |
| Native agents with mailboxes, deterministic scheduling and contained faults | done | `scripts/test-native-agents.sh` |
| Native modules loaded at runtime | done | `scripts/test-native-modules.py` |
| The workshop as one window among others on the desktop | open | it is a fixed terminal panel |

## The desktop

| Line | Status | Proof and limits |
|---|---|---|
| 1920×1080 compositor domain holding the framebuffer as its only device mapping | done | `scripts/test-graphics.sh` (frozen digest); x86-64 and the Pi 4 under QEMU |
| COSMIC palette, radii and spacing; Fira Sans and Fira Mono from disk atlases; surfaces with soft shadows and edges | done | `scripts/test-desktop-process.sh`; the shadow is a radial sum, not a Gaussian; no translucency |
| Panel with launcher and clock, dock, hover and press states, terminal panel | done | `scripts/test-native-dock.py`, `scripts/test-native-workbench.py` |
| Windows a process asks for and draws into through checked records; move, stack, minimize, maximize, corner resize; press, key, motion, release and resize events | done | `scripts/test-desktop-process.sh` |
| Keyboard shortcuts and window snapping | open | |
| More than one running program at a time on the desktop | open | one program steps between inputs; the process table holds four |
| A Gaussian blur and translucent panels | open | needs the compositor to read back what is beneath |
| SIMD or accelerated rendering | open | every pixel is written by safe Rust in a domain |

## The POSIX personality

Strata are defined in [`posix-personality.md`](posix-personality.md); the
tests run on all three research machines unless a line says otherwise.

| Line | Status | Proof and limits |
|---|---|---|
| Processes loaded from the disk into protection domains | done | `scripts/test-process.sh` |
| Files through a namespace granted per process, served by an unprivileged filesystem service; a path is never authority | done | `scripts/test-files.sh` |
| Descriptors that fail closed after a service restart (`ESTALE`) | partial | implemented, never exercised: no test restarts the service while a descriptor is held |
| `agel-libc` so C programs build from source and run; stdio with a full integer formatter and scanner; `getopt`; a free-list heap that grows at the break | done | `scripts/test-libc.sh`, `scripts/test-breadth.sh` |
| `spawn` with an explicit capability set (no `fork`), pipes, `wait`, `kill` of a child | done | `scripts/test-spawn.sh` |
| `stat`, `rename`, `unlink`, `mkdir`, directories, `chdir`, `truncate`, files up to 64 KiB | done | `scripts/test-libc.sh` |
| Monotonic clock, `sleep`, `setjmp`, environment | done | `scripts/test-libc.sh` |
| A window protocol for C programs | done | `scripts/test-desktop-process.sh` on x86-64; `-ENODEV` without a display |
| Calendar time, signal handlers, `alarm` | open | only `SIGKILL` to a child exists |
| Floating point in the library and `math.h` | open | processes run without an FPU |
| `mmap`, shared memory, threads, locale, `%[` in the scanner | open | |
| A filesystem region larger than 256 KiB, files larger than 64 KiB | open | sectors 1536–2047, 63 blocks of 4 KiB, 16 blocks per file |
| Binary compatibility with Linux ELF programs | open | not planned; source compatibility is the target |

## Boards

| Line | Status | Proof and limits |
|---|---|---|
| Raspberry Pi 4 under QEMU: flat image at 0x80000, EL2 entry, SD card, workshop and desktop | done | `scripts/test-raspi4.sh`, `scripts/test-raspi4-desktop.sh`; skipped where QEMU lacks `raspi4b` |
| Raspberry Pi 4 on hardware | open | never run on a board |
| Raspberry Pi 5 | partial | `kernel_2712.img` builds and lints from documented addresses; never run, no emulator models it |
| USB keyboard and mouse on the Pi | open | no host controller driver; input on the board is the serial console |
| One image serving both boards from the device tree | open | addresses are compiled in per board |

## Not started

- **Local inference.** CPU inference over quantized weights in a domain, in
  safe Rust, with no proprietary kernel-mode driver. External providers are
  the only model access today.
- **A network stack.** No driver, no protocol, no socket in the POSIX
  personality.
- **Firecracker as an outer envelope** for hosted deployment.
- **Training.** Out of scope for the operating system by decision; see
  [`deployment-targets.md`](deployment-targets.md).
