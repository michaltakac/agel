# Agel Milestone 1 language core

Milestone 1 is a compact, deterministic language substrate. Every source batch
is evaluated against a candidate world and becomes visible only after all forms
succeed. Conditions, exhausted budgets, failed imports, agent messaging, macro
definition, and module creation all obey that transaction boundary.

## Functions and lexical scope

`fn` creates a lexical closure and accepts multiple body forms. `let` evaluates
all initializers in the surrounding environment before introducing bindings.
Top-level functions can recursively resolve their global names.

```lisp
(def make-adder (fn (x) (fn (y) (+ x y))))
(def add-ten (make-adder 10))
(add-ten 5)
```

## Hygienic template macros

`defmacro` defines a fixed-arity template macro. Parameters are replaced with
unevaluated caller syntax. Caller syntax remains opaque, introduced `let` and
`fn` bindings are alpha-renamed, and free template identifiers resolve in the
macro's definition module. This prevents both directions of accidental capture.

```lisp
(defmacro unless (condition body) (if condition nil body))
(unless #f 42)
(macroexpand-1 (unless #f 42))
```

This is intentionally smaller than Common Lisp procedural macros or full
`syntax-rules`: there are no variadic patterns, quasiquote, or compile-time
effects yet. Macro expansion is size-preflighted and charged against fuel.

## Modules

Modules start with a private namespace. `export` declares public values or
macros; `import` installs both unqualified and `module/name` aliases.

```lisp
(module math
  (export square)
  (def square (fn (x) (* x x))))
(import math)
(math/square 9)
```

Undefined exports abort the transaction. Reopening a module replaces it, which
makes module upgrades deterministic and rollbackable.

## Persistent collections

Lists and maps are values. Collection operations return new values and do not
mutate their inputs.

```lisp
(def original (dict 'name "Agel"))
(def updated (assoc original 'revision 1))
(get updated 'revision)
(dissoc updated 'name)
(keys updated)
(count updated)
```

Map keys use structural equality. Available list operations are `list`, `cons`,
`car`, and `cdr`.

## Conditions and restarts

Uncaught failures expose a condition with `kind`, `message`, and `data` fields.
`with-handler` handles a matching kind (or `*`). A handler can return normally
or invoke a named restart established by `with-restart`.

```lisp
(with-restart (use-value replacement)
  replacement
  (with-handler (arithmetic/division-by-zero problem)
    (invoke-restart use-value 0)
    (/ 10 0)))
```

Custom conditions use `(signal 'kind "message" optional-data)`.

## Capabilities

Agel source cannot mint capabilities. A trusted host calls
`World::issue_capability`, explicitly supplies returned handles through
`EvaluationOptions`, and source requests a permitted attenuation:

```lisp
(request-capability 'filesystem/read "/workspace/source.agel")
```

Kinds must match exactly. A scope matches exactly, `*`, or a slash-delimited
descendant. `model/infer` is the first implemented effect authority; it only
permits an agent to commit an inference intent for the scoped provider. The
trusted host owns actual process execution. No file, network, clock, or FFI
capabilities are exposed to Agel code.

## Deterministic budgets

Each transaction has limits for source bytes, parse nesting, evaluation fuel,
call depth, collection length, model-prompt bytes, and pending model requests.
Macro output and mailbox growth are included.
Budget exhaustion is a structured condition and aborts the candidate world.
Successful commits report `steps_used`.

The default host API is `World::evaluate`. Trusted callers can use
`World::evaluate_with` and an explicit `EvaluationOptions` value.
