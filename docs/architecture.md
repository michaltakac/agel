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
boundaries, and the revision portion of the fourth. `agel-verify` implements the
first conservative verification gate. This is not a claim that macros—or the
current deterministic checker—can prove arbitrary code safe.

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
- Authority in the native backends is a capability slot with rights, never a
  name. A derived capability may only be equal or weaker than its parent, and
  revocation is transitive and fails stale holders closed.

Software transactional memory is only one layer. STM can roll back language
state, but cannot undo a model call, network request, disk write, or device I/O.
Model inference therefore uses a committed outbox, idempotence-guarded
completion, explicit dispatch, and exact-result replay. At v0.6, host process
execution is also routed through typed intent, policy, resource limits, and an
audit log in `agel-effects`. Future effects must use the same prepare/commit
shape plus idempotency keys or compensating actions.

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
6. **Effect interposition (complete at v0.6):** typed default-deny policy,
   constrained process execution, inspectable outcomes, and copy-on-write
   virtual workspaces. Kernel-grade syscall mediation remains a later native
   boundary.
7. **Portable images (complete at v0.7):** canonical committed-input logs,
   exact model-result replay, fresh authority on reconstruction, tamper-evident
   chains, and crash-safe file replacement with previous-image recovery.
8. **Library-first environment (complete at v0.8):** sequence/result modules
   and typed bounded worker pools implemented as ordinary Agel source. The CLI
   installs them atomically while retaining a `--no-stdlib` minimal-core mode.
9. **Diverse bootstrap (complete for the functional kernel at v0.9):** an
   independent Common Lisp evaluator is differentially checked against the Rust
   seed, while `agel/meta` evaluates lexical Agel code as data. A supervisor
   keeps whole A/B semantic images outside the candidate and binds promotion to
   zero-authority health evidence.
10. **Bootable recovery seed (complete at v1.0):** a reproducible BIOS image
   enters x86-64 long mode, runs a freestanding Rust serial HAL, and exposes an
   independent A/B recovery monitor whose policy is testable under QEMU.
11. **Native language workshop (complete at v1.1):** a fixed-memory Agel reader,
   evaluator, transactional world, definitions, recursive functions, and serial
   REPL execute inside QEMU while recovery state remains outside the language.
12. **Frozen kernel contract (complete at v1.2):** a versioned, backend-neutral
   object/rights/operation contract, an executable reference model, and an
   81-step conformance corpus whose canonical transcript is frozen and diffed.
   See [`kernel-contract.md`](kernel-contract.md).
13. **Research-kernel isolation (complete at v1.2):** kernel-built page tables,
   per-domain address spaces, write-xor-execute, descriptor tables, trap entry,
   a preemption timer, ring-3 protection domains, and a syscall boundary. An
   unprivileged world answers the whole conformance corpus, and worlds that
   fault, execute privileged instructions, or never yield are contained without
   losing the recovery monitor.
14. **Portable isolation backend (complete at v1.3):** the same contract, the
   same corpus, and the same containment tests on x86-64, AArch64, and RISC-V
   from one source, with byte-identical transcripts. Only address spaces,
   register frames, trap entry, and the privilege transition are
   per-architecture.
15. **Complete self-host:** reader, hygienic expander, agent runtime, image codec,
   and compiler in Agel; extend diverse comparison to every kernel semantic.
16. **Native agent world:** move the evaluator and the full Agel agent runtime
   into ring-3 domains, then add an allocator, drivers, and persistent images.
   Keep device access outside mutable language heaps.
17. **seL4 backend (complete at v1.4):** the same kernel contract over an
   unmodified seL4 kernel, composed with Microkit on AArch64. Four protection
   domains — recovery, world, broker, serial — where the contract is answered
   by an unprivileged server and the kernel knows nothing about Agel. The
   configuration is MCS and therefore not a proved one; the release manifest
   says so.
18. **Live system:** boot-selector-backed A/B worlds, health oracles, signed
   promotion, and watchdog-triggered rollback managed by the recovery monitor.

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
