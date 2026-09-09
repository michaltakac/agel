# Agel v0.0.9 diverse bootstrap

Self-hosting is split into independently testable claims. This avoids treating a
single metacircular demo as proof that the host can be removed.

## Common Lisp reference

`bootstrap/common-lisp/agel-reference.lisp` is a separate evaluator for the
functional kernel: literals, lexical lookup, `quote`, `if`, `begin`, `let`,
multi-body `fn`, `def`, application, checked variadic arithmetic, lists,
insertion-ordered persistent maps with structural equality, and the five
byte-oriented UTF-8 text mechanisms (`text-bytes`, `text-byte`, `text-slice`,
`text-concat`, `text-symbol`). Maps and text joined the shared corpus in
v0.2.22. It uses Common Lisp data as the bootstrap representation but does
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

Since v0.2.13 the library also implements `begin`, parallel `let`, multi-body
functions and interpreted `apply`, and drives source-backed hosted agent turns.
Shared functional success/error corpora are checked across all three evaluators.
See [Agel in Agel](agel-in-agel.md) for executable examples, measured interpreter
overhead and the explicit boundary between this hosted library and native Agel.

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

Since v0.2.24 the supervisor can be configured with a trusted verifier key:
`AbSupervisor::trust(key)`. It then refuses unsigned promotion, and
`promote_signed` accepts only `PromotionEvidence::sign`ed by exactly that key,
over the canonical bytes `"agel/promotion-evidence/v1\0" || active root ||
candidate root || checks`. A candidate image cannot change which key the
supervisor trusts, because that policy lives outside every image.

```sh
cargo run -q -p agel-supervisor --example ab_upgrade
```

## What remains after v0.1.1

The independent and metacircular evaluators cover a meaningful functional
kernel, not all Agel semantics. v0.1.0 added a bootable recovery path and v0.1.1
added a fixed-memory native evaluator. The next trust steps are native persistent
images, expanding cross-implementation conformance, and moving the agent runtime
and compiler into the VM. The Rust seed has not disappeared, and the A/B
selector does not yet survive a hostile disk controller.
