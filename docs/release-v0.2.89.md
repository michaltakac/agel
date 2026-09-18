# Agel v0.2.89 — Fuel inside compiled programs

The x86-64 backend written in Agel now meters the code it emits. A compiled
loop has a deterministic IR budget, including when a tail call reuses its
frame. Exhaustion exits the process with status 112 before evaluating the
next node. The kernel and process protocol are unchanged.

## Using the budget

```lisp
(import agel/native)
(import agel/native-x86)

; The existing entry point now supplies 50,000,000 units of IR fuel.
(native-x86-emit (native-compile '(fn (x) (+ x 2))) '(40))

; An explicit nonnegative integer budget, embedded in the executable.
(native-x86-emit-limited
  (native-compile '(fn (self) (self self)))
  nil
  1000)
```

`native-x86-emit-limited` accepts fuel from 0 through 9223372036854775807.
Negative values and non-integers raise `native-x86/unsupported` during
compilation. Zero is valid: the emitted process exits 112 without evaluating
its root function. Compilation itself still uses the evaluator's separate
budget. Previously generated executables must be recompiled to gain metering.

## Cost model

Each evaluated `agel/native-v2` node costs one unit, charged before its
operation:

- `const`, `local`, `builtin` and `fn` each cost one.
- `if` costs one plus its test and selected branch; the other branch is free.
- `begin` costs one plus its executed children. Empty `begin` returns nil;
  this release also fixes its previous unspecified leftover-register result.
- `call` and `tail-call` cost one plus the callee, arguments in order and
  invoked body. Calling a closure does not reevaluate its `fn` node.
- Inlined primitive and function callees retain their one-unit charge even
  though they no longer allocate a closure or perform a machine call.
- The root function costs one. Supplied entry arguments and the external
  invocation are not IR nodes and cost nothing. Printing and exiting are
  outside the budget.

A single register carries the remaining budget through every call and tail
jump. Each charge checks zero before decrementing, so exhaustion cannot wrap
the counter. The implementation shares one instruction sequence at emission
time; there is no extra optimization pass or kernel service.

For example, raw IR `(agel/native-v2 (fn 0 (call (builtin +)
((const 20) (const 22)))))` needs five units: root function, call, builtin,
and two constants. Five succeeds with 42; four exits 112. The frontend adds
`begin` nodes around source function bodies, so the equivalent source can
have a different IR count.

## Validation

`scripts/native_ir_fuel.py` is an independent test interpreter using ordinary
closures and recursive evaluation, with no x86 or inlining knowledge.
`scripts/test-agel-process.sh` compares its exact budget against guest CPU
execution: fourteen cases run successfully at N and exhaust at N−1. Cases
cover constants, booleans/nil, truthiness, branches, empty/nonempty `begin`,
inlining, nested calls, ordinary recursion, tail recursion and signed division.
Five further executions cover zero/one/64-unit infinite loops and the ordering
of fuel exhaustion versus division by zero. The existing compiler-in-guest
suite, including its million-iteration tail loop, remains part of this test.

Hosted tests reject invalid limits and accept zero and the largest supported
integer. Validation passed: 218 workspace tests, strict workspace and
freestanding/POSIX Clippy checks, documentation generation with warnings
denied, formatting checks, and the complete guest process suite including
all 33 new executions.

## Boundaries and next milestone

This is IR-level conformance on a fixed corpus. It is not equality with the
hosted source evaluator's `steps_used` or the managed Rust JIT's cost model.
Source-level budget portability still needs an explicit lowering/cost contract.

Fuel is not a memory quota, wall-clock deadline or security proof for arbitrary
IR. Stack/arena bounds, escaping-closure lifetimes, dynamic arity/type checks
and numeric-overflow conformance remain separate work. Process isolation
continues to contain native faults. In particular, closure lifetime safety
and allocation limits should precede broader compiled-language coverage.
