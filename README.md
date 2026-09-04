# Agel

Agel is an experimental agentic Lisp and, eventually, an operating system in
which agents are first-class values. The project starts as a safe host runtime
and will progressively replace its host components with code written in Agel.

The current repository is **v1.0: a bootable, recoverable agent-language seed**. It provides:

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
- a Rust CLI and test suite with no third-party crate dependencies.

This is the first independently bootable substrate, not yet a general-purpose
operating-system kernel: the Agel evaluator still runs as a hosted process. See
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
./scripts/run-qemu.sh       # then type: help, status, verify, promote, fault
```

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
External inspirations and the exact ideas Agel adopts from them are recorded in
[`docs/design-lineage.md`](docs/design-lineage.md).
