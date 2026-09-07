# Agel v0.2.13 — Agel interpreting agent behavior

The Agel-written hosted interpreter now supports parallel `let`, `begin`,
multi-expression functions and higher-order `apply`. It rejects malformed
closures/bindings and duplicate parameters. No Rust language primitive was added.

`agel/meta-agent` uses that interpreter for inspectable source-backed agent
behavior, with ordinary transactional state updates and rollback of failed
outgoing sends. Service bindings must be supplied explicitly; the constructor
grants no model capabilities and adds no automatic code replacement protocol.

Shared success/failure corpora now exercise the Rust, Agel and Common Lisp
evaluators. They exposed and fixed a repeated-binding mismatch in the reference
bootstrap, whose parameter and binding validation is also tightened.
The CLI now derives its installed-module list from evaluated library results
instead of a hardcoded banner.

Try `examples/metacircular-agents.agel` through the hosted CLI. The new
`metacircular_cost` Rust example measures interpreter work without model calls;
the additional interpretation layer is substantially slower in evaluator steps,
not a performance optimization. See `docs/agel-in-agel.md` for exact measurements
and tests.

This is not complete self-hosting or a native desktop update. Native rich values,
compiler bootstrapping and replacement of host runtime services remain future
work; the kernel's safety boundary has not moved into editable library code.
