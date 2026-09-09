# Agel v0.2.36 — Actor slots come back too

The native actor table has eight slots and, until this release, no way to
free one. `reap-agent` gives a slot back, and the handle problem that made
reuse unsafe is solved the way the drivers solved it: with generations.

```text
agel-native[44]> (reap-agent broken)
#t
agel-native[45]> (send broken 1)
error: stale native agent (transaction rolled back)
agel-native[45]> (def revived (spawn fragile 3))
#<native-agent:2.1>
```

## What changed

- **`reap-agent`.** Frees the agent's slot, releases any scene rectangle it
  owned, and moves the slot's generation on. Transactional like every other
  form: a failing form that reaps leaves the agent in place.
- **Generation-checked handles.** A handle is the slot number plus the
  generation it was issued against. A handle to a reaped agent answers
  `stale native agent`; a handle to a slot that never held one answers
  `invalid native agent`, and the two stay distinct. The `self` an agent
  receives on each turn carries the current generation.
- **Visible generations.** A reused slot prints as `#<native-agent:2.1>`,
  slot 2, generation 1; a first occupant prints as before.

## Verification

```sh
sh scripts/test-native-agents.sh
./scripts/test-native-repl.sh
```

## What this does not claim

A generation is one byte and wraps after 256 reaps of one slot. Actors still
share the evaluator's globals and one protection domain, and nothing here is
per-actor authority.
