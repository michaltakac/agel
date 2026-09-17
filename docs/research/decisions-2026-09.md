# Research, September 2026: JITs, tail calls, memory, and the systems that came before — decisions

Six briefs were drafted by research agents from cited public sources and
are kept beside this document: [`jit.md`](jit.md), [`tail-calls.md`](tail-calls.md),
[`gc.md`](gc.md), [`smalltalk-lisp.md`](smalltalk-lisp.md),
[`realtalk-live-systems.md`](realtalk-live-systems.md) and
[`ai-native-rust-selfhosting.md`](ai-native-rust-selfhosting.md). This
document is what Agel takes from them: the decisions, the order, and
what was done first. It is written against v0.2.86, where the hosted
runtime, the toolchain written in Agel and an integer x86-64 backend
written in Agel all run inside the OS, and the evaluator was a
tree-walker with no tail-call elimination and a call depth of 256.

## What the briefs agree on

1. **Keep the evaluator boring and the cores small; put cleverness in
   replaceable consumers of a stable IR.** Self's "simple runtime, smart
   compiler", Little Smalltalk's auditability, Chez's nanopass discipline
   and Sista's optimizer-as-a-consumer-of-bytecode all say the same thing.
   Agel's IR (`const local builtin fn if begin call tail-call`) is that
   stable surface; `agel-jit` and `agel/native-x86` are its consumers.
2. **Do not build a tracing JIT, inline caches, deoptimization or OSR
   now.** They amortize dynamism Agel's closed IR does not have, and
   their triggers are wall-clock heuristics that fight determinism and
   replay. If a middle tier is ever wanted, its trigger must be part of
   the replayable state.
3. **Copy-and-patch is the right shape for the backend written in
   Agel.** Stencils per IR node, patched by copying bytes, keep the
   backend small, deterministic and extensible — which is what
   `agel/native-x86` already is in miniature. Fuel accounting belongs
   *inside* each stencil (a reserved register or slot decremented by a
   static cost), never a separate pass, with a conformance test that the
   tree-walker and compiled code charge the same steps.
4. **Proper tail calls are a semantic guarantee, not an optimization**
   (R7RS, Clinger). For the tree-walker: loop in the tail positions,
   count depth only for non-tail nesting, let fuel bound loops. For the
   native backend: a tail call must reuse the frame; with a separate
   argument area the arity mismatch stops being a problem.
5. **Memory: one small collector per agent, not one big one.** BEAM's
   per-process heaps and Pony's ORCA are Agel's isolated agent heaps;
   the transaction is already a region and the agent turn already an
   arena (discard on rollback). For the backend the first real collector
   should be a Cheney semispace per agent with a shadow stack for roots,
   collected at safepoints/commit — three to six hundred lines of Agel.
   For the hosted runtime, the clone-per-transaction cost is best cut by
   persistent, structurally shared collections before any GC work.
   Perceus-style reference counting with reuse (Koka, Lean 4) is the
   strongest *alternative* to tracing for compiled Agel and stays on the
   table; MMTk is a reference design, not a dependency.
6. **Self-hosting continues the Squeak/Slang way**: a restricted,
   mechanically compilable subset of Agel ("Agel Slang": a fixed heap
   model, lists and texts with known layouts, tail calls, explicit roots,
   no runtime macros) in which the evaluator itself is written, run first
   interpreted by the hosted runtime, then compiled by the backend in the
   guest, then boot-imaged — each step proven by three-way byte-equality
   of digests (Rust reference, Slang-interpreted, Slang-compiled). The
   host stage is kept forever (Jikes RVM, Maxine, Racket CS): the metric
   is Rust's shrinking share, not zero Rust.
7. **What an OS for agents needs, and where Agel stands.** Deterministic
   replay with a model-effect journal (Agel has it; no surveyed agent
   platform does), capability-scoped effects (Agel has it), no hidden
   state (largely), and — Realtalk's strength and Agel's clearest gap —
   *provenance*: who currently claims a fact, since when, on what
   evidence, as a query rather than a replay. A claims/wishes/`when`
   store over agents (a small Datalog matcher, re-evaluated on commit,
   not per frame), per-binding provenance, and later Croquet/TeaTime-style
   ordered replication across machines, are the recommended additions.
   Content-addressed code (Unison) and typed effects (Koka, Verse) come
   after provenance.
8. **Rust's permanent set** is the hardware bring-up, the MMU and
   protection domains, the loader, and any collector the host adopts —
   Theseus, Hubris and Tock all draw the line there. Everything above it
   migrates upward as the backend's coverage grows.

## Decisions

- **Done first, this release (v0.2.87):** proper tail calls in
  `agel-core`. Tail positions (`if` branches, the last form of `begin`,
  `let` and a function body) hand a pending call back to the closure
  loop in `apply`, which runs it in the same frame; call depth counts
  only non-tail nesting; fuel is charged identically to before, so every
  step count in the project's tests and documents is unchanged; the
  bodies of `with-handler` and `with-restart` are deliberately not tail
  positions, since their result is inspected. Proofs in
  `crates/agel-core/tests/language_core.rs` (`proper_tail_calls`).
- **Next, in order:** (1) real tail calls in `agel/native-x86` with a
  separate argument area; (2) a Cheney semispace per agent with a
  shadow stack in the backend, growing its IR coverage to lists and
  texts — the step that lets the guest compile `agel/meta`; (3)
  persistent collections for the hosted runtime's transactions; (4)
  fuel inside the backend's emitted code with a conformance test; (5)
  `agel/claims` with provenance; (6) Agel Slang v0 and the three-way
  proof. The roadmap carries each as a row.
- **Not doing:** a tracing or meta-tracing JIT, inline caches, OSR,
  WebAssembly as a target, MMTk as a dependency, general continuations
  (agents and mailboxes are the cheaper mechanism), an exotic notation
  for determinism's sake, and rewriting the evaluator in Agel "for its
  own sake" ahead of the backend's coverage.

## Where the briefs were wrong about Agel, for the record

They were written from a description, not the code. Two corrections:
`with-handler` in `agel-core` is a Rust `match` over the body's result,
not a handler stack, so the tail-call change needed no explicit
handler-frame bookkeeping (the body is simply not a tail position); and
the model-effect journal is `agel-core`'s, not `agel-effects`'. Nothing
else material.
