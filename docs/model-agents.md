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

## Replay semantics

Provider execution itself is never replayed. `ReplayInput::ClaimModel` records
the durable dispatch transition and `ReplayInput::CompleteModel` records the
exact response alongside ordinary `ReplayInput::Evaluate` inputs.
Replaying that ordered log reconstructs the same messages, events, agent heaps,
and state digest without contacting Claude or Codex.

This distinction is fundamental: transactional memory protects Agel state; the
outbox and completion log make irreversible effects explicit and auditable.
