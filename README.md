# Agel

Agel is an experimental agentic Lisp and, eventually, an operating system in
which agents are first-class values. The project starts as a safe host runtime
and will progressively replace its host components with code written in Agel.

The current repository is **Milestone 3: real model agents**. It provides:

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
- capability-scoped, explicit adapters for real Claude Code and Codex CLIs; and
- a Rust CLI and test suite with no third-party crate dependencies.

This is deliberately not presented as an operating-system kernel yet. See
[`docs/architecture.md`](docs/architecture.md) for the trust boundaries and
bootstrap plan.

## Try it

Agel currently requires only a Rust toolchain:

```sh
cargo run -p agel-cli
```

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

See [`docs/language-core.md`](docs/language-core.md) and
[`docs/agent-runtime.md`](docs/agent-runtime.md) for the implemented language.
Runnable demonstrations live in [`examples/`](examples/).
See [`docs/model-agents.md`](docs/model-agents.md) for the real-provider trust
boundary and opt-in instructions.
