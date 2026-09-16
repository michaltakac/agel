# Agel v0.2.76 — Room to write

The rung after effects: room. The native evaluator's bounds were a
postcard's — 24 globals, 384 heap cells, 2 KiB of text, 2,000 steps and 24
call levels per form. A program that decides, logs and starts things needs
more than fourteen small forms. The bounds are larger now, still fixed and
still reported by `:limits`, and the transactional banks still live on the
evaluator's private stack, which grew and moved to hold them.

## What changed

- **The bounds:** 512 syntax nodes, 96 globals, six parameters, twelve
  locals, 224-byte bodies, twelve arguments, 48 call levels, 10,000
  evaluation steps, 32 agents with 16-message mailboxes, 128 turns per
  `run`, 4,096 heap cells, 16 KiB of text. `:limits` names every one.
- **The evaluator's stack is 4 MiB** (512 KiB before), from the frame pool,
  because the three transactional world banks grew with the bounds.
- **The stack has its own region,** 192 MiB into the domain's space. It
  began at the domain's base with the shared page a megabyte above it; a
  4 MiB stack there would run over the shared page it talks through and the
  device windows. Every domain's stack moved; the page beneath and above
  stay absent, so an overflow still faults into nothing.
- **The seL4 world domain** gets the same 4 MiB stack in its manifest,
  which is regenerated.
- **The legacy `native-selftest` is retired.** It ran the evaluator
  directly on the BIOS stage's low-memory stack, below 640 KiB with the
  kernel; the enlarged banks no longer fit there. The evaluator is tested
  where it actually runs — in a protection domain — by
  `scripts/test-native-repl.sh` on all three machines, which does a full
  session, and by the fifteen unit tests of `scripts/test-native-agents.sh`.

## Proof

`scripts/test-native-repl.sh` on all three machines: recursion past the old
depth, text of thousands of bytes, more globals than the old table held,
`:limits` naming the new bounds, and the persisted workspace surviving a
reboot. `scripts/test-native-persistence.sh` and `scripts/test-power-cut.sh`
pass. The full regression passes.

## Not claimed

The bounds are larger, not gone: there is still no allocator, no collector
beyond commit-time copying, and a form is still 256 bytes. The evaluator's
memory is copied whole at every commit, so a form's cost grows with the
banks. The persisted workspace still holds sixteen source cells; growing it
is a later rung. The runtime that holds arbitrary programs is the rung
after this one.
