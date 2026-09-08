# Agel v0.2.16 — A frontend that compiles itself

Added a closed Agel-written frontend and a managed native JIT tier supporting
lexical bindings, captured closures, immutable lists/maps, higher-order calls,
checked arithmetic, and metered recursion. The original unboxed integer tier
remains available.

The frontend compiles its own source. Tests establish identical IR from the seed
and two successive native compiler stages, then compile and run further programs
through both native stages. This is frontend self-compilation, **not complete
self-hosting of the language runtime or OS**.

Runtime limits cover fuel, allocations, collection edges, text, stack depth and
output expansion. Malformed IR fails before code generation. Failures leave no
persistent invocation state, and executable ownership stays private. No host I/O,
model calls, automatic actor JIT or kernel JIT is introduced.

Try `cargo run --release -q -p agel-jit --example self_host` (Rust 1.86+).
The demo proves compiler-stage agreement, runs replaceable captured behaviors,
and demonstrates fuel rejection. See `docs/managed-jit.md` for the precise
contract, tested limitations and remaining self-hosting work.
