# Agel v0.2.15 — Agel-authored IR, actual native JIT

Added `agel/jit`, an Agel-written compiler and portable interpreter for a finite
integer-function IR. Added the separate `agel-jit` Rust/Cranelift backend that
validates this IR and generates actual host machine code.

The subset supports integer arguments, integer/boolean literals, checked binary
arithmetic, comparisons and lazy conditionals. Native calls enforce arity and
conservative per-invocation fuel. Malformed IR is rejected before code generation;
overflow returns an error. Executable-memory ownership is explicit and tested.

Try `cargo run --release -q -p agel-jit --example native` (Rust 1.86+).
Use `-- --ir` to inspect the low-level compilation. The example reports lowering,
compilation and native-call timings separately without model calls.

This is the first real hosted JIT, **not complete self-hosting**: no general calls,
recursion, closures, collections, automatic actor specialization or kernel JIT.
The ordinary CLI and freestanding kernel do not link Cranelift. The narrow unsafe
ABI/memory boundary is confined to the new crate; other crates retain their
unsafe-code prohibition. See `docs/integer-jit.md` for the contract and next stages.
