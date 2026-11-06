# v0.3.3 — Agel-written evolution policy

`agel/evolution` implements immutable version records and active pointers for
cells, programs, domains and kernel-slot names. An edit carries its exact
before/after source. A stale base fails before asking the judge. A judge
receipt precedes replay; only an explicit yes followed by a nonempty passing
corpus commits. Refusals, exceptions, differing results and empty corpora
preserve the active version and append evidence to the returned state.

Revert is another judged, replayed edit, retaining all prior versions and
history. Bounded retry passes the complete attempt history, including refusal
reasons, to the proposal function. Costs are integer microdollars; missing,
negative or invalid costs count as unknown. No model calls occur in these
tests; judge receipts are deterministic fixtures.

Replay evaluates code-as-data with the existing Agel-written metacircular
evaluator and its explicit arithmetic/collection environment. Candidate code
has no implicit access to host file, model or process operations. This is a
functional tool replay contract, not replay of game effects. Process-level
fuel exhaustion still needs supervisor recovery; a language exception handler
is not a watchdog.

Validation: `cargo test -p agel-stdlib`, `sh scripts/test-evolution.sh`.
The latter builds a boot image and runs the same Lisp contract in the
`agel` protection-domain process. Cases cover live replacement, source diffs,
macro-generated tools, bad replay, throwing code, judge failure, stale edits,
bounded repair, historical revert and namespace separation.

The Rust change only includes the new Agel source in the library; the policy
and its assertions are Agel. The existing Rust bootstrap remains. This
release does not add protected append-only OS storage, activate real domain
or kernel-slot replacements, replace DOOM or claim an Exo benchmark win.
The remaining work is in [the release programme](exo-programme.md).
