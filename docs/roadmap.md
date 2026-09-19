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
| Evidence-carrying upgrades: a proposal runs in a zero-authority canary and is promoted or discarded atomically | done | `cargo run -q -p agel-verify --example safe_upgrade`; since v0.2.81 the digest binding evidence to a world state is SHA-256 over a canonical, versioned, length-delimited encoding of the state (`crates/agel-core/src/canon.rs`), pinned by a test vector |
| Standard library, metacircular evaluator, UI model, layout engine and vector renderer written in Agel | done | `scripts/test-kitchensink.sh`, `scripts/test-fixed-point.sh` |
| Managed JIT with an opt-in integer path | done | `cargo run --release -p agel-jit --example self_host` |
| The hosted agent runtime running inside a protection domain | done | `scripts/test-agel-process.sh`: `agel-core` without `std` and the standard library run as a loaded process (`:exec agel -- FILE`) with the desktop's effect words (`file-read`, `file-write`, `file-append`, `file-list`, `clock`, `console-log`, `exec`) over the process protocol, each behind a capability the evaluator checks, so an agent spawned without one cannot write a file; model requests stay pending for a host bridge as the desktop's do, and images remain a host tool |
| Room in the native evaluator for programs of kilobytes: 96 globals, 4,096 heap cells, 16 KiB of text, 10,000 steps, 48 call levels, 32 agents | done | `scripts/test-native-repl.sh`: recursion past the old depth, text of thousands of bytes, more globals than a postcard held; the evaluator domain keeps its own 4 MiB stack, moved clear of the shared page |
| Programs from files: the desktop loads Agel from its own filesystem (`:load-file`, `:load NAME` as `/NAME.agel`, `/init.agel` at boot), and an Agel form can write a program and load it | done | `scripts/test-programs.sh`: a form writes `tool.agel`, `:load-file` runs it, a failing form stops a load with the forms before it kept, and `/init.agel` runs at the next boot |
| A session in the OS: the loaded runtime reads the console line by line, each line a transaction in one world, until `:eof`; with `--world NAME` the world is kept in a file and continued by the next run | done | `scripts/test-agel-process.sh`: `:exec agel` answers `PROCESS READING`, lines typed at the desktop's prompt are the program's, a failed transaction leaves the world as it was, `:eof` ends it; `--world kept.agel` writes a delta over the library after every transaction and the next run reads it back at its revision, agent and mailbox included |
| Programs installed from files: `:install NAME PATH` puts hex text the OS can read into the program region, where `:exec NAME` runs it | done | `scripts/test-install.sh`: the hello program's ELF as hex in the data region becomes `hello2`, runs with its status, is replaced in place, and a missing file, a bad name or text that is not hex is refused before the table changes |
| A backend written in Agel, in the guest: the native IR's integer subset to x86-64 machine code in a static ELF the OS installs and runs | done | `scripts/test-agel-process.sh`: `agel/native-x86` compiles fib, a `let`, a tail-recursive sum and a division by zero in the loaded runtime, `:install` puts each image in the program region and `:exec` runs it with the printed result and status; not the JIT's coverage (no lists, texts or maps), supported tail calls now reuse frames (see the next row) |
| Proper tail calls in the hosted evaluator: a call in tail position — an `if` branch, the last form of `begin`, `let` or a body — runs in the caller's frame; call depth counts non-tail nesting only, fuel bounds loops | done | `crates/agel-core/tests/language_core.rs` (`proper_tail_calls`): a hundred-thousand-step loop, mutual recursion, tail calls through `begin` and `let`, a handler catching what a tail call signals, non-tail recursion still bounded at 256, identical fuel accounting |
| Real tail calls in the backend written in Agel: a tail call through a parameter reuses the frame, a `let` is slots of the frame it appears in, so a loop runs in constant stack | done | `scripts/test-agel-process.sh`: `agel/native-x86` compiles a million-iteration tail loop through a `let` (`(loop 1000000 0)` → 500000500000) and the OS runs it in one frame; the callee pops its own block (`ret 8(arity+1)`), and a tail call whose callee takes more arguments than the frame is a plain call ([`research/decisions-2026-09.md`](research/decisions-2026-09.md)) |
| Owned closure captures and bounded allocation in the Agel-written backend | done | `scripts/test-agel-process.sh`: returned/transitive/independent captures, captured arguments across tail reuse, exact arena bounds, checked dynamic calls and numeric operands; a bounded bump arena, no collector ([v0.2.90](release-v0.2.90.md)) |
| A collector for the backend's emitted code: a Cheney semispace per agent with a shadow stack for roots, at safepoints and commit, and with it lists and texts in the IR the guest compiles | open | the backend is integer-only with a bump arena; this is what lets the guest compile `agel/meta` ([`research/gc.md`](research/gc.md)) |
| Fuel inside emitted code, with an independent IR-interpreter conformance corpus | done | `scripts/test-agel-process.sh`: fourteen exact-budget/N−1 comparisons, infinite tail loops, and fault ordering; `native-x86-emit-limited` sets the budget, the existing entry point defaults to 50 million, exhaustion exits 112 ([v0.2.89](release-v0.2.89.md)) |
| Portable source-level fuel accounting between the hosted evaluator and compiled execution | open | IR metering counts lowered nodes, including synthetic `begin` and `fn` nodes; it is not the hosted source evaluator's `steps_used` or the managed Rust JIT's cost model |
| Persistent, structurally shared collections for the hosted runtime, so a transaction's copy of the world is cheap | open | a world with the library is a megabyte and is cloned per transaction and per agent turn |
| A claims/wishes/`when` store with provenance — who asserts a fact, since when, on what evidence — as an Agel library over agents | open | the event log records what happened, not who currently claims what ([`research/realtalk-live-systems.md`](research/realtalk-live-systems.md)) |
| Agel Slang: a restricted subset in which the evaluator itself is written, run interpreted, compiled by the guest backend, boot-imaged, each step proven by byte-equal digests against the Rust reference | open | the frontend and backend run in the guest; the evaluator is Rust ([`research/ai-native-rust-selfhosting.md`](research/ai-native-rust-selfhosting.md)) |
| The compiler and reader running in the guest | done | `scripts/test-agel-process.sh`: the loaded runtime reads a module bundle from the data region with `native-read`, links it with `native-link`, compiles it with `native-compile` to the native IR, and writes the linked definition, which the desktop's evaluator loads and runs; the adapted definition is byte-equal to the host toolchain's. The machine-code backend (Cranelift) stays on the host, and the fixed native evaluator still cannot hold the compiler: the loaded runtime can |

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
| More than one running program at a time on the desktop | done | `scripts/test-desktop-process.sh`: two sketches and a chart; the table holds four processes in all; a program that only computes is handed the prompt back after 256 passes and stepped between inputs (v0.2.71) |
| A Gaussian blur and translucent panels | open | needs the compositor to read back what is beneath |
| SIMD or accelerated rendering | open | every pixel is written by safe Rust in a domain |

## The POSIX personality

Strata are defined in [`posix-personality.md`](posix-personality.md); the
tests run on all three research machines unless a line says otherwise.

| Line | Status | Proof and limits |
|---|---|---|
| Processes loaded from the disk into protection domains | done | `scripts/test-process.sh` |
| Files through a namespace granted per process, served by an unprivileged filesystem service; a path is never authority | done | `scripts/test-files.sh` |
| Descriptors that fail closed after a service restart (`ESTALE`) | done | `scripts/test-stale.sh`: the `stale` program holds a descriptor while the desktop's `:fs-restart` replaces the service; its next read answers 116 and it exits 116, and the file is read again by a fresh descriptor |
| `agel-libc` so C programs build from source and run; stdio with a full integer formatter and scanner; `getopt`; a free-list heap that grows at the break | done | `scripts/test-libc.sh`, `scripts/test-breadth.sh` |
| `spawn` with an explicit capability set (no `fork`), pipes, `wait`, `kill` of a child | done | `scripts/test-spawn.sh` |
| `stat`, `rename`, `unlink`, `mkdir`, directories, `chdir`, `truncate`, files up to 64 KiB | done | `scripts/test-libc.sh` |
| Monotonic clock, `sleep`, `setjmp`, environment | done | `scripts/test-libc.sh` |
| A window protocol for C programs | done | `scripts/test-desktop-process.sh` on x86-64; `-ENODEV` without a display |
| Calendar time, signal handlers, `alarm` | open | only `SIGKILL` to a child exists |
| Floating point in the library and `math.h` | done | `scripts/test-libc.sh` on x86-64 and AArch64; not on RISC-V (no unit); no floating point in `printf` or `scanf` |
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

## Does it run DOOM?

The programme in [`doom.md`](doom.md): the game as a POSIX process on
Agel's kernel, an agent that plays it, the data it leaves and a model
trained on it.

| Line | Status | Proof and limits |
|---|---|---|
| A canvas: a window record backed by pages a process draws and the compositor blits, scaled | done | `scripts/test-desktop-process.sh` reads the program's pixels back; one canvas per window, 640×400 at most, no partial damage |
| Key press and release events with key codes | done | `scripts/test-desktop-process.sh` reads `keys.c`; the serial console remains characters only |
| Room: a bitmap frame ledger and pool, a 32 MiB disk, a 4 MiB program region, a data region of large read-only files under `/data` | done | `scripts/test-isolation.sh` counts the frames back; `scripts/test-libc.sh` reads a 100 KB data file by digest on three machines; `agelfs` files stay 64 KiB |
| The C library's missing functions (`strcasecmp`, `fseek`/`ftell`, `remove`, `atof`, `math.h`) and floating point for processes | done | `scripts/test-libc.sh` runs `float.c` on x86-64 and AArch64; RISC-V has no unit here and stays soft-float |
| DOOM runs on the desktop, keyboard-playable, `-timedemo` frame rate reported | done | `scripts/test-doom.sh`: the shareware demo timed at 170 frames per second under TCG, on CI's toolchain as well as Homebrew's since v0.2.73; x86-64 and AArch64 only, no sound |
| Agel plays it: the perceive-decide-act loop an Agel program in the OS, stepping the game through the window and the engine's state | done | `scripts/test-play.sh`: the desktop loads `doom-agent.agel` into its native evaluator and `:play 8` steps the game through it, the player moving and firing, with no host policy or model in the path |
| Effects for Agel in the OS: files in the region, the clock, the console, starting a program | done | `scripts/test-play.sh`: `doom-agent.agel` appends its own `play.log` with `file-append` each step, and the test reads it back with `file-read` and `file-list`; `exec` is exercised by the workshop, not yet by a test |
| A model decides for the in-OS loop, reached through a host bridge | done | `scripts/test-play-bridge.sh`: `agel-play` boots the desktop, loads `doom-agent-model.agel` and answers every `model-request` the program makes with a policy through the bridge — the echo policy in CI, a stand-in provider on the same path the Claude and Codex providers take — for a whole episode, each step's request, reply and state recorded in `steps.jsonl`; a real model needs credentials and is run by hand on that path |
| A System One model judges for the in-OS loop, policy in the program | done | `scripts/test-play-bridge.sh`: `agel-play --policy jev` loads `doom-agent-judge.agel`, whose steps send typed `(judge ...)` questions; the bridge adds the state line and window, asks the `jev` provider and types back an answer line the program reads with `text-field`/`text-int`, gating on confidence; in CI the provider's curl is a stand-in answering as the endpoint does, the live endpoint is by hand with a key |
| Typed judgments from a program on the OS, through its own console | done | `scripts/test-agel-process.sh`: `agel/judgment` prints the request, `model-request` sends the block on the process console and reads `:model-reply N ...` given to it; `judge-locally` answers the same questions by rules without leaving the process |
| A browser on the OS whose accessibility tree is a state and whose actions are a choice | open | the order of work in [`system-one.md`](system-one.md); nothing exists |
| An agent asks a judge and reads the answer in the language | done | `crates/agel-stdlib/tests/judgment.rs`: `judge-request` puts the printed request into the outbox under the agent's `model/infer` capability, `judgment-of` reads the `system/model-result` message into groups, `judgment-failure` the error; `examples/judged-gate.agel` |
| Judgments as gates on effect approval, recorded beside the decision | done | `crates/agel-cli`: `--gate agel` evaluates `(effect-gate REQUEST)` in the world before dispatch (a committed input, in the image's log), `--gate jev` asks the model one yes/no question under its own audit; a denial is the request's journaled completion `effect/denied` with the judge's line, `:effects` lists every verdict; the judged gate's allowances are audited in the session only, and no effect on the OS is gated |
| A learned judge of Agel's own behind `judge-locally`'s contract | open | the judge written in Agel is rules |
| A run window on the desktop drawing the agent's state, frame time and hardware use | open | the engine's state lines in the terminal are what the desktop shows of the run today |
| A trained policy from the dataset; a world model for predictions | open | training is orchestrated through a provider, never performed by the OS |
| Speech and steering of the run | open | |

## Not started

- **Local inference.** CPU inference over quantized weights in a domain, in
  safe Rust, with no proprietary kernel-mode driver. External providers are
  the only model access today.
- **A network stack.** No driver, no protocol, no socket in the POSIX
  personality.
- **Firecracker as an outer envelope** for hosted deployment.
- **Training.** Out of scope for the operating system by decision; see
  [`deployment-targets.md`](deployment-targets.md).
