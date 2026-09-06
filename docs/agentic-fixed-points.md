# Agentic fixed points

Agel already has ordinary recursion. Use it by default:

```lisp
(def fact (fn (n) (if (= n 0) 1 (* n (fact (- n 1))))))
```

`agel/fixed-point` exists for the narrower cases where recursive identity must
be passed, wrapped, replaced, or scheduled as data. It does not make recursion
possible, and it is not a substitute for isolation.

This module currently runs in the hosted Agel agent runtime—the OS/application
layer that already provides agents, protocols, snapshots, effects and real
model adapters. The bootable graphical workshop currently exposes the smaller
native evaluator and cannot yet spawn these agents. Porting the agent runtime
and this ordinary Agel library into that persistent native workspace is a
remaining bootstrap step; the implementation here is the executable semantic
reference for that port.

## Review of the Y-combinator proposal

The motivating proposal contains a sound architectural intuition: a recursive
continuation supplied by the environment is a useful interception point. Four
qualifications matter in a real system:

1. Agel evaluates arguments eagerly. The untyped call-by-name Y combinator
   expands forever before reaching the function body. `fix` therefore uses the
   applicative-order fixed point often named **Z**, delaying recurrence inside
   a closure.
2. A lexical `gas` value is not a meter unless each continuation receives a
   decremented value. The example in the proposal checks the same captured gas
   forever. `fix-bounded` threads a decreasing budget through every supplied
   continuation.
3. Interception is voluntary. A builder can bypass its supplied continuation
   by calling a global function, and an untrusted function can consume resources
   before it recurs. Evaluator fuel, call-depth and collection limits remain the
   enforcing boundary.
4. Lexical retry cannot undo external effects. Agel instead commits a model
   request to a durable outbox, dispatches it through a separate explicit gate,
   and records the exact completion. State rollback never pretends to reverse a
   provider invocation.

For those reasons Agel does not silently desugar every recursive definition
through `fix`. That would add overhead and hide a policy decision. The standard
library exposes three explicit levels:

| Tool | Use | Boundary |
|---|---|---|
| Named recursion | Normal local algorithms | evaluator fuel and call depth |
| `fix`, `fix-bounded`, `converge-bounded` | Anonymous recursion and immutable refinement | lexical continuation plus evaluator limits |
| Fixed-point agent | Long-lived, traceable, evolvable work | one transaction and scheduler turn per continuation |

## Lexical fixed points

`fix` takes a builder. The builder receives the recursive continuation and
returns the callable body. The postcard implementation supplies a unary
continuation; bundle multiple recursive arguments into a list or map:

```lisp
(def factorial
  (fix
    (fn (recur)
      (fn (n)
        (if (= n 0) 1 (* n (recur (- n 1))))))))
```

`fix-bounded` adds an explicit continuation budget. Exhaustion signals
`fixed-point/exhausted`; the surrounding Agel transaction rolls back normally.
`converge-bounded` repeatedly applies a refinement function until an explicit
equivalence predicate says the immutable value is stable. This is the useful
interpretation for composing applications: the fixed point is an application
description satisfying contracts, not two arbitrary programs merging their
authority.

## The agent fixed-point driver

An agent step has no recursive name. It receives `(state event)` and returns one
transparent transition:

```lisp
(fixed-continue next-state observation)
(fixed-done result observation)
(fixed-model next-state 'claude "prompt" observation)
```

`make-fixed-agent` wraps the step in a typed agent. `fixed-start` invokes the
first step. A `continue` queues `(fixed/advance)` to itself, so the next logical
recursive boundary is a new scheduler turn rather than a deeper native stack.
Consequently every boundary gets:

- an isolated private heap and deterministic round-robin scheduling;
- nested transaction commit or rollback;
- evaluator fuel, call-depth and collection enforcement;
- a bounded trace of event summaries and language-authored observations;
- a decreasing logical-step budget;
- a point where another message can inspect or evolve the step; and
- ordinary supervision if the step violates its contract or fails.

`fixed-propose` stages a new closure only when its expected version matches.
The candidate is inspectable but inactive, giving a supervisor or verifier a
place to run canaries. `fixed-commit` installs exactly that preview if the base
version is still current; `fixed-discard` removes it. Stale and preview-less
commits are rejected without changing the active step.

Evolution is mailbox ordered: committed turns keep their old semantics, and
the first continuation after the commit message uses the new closure and
increments the visible version. There is no active recursive stack to patch.
The new closure must still understand the old state representation; future
schema migration should be a separate validated transition, not an implicit
cast. Possession of the fixed-point agent handle grants access to its typed
control protocol, so callers should pass that handle only to the supervisory
agents intended to propose or admit evolution.

Model result and error messages are unspoofable `system/*` inputs. Their text is
passed to the step, but the default trace retains only request id, provider and
outcome kind—not the potentially large model output. A step may explicitly keep
selected output in its state or final result.

## Cost of model-driven fixed points

The combinator itself never calls an AI model. `continue` and `done` are local
Agel work and have zero provider invocations. Only `fixed-model` creates a model
request, and even that merely commits an outbox record. The provider runs only
when the host crosses `:dispatch`.

For a cooperative step with `max-model-calls = N`, one run creates at most `N`
requests through the driver. If Claude is configured with a per-invocation cap
of `B`, the configured ceiling is at most `N × B`; actual subscription or API
accounting remains provider-specific. Codex has the same request-count,
timeout, prompt and output gates, but Agel does not invent a monetary estimate
when its local subscription interface does not report one.

The complete limiting stack is:

- fixed policy: logical steps, retained trace entries, model-call count and
  cumulative prompt characters;
- evaluator: transaction fuel, call depth, collection size, exact per-prompt
  byte size and maximum pending requests;
- capability: explicit `model/infer` scope for one provider;
- host adapter: provider enablement, explicit dispatch, timeout, output bytes,
  read-only process sandbox and Claude's per-invocation dollar cap;
- effect journal: claim-before-launch and exact completion replay, preventing a
  restored world from silently paying for the same request again.

The fixed policy is an inspectable agreement inside the same agent principal,
not a security boundary against malicious step code: a step holding the
agent's model capability could directly invoke `model-request` and bypass the
library counter. Today the host's dispatch gate and runtime budgets remain the
hard spending controls. Fully autonomous paid dispatch will require a
use-limited capability or trusted metering broker in the runtime before it is
safe; changing the combinator alone is insufficient.

One scheduler transaction per logical step also costs more CPU and memory than
ordinary recursion because the current runtime checkpoints candidate state and
records events. That overhead is intentional for long-lived autonomous work,
but wasteful for factorials and tight numeric loops.

As an illustrative v0.2.7 measurement, evaluating factorial 10 consumed 149
deterministic evaluator steps with named recursion and 239 through `fix`, about
60% more fuel. The four-transition countdown example emitted 12 scheduler
events. These are implementation measurements rather than compatibility
promises, but they show why fixed-point instrumentation is opt-in.

## Try it

The deterministic example demonstrates named recursion, Z, bounded application
convergence, transactional stepping, bounded trace, and message-ordered live
evolution:

```sh
cargo run -q -p agel-cli < examples/agentic-fixed-point.agel
```

The two-provider example creates at most one Claude and one Codex request and
keeps `:dispatch` visible:

```sh
cargo run -q -p agel-cli -- --enable-claude --enable-codex \
  --claude-max-budget-usd 0.10 < examples/model-fixed-point.agel
```
