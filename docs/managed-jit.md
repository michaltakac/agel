# Managed native compilation and frontend self-compilation — v0.2.16

Historical v0.2.16 contract. [v0.2.17](native-tail-agents.md) extends this tier
with v2 tail-call IR, primitive caching and an isolated compiled agent scheduler.
The v1 IR remains accepted; the old no-tail-call limitation below describes v0.2.16.

The Agel frontend can now compile its own source. This is a tested, restricted
compiler bootstrap, **not complete self-hosting of Agel or the OS**.

```sh
cargo run --release -q -p agel-jit --example self_host
cargo test -p agel-jit
```

The demo checks seed IR = first native compiler IR = second native compiler IR.
It then uses that native frontend to compile `examples/jit-closure-workshop.agel`:

```text
{before {counter 40} original {counter 41} upgraded {counter 42} factorial-10 3628800}
```

Two closures capture different increments, remain callable in a map, and produce
different immutable state transitions without changing the original state. This
is a pure behavior-composition example, not a live upgrade of an OS actor.

## Ownership of the implementation

`crates/agel-stdlib/native-compiler.agel` is a **closed Agel function**. It performs
source validation, lexical name resolution, lowering of parallel `let` to
function application, sequence lowering, and closure-body construction. Its
recursive helpers explicitly pass themselves as arguments. No Rust source
compiler duplicates these rules. The standard-library loader installs the same
file as both `native-compile` and quoted `native-compiler-source` in `agel/native`;
there are no manually synchronized source copies.

```lisp
(import agel/native)
(native-compile
  '(fn (x)
     (let ((make (fn (offset) (fn (y) (+ offset y)))))
       ((make x) 2))))
```

This prints IR, not machine code. `agel_jit::managed::Native::compile` is the
explicit host action that validates IR and compiles each function body with
Cranelift. Native code implements branching, sequencing and calls to bounded
runtime mechanisms. Those mechanisms handle tagged immutable values, primitive
operations and private closure entry points. They **do not interpret source or
walk IR at invocation time**. A native-compiled frontend invocation needs neither
`World` nor the Rust evaluator; the demo uses the seed reader to supply source data.

The Rust backend remains responsible for independent IR validation, function
hoisting, native emission and memory ownership. There is no self-hosted reader,
native linker, allocator or complete standard library yet.

## Supported semantics

- Single entry `fn`; nested lexical functions and multi-expression bodies.
- Parallel `let`: initializers see the outer environment. Repeated binding names
  select the last value in the body, as in the seed. Function parameters must be
  unique. Primitive names can be shadowed lexically and passed as values.
- `if`, `begin`, `quote`, nil, booleans, signed 64-bit integers, strings, symbols,
  lists and insertion-ordered maps. Nil and false are false; integer zero is true.
- Captured immutable lexical frames. Closures can outlive their creating call,
  be returned to native callers, and be stored in lists/maps. There is no capture
  by mutable global name and no ambient authority.
- Checked variadic `+ - * /`, structural data `=`, integer `<`, `list`, `cons`,
  `car`, `cdr`, `dict`, `get`, `assoc`, `dissoc`, `keys`, `count`, `has-key?`,
  `type-of` and first-class `apply`. Maps accept data keys, including lists.
- Recursion through first-class/self-passed closures and fixed-point patterns;
  every call is metered. There is no `letrec`, global `def`, or tail-call
  optimization in this tier. Recursion consumes bounded stack depth.

Closure equality is deliberately rejected (`Fault::Type`), including closures
used as map keys. This avoids claiming seed structural closure equality for an
arena-based representation. `signal` stops execution with `Fault::Signaled`;
structured signal payloads/catching are not implemented here. Unknown names,
effects, modules, macros and unsupported forms fail rather than falling back.

The host invocation API accepts and returns **inert data only**. Closures may
escape lexical scopes within one invocation, but cannot be exported as a host
code pointer or persist across separate invocations. Returning a closure to the
host reports `NonData`; so does importing an agent, capability or seed callable.

## IR and native boundary

The versioned tree is `(agel/native-v1 (fn arity body))`. Nodes are `const`,
`local depth slot`, `builtin name`, `fn`, `if`, `begin` and `call function args`.
All lexical addresses are checked against enclosing frame widths before native
code exists. Limits: 16,384 syntax/data nodes, depth 128, 512 functions, arity 64,
and 1,000,000 bytes of constant text. The primitive whitelist grants no I/O.

Private native functions take an opaque runtime pointer and an environment
index, and return a private value handle. Generated code does not dereference
that pointer. One registered Rust callback implements runtime mechanisms; every
callback result is checked before execution continues. No user value supplies a
pointer or call target. Rust panics in callbacks are caught, not unwound through
generated frames. Arithmetic failures return errors rather than native traps.

Native allocation ownership outlives all invocations and frees executable memory
on drop, including partial compilation failures. The existing integer tier keeps
its separate unboxed arithmetic fast path. Neither tier is linked into the
freestanding kernel or automatically enabled for hosted actors.

These guarantees trust the validator, emitter, callback ABI, Cranelift, allocator
and stack-growth mechanism. They are **not formal verification or process
isolation**. Backend bugs and system allocation failures remain possible.

## Resource accounting and performance

`Limits` bounds fuel, cumulative logical allocations, collection/frame edges,
text bytes, and call depth. Defaults are 4M fuel, 1M allocations, 4M edges, 16M
text bytes and 256 calls; 256 is also the hard call-depth ceiling. Limits cover
argument import, execution, collection copying/comparison and output expansion.
Output is a tree: each expansion of a shared subgraph is charged again. A small
shared DAG cannot produce an unbounded returned tree under a small heap budget.

Each invocation owns an arena that is discarded on success or failure. It uses
no tracing GC yet, so dead intermediate values remain until return. Counters
bound logical storage, not exact resident memory/allocator overhead. Native
stack segments grow only after the semantic call-depth check; a small host
thread stack must not crash before the configured error. This is a bounded
native call stack, not serializable continuations or a heap-based scheduler.

This general tier intentionally has more overhead than the integer tier:
tagged values, helper calls, copied collection vectors, and per-node metering.
There is no universal speedup claim. The demo reports individual seed-lowering,
native-backend and native-frontend timings, not a statistically controlled
benchmark. It makes zero model calls and consumes no subscription tokens.

## Tests and remaining work

Tests compare native results with the seed on lexical shadowing, parallel and
duplicate bindings, escaping closures, closures in collections, maps, Unicode
counts, variadic arithmetic, lazy branches, recursion and higher-order mapping.
Bootstrap tests compile the compiler twice and compare exact IR, then compile
and execute additional programs through both stages. Safety regressions cover
overflow, division by zero, invalid source/IR, lexical bounds, excessive IR size,
fuel/heap/depth limits, repeated recovery, output DAG expansion and a 64KiB host
thread stack. These are conformance tests, not a proof of equivalence for all
programs.

Next steps toward the full system are language-owned module/macro compilation,
structured conditions, persistent executable closures with versioned dependencies,
GC or region reclamation, tail calls/explicit continuations, and transactional
actor integration. No placeholders for these features are advertised as working.
