# Agel v0.2.90 — Owned closures and bounded allocation

Closures emitted by the Agel-written x86 backend now own their captured
values. A returned closure no longer refers to a dead stack frame, and a
closure passed through a tail call keeps its values when that frame is
reused. Allocation has a checked arena limit; invalid calls stop with a
defined status before changing the call frame.

## Representation and lifetime

A closure record contains a code address, arity and a flat copy of every
lexically visible value. Its size is `16 + 8 × captured-value-count` bytes.
Captured closure values refer to other arena records, never to stack slots.
Creating another closure copies the existing captured values again, so
captures survive multiple returns. Values are immutable in this subset;
no mutable environment cells or write barriers are needed.

The record's eight-byte-aligned pointer carries tag 7. Integers remain even;
false, true and nil remain 1, 3 and 5. Calls check the tag and recorded arity
before dispatch. Numeric operations reject non-integer operands; equality
continues to compare tagged values (closure equality is identity).
Lexical depths/slots and encodable function arities are checked while emitting.

Tail calls through local closures can reuse a sufficiently large frame,
including a closure in an inline `let` slot. Captured values and arguments
are already independent of that frame. A larger argument block still uses
an ordinary call.

## Allocation budget

```lisp
(import agel/native)
(import agel/native-x86)

(native-x86-emit-bounded
  (native-compile '(fn () ((let ((n 40)) (fn (x) (+ n x))) 2)))
  nil
  1000       ; IR fuel
  4096)      ; closure arena bytes
```

The arena limit accepts integers from 0 through 1048576, inclusive; it need
not be a multiple of eight. The existing `native-x86-emit` and
`native-x86-emit-limited` APIs use the full one-MiB arena. The root function
needs 16 bytes. An immediately applied function literal is inlined and does
not allocate a record, although its IR fuel charge remains.

Each allocation checks its entire record against the remaining space before
writing the header or captures. Exactly fitting succeeds. Space is cumulative
for the process lifetime; no collector or reclamation is introduced. The ELF
still maps the fixed one-MiB arena, even for a smaller logical allocation limit.

| Status | Meaning |
| --- | --- |
| 111 | Division by zero |
| 112 | IR fuel exhausted |
| 113 | Closure arena exhausted |
| 114 | Non-closure call, arity mismatch, invalid numeric operand, or a closure returned as the process's final scalar result |

These statuses use the existing process exit protocol. Successful integer
results still print and exit with their low byte, so status alone is not a
unique error discriminator. No kernel or process-wire-format change is needed.

## Validation

The independent IR interpreter and guest CPU are compared at exact fuel and
one-short budgets for returned closures, transitive captures, captures passed
through tail calls, and independent closures from separate invocations.
Existing fuel cases and the million-iteration tail loop remain in the suite.
Additional guest executions cover exactly fitting root/capture records,
repeated allocation, exhausted arenas, invalid callees, wrong arities in
ordinary/tail calls, non-integer arithmetic, and non-scalar final results.
Hosted regressions reject invalid arena limits, arities and lexical addresses.

Validation passed: 219 workspace tests; 54 guest executions covering exact
fuel, closure lifetimes, allocation boundaries and errors; the existing
compiler-in-guest suite including the million-iteration loop; strict workspace
and freestanding/POSIX lint checks; documentation generation with warnings
denied; and formatting checks.

## Remaining limits

This is a checked bump arena, not a collector. Copying all visible values is
intentionally simple; removing unused captures is future optimization work.
The IR fuel model is unchanged and still distinct from source-evaluator fuel.
Stack growth from ordinary recursion remains bounded by the process mapping,
not a new language-level stack counter. Checked integer-overflow semantics,
full adversarial IR validation, and lists/texts/maps remain separate work.
Existing binaries retain their old representation until recompiled. These
checks do not turn arbitrary ELF programs into metered or trusted programs;
kernel protection domains remain the containment boundary.
