# Agel

Agel is an experimental agentic Lisp and, step by step, an operating system
in which agents are first-class values. It began as a safe hosted runtime
and keeps replacing host components with code written in Agel, on research
kernels of its own and on an unmodified seL4.

**Current release: v0.2.63.** Every release is one milestone with honest
notes; the whole line is in [`docs/versioning.md`](docs/versioning.md).

![The native desktop at v0.2.63: two windows a C program asked for, over the workshop](docs/images/native-desktop-v0.2.63.png)

## What exists

**A language and a hosted runtime.** A small homoiconic Lisp with atomic
evaluation (a batch commits whole or not at all), versioned world state with
rollback, lexical closures, hygienic template macros, modules, conditions and
restarts, capability-scoped effects with deterministic budgets, agents with
isolated heaps and typed protocols, supervision, event history, snapshots and
replay, transactional model requests, Ed25519-signed portable images with a
tamper-evident chain, and evidence-carrying upgrades: a proposal runs in a
zero-authority canary and is promoted atomically or discarded. A standard
library, a metacircular evaluator, a UI model, a layout engine and a vector
renderer are written in Agel. See [`docs/language-core.md`](docs/language-core.md),
[`docs/agent-runtime.md`](docs/agent-runtime.md) and
[`docs/standard-library.md`](docs/standard-library.md).

**Research kernels on three machines.** Freestanding Rust kernels for
x86-64, AArch64 and RISC-V build per-domain address spaces with
write-xor-execute, a trap gate and a preemption timer, and run every world
unprivileged: the evaluator, the console driver, the disk driver (ATA,
virtio-blk, or the Raspberry Pi's SD host controller), the filesystem
service, the compositor, and loaded processes. A versioned kernel contract
with a 118-step corpus is answered byte-identically by all three and by a
second implementation on seL4 under Microkit. Worlds that write kernel
memory, run privileged instructions, touch ungranted devices or never yield
are contained; a driver that dies is replaced at a new generation and its
old handles fail closed. See [`docs/architecture.md`](docs/architecture.md),
[`docs/kernel-contract.md`](docs/kernel-contract.md) and
[`docs/threat-model.md`](docs/threat-model.md).

**A durable workshop.** The serial workshop on each machine edits named
source cells, saves them to alternating checksummed disk slots, replays them
after reboot, survives a power cut at any write, and on x86-64 selects
between A/B kernel slots with signed candidates and watchdog rollback. See
[`docs/native-workshop.md`](docs/native-workshop.md) and
[`docs/native-boot.md`](docs/native-boot.md).

**A desktop.** At 1920×1080, styled with COSMIC's palette, radii and
spacing: anti-aliased Fira Sans and Fira Mono from font atlases on the disk,
surfaces with soft shadows and edges, a panel with a launcher and a clock, a
dock, hover and press states, a terminal panel, and windows that processes
ask for and draw into through records the supervisor checks before it
paints them. Windows move by their header, stack, minimize to panel pills,
maximize, resize by their corner, and hand a process its presses, keys,
motion, releases and resizes while the desktop keeps running. The
compositor holds the framebuffer as its only device mapping; every click is
a typed command underneath. See [`docs/native-graphics.md`](docs/native-graphics.md).

**A POSIX personality.** Processes loaded from the disk into protection
domains, files through a namespace granted per process and served by an
unprivileged filesystem service, `agel-libc` (a Rust archive with a C ABI)
so C programs build from source and run, `spawn` in place of `fork`, pipes
and `wait`, a heap that grows through pages mapped at the break, a
monotonic clock, `sleep` and `kill`, `setjmp`, `scanf` and `getopt`,
`stat`, `rename`, `unlink`, directories, `chdir` and `truncate`, files of
64 KiB, and a window protocol for C. Authority comes from the namespace,
never from a path. See [`docs/posix-personality.md`](docs/posix-personality.md).

**A board.** The AArch64 kernel builds as a flat image for the Raspberry
Pi 4, boots on QEMU's model of it from an EL2 entry with RAM at zero,
drives the SD card, and runs the desktop with its framebuffer from the
firmware's mailbox. A Pi 5 image is built from documented addresses and has
not run yet. See [`docs/raspberry-pi.md`](docs/raspberry-pi.md).

Agel is pre-production. It does model **inference, not training**, and
takes its Linux application compatibility from the POSIX personality above
the kernel rather than from Linux underneath; scope and tiers are in
[`docs/deployment-targets.md`](docs/deployment-targets.md).

## Try it

The hosted runtime needs a Rust toolchain and a C compiler (Xcode
command-line tools on macOS; GCC or Clang on Linux):

```sh
cargo run -p agel-cli
```

The banner lists the installed Agel libraries; `--no-stdlib` exposes only
the substrate and `--image PATH` persists the world as a portable image.
Inside the REPL, `:help` lists the commands; `:propose FILE`, `:promote` and
`:discard` run the upgrade gate. A batch of forms is one transaction:

```lisp
(def answer 42) (def broken (/ answer 0))
```

The division error leaves both definitions uncommitted.

### The native desktop

The boot scripts need `qemu-system-x86_64`, `clang`, GNU `objcopy` and the
`x86_64-unknown-none` Rust target (`brew install qemu binutils` on macOS);
C programs need a clang with `lld` (`brew install llvm lld`).

```sh
./scripts/run-graphics.sh
```

QEMU opens its own window at 1920×1080. The keyboard and mouse are the
guest's; `--web` adds a host-side console with layout composition and paste.
Try:

```lisp
(def square (fn (x) (* x x)))
(square 12)
:cell mathematics (def triangular (fn (n) (/ (* n (+ n 1)) 2)))
:save
(accent cyan)
(workspace 2)
:fs-format
:exec c-chart -- 3 7 5 9
:help
```

`:save` keeps cells across `:shutdown` and the next launch. Click
**Applications** for the launcher, a dock tile for its action, a window's
header to move it, its corner to resize it, its controls to minimize,
maximize or close it. Walkthroughs: [`examples/graphical-workshop.txt`](examples/graphical-workshop.txt),
[`examples/native-workbench.txt`](examples/native-workbench.txt),
[`examples/native-agents.txt`](examples/native-agents.txt),
[`examples/native-dock.txt`](examples/native-dock.txt).

### The serial workshop

```sh
./scripts/run-qemu.sh              # x86-64
./scripts/run-qemu.sh aarch64
./scripts/run-qemu.sh riscv64
```

```lisp
(def fact (fn (n) (if (= n 0) 1 (* n (fact (- n 1))))))
(fact 6)
:edit boot
(def answer 42)
:run boot
:save
:shutdown
```

Run it again: the workspace is replayed before the first prompt and
`answer` is `42`. `:exec NAME [ROOT] [ro] [-- ARG...]` runs a program
from the disk's program region in a namespace; `:fs-format`, `:fs-mkdir`
and `:fs-ls` are the filesystem commands.

### The Raspberry Pi

```sh
./scripts/build-kernel.sh raspi4 --features isolated-repl     # kernel8.img
./scripts/build-kernel.sh raspi4 --features native-graphics   # the desktop
./scripts/build-kernel.sh raspi5 --features isolated-repl     # kernel_2712.img, unrun
```

[`docs/raspberry-pi.md`](docs/raspberry-pi.md) has the card layout, the
`config.txt`, what the UART should say, and what the Pi 5 changes.

### The hosted vector kitchen sink

![The hosted vector kitchen sink](output/playwright/agel-kitchensink.png)

One Agel value combining a shell, an agent graph, prompts, inspectors, a
network trace, gradients, clipping, paths, transforms and scalable text,
rendered to a resolution-independent SVG by the Agel-written vector layer:

```sh
cargo run -q -p agel-vector -- \
  --program examples/kitchensink.agel \
  --output target/agel-kitchensink.svg
```

## Development

```sh
cargo fmt --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

The native suites boot QEMU (`qemu-system-x86_64`, `-aarch64`, `-riscv64`;
Python 3.10 or newer drives the prompt-synchronized ones):

```sh
./scripts/test-isolation.sh              # the kernel contract from ring 3, on three machines
./scripts/test-native-repl.sh [ARCH]     # the serial workshop session
./scripts/test-native-persistence.sh     # save, reboot, reject, recover
./scripts/test-power-cut.sh              # a power cut at every write of a save
./scripts/test-kernel-rollback.sh        # A/B kernel slots on x86-64
./scripts/test-files.sh [ARCH]           # namespaces, descriptors, service restart
./scripts/test-libc.sh [ARCH]            # C programs against agel-libc
./scripts/test-spawn.sh [ARCH]           # pipes, spawn, wait
./scripts/test-graphics.sh               # the compositor's frozen digest
./scripts/test-desktop-process.sh        # the desktop: launcher, windows, drags, a listening program
./scripts/test-raspi4.sh                 # the Pi 4 under QEMU, with and without a card
./scripts/test-raspi4-desktop.sh         # the desktop on the Pi 4 under QEMU
./scripts/test-sel4.sh                   # the same contract on seL4
```

Hosted demonstrations live in [`examples/`](examples/): the evidence-carrying
upgrade (`cargo run -q -p agel-verify --example safe_upgrade`), portable
images (`cargo run -q -p agel-image --example portable_image`), the
metacircular evaluator (`cargo run -q -p agel-cli < examples/metacircular.agel`),
the compiler bootstrap and the opt-in integer JIT (`cargo run --release -q
-p agel-jit --example self_host`, Rust 1.86+), compiled actors
(`--example agent_swarm`) and live behavior upgrades (`--example live_upgrade`).

## Documents

- [`docs/architecture.md`](docs/architecture.md): the trust boundaries and the bootstrap ladder, one rung per milestone.
- [`docs/versioning.md`](docs/versioning.md): every release and what it claimed.
- [`docs/threat-model.md`](docs/threat-model.md): what each release does and does not defend.
- [`docs/native-graphics.md`](docs/native-graphics.md), [`docs/native-workshop.md`](docs/native-workshop.md), [`docs/native-boot.md`](docs/native-boot.md): the desktop, the workshop, the disk.
- [`docs/posix-personality.md`](docs/posix-personality.md): the process protocol and the C library, stratum by stratum.
- [`docs/raspberry-pi.md`](docs/raspberry-pi.md): the board.
- [`docs/kernel-contract.md`](docs/kernel-contract.md), [`docs/sel4-manifest.md`](docs/sel4-manifest.md), [`docs/microkernel-research.md`](docs/microkernel-research.md): the contract, the seL4 build, the research behind them.
- [`docs/deployment-targets.md`](docs/deployment-targets.md): scope, tiers, what does not exist.
- [`docs/language-core.md`](docs/language-core.md), [`docs/language-postcard.md`](docs/language-postcard.md), [`docs/agent-runtime.md`](docs/agent-runtime.md), [`docs/standard-library.md`](docs/standard-library.md), [`docs/agel-in-agel.md`](docs/agel-in-agel.md): the language.
- [`docs/evidence-upgrades.md`](docs/evidence-upgrades.md), [`docs/portable-images.md`](docs/portable-images.md), [`docs/effect-sandbox.md`](docs/effect-sandbox.md), [`docs/model-agents.md`](docs/model-agents.md): upgrades, images, effects, model providers.
- [`docs/managed-jit.md`](docs/managed-jit.md), [`docs/native-reader.md`](docs/native-reader.md), [`docs/native-modules.md`](docs/native-modules.md), [`docs/native-tail-agents.md`](docs/native-tail-agents.md), [`docs/native-code-upgrades.md`](docs/native-code-upgrades.md): the compiler and native Agel.
- [`docs/design-lineage.md`](docs/design-lineage.md): what Agel takes from whom.
