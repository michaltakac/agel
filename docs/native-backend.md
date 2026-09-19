# The backend in Agel

`agel/native-x86` in the standard library (`crates/agel-stdlib/native-x86.agel`)
is an x86-64 backend written in Agel: `(native-x86-emit IR ARGUMENTS)` turns
the IR `native-compile` produces into a static ELF process image for the
Agel supervisor, as hex text, which a file holds and the desktop's `:install`
puts into the program region for `:exec` to run. It runs where the rest of
the toolchain runs since v0.2.80: in the loaded runtime, in a protection
domain, in the OS.

## What it compiles

The IR's integer subset: integer, boolean and nil constants; two-argument
`+ - * / = <`; `if`, `begin`, lexical functions and calls. The frontend's
immediately applied function literals (including `let`) are inlined. Other
functions become closures with copied environments. Returned closures,
transitive captures and captured arguments across tail calls are supported.

Tail calls through local closures reuse a large-enough frame. Calls needing
more arguments use an ordinary frame. The callee pops its argument/closure
block with `ret 8(arity+1)`; encodable arity is 0 through 8190. Invalid lexical
addresses and unencodable arities are rejected while emitting. Calls check
closure tags and dynamic arity before dispatch or tail-frame reuse.

Lists, texts, maps, other builtins and builtin values are refused. Numeric
operations require integers; equality compares tagged values, including
closure identity. A final process result must be integer, boolean or nil.

## The image and closure arena

The ELF contains a code segment at the process window's base and a one-MiB
read/write arena one MiB above it. A closure record has a code address,
arity and flat copied lexical values: 16 header bytes plus eight per capture.
Records never contain stack-frame links. A copied closure value refers to
another arena record; all records live until process exit.

Integers are `2n`, false/true/nil are 1/3/5, and closure pointers have tag 7.
`rbp` names the current frame, `[rbp-8]` its closure, and `r12` passes that
closure to a callee. Captured loads use fixed offsets in the record. Inline
locals remain stack slots and are copied if captured. `r13` is the arena
bump pointer, `r9` its logical limit, `r14` remaining fuel, and `r15` the
shared process page.

`(native-x86-emit-bounded IR ARGUMENTS FUEL ARENA-BYTES)` sets both budgets.
The arena limit is any integer from 0 through 1048576; the existing entry
points use one MiB. Each complete record must fit before any part is written.
Exactly fitting succeeds; exhaustion exits 113. This is cumulative allocation,
without garbage collection, and does not shrink the fixed ELF mapping.

The entry creates the root closure, supplies constant arguments (and the
root closure itself for the explicit-self convention), then invokes it.
Integer results print through the process protocol and exit with their low
byte. False/true/nil exit 0/1/2. Division by zero exits 111, exhausted fuel
112, arena exhaustion 113, invalid calls/numeric operands or a non-scalar
final result 114. Successful integers can share these exit codes.

Machine code remains a tree of short byte leaves and symbolic addresses,
assembled in two passes: sizes/labels, then resolved bytes.

## Execution fuel

Since v0.2.89, `native-x86-emit` supplies 50,000,000 units of fuel.
`(native-x86-emit-limited IR ARGUMENTS FUEL)` accepts an explicit nonnegative
integer, including zero. Each evaluated IR node costs one, including
inlined callees; only the selected branch runs. The root function is charged,
but entry arguments, the external invocation, printing and exit are not.
Exhaustion checks zero before decrementing and exits 112 before the next
node's operation. An empty `begin` costs one and returns nil.

This cost model is tested against an independent IR interpreter, not the
hosted source evaluator's `steps_used`. See [v0.2.89](release-v0.2.89.md) for
the contract, exact-boundary tests and remaining allocation/lifetime limits.

## Why a tree

The evaluator's call-depth budget is 256, and walking a long list
(`map`, `append`, `cat`) is not tail recursion — it conses on the way
back — so a list of two thousand bytes cannot be walked as a flat
sequence without meeting the budget, even with the proper tail calls
v0.2.87 added. The backend's code is a binary tree of short leaves (an
instruction's bytes, a label, a `rel32` or an `abs64`), and every pass —
measuring, resolving, rendering to hex — recurses by the tree's depth,
which is the program's nesting, never by its length. Hex text is built by
concatenation at the nodes.

## Validation

`crates/agel-stdlib/tests/backend.rs` runs it hosted: the fib IR becomes
a well-formed static ELF for the process window, and what it cannot
compile is refused. `scripts/test-agel-process.sh` runs it in the OS: the
loaded runtime compiles fib, a `let`, tail-recursive sums, a nested-call
regression and a division by zero. The desktop installs and executes the
images. The suite also checks exact IR fuel boundaries; see the execution
fuel section above.

The following transcript is from v0.2.85; sizes and compiler steps change
as the backend grows.

```text
live-desktop> :exec agel -- backend.agel
agel: standard library installed, 3157 steps
...
=> 1736
agel: 6 forms, 209865 steps, revision 2
process agel exited with status 0
live-desktop> :install fib /fib.hex
INSTALLED fib: 796 BYTES AT SECTOR 2956
live-desktop> :exec fib
55
process fib exited with status 55
```

## Remaining boundaries

This integer-subset backend is separate from the managed Cranelift JIT.
Owned captures and arena checks do not add a collector, lists/texts/maps,
checked integer-overflow semantics, full adversarial IR validation, or a
language-level stack quota. Source-evaluator and IR fuel remain distinct.

Emitted images are unsigned and run inside ordinary process protection
domains. Other ELF programs need not contain these runtime checks. Kernel
isolation remains the containment boundary. See [v0.2.90](release-v0.2.90.md)
for the closure contract and regressions.
