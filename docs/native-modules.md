# Modules, expression macros and the live OS bridge — v0.2.21

This milestone connects a **bounded static module dialect** to native compilation
and to the existing graphical OS. It is not general compilation of every hosted
Agel module or procedural `defmacro`, and it does not put Cranelift in the guest.

## Run it

```sh
cargo run --release -q -p agel-jit --example module_workshop
./scripts/run-graphics.sh --workbench --web
```

The first command links the example, runs its compiled behavior with state 40
and message 1, checks the result 42, and prints the OS adapter definition.

The second opens a real QEMU guest with a host-layout input panel. In a fresh
world, enter `:workbench`. Open **Compile a modular dock behavior**:

1. **Compile and preview** reads/links/expands/compiles the module source on the
   host, then submits one `:preview` command to the guest. It installs the new
   definition only in the candidate world, changes the dock behavior there and
   activates the currently selected dock item. The candidate scene is visible.
2. Inspect the result. Enter `:promote` to adopt the candidate or `:discard` to
   keep the old world. Preview executes a turn, so promotion adopts its tested
   state as well as code; this differs from hosted `NativeState` code-only promotion.
3. **Stage expanded source in wb-3** replaces that known workbench source cell,
   not the live world. Enter `:save` separately to replay-validate and persist it.
   Reboot restores the expanded behavior. Counters reconstruct from source rather
   than saving arbitrary heap state. Keep the module bundle in your own source
   file: this release saves the expanded definition, not the original modules.

The bridge targets the standard workbench's `dock`, `behavior`, `paint` and
`wb-3` conventions, not arbitrary OS applications. It never auto-promotes or
auto-saves. The direct QEMU window remains the default without `--web`. You can
also print an adapter for manual source inspection/pasting:

```sh
cargo run --release -q -p agel-jit --example module_workshop -- --workbench < examples/jit-module-dock.agel
```

## Library semantics

`agel/native-modules` exports `native-link` and `native-link-source`, installed
from the same closed Agel function. Its API is:

```lisp
(native-link module-forms 'entry-module 'entry-name)
```

The native reader supplies `module-forms`. The native linker returns a closed
source expression, which the native Agel compiler lowers before Rust validates
and emits machine code. The command-line tool drops the seed evaluator before
reading and compiling the linker itself from text or processing user modules.
No module initializer or macro body is evaluated during linking.

```lisp
(module arithmetic
  (export double)
  (defsyntax double (x) (* x 2)))
(module dock
  (import arithmetic)
  (export behavior)
  (def behavior (fn (self state message) (+ state (double message)))))
```

- Modules are ordered and uniquely named. Imports must refer to an earlier
  module and expose only its exports. No filesystem search, cycles, aliases or
  forward references. Colliding imports/definitions and invalid exports fail.
- Definitions are static syntax: functions, literal/quoted constants or aliases
  to earlier bindings, not arbitrary initializer computations. Definitions are
  expanded against the current lexical module environment and dependencies are
  substituted as closed source. Unexported helpers remain inaccessible to importers.
- Function parameters and parallel `let` bindings have lexical scope; nested
  shadowing works. Primitive and special-form names cannot be rebound in this
  dialect. Recursive functions use the existing explicit-self convention.
- `defsyntax` is an expression-template macro, **not** hosted `defmacro`.
  Parameters receive unevaluated syntax. Templates may use parameters, primitive
  names, `if`, `begin`, literals and quoted data. They cannot introduce binders,
  refer to private/global helpers, execute compile-time code or recursively call
  other macros. Quoted data is untouched. Arity and duplicate parameters are checked.
  This restricted design avoids introduced-binding capture; it is not a general
  hygienic syntax-object system. Macro arguments may be duplicated or discarded,
  so authors must account for their eventual runtime evaluation count.

An exported runtime entry must be a closed function to pass the backend. The
module/linker library itself returns source data, not an executable authority.
Its format is an opt-in library dialect; existing hosted module/macro semantics
and kernel wire contracts are unchanged. No new evaluator primitive was needed.

## OS and resource boundaries

The dock export takes `(self state message)` and returns the next integer state.
An Agel-written adapter wraps it so the workbench paints that state exactly once.
The host checks a three-argument probe at `(0 0 1)` and rejects non-integer output.
That probe is not proof: the real guest preview tests the current agent state.
The adapter permits only serializable scalar/function syntax without strings or
opaque values. Guest parser, parameter, local, body, node and fuel limits still
apply; a hosted-compiled program may be too large or unsupported for the guest
and will be rejected there without committing the candidate.

Source input is bounded to 65,536 UTF-8 bytes. Reader syntax and linker expansion
depth are limited to 64, in addition to native fuel, heap, export and call-depth
budgets. Bootstrap reading of the linker uses a larger explicit 20-million-fuel
budget. Source substitution may duplicate functions and templates; no linear
code-size or speedup guarantee is claimed. Expanded preview commands must fit
256 bytes. This is intentionally small until the guest's source ABI grows.

The loopback endpoint requires the existing random route token and same-origin
header. It uses a fixed local compiler executable, no shell interpolation, a
30-second timeout and one concurrent module compilation. Compilation never
invokes a model or spends subscription tokens. It currently bootstraps tools
per request; a persistent compiler service is future performance work.

Rust remains the host bootstrap/runtime/backend and guest evaluator substrate.
The reader, linker, macro expansion, compiler frontend and source adapter are
Agel. Guest scheduling, behavior turns, scene drawing and source persistence run
in the actual OS. The self-hosted toolchain itself is **not yet running in the
guest**. General procedural/hygienic macros, dynamically linked modules, module
bundle persistence and an in-guest compiler/runtime remain subsequent milestones.

## Verification

```sh
cargo test -p agel-jit --test modules
cargo build --release -p agel-jit --example module_workshop
./scripts/build-boot.sh --features native-graphics
python3 scripts/test-native-modules.py target/boot/agel-v1.img
```

Tests cover native compilation, import privacy, lexical shadowing, lazy macro
branches, malformed definitions/imports/exports, capture restrictions, and reuse
after failure. The real QEMU test checks candidate discard/promotion, visible
scene changes, live-state-specific failure containment, staging, save and reboot.
It uses a temporary disk copy and never clears the user's persistent workshop.
