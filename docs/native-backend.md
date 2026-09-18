# The backend in Agel

`agel/native-x86` in the standard library (`crates/agel-stdlib/native-x86.agel`)
is an x86-64 backend written in Agel: `(native-x86-emit IR ARGUMENTS)` turns
the IR `native-compile` produces into a static ELF process image for the
Agel supervisor, as hex text, which a file holds and the desktop's `:install`
puts into the program region for `:exec` to run. It runs where the rest of
the toolchain runs since v0.2.80: in the loaded runtime, in a protection
domain, in the OS.

## What it compiles

The IR's integer subset: constants that are integers, `#t`, `#f` or `nil`;
`+ - * / = <` applied to two arguments; `if` and `begin`; `fn` addressed
lexically through a static link, so a `let` (which the
frontend lowers to a nested function called at once) and a closure over an
enclosing live frame work; calls through closure values include the
explicit-self convention, so recursion works. Escaping closures still have
unresolved lifetime hazards. Since v0.2.88 a call in tail
position through a parameter reuses the frame (the calling convention is
callee-pops, `ret 8(arity+1)`), and a `let` is slots of the frame it
appears in, so a loop written as a function calling itself last runs in
constant stack; a tail call whose callee takes more arguments than the
frame holds is a plain call.

Not compiled, refused with `native-x86/unsupported`: lists, texts, maps,
any other builtin, a builtin as a value, a primitive with other than two
arguments, entry arguments that do not match the top-level function's
arity, and mismatched arguments to an inlined function literal. Dynamic
closure calls still need runtime arity/type checks.

## The image

One code segment holding the whole file — a 64-byte ELF header, two
program headers, then the code — at the process window's base, and an
arena segment of fresh pages a megabyte above it, where closure records
of two words (code address, captured frame) are bumped out and never
freed. The entry saves the shared page in `r15`, points `r13` at the
arena, makes the program's closure record, pushes it (twice under the
self convention), pushes the constant arguments, calls, then prints the
result as a decimal line through a `write` request and exits with its low
byte through an `exit` request, both by the process protocol's
`endpoint.send` (`int 0x80`). A division by zero exits 111; exhausted IR
fuel exits 112. The entry initializes `r14` with the budget and all emitted
node checks share it through ordinary and tail calls.

Values are tagged: an integer n is 2n, `#f` is 1, `#t` is 3, `nil` is 5;
`if` tests for 1 and 5. A frame is `rbp`; the static link is pushed under
it; the caller pushes the callee's closure then the arguments in order, so
argument *s* of an *n*-ary frame is at `rbp + 16 + 8(n − 1 − s)`, and
the callee pops the argument/closure block on return. Every jump is a
`rel32` and every address a `movabs`, so the assembler is two passes over a tree of code:
sizes and labels, then bytes.

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

## Not claimed

This is not the JIT: `agel-jit`'s Cranelift backend compiles the whole IR
with a managed heap, lists and texts, on the host, and stays the
toolchain's full backend. This one compiles integer programs to a
standalone process and proves that the last Rust piece of the toolchain
has an Agel counterpart in the guest; growing it toward the JIT's
coverage is the road, not this rung. Nothing it emits is signed or
checked beyond the loader's rules for an ELF; it runs in a protection
domain like any program.
