# Agel v0.2.17 — Tail calls and an Agel-written native scheduler

The Agel frontend now marks tail calls in v2 IR; the backend independently
validates those positions and executes them through a metered trampoline.
The v1 IR remains supported. Cached immutable primitive handles reduce repeated
allocations. Compiler self-compilation and native-stage IR agreement remain tested.

Added an isolated Agel-written mailbox scheduler: closed behaviors, FIFO messages,
bounded outboxes, explicit sender peer permissions, and immutable state transitions.
A small Rust commit wrapper publishes only successful batches with a checked
revision. Failed turns, budget exhaustion and output failures preserve state and
queued messages. This does not replace hosted World actors or the freestanding OS.

Try:

```sh
cargo run --release -q -p agel-jit --example agent_swarm
cargo run --release -q -p agel-jit --example compiler_bench
```

Tests include 10,000 stack-bounded tail steps, 1,000 message turns with explicit
fuel, forged tail annotations, queue/peer restrictions and atomic failure recovery.
The paired local compiler workload used about 20% fewer logical allocations;
timings and exact methodology are in `docs/native-tail-agents.md`.

The frontend and this scheduler are Agel-written; the Rust runtime/backend and
whole OS are not yet self-hosted. Tail calls still allocate in a bounded arena;
GC, persistent closures, module/macro compilation and production actor integration
remain future work. No model calls or new I/O authority are introduced.
