# Agel in Agel — v0.2.13

The hosted standard library now evaluates a larger functional Agel subset and
uses it as the behavior engine for real scheduled agents. The implementation is
in `crates/agel-stdlib/stdlib.agel`, not a new Rust evaluator primitive.

## What is actually language-owned

`agel/meta` implements lexical lookup, closure representation and application,
`quote`, lazy-branch `if`, `begin`, parallel `let`, multi-expression `fn` bodies,
and recursive self-application. `apply` works with interpreted closures as well
as supplied host callables. Special forms dispatch through an Agel map of Agel
handlers rather than a deeply nested conditional chain. Binding initializers see the outer scope; repeated
`let` names use the last binding in the body, matching the Rust seed. Duplicate
function parameters and malformed bindings/closures are rejected.

An interpreted closure is `(meta/closure parameters body environment)`. Its body
and captured environment are ordinary inspectable Agel data. This representation
is not opaque or an authority token. Its `type-of` is `list`, not `callable`;
use exported `meta-apply` to invoke it from outside the interpreter. Multi-body
functions are represented with a `begin` body, preserving the four-field format.

Linux CI exposed a hosted machine-stack overflow during recursive interpretation.
The Rust seed now uses [`stacker`](https://docs.rs/stacker/0.1.25/stacker/fn.maybe_grow.html)
at evaluation/application boundaries, so recursive work can reach its existing
language fuel/depth checks on small embedding stacks. This adds a hosted build
dependency (and a C/assembly toolchain requirement), not an Agel primitive or a
native-kernel dependency. It is not a global memory quota or a claim that every
reader/value traversal is stack-independent. The corpus runs on an explicit
2 MiB thread; a separate 256 KiB embedding test verifies depth-error rollback.

```lisp
(import agel/meta)
(meta-eval
  '(let ((x 40))
     (apply (fn (y) (+ x y)) '(2)))
  (meta-base-env))
; => 42
```

The base environment exposes arithmetic and collection operations, not host
globals, agent services or model APIs. An explicit environment can inject a
service. This is name visibility, not a new security boundary: injected host
closures carry their own behavior and authority, and the seed still enforces
capabilities, fuel, call depth and transactional turns.

## Source-backed agents

```lisp
(import agel/meta-agent)
(def source '(fn (self state message) (+ state message)))
(def counter (make-meta-agent "interpreted counter" source 0 (meta-base-env)))
(send counter 20)
(send counter 22)
(run 2)
(meta-agent-state counter)
; => 42
(meta-agent-source counter)
; => (fn (self state message) (+ state message))
```

`make-meta-agent` validates a literal three-parameter `fn` before spawning. The
heap retains its source, interpreted closure and user state. A small Agel wrapper
threads state through `meta-apply` on each turn; the Rust scheduler does not
interpret that source. These agents accept any message value and use the seed's
default stop-on-failure policy. Their constructor grants no model capabilities.

The executable example `examples/metacircular-agents.agel` explicitly injects
`send` and an observer into the environment. It demonstrates two successful
turns, then a send followed by division by zero. The failed turn leaves state
at 42, leaks no outgoing message, and stops the agent. Source inspection is not
automatic live replacement: this module adds no hidden rewrite messages or
formal proof mechanism. Existing candidate/upgrade mechanisms remain separate.

## Cost and evidence

```sh
cargo run -q -p agel-cli < examples/metacircular-agents.agel
cargo run -q -p agel-stdlib --example metacircular_cost
cargo test -p agel-stdlib
./scripts/test-bootstrap.sh
```

The cost example asserts equal results and reports deterministic evaluator steps
with installation/environment setup excluded. At v0.2.13:

| Expression | Rust seed steps | Agel interpreter steps |
| --- | ---: | ---: |
| `(+ 20 22)` | 4 | 227 |
| Lexical capture adding 40 and 2 | 9 | 947 |
| Recursive factorial of 5 | 88 | 7,651 |

These roughly 57–105× step counts are **not wall-clock ratios or model tokens**.
There is no AI call in evaluation or agent scheduling. Interpreting more Agel
currently costs more host computation and consumes the same bounded fuel. Use
this path for inspection and language experiments, not a claim of fast compiled
self-hosting. Compilation and specialization are future performance work.

Seventeen shared successful expressions and fourteen required failures are
checked by the Agel evaluator and Rust seed; the bootstrap script also checks
the independent Common Lisp reference. This caught and fixed the reference's
first-binding-wins `let` bug and missing function/binding validation. Further
tests cover denied implicit host lookup, malformed closure values, bounded
nontermination, actor rollback and the executable example.

## Honest boundary

This is a hosted functional interpreter, **not yet the native OS evaluator**.
It does not implement `def`, modules, macros, condition/restart syntax or the
entire standard library inside itself. The Rust reader, allocation, primitive
arithmetic/collections, scheduler, capability enforcement and recovery machinery
remain bootstrap infrastructure. Native Agel currently lacks the persistent
list/map/closure representation needed to run this module; its existing dock and
workbench policies remain Agel-authored, but this release does not change the
native desktop. No new driver, GUI or speech capability is implied.
