# Native agent-proposed code upgrades — v0.2.19

```sh
cargo run --release -q -p agel-jit --example live_upgrade
cargo test -p agel-jit --test upgrades
```

The example boots a native Agel compiler, a native source composer, and an
Agel-written scheduler. A compiled designer actor then constructs a new counter
behavior as ordinary source data:

```lisp
(fn (heap message)
  (dict 'state (+ heap 10) 'outbox nil))
```

No Rust evaluator calls occur after initial construction. The host selects the
proposal and routes it to the native composer and native compiler. Rust validates
their IR and emits machine code. The host explicitly previews and promotes it.
This is real native code evolution, not a model response printed as a result.
It is also **not full language or OS self-hosting**.

## What the demonstration checks

1. The designer processes one message and emits source as its state. Two counter
   messages remain queued, with the counter still at zero.
2. Native Agel composes the replacement catalog and lowers the resulting source.
3. A one-turn preview predicts counter state 10, leaving live state untouched.
4. Promotion changes code only; both messages remain queued.
5. One committed turn uses the new code, reaching 10.
6. The previous code is probed against current state, then restored without
   rewinding state or messages. The last queued turn reaches 11.

The executable example is also an integration test. Additional tests reject
foreign, stale and competing candidates, failed probes, wrong arity, non-data
outputs, depleted fuel, and incompatible rollback. Dropping a candidate changes
nothing. Failed operations preserve active code, committed state and revision.

## Agel owns the composition semantics

`agel/native-system-builder` exports `native-system-builder` and
`native-system-builder-source`, executable and quoted forms of one Agel file.
The closed builder takes `(compiler kernel sources)`. It validates catalog keys,
compiles every behavior independently to reject ambient free names, checks its
two-argument shape, and constructs a closed scheduler application as syntax.

`agel/native-agents` keeps its existing `native-system-source` API, now a small
wrapper selecting the standard compiler and kernel. The example embeds these
same trusted sources in a closed function and compiles it. Seed/native composed
source equality and native rejection of ambient capture are checked.

The low-level builder's compiler and kernel arguments are explicit trusted
choices. Passing a fake compiler is not an authority-safe validation service.
The standard kernel still enforces peer allowlists, FIFO delivery, bounded queues
and exact behavior result shape. The host can replace that kernel, so host code
remains in the trust boundary. A behavior catalog entry may serve multiple actors;
changing it upgrades every actor using that kind, not just one actor ID.

## Small host-side commit mechanism

`agel_jit::state::NativeState` owns the active program, state, revision and one
previous program. Its upgrade API is:

- `preview(expected, program, input, limits)` executes the proposed program on
  current state and returns an opaque `Candidate`, bound to this machine's owner
  identity and exact revision. Its `probe()` exposes output and resource metrics.
- `promote(expected, candidate)` checks owner and both revision conditions,
  retains previous code, switches active code and increments revision. It does
  **not** commit the probe output.
- `rollback_program(expected, input, limits)` executes previous code against
  **current** state. Only success swaps programs and increments revision. State
  is unchanged. Repeated rollback toggles the two retained programs; it is not
  an unbounded history or a state rewind.

Any successful transaction or code change invalidates outstanding candidates.
Ownership is an opaque allocation identity, not a reusable numeric address.
Exclusive mutable access prevents promotion during an active invocation. Code
pointers cannot escape as Agel values. Revision overflow is rejected before work.

`Native::ir()`, `NativeState::program_ir()` and `Candidate::ir()` expose immutable
validated IR for inspection. This is not a signed artifact, cryptographic hash,
or complete identity of backend settings.

## Safety, cost and remaining bootstrap work

A passing probe is evidence for **one input and one state**, not formal proof
of future behavior, state-schema compatibility, termination under every budget,
or semantic correctness. A zero-turn probe may exercise no behavior at all.
Choose meaningful probes and independently inspect or test candidates. No STM,
proof macro, external-effect rollback or crash recovery is claimed here.

This path contains no model calls, network, process or filesystem primitives.
It spends CPU/fuel, not subscription tokens. A future model can propose source
through a separate authorized bridge; model output must remain untrusted data.
The current designer is deterministic Agel, not Claude or Codex.

The example reports native composition/lowering and backend compilation time
separately, excluding bootstrap. One local release run measured about 20 ms and
28 ms respectively; these are illustrative, not a general performance guarantee.
Compilation happens on replacement, not every message turn. Each preview and
rollback has explicit fuel, heap and export budgets. Compilation/validation has
structural limits but no shared host-wide compilation quota. Active/previous
code, retained validated IR and outstanding candidates (including probe output)
consume host memory outside per-invocation arena metrics and quotas. Callers
must bound outstanding proposals and compilation frequency.

Rust still supplies the reader/bootstrap, runtime primitives, memory collector,
IR validator, executable-memory lifetime and backend. The compiler, composer,
actor behaviors and scheduling policy execute as native Agel. Module/macro
compilation, persistent executable closures, structured conditions and integration
with real World/OS authority and recovery remain future work. This in-memory
hosted JIT path does not update the graphical OS or add autonomous promotion.
