# Native agents in Agel OS

Project v0.2.8 moves the first executable agent semantics through the host/VM
boundary. Agents now live in the fixed-memory world owned by the unprivileged
native evaluator domain, so the serial and graphical workshops execute the same
actor primitives without a host process or Rust allocator.

The native surface is intentionally small:

```text
spawn  send  step  run
agent-state  agent-pending  agent-turns  agent-faulted?
restart-agent  drop-message  agent-count
```

`(spawn behavior initial-state)` requires a stored three-argument function. On
each turn Agel invokes `(behavior self state message)` and commits its scalar
result as the next state. `send` appends a scalar message to a FIFO mailbox.
`step` schedules at most one ready agent and `run` schedules at most the supplied
number of turns. Selection is deterministic round-robin, including sends an
agent performs from inside its own behavior.

```lisp
(def accumulate (fn (self state message) (+ state message)))
(def counter (spawn accumulate 0))
(send counter 20)
(send counter 22)
(run 2)
(agent-state counter) ; 42
```

## Transaction and fault boundary

The native evaluator already copies the complete world before every submitted
form. Agent tables, mailboxes, state, turn counters, and scheduler position are
now part of that world. Therefore this does not consume a message:

```lisp
(begin (run 1) (/ 1 0)) ; error; the submitted world is discarded
```

Each behavior turn also takes an inner world checkpoint. If the behavior fails,
all of that turn's state writes and sends are discarded, the input remains at
the head of its mailbox, and only that actor becomes faulted. Other actors can
continue. The operator can inspect it, remove a poison message with
`drop-message`, and explicitly `restart-agent` without rebooting the OS.

Scheduler calls from inside a behavior are rejected, preventing recursive
scheduler corruption. Ordinary sends remain valid and transactional. A mailbox
overflow aborts its containing turn instead of dropping a message. Agent IDs
are values (`#<native-agent:N>`), not ambient pointers or kernel capabilities.

## Hard bounds and durability

The seed admits eight agents, eight queued scalar messages per agent, and at
most 32 scheduler turns per `run`. All work shares the submitted form's 2,000
evaluation-step budget. `:limits` reports these constants from the running
implementation.

Fuel exhaustion rejects the entire submitted form, including earlier turns
within that form. Ordinary behavior errors fault the selected actor. `step`
returns whether a turn was attempted; `run` counts attempted turns, including
faults. Inspect `agent-faulted?` to distinguish a failed turn from a committed
one. Recovery operations are rejected inside behavior execution.

Actors currently share the evaluator's global definitions and one protection
domain. Their handles do not establish security isolation between mutually
untrusted behaviors. Behaviors are copied at spawn time, while their global
references resolve at call time. Native actor slots are retained for the
session; reclaiming slots and stronger per-actor authority are future work.

Workspace images continue to store source, not live memory or authority. Save
behavior definitions in cells; after reboot, replay reconstructs those
definitions and a fresh form spawns fresh agents. This avoids serializing stale
mailboxes or capability-like identities while making native agent programs
durable and editable from the graphical workshop.

This is the downward-bootstrap seed, not parity with the hosted runtime yet.
Native messages and states are scalar, and protocols, supervision trees, model
requests, persistent collections, and richer trace data remain hosted Agel
libraries to be ported downward. In particular, a native turn never invokes an
AI model implicitly: model requests will remain explicit, metered effects when
that adapter crosses the boundary.

Try the complete workshop sequence in
[`examples/native-agents.txt`](../examples/native-agents.txt).

Validation runs the exact native evaluator through adversarial host tests
(`sh scripts/test-native-agents.sh`), plus the real serial and graphical QEMU
workshops. The example also demonstrates self-message continuations: each
logical step returns to the scheduler, so the operator can inspect progress
between steps without accumulating a recursive call stack.
