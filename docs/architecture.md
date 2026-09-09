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
first conservative verification gate, and since v0.2.22 the CLI exposes it as
`:propose`, `:promote` and `:discard`. This is not a claim that macros—or the
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
   generation, and refuses handles issued before the restart. v0.2.26 moves
   the x86-64 disk driver into the same kind of domain. Timers, serial input,
   networking and model brokering are still the supervisor's.
16. **Complete self-host (in progress):** reader, hygienic expander, agent
   runtime, image codec, and compiler in Agel; extend diverse comparison to
   every kernel semantic. Rungs 32 through 36 supply the reader, a restricted
   expander, a compiled agent kernel and the compiler frontend as Agel; the
   image codec, the backend and the in-guest toolchain remain Rust or host-side.
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
   standard-library code. At that rung this was not yet a renderer or native
   graphical shell; rungs 22 through 24 supplied both.
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
   good pixels remain. That rung was output only; rung 24 added input.
24. **Live native desktop (complete at v0.2.4):** nonblocking serial and PS/2
   keyboard adapters normalize bytes into a visible command surface. A bounded
   Lisp grammar produces semantic candidate scenes; complete frames are
   validated and rendered before the revision advances, rejected input leaves
   state untouched, and rollback restores the preceding scene while QEMU runs.
25. **Graphical kitchen sink (complete at v0.2.5):** one Agel value combining
   shell, agent graph, inspectors, gradients, clipping, paths, transforms and
   scalable text renders to a frozen SVG digest and a checked 2880×1800
   screenshot.
26. **Persistent graphical workshop (complete at v0.2.6):** the graphical
   command surface and the crash-tolerant named-source-cell workspace share
   the protected native evaluator, with replay-validated save and
   reconstruction after reboot.
27. **Agentic fixed points (complete at v0.2.7):** eager-safe and bounded
   lexical fixed points, immutable convergence, and a transactional agent
   fixed-point driver with bounded tracing, explicit model transitions and
   message-ordered code evolution, all as Agel library code.
28. **Native agents (complete at v0.2.8):** the first downward bootstrap of
   executable agents into the freestanding evaluator: bounded scalar
   mailboxes, deterministic round-robin turns, atomic behavior turns,
   inspection, and contained fault recovery inside the graphical OS.
29. **Layout-aware input (complete at v0.2.9):** native punctuation and
   modifiers, and a loopback host-layout console for Unicode composition and
   paste without mouse capture.
30. **Agel-authored live native scenes (complete at v0.2.10):** committed
   native scene records reach the compositor; an Agel dock library, actor-driven
   repaint, rollback and reboot replay.
31. **Native agent workbench (complete at v0.2.11, repaired at v0.2.12):**
   pointer-to-agent actions, keyboard focus, source inspection, isolated
   candidate previews, turn-boundary behavior replacement, explicit
   promotion/discard, persisted source upgrades, failed-save recovery and
   hardened process limits.
32. **Agel in Agel (complete at v0.2.13):** an Agel-written functional
   interpreter, source-backed hosted agents, and a three-evaluator conformance
   corpus shared by the Rust seed, the Common Lisp reference and `agel/meta`.
33. **Analyzed execution and shared closures (complete at v0.2.14):** reusable
   execution plans analyzed in Agel, opt-in analyzed agents, and structurally
   shared immutable closure code and lexical frames in the Rust bootstrap.
34. **Machine code (complete at v0.2.15 and v0.2.16):** an Agel-authored
   integer-IR compiler with an isolated Cranelift backend, then a self-compiling
   Agel frontend over managed native closures, immutable collections and
   metered calls, with three-stage IR agreement.
35. **Compiled actors (complete at v0.2.17 through v0.2.19):** tail-call IR with
   native trampolining, an Agel-written compiled scheduler with peer permissions
   and revision-checked commits, metered compacting collection at tail
   boundaries, and agent-proposed compiled behavior upgrades with owner- and
   revision-bound previews, code-only promotion and checked rollback.
36. **Native reader and modules (complete at v0.2.20 and v0.2.21):** an
   Agel-written reader that reads and rebuilds the reader and compiler from
   text, and an Agel-authored static module linker with restricted expression
   templates whose expanded behaviors reach the real OS through candidate
   validation and source persistence. Compilation remains host-assisted.
37. **Live upgrade pipeline and portable worlds (complete at v0.2.22):** the
   verification gate, portable images and typed effect policy become live paths
   rather than library demonstrations. The CLI verifies and promotes proposal
   files, persists every committed input to a tamper-evident image, and effect
   inference is conservative over first-class builtins. The Common Lisp
   reference and `agel/meta` cover maps and text, and the freestanding
   evaluator gains `let`, variadic arithmetic and multi-form functions.
38. **Native data (complete at v0.2.23):** the freestanding evaluator gains
   strings, symbols, lists and insertion-ordered maps as first-class values in
   a bounded heap inside the transactional world, with the hosted seed's list,
   map and text builtins, structural equality, quoted data that persists in
   globals and travels in agent messages, and a copying collector at every
   commit boundary. This is the first precondition for running the Agel-written
   reader and compiler inside the OS; their working sets still exceed the
   fixed native bounds.
39. **Signed roots and evidence (complete at v0.2.24):** a dependency-free
   Ed25519 and SHA-512 implementation in `agel-integrity`, checked against the
   RFC 8032 vectors; signed portable-image envelopes with verified loads,
   trusted-signer fallback and no unsigned downgrade; A/B promotion evidence
   signed over canonical bytes and a supervisor that refuses unsigned
   promotion once a key is trusted; and operator key generation in the CLI.
   Native disk slots, the seL4 manifest and kernel images remain unsigned.
40. **Diverse kernel contract (complete at v0.2.25):** a second implementation
   of the kernel contract written from the contract document and corpus
   without reading the reference model, reproducing the frozen transcript and
   agreeing with the model on all 81 steps; the seL4 broker runs it, so the
   seL4 transcript is now the agreement of two implementations.
41. **Storage driver domain (complete at v0.2.26):** the primary ATA driver
   runs unprivileged in its own restartable domain, granted exactly the disk's
   ports through the task-state-segment bitmap, exchanging one sector at a time
   through a block area of its shared page. The supervisor keeps the dual-slot
   policy and codec, checks a generation on every request, and CI proves a
   non-driver world touching the disk faults, the driver can be lost and
   replaced, and its old handle is refused.
42. **Live system:** boot-selector-backed A/B worlds, health oracles, signed
   promotion, and watchdog-triggered rollback managed by the recovery monitor.
43. **POSIX personality:** a Rust C library and the filesystem and process
   services beneath it, running unprivileged above the contract, so that
   Unix-like software builds and runs on Agel. A path resolves through a
   namespace capability; there is no ambient root.
44. **Local inference:** model inference in its own domain, over quantized
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
