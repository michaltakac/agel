# Agel Milestone 3: real model agents

Milestone 3 connects transactional Agel agents to locally installed Claude Code
and Codex CLIs without pretending an external inference can be rolled back.
The core runtime remains deterministic; provider execution lives behind the
typed `agel-effects` boundary.

## The effect protocol

An agent calls:

```lisp
(model-request 'claude "Analyze this proposal" self)
```

This does **not** start a process. It adds a `ModelRequest` to the candidate
world's durable outbox. The turn must commit before the host can observe it. If
the behavior subsequently fails, the request disappears with every other
provisional turn effect.

The agent must possess a host-issued `model/infer` capability scoped to the
provider. A capability for `claude` cannot call `codex`, and merely enabling a
provider does not implicitly give it to an agent. The host makes the handle
available, Agel source requests it, and `spawn` explicitly delegates it.

At the REPL, `:dispatch` is a second, visible gate. Before process launch it
atomically claims each request, moving it from `pending` to `dispatching`. This
prevents an interrupted or failed completion commit from silently invoking a
paid request twice; such a request remains visibly `dispatching/in-doubt` for
operator reconciliation. It then invokes enabled providers. The exact success
or failure is committed through
`World::complete_model_request` and delivered as one of these unspoofable
runtime messages:

```lisp
(system/model-result request-id provider text)
(system/model-error request-id provider error-kind message)
```

Normal `send` still validates the target protocol and cannot manufacture a
`system/*` message. Completion is idempotence-guarded, so a request cannot
deliver twice. If its target has stopped or its mailbox is full, the result
remains completed and the runtime records `model-delivery-dropped`; it never
reissues the external call merely because delivery became impossible.

## Provider process boundary

No provider is enabled by default, and no request is automatically dispatched.
The adapters invoke binaries directly without a shell and send prompts on
stdin.

- Claude Code uses print mode, text output, no session persistence, restricted
  execution, and no permission-prompt tool. A model and per-invocation dollar
  cap can be configured.
- Codex uses `exec`, an ephemeral session, ignored user project configuration,
  a read-only sandbox, and an explicit working directory.
- Both have host-enforced time and captured-output limits. Nonzero exits,
  invalid UTF-8, timeouts, and excessive output become structured model-error
  messages rather than runtime crashes.
- Both clear ambient process environment and restore only named login/config
  variables required to find the executable and existing subscription state.
- Since v0.2.22 each adapter's sandbox consults a default-deny effect policy
  that admits only `model/infer/<provider>/request/*` process intents before
  the executable allowlist is checked; a denial is audited and never spawns.
  Every decision and outcome is retained in a typed audit log; use `:effects`
  after dispatch to inspect it.

This milestone intentionally does not use either CLI's dangerous permission
bypass. Later milestones may offer write-capable coding agents, but only behind
a separate proposal/verifier/canary capability—not by weakening `model/infer`.

## Running real agents

The CLIs must already be installed and authenticated. Opt in at startup:

```sh
cargo run -p agel-cli -- --enable-claude \
  --claude-max-budget-usd 0.25

cargo run -p agel-cli -- --enable-codex
```

Useful flags include `--claude-model`, `--codex-model`, `--model-workspace`,
`--model-timeout-seconds`, and `--model-max-output-bytes`. Use `--help` for the
complete list.

Inside the REPL, `:providers` shows enabled adapters, `:requests` shows the
committed outbox, and `:dispatch` performs the paid/nondeterministic work.

The two-provider demonstration is:

```sh
cargo run -q -p agel-cli -- --enable-claude --enable-codex \
  --claude-max-budget-usd 0.25 < examples/model-swarm.agel
```

`examples/model-fixed-point.agel` demonstrates model requests inside bounded,
traceable logical continuation loops. The fixed-point combinator does not call
a provider automatically: only an explicit `fixed-model` transition creates an
outbox record, and `:dispatch` remains the irreversible gate. See
[`agentic-fixed-points.md`](agentic-fixed-points.md) for the full cost and trust
analysis.

## Typed judgments (v0.2.91)

A third kind of provider answers typed questions instead of writing text:
`jev`, TypeSafe's System One model, enabled with `--enable-jev` and the
key in `TYPESAFEAI_API_KEY`. Its prompt is a `(judge ...)` form and its
result one line of integers in thousandths, read by the `agel/judgment`
library; the same `model/infer` capability, outbox, `:dispatch` and
`system/model-result` path carry it. See [`system-one.md`](system-one.md).

## A judged gate on dispatch (v0.2.92)

`--gate agel|jev` adds a third gate between the outbox and the provider,
after the capability and before `:dispatch` invokes anything. For each
pending request the host consults the gate: `agel` evaluates
`(effect-gate REQUEST)` in the world — `REQUEST` is
`("model/infer" "provider" ID AGENT "text")`, the answer `allow`, `deny`
or either with a text, and `make-gate` from `agel/judgment` builds one
from rules — as an ordinary committed input, so an image holds it; `jev`
asks the System One model one yes/no question about the request and
allows at a threshold in thousandths. A denial is committed as the
request's completion:

```lisp
(system/model-error request-id provider effect/denied "gate agel: run noul 90")
```

and replays as any completion does. A gate that cannot decide leaves the
request pending. `:effects` lists each verdict with the line it was given.
The gate can only refuse: it grants nothing the capability did not.

## Replay semantics

Provider execution itself is never replayed. `ReplayInput::ClaimModel` records
the durable dispatch transition and `ReplayInput::CompleteModel` records the
exact response alongside ordinary `ReplayInput::Evaluate` inputs.
Replaying that ordered log reconstructs the same messages, events, agent heaps,
and state digest without contacting Claude or Codex.

This distinction is fundamental: transactional memory protects Agel state; the
outbox and completion log make irreversible effects explicit and auditable.

## Native repair controller

v0.3.13 exposes the same `agel/repair` policy as pure session transitions:
`repair-new`, `repair-start`, `repair-pending`, `repair-complete`, `repair-fail`
and `repair-advance`. `repair-exchange` logs before inference and before
policy delivery. With `defer-activation`, approved text waits for a separate
advance; no version is committed at approval. `kind` selects the versioned
object, and an explicit activation hook supplies native execution evidence.
`agel/inference` maps caller-configured routes to a bounded byte envelope.
See [the native example, real records and limits](release-v0.3.13.md).

`--scenario runtime` exercises the same controller against versioned language
forms in v0.3.14. An existing runtime closure uses a repaired form without
recreation. See [the runtime API](live-runtime.md) and [native evidence](release-v0.3.14.md).

## Polling inference during a live byte session

The optional `agel/inference` module also exports `inference-start`,
`inference-poll` and `inference-serviced`. Start sends
`AGEL-INFER/2 start ROUTE\nHEX-PROMPT` and receives either a completed reply
or `{status pending, job "ID"}`. Poll sends `AGEL-INFER/2 poll ID` and checks
that a pending response retains the same ID. A job is submitted once; polling
never resubmits the prompt. The native companion admits one outstanding job,
refuses a second start, and consumes a result only on a matching poll. A
wrong or already-consumed ID is an error. Provider work remains bounded by
the existing 90-second process timeout and 1900-byte native reply limit.

`inference-serviced(routes, ask, provider, prompt, service, state, polls)`
invokes the Agel callback `(service state)` before each poll. It returns
`{reply, state, polls}` or an additional `error` field. State acknowledged by
a successful callback survives a subsequent poll error. The callback owns
all maintenance decisions and effects. At budget exhaustion the pending job
is returned, so a caller can continue with `inference-poll` rather than submit
again. The poll budget is 0–4096. A service callback that fails after a partial
external effect cannot promise rollback; close or reconcile that resource.

The combined `guest_inference` companion accepts an optional numeric endpoint
in `AGEL_BYTE_ENDPOINT`. It shares the same bounded TCP/entropy carrier as
`byte_bridge`, runs the selected provider job on a worker thread, and waits up
to 250 ms for a result per poll. Agel chooses when to poll and which service
bytes to send. The carrier has no heartbeat timer, game policy, diagnosis or
activation logic. Both waiting bootstrap words relay protected-ledger requests,
so receipts can be committed while Agel waits for a byte acknowledgment.

This is cooperative servicing, not preemption. Long Agel computation or an
ordinary blocking model call can still starve a connection. Jobs are owned by
one live broker and are not recoverable after a broker crash or OS reboot.

For a mailbox-driven repair, set `model-dispatch` to `external` in
`make-repair-agent` options. The retained actor then creates logical pending
requests in its heap without dispatching a second runtime model effect.
After its `repair/start` turn, a top-level controller calls
`repair-agent-exchange(actor, infer, append-record)`. This logs the exchange,
invokes the caller's inference adapter, delivers the completion to the same
actor, and schedules until that actor changes state. Other actors can run
between those turns. With deferred activation the judge result leaves the
actor `approved`. A following exchange advances activation, or first runs the
optional top-level `probe` callback. The callback receives the pending change
and returns a receipt with `passed` and `reason`. The controller logs and
delivers the receipt for that pending ID. A failed receipt retries with its
reason; a passing receipt allows the remaining recorded replay before commit.
A plain advance cannot bypass a configured probe. The controller is trusted;
the mailbox is not an authentication boundary. Existing implicit hosted
model dispatch remains the default.
The repair actor can have no effect capabilities; the controller owns provider
and audit I/O. Nested scheduler calls from inside a behavior remain refused.
