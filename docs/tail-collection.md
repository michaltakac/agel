# Reclaiming storage in compiled Agel loops — v0.2.18

Tail calls alone did not reclaim the runtime's invocation arena. This milestone
adds metered compacting collection at a narrow, explicitly checked safe boundary.
The existing Agel-written compiler and scheduler benefit without new language
syntax, I/O authority, or changes to the freestanding OS.

```sh
cargo run --release -q -p agel-jit --example memory_bench
cargo run --release -q -p agel-jit --example agent_swarm -- --ping
cargo run --release -q -p agel-jit --example self_host
cargo test -p agel-jit
```

`--ping` runs the Agel-compiled actor from
[`examples/jit-agent-ping.agel`](../examples/jit-agent-ping.agel) for 1,000
self-message turns, with an explicit 10M fuel allowance. It commits state 1,000 and leaves one queued message. It also
demonstrates that a failed batch preserves state/messages/revision and prints
collection statistics. This is the isolated hosted scheduler, not a graphical
OS actor or a model-backed agent.

## The boundary and roots

Collection occurs only after the outermost generated function has returned a
tail transfer, and after that transfer's closure arguments have been placed in
the next frame. Runtime depth must be exactly one and no pending transfer may
remain. The next environment is the only active execution root; nil and imported
constant/builtin caches are additional roots. Lists, maps (including keys),
closure environments, frame parents and frame slots are traced iteratively.

The collector constructs private handle remappings, moves live values/frames,
rewrites all reachable references and caches, and drops unreachable storage.
Strings and vectors move by ownership, not by recursively cloning the data tree.
No raw code address changes, and the collector adds no unsafe Rust.

Crucially, **nested native calls are not collection points**. A suspended caller
could retain handles in native registers or Rust temporaries that this collector
does not trace. Collection is therefore skipped there. It does not happen during
arbitrary callbacks, allocation, input import, output export or a final builtin
return. General safepoints and stack maps remain future work.

`NativeOptions::collection_interval` defaults to 4,096 newly retained slots;
zero disables collection, and nonzero values are clamped to at least 256. After
a collection, the next threshold is live slots plus the interval. A large live
graph therefore does not trigger a full scan on every following tail call.
An interval is a trigger, **not a hard resident-memory bound**: a nested call may
allocate substantially before a safe boundary is reached.

## Budgets, atomicity and observability

Tracing, scans and handle rewriting spend fuel. Values, edges and text quotas
remain cumulative across the entire invocation: reclaiming an object does not
refund its allocation quota. Input and output expansion are still charged. This
prevents collection from turning a bounded computation into unlimited work.
Existing quota-dependent programs may exhaust fuel earlier due to collector work.

`Outcome` and transaction `Receipt` report collection count, reclaimed slots and
peak retained arena slots. Existing `allocated_values` still counts cumulative
logical allocations, including output, rather than current live storage.
Retained-slot metrics exclude collector scratch buffers, vector spare capacity,
exported host trees, native stack/code, and allocator overhead; they are not RSS.
Scratch metadata is proportional to the bounded arena and traced graph.

A collection/fuel error aborts the invocation. `NativeState` commits only after
the full invocation and export succeed, preserving the original state and revision
on failure. Repeated invocations have independent arenas and caches.

The collector is trusted Rust bootstrap machinery, not a self-hosted Agel GC or
a formally verified sandbox. It relies on immutable runtime values and the private
native ABI/handle discipline. It does not provide persistent executable closures,
cross-invocation heaps, concurrent collection, or OS-level process isolation.

## Evidence

Tests compare collection on/off for the same program; exercise captured lexical
frames, callable values in maps, shared data and constant caches; verify growing
live collections survive; ensure nested callers are not collected; preserve
cumulative quotas; and check transaction recovery under exhausted fuel. The
compiled scheduler's 1,000-turn test asserts collections, reclaimed slots and a
peak arena below 20,000 slots. Compiler bootstrap-stage agreement remains tested.

One local macOS arm64 release run of `memory_bench` measured:

| Same 10,000-step tail loop | Collection off | Collection on |
| --- | ---: | ---: |
| Peak retained arena slots | 130,021 | 4,122 |
| Cumulative allocations | 130,022 | 130,022 |
| Reclaimed slots | 0 | 127,326 |
| Collections | 0 | 31 |
| Fuel | 770,086 | 1,154,947 |
| Median invocation | 11.753 ms | 12.036 ms |

This is about 97% fewer peak retained slots for this workload, with additional
collector work—not a universal speed or RSS claim. The benchmark alternates
configuration order, discards one warmup per configuration and reports seven-run
medians. Both use identical IR/input and all other options; invocation includes
import/export but excludes compilation. No models or subscription tokens are used.

Remaining self-hosting work includes the reader, module/macro compilation,
structured conditions, persistent compiled behaviors, general heap reclamation,
and integration with the real hosted/OS agent runtimes. This milestone removes
one retention bottleneck; it does not complete that roadmap.
