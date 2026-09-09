# Agel standard library

The standard library is one Agel source file installed as one transaction. A
syntax, module, or test error leaves no partial library behind. Its implementation
is available at `crates/agel-stdlib/stdlib.agel` and receives no capabilities.

## `agel/sequence`

Import with `(import agel/sequence)`. It exports:

- `(append left right)`
- `(reverse values)`
- `(map function values)`
- `(filter predicate values)`
- `(foldl function initial values)`
- `(each function values)`

Collections remain persistent. All traversal is deterministic and charged to
the transaction's fuel/call-depth budgets.

## `agel/result`

Import with `(import agel/result)`. `ok` and `err` construct transparent tagged
lists. `ok?`, `err?`, `value`, `error`, and `unwrap-or` inspect them. Taking the
wrong projection signals `result/not-ok` or `result/not-error`, so callers can
use normal Agel handlers and restarts.

## `agel/swarm`

Import with `(import agel/swarm)`. It exports two protocols and three constructors:

```lisp
(def worker (make-worker "name" (fn (payload) ...result...)))
(def pool (make-pool "name" (list worker ...)))
(submit pool reply-agent payload)
```

The pool's heap is its rotating worker list. Each committed pool turn sends one
`(work reply payload)` message and moves that worker to the tail. A worker replies
with `(result worker value)`. Both routing and rotation are transactional, so a
failed send cannot advance only half the state.

The pool does not create parallel threads. It composes the deterministic
cooperative scheduler: `(run n)` performs at most `n` turns and normal mailbox,
event, fuel, call-depth, and collection bounds still apply. This predictable
micro-turn model is the substrate for later foreground/background interaction.

Run the complete example:

```sh
cargo run -q -p agel-cli < examples/worker-pool.agel
```

## `agel/fixed-point`

Agel already supports ordinary named recursion, which remains the default.
`agel/fixed-point` adds explicit recursion-as-a-value where interception is
useful: eager-safe `fix`, decreasing-gas `fix-bounded`, immutable
`converge-bounded`, and a typed fixed-point agent driver.

The agent driver turns each `fixed-continue` into a separately scheduled,
transactional turn. Steps can finish, request a model, or continue with new
state; policy bounds logical steps, trace retention, cooperative model calls,
and cumulative prompt characters. `fixed-propose` stages a closure against an
expected version; `fixed-commit` installs that preview at a message-ordered
boundary, while `fixed-discard` drops it.

This combinator is not the sandbox. Evaluator budgets, capability checks,
transaction rollback, supervision, isolated heaps, and the explicit model
dispatch gate remain the enforcing controls. The semantics, limitations, cost
model, and examples are in [`agentic-fixed-points.md`](agentic-fixed-points.md).

## `agel/meta`

`agel/meta` is an evaluator written in Agel. It exports `meta-base-env`,
`meta-eval`, `meta-apply` and `meta-analyze`. `meta-base-env` returns explicit
bindings for the arithmetic, comparison, list, map and text primitives plus
`type-of` and `apply`; `(meta-eval quoted-program environment)` evaluates
literals, strings, symbols, `quote`, `if`, `begin`, parallel `let`, multi-body
lexical `fn`, and ordinary calls. `(meta-apply closure arguments)` invokes an
interpreted closure from outside the interpreter, and `(meta-analyze source)`
performs syntax analysis once and returns an ordinary Agel closure that takes
an environment, so repeated execution skips source dispatch. Metacircular
closures are transparent tagged lists containing parameters, body, and captured
environment.

This is the first self-hosting stratum, not yet a replacement for the seed. It
deliberately omits world mutation, macros, modules, agents, effects, and resource
accounting of its own; the enclosing seed still supplies budgets and transaction
rollback. Run `examples/metacircular.agel` to inspect code, closures, and results.
See [Agel in Agel](agel-in-agel.md) and
[self-hosting and performance](self-hosting-performance.md).

## `agel/meta-agent`

`agel/meta-agent` exports `make-meta-agent`, `make-analyzed-agent`,
`meta-agent-state` and `meta-agent-source`. Both constructors validate a
literal three-parameter `fn` source form before spawning a real scheduled
agent whose heap retains the source, its interpreted (or analyzed) closure and
the user state; each turn threads the state through `meta-apply`. The source
remains inspectable while the agent runs. Neither constructor grants model
capabilities. `examples/metacircular-agents.agel` and
`examples/analyzed-agents.agel` are the executable walkthroughs.

## `agel/jit`

`agel/jit` exports `jit-compile` and `jit-run`. `jit-compile` lowers a
single-body integer function to the inspectable `agel/jit-v1` IR entirely in
Agel, and `jit-run` is the portable IR interpreter used as an oracle. Machine
code is produced only by the separate, opt-in `agel-jit` Rust crate; the CLI
never links it. See [the integer JIT](integer-jit.md).

## `agel/native` and the native toolchain modules

The standard library also installs the Agel-written native toolchain as
ordinary modules, each exporting an executable closed function and the same
source as quoted data: `agel/native` (`native-compile`,
`native-compiler-source`), `agel/native-reader` (`native-read`,
`native-reader-source`), `agel/native-modules` (`native-link`,
`native-link-source`), `agel/native-agent-kernel`, `agel/native-system-builder`
and `agel/native-agents`. They are documented in
[managed native compilation](managed-jit.md), [reading Agel in Agel](native-reader.md),
[modules and macros](native-modules.md), [tail calls and compiled agents](native-tail-agents.md)
and [native code upgrades](native-code-upgrades.md). Installing them grants no
authority: they return IR or source data that only the host backend can compile.

## `agel/ui`

`agel/ui` is the first language-level desktop stratum. It represents retained UI
nodes, semantic capability requirements, patches, preview state, and revision
history as persistent Agel data. A typed desktop agent validates proposed scene
changes before commit and retains the preceding scene for live rollback.
Proposals are bound to an expected base revision so delayed agents fail closed
instead of overwriting newer work.

The module exports generic and convenience node constructors, `scene?` and
`find-node`, three inspectable patch constructors, patch application, and the
`inspect`/`propose`/`commit`/`discard`/`rollback` desktop protocol. `ui-spec`
describes that surface as data from inside Agel.

See [`agentic-desktop.md`](agentic-desktop.md) for the design and run
`examples/agentic-desktop.agel` for an end-to-end live mutation demonstration.

## `agel/ui-layout`

`agel/ui-layout` deterministically compiles a valid scene, positive viewport,
and valid theme into a transparent display frame. Fixed `basis` children and
flexible siblings compose in rows and columns; integer division remainders go to
the final flexible child. The output contains geometry boxes, ordered
fill/stroke/text commands, and semantic action regions.

`hit-test` searches the action regions from front to back and returns the
component identity, bounds, and intent without executing it. The typed layout
agent accepts `render` and `hit` messages. Failed layout preserves its preceding
good frame and returns a structured rejection.

## `agel/vector`

`agel/vector` is the resolution-independent graphics postcard. It defines
points, 1024-unit fixed-point affine transforms, solid and linear-gradient
paints, rounded rectangles, ellipses, arbitrary paths with cubic Bézier curves,
fill/stroke/text commands, and nested transform/clip state. These are persistent
maps and lists rather than evaluator syntax or privileged renderer objects.

`vector-command?`, `vector-commands?`, and `balanced-vector-commands?` validate
the display stream, including underflow and leaked transform/clip frames.
`vector-spec` describes the complete contract from inside Agel.

## `agel/ui-vector`

`agel/ui-vector` converts a valid layout frame to a validated vector frame.
Logical geometry does not change with display density: an integer `scale` only
sets physical dimensions. Its typed vector agent atomically installs a complete
new frame and keeps the last good frame if a malformed source or density is
proposed.

The `agel-vector` executable is the first replaceable output service. It treats
the language-produced frame as untrusted, checks structure, arithmetic,
dimensions, colors, path budgets, state-stack balance, and output size again,
then emits deterministic SVG. The renderer crate declares no third-party
dependency of its own; its integration tests freeze the kitchen-sink SVG digest.

## `agel/desktop`

`agel/desktop` supplies the default COSMIC-inspired Agel shell as ordinary data:
a panel with launcher/workspace/settings affordances, central workspace, and
application dock. `default-theme`, `default-viewport`, `default-scene`, and the
machine-readable `desktop-spec` are all replaceable language bindings rather
than kernel policy.

Run `examples/cosmic-desktop.agel` to compile the default shell, resolve pointer
coordinates to semantic intents, customize it through the desktop agent,
recompile it through the layout agent, and reject an impossible frame safely.
Run `examples/vector-desktop.agel` for the polished 2× UI and
`examples/vector-primitives.agel` for paths, gradients, clipping, and transforms.
The bootable system's smaller native vector projection and contained compositor
are documented in [`native-graphics.md`](native-graphics.md).
