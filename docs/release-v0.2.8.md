# Agel v0.2.8 — Native agents in the graphical workshop

The bootable OS now executes fixed-memory Agel actors inside its unprivileged
evaluator. Define a behavior in the graphical workshop, save its source cell,
spawn actors, exchange messages, step them, inspect their state, and rebuild
the behavior from saved source after reboot.

- Eight actors with bounded FIFO mailboxes and scalar states/messages.
- Deterministic round-robin scheduling and self-message continuations.
- Atomic behavior turns, including state, global writes, and outgoing messages.
- Contained behavior errors, retained poison messages, and explicit operator
  drop/restart recovery. Fuel exhaustion aborts the whole submitted form.
- Adversarial tests for fairness, rollback, capacities, nested scheduling,
  recovery boundaries, and shared fuel; serial and graphical QEMU coverage.
- Size-optimized freestanding builds retain the existing disk layout and
  persistent source slots.

Run `./scripts/run-graphics.sh` and follow `examples/native-agents.txt`.
`docs/native-agents.md` describes the primitive contract and current bounds.

The native actor seed shares one evaluator protection domain and global
definitions. Rich protocols, per-actor authority, model adapters, and the full
hosted fixed-point library remain future downward ports. Native scheduling
does not invoke or charge an AI model.
