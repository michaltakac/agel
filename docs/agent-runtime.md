# Agel Milestone 2 agent runtime

Milestone 2 turns passive identities into a deterministic actor system. It keeps
Agel's outer world transaction and adds a nested transaction around each agent
turn. This gives failures precise semantics without threads, locks, or shared
mutable heaps.

## Typed protocols

Protocols describe message tags and payload types:

```lisp
(defprotocol counter-protocol
  (add int)
  (read agent)
  (reset))
```

A message is a tagged list such as `'(add 5)`. Supported payload types are
`any`, `nil`, `bool`, `int`, `string`, `symbol`, `list`, `map`, `agent`,
`module`, `capability`, `callable`, and `protocol`. Invalid messages are rejected
before entering a mailbox. `system/*` tags are reserved and can only be enqueued
by the runtime, preventing supervision-message spoofing.

## Behaviors and isolated heaps

An executable behavior is a closure with three parameters:

```lisp
(fn (self heap message)
  ...new-heap...)
```

Its return value atomically replaces its private heap. The behavior may send
messages or spawn children, but cannot use `def`, alter modules/macros/protocols,
run a nested scheduler, consume another mailbox, inspect another heap, or inherit
ambient host capabilities. Any such attempt is an `agent/isolation` condition.

Create an active agent with:

```lisp
(spawn "name" behavior initial-heap protocol
       optional-supervisor optional-policy optional-max-restarts
       optional-capability-list)
```

The one-argument `(spawn "name")` form remains a passive mailbox, useful as an
observer or external boundary.

## Deterministic scheduling

`(step)` executes at most one ready turn. `(run)` drains the ready queue within
the transaction budget, while `(run n)` executes at most `n` turns. Agents are
round-robin FIFO: one message per turn, then an agent with more work returns to
the back of the queue.

`(pending-turns)` returns the number of ready agents. `(agent-info actor)` returns
a map containing its name, status, heap, mailbox size, restart count, supervisor,
policy, and protocol.

## Transactional failure and supervision

Each turn starts after its input message is dequeued. If the behavior signals a
condition, every effect produced during that behavior—including outgoing
messages and spawned children—is discarded. The failed input is consumed and
one of these policies runs:

- `restart`: reset the heap to its initial value up to `max-restarts`; exhaustion
  stops the child and escalates to its supervisor.
- `stop`: stop the agent without escalation.
- `escalate`: stop it and enqueue `(system/child-failed child condition)` to its
  supervisor.

Resource-limit conditions abort the whole scheduler transaction rather than
being treated as application failures, preserving the outer budget guarantee.
Supervisor links always point to older agents, so construction forms a tree and
cannot introduce cycles.

## Events, snapshots, and replay

Spawn, enqueue, turn start/commit/failure, restart, stop, and escalation append
sequenced structured events. `(event-log)` exposes them as ordinary Agel data;
`:events` prints them in the REPL. Events are append-only within a world branch
and roll back with that branch's transactional state.

The Rust API provides `World::snapshot`, `World::from_snapshot`,
`World::restore_snapshot`, and `World::replay`. Replay starts from an immutable
snapshot and applies an ordered transaction-input log, returning values, events,
fuel use, and a deterministic final-state checksum. The checksum detects replay
divergence within a runtime version; it is deliberately not a cryptographic
signature.

Milestone 3 adds model-requested, model-dispatch-started, model-completed,
model-failed, and model-delivery-dropped events.
Provider completions can be part of `World::replay_inputs`; the external model
is never called during replay. See [`model-agents.md`](model-agents.md).

The REPL offers `:snapshot NAME`, `:restore NAME`, and `:snapshots` for live
branching. Restoring creates a fresh monotonic world revision.

## Demonstrations

```sh
cargo run -q -p agel-cli < examples/counter.agel
cargo run -q -p agel-cli < examples/resilient-system.agel
cargo run -q -p agel-cli < examples/time-travel.agel
cargo run -q -p agel-core --example deterministic_replay
```

These demonstrate typed stateful agents, rollback of a failed turn, bounded
restart plus escalation, structured observability, and live snapshot branching.
