# Agel v0.2.38 — The evaluator on seL4

Since v0.1.4 the seL4 backend has answered the kernel contract from an
unprivileged protection domain and had its faulting world contained by its
parent. It had never run Agel. The world domain now runs the native
evaluator, the same source the research kernels compile into their evaluator
domains, over the same forms their isolation self-test checks.

```text
world: contract invariants hold across the boundary
world: native Agel evaluated factorial with transactional rollback in an unprivileged protection domain
recovery: contained it without replying; the world is not resumed
```

## What changed

- **One evaluator source, four backends.** The Microkit crate compiles
  `boot/kernel/src/native.rs` into the world domain. Factorial, a
  definition, a transaction rolled back by a division by zero, and a closure
  with a captured value evaluate to the same answers at the same revisions
  as on x86-64, AArch64 and RISC-V.
- **A stack for a world.** The evaluator keeps its three transactional world
  banks on the domain's stack, so `agel.system` gives the world domain
  512 KiB, the same budget the research kernels give their evaluator domains.
- **No new authority.** The world domain still holds two channels and one
  page. The evaluator's answers leave through the serial domain like the
  transcript does.

## Verification

```sh
./scripts/test-sel4.sh
```

## What this does not claim

The seL4 world runs a fixed corpus, not an interactive workshop; it has no
disk, workspace or recovery record. The evaluator's bounds are its own, not
enforced by seL4.
