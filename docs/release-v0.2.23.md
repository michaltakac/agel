# Agel v0.2.23 — Data inside the native world

The freestanding evaluator was scalar-only: integers, booleans, nil, agent
handles and stored functions. Everything the Agel-written toolchain needs to
run inside the OS starts with strings, symbols, lists and maps as values, so
this release puts them into the fixed-memory transactional world.

```sh
./scripts/run-qemu.sh
```

```lisp
(def plan '(compile (core) "v1"))
(car (cdr plan))
(def table (assoc (dict 'plan plan) 'n 1))
(keys table)
(text-concat "Ag" "el")
(def collect (fn (self state message) (cons message state)))
(def log (spawn collect nil))
(send log '(open "a"))
(step)
(agent-state log)
:rollback
```

## What changed

- **A bounded heap in the world.** 384 cons cells and a 2,048-byte immutable
  text arena are part of every world bank, so a failed form, a rejected
  candidate and `:rollback` handle data and bindings together. Both bounds
  appear in `:limits`.
- **Copying collection at every commit.** After each evaluated form, validated
  preview or staged source cell, a Cheney-style collector keeps exactly what
  is reachable from bindings, agent states and queued messages and rewrites
  handles in place. Garbage never accumulates across revisions; a live set
  that cannot fit rejects the transaction.
- **The hosted data builtins.** `list cons car cdr count dict get has-key?
  assoc dissoc keys type-of text-bytes text-byte text-slice text-concat
  text-symbol`, string literals with the hosted escapes, structural `=`, and
  persistent insertion-ordered maps whose `assoc`/`dissoc` share untouched
  entries.
- **Quoted data is data.** `quote` builds inert values that `def` persists,
  agents carry as state and messages, and `eval` re-reads from a rendering
  that must fit one payload. Symbols intern by content.
- **Results are text.** A result is rendered into the 256-byte reply before
  collection and reported as `Value::Data`; the serial and graphical
  frontends print it the way the hosted REPL would.
- **The image still fits.** The heap made the empty world banks non-zero
  constants and pushed the kernel to 151 KB; declaring `Nil` first makes them
  all-zero, and the image is 111 KB against the 127 KB BIOS limit.

## Verification

```sh
sh scripts/test-native-agents.sh
./scripts/test-native-repl.sh
./scripts/test-native-persistence.sh
python3 scripts/test-native-workbench.py target/boot/agel-v1.img
./scripts/test-isolation.sh
```

Host tests cover every builtin against the hosted seed's results, quoted-graph
persistence and rollback, data-carrying agent turns and faults, heap
collection across forty garbage transactions, and transactional exhaustion.
The serial REPL suite evaluates data forms inside QEMU.

## What this does not claim

The native reader and compiler still exceed these bounds, so compilation stays
on the host. Native data does not cross into the hosted runtime's values,
messages are not typed protocols, and there is no allocator: the heap is
fixed-size world state on the evaluator domain's private stack.
