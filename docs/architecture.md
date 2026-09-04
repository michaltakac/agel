# Agel architecture: transactional seed

## Direction

Agel aims to be a homoiconic language and live operating environment where
agents are ordinary programmable values, applications are compositions of
agents, and a running system can propose and adopt its own changes. Common Lisp
is the conceptual and bootstrap lineage; Rust and C are temporary substrate for
memory-safe runtime machinery and narrow hardware interfaces.

The essential design constraint is that *self-modifying* must not mean
*unreviewed mutation of the only running world*. Agel therefore separates four
roles that classic Lisp images often combined:

1. **World** — immutable-at-commit language bindings, agents, mailboxes, and
   resource references.
2. **Evaluator** — computes against a private candidate world.
3. **Verifier** — checks evidence and invariants before a candidate may commit.
4. **Supervisor** — owns revisions, rollback, resource budgets, and recovery.

The language core implements the first two roles, explicit authority and resource
boundaries, and the revision portion of the fourth. The verifier is an explicit
future boundary, not a claim that macros alone can prove arbitrary code safe.

## Safety invariants

- A failed evaluation never changes the committed world.
- Code under evaluation cannot obtain a mutable reference to committed state.
- Agent creation, message send, and receive participate in the same transaction
  as ordinary definitions.
- A successful commit gets a monotonically increasing revision.
- A prior committed revision can be restored without re-evaluating code.
- Host capabilities are denied unless represented by an explicit, scoped value.
  Model inference is the first effect: agents can only create transactional
  requests, while a trusted host adapter owns process execution.
- `unsafe` Rust is forbidden in the seed workspace.

Software transactional memory is only one layer. STM can roll back language
state, but cannot undo a model call, network request, disk write, or device I/O.
Model inference therefore uses a committed outbox, idempotence-guarded
completion, explicit dispatch, and exact-result replay. Future effects must use
the same prepare/commit shape plus idempotency keys or compensating actions.

## Current execution model

Source text is read into `Expr`, preserving code as data. Evaluation happens
against a cloned candidate `State`. If every form succeeds, the old state is
saved in a bounded history and the candidate becomes visible. If any form
fails, the candidate is discarded.

Agents execute as deterministic cooperative turns. Each has a FIFO mailbox,
typed protocol, behavior closure, isolated persistent heap, explicit capability
set, and optional supervisor. A failed turn rolls back its heap writes, outgoing
messages, child creation, and provisional events before supervision runs.

## Bootstrap ladder

Each rung must be runnable and differentially testable against the rung below:

1. **Rust seed (complete):** reader, evaluator, atomic world, passive agents.
2. **Language core (complete):** lexical closures, hygienic macros, modules, conditions/restarts,
   persistent collections, structured capabilities, and resource accounting.
3. **Agent runtime (complete):** deterministic cooperative scheduler, supervision trees,
   typed protocols, event log, snapshot/replay, and isolated heaps.
4. **Model-agent bridge (complete):** transactional inference intents,
   capability-scoped Claude Code and Codex adapters, trusted result injection,
   and deterministic replay without provider re-execution.
5. **Verification gate (complete at v0.5):** content-bound proposals,
   conservative effect declarations, executable evidence, zero-authority
   canaries, and atomic promotion. A small trusted checker, not a macro or
   model, decides admission. Finite protocol model checking remains a
   library-layer extension.
6. **Common Lisp bootstrap:** a portable reference implementation that emits
   the same core IR and runs conformance tests against the Rust seed.
7. **Self-host:** reader, expander, evaluator/compiler, and standard library in
   Agel; use diverse bootstrap comparison to detect trusting-trust failures.
8. **Native substrate:** minimal Rust/C HAL, allocator, interrupt/trap entry,
   drivers, and a QEMU image. Keep device access outside mutable language heaps.
9. **Live system:** A/B system worlds, health oracles, signed promotion,
   watchdog-triggered rollback, and a recovery monitor outside the self-editing
   runtime.

## Change protocol for privileged code

A future kernel-changing agent must submit an immutable proposal containing the
base revision, source/IR hash, declared effects, tests, resource bounds, and
proof or model-checking evidence. The supervisor builds it in an isolated world,
runs deterministic and adversarial tests, canaries it under budgets, and only
then atomically promotes it. The previous image and an independent recovery
monitor remain available.

No component may both author a privileged change and unilaterally waive its
verification policy. Natural-language input creates proposals; it is never
itself authority.
