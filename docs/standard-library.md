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

## `agel/meta`

`agel/meta` is an evaluator written in Agel. `meta-base-env` returns explicit
bindings for its primitives; `(meta-eval quoted-program environment)` evaluates
literals, symbols, `quote`, `if`, single-body lexical `fn`, and ordinary calls.
Metacircular closures are transparent tagged lists containing parameters, body,
and captured environment.

This is the first self-hosting stratum, not yet a replacement for the seed. It
deliberately omits world mutation, macros, modules, agents, effects, and resource
accounting of its own; the enclosing seed still supplies budgets and transaction
rollback. Run `examples/metacircular.agel` to inspect code, closures, and results.

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
