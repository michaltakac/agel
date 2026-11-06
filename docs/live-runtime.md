# Versioned language forms in Agel

`agel/live-runtime` is an evaluator written in Agel. It evaluates literals,
variable references, applications, quote, if, begin, lexical fn and parallel
let. It resolves additional language forms from an evolution version store.
A form's source is a pure function from unevaluated operands to an AST;
that AST executes in the caller's environment.

This is an optional module. Evaluate `crates/agel-stdlib/live-runtime.agel`
in a world with the standard library installed before importing it. The
native harness assembles that source automatically; the hosted test shows
loading it through `World::evaluate`.

```lisp
(import agel/live-runtime)
(def runtime (runtime-new versions))
(def environment (runtime-environment (dict 'saved 9)))
(def closure
  (runtime-eval runtime '(fn (condition) (unless condition (+ saved 33))) environment))
(runtime-apply runtime closure '(#t))
```

A runtime closure stores parameters, body and lexical environment. It does
not capture the form-version store. `runtime-apply` supplies the current
runtime explicitly, so the same closure can use a changed form without being
recreated. Keeping an earlier runtime value permits explicit historical
execution; nothing mutates all runtime values globally.

`runtime-edit` takes runtime, name, before source, after source, expansion
cases, behavioral cases and a judge. Expansion cases use the evolution
schema (`arguments`, `expected`). Behavioral cases contain `expression`,
`bindings` and `expected`. The gate judges the source, replays expansion
cases, then runs expressions against a temporary candidate overlay. Only
successful behavioral replay permits the version to commit. An exception
also refuses activation through the existing evolution hook.

`runtime-revert` takes runtime, name, version, expansion cases, behavioral
cases and a judge. It submits the retained source through the same gate and
creates a new version. It cannot bypass the current contract or select a
version belonging to another form. Both functions return the updated runtime;
its `versions` field retains sources, diffs, receipts and activation evidence.

Quote always preserves syntax data. Other forms resolve from the runtime's
cell store before core dispatch. A transformer can therefore introduce a
new form or change a dispatched form's semantics. Expansions may use core
forms recursively. A form that expands into itself without progress can
exhaust the shared evaluator budget; this release does not isolate each
expansion in a disposable process.

This is a small evaluator, not complete parity with the seed language. It has
no def, module loader, agent scheduler, analyzer, rest parameters or hygienic
macro expander of its own. It delegates primitive values to the supplied
environment; `runtime-environment` starts with pure metacircular primitives
and adds explicit caller bindings. Native services still run through the
Rust seed runtime. Native kernel/domain replacement and automatic recovery
of a canonical runtime are separate unfinished work.

Run `cargo test -p agel-stdlib --test live_runtime` for retained closures,
lexical values, lazy branches, quote, parallel let, parameter checks, rejected
reverts and equivalent-version reverts. Run
`sh scripts/test-native-repair.sh --scenario runtime` for an unpaid native
fixture, or add `--live` for real inference with protected OS judge accounting.
The native example archives the final version store in protected `judge-log`
and verifies its bytes through `file-read`; this archive is evidence, not an
automatically restored canonical active pointer.

For new durable source-store commits, v0.3.15 supplies [linked checkpoints](checkpoints.md).
Its reboot test restores the active form and verifies behavior before use.
The earlier printed v0.3.14 archive remains evidence; it is not itself a
checkpoint in the new inert format.
