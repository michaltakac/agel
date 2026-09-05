# Agel architecture: transactional seed

## Direction

Agel aims to be a homoiconic language and live operating environment where
agents are ordinary programmable values, applications are compositions of
agents, and a running system can propose and adopt its own changes.

Agel is a **Unix-like agentic operating system on a microkernel**. It does
model **inference, not training**: training would require a proprietary
kernel-mode GPU stack and therefore Linux underneath, which is the one trade the
project does not make. Linux application compatibility comes from a **POSIX
personality written in safe Rust** running unprivileged above the kernel — the
Redox approach — with authority derived from capabilities rather than from
paths. Scope, tiers and hardware are in
[`deployment-targets.md`](deployment-targets.md).

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
state, but cannot undo a model call, network request, disk write, or device
I/O.
Model inference therefore uses a committed outbox, idempotence-guarded
completion, explicit dispatch, and exact-result replay. At v0.0.6, host process
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
5. **Verification gate (complete at v0.0.5):** content-bound proposals,
   conservative effect declarations, executable evidence, zero-authority
   canaries, and atomic promotion. A small trusted checker, not a macro or
   model, decides admission. Finite protocol model checking remains a
   library-layer extension.
6. **Effect interposition (complete at v0.0.6):** typed default-deny policy,
   constrained process execution, inspectable outcomes, and copy-on-write
   virtual workspaces. Kernel-grade syscall mediation remains a later native
   boundary.
7. **Portable images (complete at v0.0.7):** canonical committed-input logs,
   exact model-result replay, fresh authority on reconstruction, tamper-evident
   chains, and crash-safe file replacement with previous-image recovery.
8. **Library-first environment (complete at v0.0.8):** sequence/result modules
   and typed bounded worker pools implemented as ordinary Agel source. The CLI
   installs them atomically while retaining a `--no-stdlib` minimal-core mode.
9. **Diverse bootstrap (complete for the functional kernel at v0.0.9):** an
   independent Common Lisp evaluator is differentially checked against the Rust
   seed, while `agel/meta` evaluates lexical Agel code as data. A supervisor
   keeps whole A/B semantic images outside the candidate and binds promotion to
   zero-authority health evidence.
10. **Bootable recovery seed (complete at v0.1.0):** a reproducible BIOS image
   enters x86-64 long mode, runs a freestanding Rust serial HAL, and exposes an
   independent A/B recovery monitor whose policy is testable under QEMU.
11. **Native language workshop (complete at v0.1.1):** a fixed-memory Agel reader,
   evaluator, transactional world, definitions, recursive functions, and serial
   REPL execute inside QEMU while recovery state remains outside the language.
12. **Frozen kernel contract (complete at v0.1.2):** a versioned, backend-neutral
   object/rights/operation contract, an executable reference model, and an
   81-step conformance corpus whose canonical transcript is frozen and diffed.
   See [`kernel-contract.md`](kernel-contract.md).
13. **Research-kernel isolation (complete at v0.1.2):** kernel-built page tables,
   per-domain address spaces, write-xor-execute, descriptor tables, trap entry,
   a preemption timer, ring-3 protection domains, and a syscall boundary. An
   unprivileged world answers the whole conformance corpus, and worlds that
   fault, execute privileged instructions, or never yield are contained without
   losing the recovery monitor.
14. **Portable isolation backend (complete at v0.1.3):** the same contract, the
   same corpus, and the same containment tests on x86-64, AArch64, and RISC-V
   from one source, with byte-identical transcripts. Only address spaces,
   register frames, trap entry, and the privilege transition are
   per-architecture.
15. **Split privileged services (started at v0.1.5):** the console driver runs in
   its own unprivileged, restartable domain on all three research backends,
   holding the device by whatever mechanism the architecture uses to grant one.
   The supervisor prints through it, can lose it, replaces it at a new
   generation, and refuses handles issued before the restart. Timers, storage,
   networking and model brokering are still the supervisor's.
16. **Complete self-host:** reader, hygienic expander, agent runtime, image codec,
   and compiler in Agel; extend diverse comparison to every kernel semantic.
17. **Native evaluator world (complete at v0.1.6):** the fixed-memory evaluator
   runs at the lowest privilege level on all three research backends. The x86-64
   interactive workshop sends source over a bounded shared page and prints
   through the restartable console domain. At this rung the full agent runtime,
   allocator, persistent images, and in-OS editor were still future work; device
   access stays outside mutable language heaps.
18. **Durable native workspace (complete at v0.1.7):** a bounded named-source-cell
   editor runs in the x86 workshop. Canonical cells are validated by replay into
   a fresh evaluator, committed to alternating raw-disk slots, and reconstructed
   at boot; a corrupt or semantically invalid newest generation falls back to
   the preceding slot.
19. **seL4 backend (complete at v0.1.4):** the same kernel contract over an
   unmodified seL4 kernel, composed with Microkit on AArch64. Four protection
   domains — recovery, world, broker, serial — where the contract is answered
   by an unprivileged server and the kernel knows nothing about Agel. The
   configuration is MCS and therefore not a proved one; the release manifest
   says so.
20. **Agentic desktop object model (complete at v0.2.0):** retained scene nodes,
   semantic authority-bearing intents, structural validation, inspectable
   patches, and a typed preview/commit/discard/rollback desktop agent are Agel
   standard-library code. This is not yet a renderer or native graphical shell.
21. **Default shell and deterministic layout (complete at v0.2.1):** a
   COSMIC-inspired panel/workspace/dock scene, theme tokens, fixed/flexible
   geometry, validated display lists, semantic hit-testing, and a transactional
   layout agent are Agel library code. No pixel renderer is claimed yet.
22. **Hosted vector graphics (complete at v0.2.2):** Agel-authored paths,
   curves, shapes, paints, transforms, clips, and UI-to-vector compilation feed
   a bounded deterministic SVG output service.
23. **Native vector desktop (complete at v0.2.3):** the BIOS hands off a
   1024×768×32 VBE framebuffer. Only a ring-3 compositor maps its device pages;
   it consumes a build-validated Agel vector stream, rejects malformed records
   without changing the frame, and can fault and be replaced while the last
   good pixels remain. This is output, not yet interactive input.
24. **Live system:** boot-selector-backed A/B worlds, health oracles, signed
   promotion, and watchdog-triggered rollback managed by the recovery monitor.
25. **POSIX personality:** a Rust C library and the filesystem and process
   services beneath it, running unprivileged above the contract, so that
   Unix-like software builds and runs on Agel. A path resolves through a
   namespace capability; there is no ambient root.
26. **Local inference:** model inference in its own domain, over quantized
   weights, requiring no proprietary kernel-mode driver. External providers
   already work through the same capability-scoped effect boundary.

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
