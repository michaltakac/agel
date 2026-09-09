# Agel v0.2.18 — Reclaiming compiled-agent heap storage

Added metered compacting collection at outermost native tail-call boundaries.
It traces immutable values, captured frames and caches, remaps private handles,
and frees unreachable invocation-arena storage without new unsafe code.
Nested native callers are explicitly excluded until general stack maps exist.

Cumulative allocation/edge/text quotas are preserved; collector work consumes
fuel. Failed invocations still leave committed state and revisions unchanged.
New metrics separate peak retained arena slots from cumulative allocations.

Try:

```sh
cargo run --release -q -p agel-jit --example memory_bench
cargo run --release -q -p agel-jit --example agent_swarm -- --ping
```

Tests cover captured closures and shared data across compaction, growing live
graphs, nested-call exclusion, quota preservation, failure recovery, and 1,000
compiled message turns. A paired local 10,000-step loop reduced peak retained
slots from 130,021 to 4,122, with extra fuel and a small wall-time cost. These are
logical arena metrics, not an RSS measurement or general performance promise.

The Agel compiler still self-compiles and the scheduler remains Agel-written.
The collector/runtime/backend are Rust bootstrap machinery; the whole OS is not
yet self-hosted and this scheduler is not integrated into the graphical OS.
See `docs/tail-collection.md` for the exact boundary and remaining work.
