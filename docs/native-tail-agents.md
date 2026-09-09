# Tail calls and compiled Agel agents — v0.2.17

Update: [v0.2.18](tail-collection.md) reclaims dead arena storage at outermost
tail boundaries. The no-reclamation description below is historical; nested
calls and live captures still limit when and how much can be collected.

This milestone moves tail-position analysis and an entire small mailbox scheduler
into Agel code. The compiler frontend still compiles itself, and bootstrap-stage
IR agreement remains tested. It is **not complete language or OS self-hosting**.

```sh
cargo run --release -q -p agel-jit --example self_host
cargo run --release -q -p agel-jit --example agent_swarm
cargo run --release -q -p agel-jit --example compiler_bench
cargo test -p agel-jit
```

## Tail calls without losing metering

The Agel frontend emits `agel/native-v2` IR. It marks `tail-call` only at the
end of a function body, the last expression of a tail-position `begin`, and
the branches of a tail-position `if`. Argument/operator/predicate expressions
are never marked tail. Parallel `let` still lowers to an applied lexical function.
The Rust validator independently checks these positions; a forged annotation
in an argument or before a later expression is rejected before code generation.
Existing `agel/native-v1` trees remain supported and retain ordinary calls.

Native tail-call code schedules a private target/argument continuation and
returns to a trampoline. The trampoline validates arity and invokes the next
closure without retaining the caller's native stack frame. Tail `apply` chains
are resolved iteratively too. Non-tail recursion still observes the configured
depth limit and the hard 256-frame ceiling. Generated functions never export
code pointers or let user data select arbitrary addresses.

Every transition still spends fuel. Allocation, collection-copy and output
budgets remain in force. Tail calls are **stack-bounded, not constant-memory**:
captured frames and intermediate values remain in the invocation arena until
return. GC/region reclamation is still needed for truly long-lived computations.
The current trampoline is not a serializable continuation or a preemptive scheduler.

Tests exercise 10,000 recursive steps with a depth limit of four; fuel and heap
exhaustion; non-tail depth failure; tail `apply`; nested normal/tail calls; lazy
branches; and invalid annotations. Compiler self-compilation still agrees across
the seed and two native compiler stages.

## Agents and scheduler written in Agel

`agel/native-agents` provides `native-empty`, `native-spawn`, `native-send`, and
`native-system-source`. `agel/native-agent-kernel` exposes the closed scheduler
function and its source as data. Both executable and source forms come from the
same file. `examples/jit-agent-swarm.agel` is a complete construction example.

Each actor has a behavior kind, immutable state, and explicit peer allowlist.
A behavior is a closed `(fn (state message) ...)` returning exactly:

```lisp
(dict 'state next-state 'outbox (list (list receiver message)))
```

The Agel kernel owns input validation, FIFO delivery, turn limits, state updates,
bounded queuing, outbox validation, and sender peer checks. Behaviors receive
only their state and message. The source composer compiles each behavior alone
before inserting it into the system, preventing free `world` or `turns` names
from acquiring access to the enclosing world during composition. The compiler
whitelist contains no model, process, filesystem or network calls.

Queue state uses front/back lists. New messages follow already queued messages;
a bounded number of turns may leave work pending for a later invocation. Lists
still use vector-backed runtime storage, so this is not a claim of O(1) list
operations. Actor IDs are symbols local to this scheduler, not hosted `World`
agent handles or kernel endpoints. The host chooses initial state, behavior
sources, and peer permissions; these are not cryptographic capabilities.

The example compiles the scheduler and behaviors using the **native Agel
frontend**, then executes three turns: a relay forwards `10` to a sink after an
already queued `20`. The sink records `(10 20)` newest-first, demonstrating
delivery order `20`, then `10`. Neither behavior nor scheduler is reinterpreted
by the Rust seed during those turns.

## Explicit atomic commit point

`agel_jit::state::NativeState` owns a compiled pure `(state, input) -> state`
program and an in-memory revision. For this scheduler, input is a turn budget.
It checks the expected revision, invokes using borrowed host values (no preliminary
deep state clone), and commits only after execution **and output export** succeed.
The Agel program owns scheduler policy; this Rust wrapper only owns the commit point.

A failure in any turn rolls back the **whole submitted batch**, including earlier
tentative state changes and outbox deliveries. Set the turn budget to one for
one-turn commits. This is not per-actor supervision, durable logging, or crash-safe
storage. A stale revision fails before execution. The initial state/program are
explicit host choices; construction does not execute or validate the program.
The scheduler validates its input world even for a zero-turn invocation.

Tests compare native scheduling against seed execution, compare split and combined
batches, reject ambient-world capture and invalid state, check peer/queue/result
failures, and verify unchanged state/revision after budget, behavior, or output
failures. A 1,000-message self-send test uses an explicit larger fuel budget while
keeping native call depth at most 16. Turn limits do not replace fuel/heap limits.

This is a separate hosted library, **not a replacement for existing `World`
actors and not installed into the freestanding graphical OS**. No new authority
or automatic native execution is added to either system. Integration still needs
versioned behavior dependencies, recovery and supervision semantics, and a safe
authority bridge. The validator/backend/ABI/runtime remain trusted bootstrap code,
not a formally verified sandbox.

## Performance evidence

Immutable builtin handles are now cached once per invocation rather than
allocated on every reference. `NativeOptions` exposes diagnostic switches for
this cache and tail calls; disabled tail calls retain normal bounded recursion.
The benchmark compiles the same frontend IR both ways, alternates run order,
discards one warmup per configuration, and reports medians of seven runs. Each
measured invocation includes arena setup, input import and output export; parsing
and machine-code compilation are excluded. Results are checked against the same IR.

A local macOS arm64 release run measured:

| Same compiler workload | Baseline switches off | Optimized |
| --- | ---: | ---: |
| Median native invocation | 11.054 ms | 10.495 ms |
| Logical allocations | 128,127 | 102,494 |
| Fuel units | 797,620 | 786,103 |
| Peak call depth | 137 | 47 |

The allocation reduction is about 20%; the modest timing difference is one local
measurement, not a broad speedup claim. Fuel is an implementation work budget,
not a stable cross-release cost or subscription token count. No models are used.

Remaining self-hosting work includes module/macro compilation, the reader,
structured conditions, persistent executable closures, memory reclamation and
integration with the real agent/OS runtimes. Tail-call support does not imply
those features are implemented.
