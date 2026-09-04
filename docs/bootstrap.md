# Agel v0.9 diverse bootstrap

Self-hosting is split into independently testable claims. This avoids treating a
single metacircular demo as proof that the host can be removed.

## Common Lisp reference

`bootstrap/common-lisp/agel-reference.lisp` is a separate evaluator for the
functional kernel: literals, lexical lookup, `quote`, `if`, `begin`, `let`,
multi-body `fn`, `def`, application, and foundational collection/arithmetic
operations. It uses Common Lisp data as the bootstrap representation but does
not share evaluator code with Rust.

Both evaluators consume `bootstrap/conformance.forms` plus a required-failure
corpus covering arity, overflow, and division errors. The Rust runner prints
Agel's canonical value syntax; the Common Lisp implementation has an independent
canonical printer. This command requires SBCL and fails on any byte difference:

```sh
./scripts/test-bootstrap.sh
```

CI installs SBCL and runs the comparison on every push and pull request.

## Agel evaluating Agel

The standard-library module `agel/meta` represents inner closures as ordinary
tagged lists and recursively evaluates quoted syntax against an explicit map.
Nested lexical capture works:

```lisp
(import agel/meta)
(meta-eval
  '((fn (x) ((fn (y) (+ x y)) 2)) 40)
  (meta-base-env))
; => 42
```

The two new seed primitives are general Lisp fundamentals: `type-of` observes a
value category and `apply` invokes a callable with an argument list. Neither
grants authority.

## A/B semantic images

`agel-supervisor` owns the active image and candidate slot. Staging requires the
candidate to extend the exact active committed-input chain. Every declared
health check runs in a separately forked, zero-capability canary world. Evidence
binds active root, candidate root, and passed-check count; only matching evidence
can atomically select the candidate. The old image remains available for
rollback.

```sh
cargo run -q -p agel-supervisor --example ab_upgrade
```

## What remains for v1.0

The independent and metacircular evaluators cover a meaningful functional
kernel, not all Agel semantics. The next trust step is a bootable supervisor and
recovery path, followed by expanding cross-implementation conformance. v0.9 does
not claim the Rust host has disappeared or that the A/B selector survives a
hostile disk controller.
