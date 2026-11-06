# v0.3.10: Live Agel tools during RuneScape play

The version store now drives executable calls through a running Agel actor.
Load `crates/agel-stdlib/live-tools.agel`, import `agel/live-tools`, and create
an actor with `live-tools`. Send `edit`, `call` or `revert` messages. Each call
resolves the active version and evaluates its source through `agel/meta`.
It does not retain a stale closure across edits. All implementation logic is
Agel; this release adds no Rust runtime or host behavior.

Edits carry an exact before/after syntax diff, recorded input/output cases
and a judge receipt. Existing evolution policy checks the base, judges the
candidate, replays it, then changes the active version or records refusal.
The actor retains call requests/results and evolution history. Calls that
raise a condition return an error and preserve the actor. An edit is applied
between mailbox turns; an already executing call is not patched mid-frame.
Replay runs with the restricted metacircular base environment, without game
transport effects.

## Executable evidence

Run `sh scripts/test-live-tools.sh` with the private world running. The
[recorded run](evidence/v0.3.10-live-tools.txt) demonstrates:

- One actor processes calls queued on both sides of an edit, changes behavior,
  and preserves its identity, mailbox and call history.
- Stale edits, behavioral regressions and an unsafe revert preserve version 2.
  A macro produces a different equivalent implementation, which is committed
  and reverted through the same gates. Failed calls preserve the actor.
- On one actual RuneScape connection, the initial observation has no target.
  The old tool returns an invalid fallback of zero. That value is recorded,
  never sent as an attack. A scripted repair adds a readiness check; replay
  includes both that observation and a later observation with a visible rat.
  The next call returns nil for the initial observation. A third call returns
  a real rat observation and drives the existing combat test.
- The fresh normal account gains exactly 8 Attack XP and increased Hitpoints
  XP. A separate read-only server oracle verifies saved XP and 1× account rates.

The generic Rust test harness only loads source and checks the Agel result.
The existing byte carrier, client ciphers, live packets and world profile are
unchanged. No evaluator, application or socket reload occurs during the edit.

## Scope and next experiment

The repair and zero-cost judge are deterministic test fixtures. These judge
receipts and tool calls remain in the actor heap; this test does not exercise
the protected OS judge ledger. The live-tools module is explicitly loaded,
not added to the default bootstrap image. Native game transport, durable
activation/recovery, replacement of actual OS services and extension of the
running evaluator remain unfinished. This is not autonomous self-improvement
or an Exo comparison.

The next agent experiment must require an Agel agent to explain an observed
failure, predict the behavior of its proposed diff, select or build a tool,
and submit that diff with its reason. Replay must include held-out failures,
not only the examples used to propose the patch. Measure the prediction,
repair correctness, retained session state, time and all model/judge costs
against a frozen policy on matched accounts. That separates understanding
from an implementation that merely follows a supplied repair script.
