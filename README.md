# Agel

Agel is an experimental agentic Lisp and, eventually, an operating system in
which agents are first-class values. The project starts as a safe host runtime
and will progressively replace its host components with code written in Agel.

The current repository is **v1.5: a native Agel workshop with a frozen kernel
contract, a portable isolation backend, the same contract running on an
unmodified seL4 kernel, and the first privileged service split out into a
restartable domain**. It provides:

- a small, homoiconic Lisp reader and evaluator;
- atomic evaluation: a submitted batch either commits completely or changes
  nothing;
- versioned world state with explicit rollback;
- lexical closures and definition-site hygienic template macros;
- explicit modules, persistent maps, structured conditions and named restarts;
- host-issued, scope-checked capabilities and deterministic resource budgets;
- executable agents with isolated heaps and typed message protocols;
- deterministic cooperative scheduling and supervision trees;
- transactional agent turns, structured event history, snapshots, and replay;
- transactional model-request outboxes with exact-response replay;
- capability-scoped, explicit adapters for real Claude Code and Codex CLIs;
- SHA-256-bound proposals, zero-authority canaries, executable evidence, and
  atomic promotion;
- a non-rollback effect journal and epoch-bound capability revocation; and
- typed, default-deny effect intents with inspectable audit records;
- one constrained process boundary used by both real model adapters; and
- an in-memory copy-on-write workspace for disposable agent changes;
- canonical event-sourced images with a tamper-evident SHA-256 chain;
- exact offline reconstruction with fresh capability authority; and
- atomic image replacement, stale-writer detection, and previous-image recovery;
- an atomic standard library written in Agel, not privileged Rust;
- persistent sequence and tagged-result libraries; and
- typed round-robin worker pools with bounded transactional scheduling;
- `type-of` and `apply`, the two small reflective primitives needed by libraries;
- an `agel/meta` evaluator written in Agel for a lexical functional subset;
- an independent Common Lisp reference checked against the Rust seed; and
- external A/B image canary, evidence-bound promotion, and rollback;
- a modality-neutral text/voice interaction handoff with a 200 ms foreground
  acknowledgement contract, bounded background work, and verified human authority;
- a reproducible 64 KiB BIOS disk seed that enters x86-64 long mode in QEMU;
- a freestanding Rust serial HAL and interactive recovery monitor; and
- boot-time A/B denial, verification, promotion, and watchdog rollback checks;
- a fixed-memory Agel reader and evaluator running inside the VM;
- native transactional definitions, functions, recursion, quote/eval, monotonic
  revisions, and one-step world rollback;
- a versioned, backend-neutral kernel contract with an executable reference
  model and an 81-step conformance corpus frozen as a canonical transcript;
- kernel-built page tables, per-domain address spaces, write-xor-execute, trap
  entry, and a 100 Hz preemption timer;
- protection domains on x86-64, AArch64, and RISC-V that answer the whole
  kernel-contract corpus from the machine's lowest privilege level, through one
  trap gate, holding capability slots rather than references;
- byte-identical conformance transcripts from all three, checked against one
  frozen reference;
- the same contract on an **unmodified seL4 kernel** under Microkit: four
  protection domains where an unprivileged world asks an unprivileged broker,
  and the kernel is never taught what Agel is;
- a release manifest naming the exact kernel, configuration and toolchain, and
  stating plainly that the configuration is not a proved one;
- containment, on every architecture, of worlds that write kernel memory,
  execute instructions they are not allowed to, touch a device they were not
  granted, or never yield;
- a console driver in its own unprivileged, restartable domain, holding the
  device by whatever mechanism the architecture grants one, which the supervisor
  can lose and replace at a new generation while handles from before the restart
  fail closed; and
- a Rust CLI and test suite with no third-party crate dependencies.

Agel's primary deployment target is a bare-metal NVIDIA DGX node, single or
clustered, where the GPUs fine-tune and train models as well as serve them.
Inference-only deployments run on ordinary hardware or in a virtual machine and
can use external model providers instead. Those tiers, what each requires, and
how little of the DGX tier exists today are in
[`docs/deployment-targets.md`](docs/deployment-targets.md).

This is the first Agel evaluator running on the independently bootable
substrate, and the first hardware protection boundary the project can point at,
but not yet a general-purpose operating system. The evaluator itself still runs
privileged in the default image, and the full agent runtime, filesystem,
compiler, and persistent images still run as hosted components. See
[`docs/architecture.md`](docs/architecture.md) for the trust boundaries and
bootstrap plan.

## Try it

Agel currently requires only a Rust toolchain:

```sh
cargo run -p agel-cli
```

The CLI installs `agel/sequence`, `agel/result`, `agel/swarm`, and `agel/meta` by default.
Use `--no-stdlib` to expose only the minimal language substrate.

Example session:

```lisp
(def worker (spawn "worker"))
(send worker '(compile core))
(recv worker)
```

REPL commands:

- `:revision` shows the committed world revision.
- `:rollback` restores the preceding committed revision.
- `:stats` reports fuel consumed by the last transaction.
- `:budget` shows the default resource limits.
- `:help` shows command help.
- `:events` prints the agent event timeline.
- `:effects` prints host-effect authorization and outcome records.
- `:providers`, `:requests`, and `:dispatch` control explicit model invocation.
- `:snapshot NAME`, `:restore NAME`, and `:snapshots` provide live time travel.
- `:quit` exits.

Balanced expressions may span multiple lines and commit as one transaction.

Each submitted balanced batch is one transaction. Multiple forms commit together:

```lisp
(def answer 42) (def broken (/ answer 0))
```

The division error leaves both definitions uncommitted.

## Development

```sh
cargo fmt --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Self-improvement demonstration:

```sh
cargo run -q -p agel-verify --example safe_upgrade
```

Disposable filesystem demonstration:

```sh
cargo run -q -p agel-effects --example cow_workspace
```

Portable image demonstration:

```sh
cargo run -q -p agel-image --example portable_image
```

Library-defined orchestration demonstration:

```sh
cargo run -q -p agel-cli < examples/worker-pool.agel
```

Metacircular and A/B bootstrap demonstrations:

```sh
cargo run -q -p agel-cli < examples/metacircular.agel
./scripts/test-bootstrap.sh
cargo run -q -p agel-supervisor --example ab_upgrade
```

Two-lane human interaction and the bootable recovery monitor:

```sh
cargo run -q -p agel-interaction --example two_lane
./scripts/test-boot.sh
./scripts/test-monitor.sh
./scripts/run-qemu.sh       # boots directly to agel-native[0]>
```

The VM now opens directly into Agel. Try this inside `run-qemu.sh`:

```lisp
(def fact (fn (n) (if (= n 0) 1 (* n (fact (- n 1))))))
(fact 10)
(eval '(+ 20 22))
(begin (def answer 99) (/ 1 0))
answer
:defs
:rollback
```

Run the prompt-synchronized native language conformance session, the frozen
kernel-contract transcript, and the isolation suite with:

```sh
./scripts/test-native.sh
./scripts/test-native-repl.sh
./scripts/test-kernel-contract.sh
./scripts/test-isolation.sh              # x86-64, AArch64 and RISC-V
./scripts/test-isolation.sh aarch64      # or one of them
./scripts/build-kernel.sh riscv64        # just build an image
./scripts/test-sel4.sh                   # the same contract on seL4
./scripts/sel4-manifest.sh               # what that was built from
```

For each architecture the isolation suite boots a protection domain that answers
all 81 steps of the kernel contract from the machine's lowest privilege level,
requires the transcript to match the frozen reference byte for byte, then
deliberately makes worlds misbehave and requires each to be contained without
losing the recovery monitor.

The AArch64 and RISC-V suites need `qemu-system-aarch64` and
`qemu-system-riscv64` and the `aarch64-unknown-none-softfloat` and
`riscv64imac-unknown-none-elf` Rust targets. The seL4 suite additionally fetches
and checksum-verifies the Microkit SDK on first use; set `MICROKIT_SDK` to use
one you already have.

The boot scripts require `qemu-system-x86_64`, `clang`, GNU `objcopy`, and the
Rust `x86_64-unknown-none` target. On macOS: `brew install qemu binutils`.

See [`docs/language-core.md`](docs/language-core.md) and
[`docs/agent-runtime.md`](docs/agent-runtime.md) for the implemented language.
Runnable demonstrations live in [`examples/`](examples/).
See [`docs/model-agents.md`](docs/model-agents.md) for the real-provider trust
boundary and opt-in instructions.
See [`docs/evidence-upgrades.md`](docs/evidence-upgrades.md) for safe staged
self-modification and [`docs/threat-model.md`](docs/threat-model.md) for the
growing adversarial model.
See [`docs/effect-sandbox.md`](docs/effect-sandbox.md) for the v0.6 host-effect
boundary and its deliberately explicit limitations.
The stable v0.7 image format and recovery behavior are specified in
[`docs/portable-images.md`](docs/portable-images.md).
The v0.8 library APIs are documented in [`docs/standard-library.md`](docs/standard-library.md),
and the whole reader grammar fits in [`docs/language-postcard.md`](docs/language-postcard.md).
The v0.9 bootstrap trust story and its current limits are in
[`docs/bootstrap.md`](docs/bootstrap.md).
The native seed and recovery boundary are documented in
[`docs/native-boot.md`](docs/native-boot.md); text/voice scheduling and authority
are specified in [`docs/interaction.md`](docs/interaction.md).
The native subset, fixed limits, transactions, and workshop commands are in
[`docs/native-workshop.md`](docs/native-workshop.md).
What the seL4 backend was built from, and what is and is not verified about it,
is in [`docs/sel4-manifest.md`](docs/sel4-manifest.md).
The deployment tiers, the GPU plane, and the requirements the DGX target adds
are in [`docs/deployment-targets.md`](docs/deployment-targets.md).
The evaluated microkernel foundations, the seL4/Microkit decision, and the
staged native roadmap are in
[`docs/microkernel-research.md`](docs/microkernel-research.md); the versioned
backend-neutral kernel contract it freezes is in
[`docs/kernel-contract.md`](docs/kernel-contract.md).
External inspirations and the exact ideas Agel adopts from them are recorded in
[`docs/design-lineage.md`](docs/design-lineage.md).
