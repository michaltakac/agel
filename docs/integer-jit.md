# The first real JIT — v0.2.15

Agel now has a hosted machine-code path, not just an analyzed interpreter.
`agel/jit` lowers a restricted function to inspectable IR **in Agel**.
`agel-jit`, a separate Rust crate, validates that IR and uses Cranelift to emit
and invoke machine code for the host CPU. It has been exercised locally on
macOS arm64; the CI workspace tests exercise Linux x86-64.

```sh
cargo run -q -p agel-cli < examples/jit-kernel.agel
cargo run --release -q -p agel-jit --example native
cargo run --release -q -p agel-jit --example native -- --ir
cargo test -p agel-jit
```

The first command only prints IR. The second performs native compilation and
execution. `--ir` also prints Cranelift's low-level representation. Compiling
this separate crate or testing the whole workspace requires **Rust 1.86+** and
the existing C/assembly build tools. Other crates retain their declared MSRV.
Cranelift 0.123.14 is pinned for this toolchain line; this is not a claim to use
the latest Cranelift major release. The ordinary CLI does not depend on the JIT.

## The language-owned part

```lisp
(import agel/jit)
(def source '(fn (x limit) (if (< x limit) (+ (* x x) 1) limit)))
(def ir (jit-compile source))
(jit-run ir '(6 50)) ; portable Agel IR oracle: 37
```

IR:

```lisp
(agel/jit-v1 2
  (if (lt (arg 0) (arg 1))
      (add (mul (arg 0) (arg 0)) (i64 1))
      (arg 1)))
```

Argument-name resolution, source-form lowering and the portable IR interpreter
are standard-library Agel code. The Rust backend accepts only the resulting IR;
it does not independently compile Lisp syntax. Zero through eight integer
arguments are assigned zero-based slots. Source stays available separately.

This is an **explicit fixed-primitive integer kernel subset**, not transparent
specialization of arbitrary Agel functions. It supports a single-body `fn`,
integer/boolean literals, parameters, binary `+ - * = <`, and `if`. Arithmetic
and comparison operands must be integers. Both conditional branches must have
the same result type; the result may be an integer or boolean. Integer zero is
truthy, as in Agel. Unsupported constructs fail instead of silently falling back.

No `let`, closures, recursion, loops, general calls, division, mutable globals,
maps, strings, model calls or agent effects are compiled in this tier. Arithmetic
names are reserved and cannot be shadowed by parameters. Lowering uses the
subset's fixed primitive meanings, not rebinding of ambient host globals. There
is no hotness detector, speculative inline cache or automatic actor compilation.

## Validation and execution boundary

- Rust independently checks the exact version/list shapes, opcodes, argument
  indices, operand/result types, depth (32 edges), total nodes (256), and arity.
  Invalid data never reaches code generation.
- The emitted program is finite and nonrecursive. It has no user-provided memory
  operations, host-call instructions or indirect branches. The emitter generates
  bounded reads from the checked argument slice and one result-slot write.
- Addition, subtraction and multiplication use signed-overflow checks. Overflow
  branches to a status return rather than wrapping or issuing a machine trap.
  An unselected branch is not evaluated, including its arithmetic errors.
- `Compiled::invoke` checks exact arity and conservative fuel **before entry**.
  Cost is the number of all IR nodes, including untaken branches. This is a
  per-invocation admission budget, not the hosted evaluator's step metric or a
  global quota. A future looping tier needs backedge/call metering in its code.
- The executable owner retains the code allocation and never exports raw entry
  pointers. Invocation borrows that owner. Drop frees code memory, also on partial
  compilation errors; code cannot be called through this API after its owner dies.

The JIT crate has two narrowly allowed unsafe regions: entering the generated
function with its fixed ABI, and releasing executable memory. Their safety
arguments are documented next to them. Other crates keep their existing unsafe
code prohibition. The validator, emitter, ABI wrapper, Cranelift and executable
allocator are trusted bootstrap machinery. This is **not formal verification or
an adversarial-process sandbox**, and a compiler/backend bug remains a risk.
No native code address is an Agel value and no actor receives new authority.

## Evidence and measurement

Native tests compare source evaluation, the Agel IR interpreter and generated
machine code across signed input grids. Additional tests exercise arithmetic
boundaries, integer-zero truthiness, lazy branches, invalid indices/types/forms,
oversized/deep IR, insufficient fuel, wrong arity and repeated code-owner drops.

The example separately reports Agel lowering time, native compilation time and
native invocation-wrapper time. The wrapper benchmark discards one warmup batch
and reports the median of seven batches of 100,000 calls, with varying inputs
passed through `black_box`. It excludes parsing, world transactions, model calls
and compilation. It is deliberately not divided into the older interpreter
benchmark to claim a misleading end-to-end speedup. All runs use zero models.

## What “self-hosting” means next

This milestone makes the frontend language-owned and the backend real, but the
integer subset cannot compile its own compiler. The compiler still runs on the
Rust seed, and ordinary agents still use their existing interpreter/analyzer.
The backend is not linked into the freestanding OS or kernel.

To reach full self-compilation we still need general lexical bindings, closure
conversion, collections/allocation, explicit call stacks and metered recursion,
then an Agel compiler capable of compiling all its own source. Bootstrap stages
must agree on a shared conformance corpus. Only then can native agent behavior
replacement gain a JIT tier with versioned dependencies, preserved recovery,
and capability checks. No stubs pretending to implement those stages are added.
