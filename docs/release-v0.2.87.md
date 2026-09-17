# Agel v0.2.87 — Proper tail calls, and the research behind what comes next

Two things. The hosted evaluator gains proper tail calls: a call in tail
position runs in its caller's frame, so a loop written as a function
calling itself last costs one level of depth and runs until its fuel is
spent, not until a depth of 256. And six research briefs — on JITs, tail
calls, memory management, what Smalltalks and Lisps did, Realtalk and
live systems, and what an AI-native language and a Rust substrate should
be — are in `docs/research/`, with the decisions taken from them in
[`docs/research/decisions-2026-09.md`](research/decisions-2026-09.md).
Tail calls were the first of those decisions to act on: the backend
written in Agel had to represent code as a tree because passes could not
recurse by length, and the evaluator that will one day be written in
Agel needs a loop that does not grow.

## What changed

- **`eval_tail` and a loop in `apply`.** The tail positions — the
  branches of `if`, the last form of `begin`, of `let` and of a function
  body — evaluate to a value or to a *pending call*, which the closure
  loop in `apply` makes in the same frame; a builtin there is applied
  and returned. Call depth is counted once per chain of tail calls;
  non-tail nesting (an operand, a handler body) counts as before. Fuel
  is charged exactly as the recursive path charged it, form by form, so
  every documented step count and every replay is unchanged.
- **`with-handler` and `with-restart` bodies are not tail positions**, by
  design: their result is inspected by the handler, so a condition a
  tail-called function signals inside them is still caught.
- **The research briefs** are kept as drafted by the agents, marked as
  background from cited sources and reviewed; the decisions document is
  what Agel takes from them and where they were wrong about Agel.
- **The roadmap** gains the ladder those decisions set: real tail calls
  in the backend written in Agel, a per-agent Cheney collector with a
  shadow stack there (lists and texts, then `agel/meta` compiled in the
  guest), fuel inside emitted code with a conformance test, persistent
  collections for the host's transactions, a claims store with
  provenance, and Agel Slang.

## Proof

`crates/agel-core/tests/language_core.rs`, `proper_tail_calls`: a sum
by a hundred thousand tail calls answers 5,000,050,000 in one frame;
`even?`/`odd?` to 10,001 by mutual recursion; a loop through `begin` and
`let` to 5,000; non-tail recursion to 300 is still `resource/call-depth`
and to 200 answers; a handler catches what a function a thousand tail
calls deep signals; and the same program in tail and non-tail form
differs in steps by exactly the three ticks of each `(+ 0 …)` that made
it non-tail. Two older tests that used tail-recursive loops as "deep
recursion" now use non-tail recursion, and one asserts that the tail
form ends on fuel. The whole workspace's tests pass; the OS suite runs
the library and the backend on the new evaluator unchanged.

## Not claimed

The backend written in Agel still compiles a tail call as a call and a
return; that is the next rung. The evaluator's own passes over long
lists (the reader, `append`, the backend's trees) are recursion by length
where they are not tail calls, and the depth budget still applies to
them. Nothing in the briefs is a claim about Agel's code; the decisions
document is.
