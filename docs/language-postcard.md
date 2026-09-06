# The Agel language postcard

Agel keeps syntax small and moves power into libraries. The reader grammar is:

```text
form   := atom | "(" form* ")" | "'" form
atom   := nil | #t | #f | integer | string | symbol
string := '"' (escape | any-character-except-quote)* '"'
escape := \\n | \\r | \\t | \\\\ | \\" 
comment starts with ; and ends at newline
```

That is the surface syntax. Evaluation adds a compact set of special forms:

```text
quote if begin def fn let defmacro macroexpand-1
module export import
with-handler with-restart invoke-restart
defprotocol
```

Everything else is a call. The seed builtins are checked arithmetic and integer
ordering (`<`); equality; persistent
list/map construction and access; agent spawn/send/receive/scheduling/introspection;
conditions; capability attenuation; and transactional model intent creation.
The reflective pair `type-of` and `apply` is sufficient for the library-written
metacircular evaluator without growing surface syntax.

There is intentionally no special syntax for agents, swarms, persistence,
sandboxes, verification, or AI models. They are values and libraries built on
the same few forms. `cargo run -p agel-cli -- --no-stdlib` starts exactly this
postcard-sized substrate; the default CLI atomically installs the Agel-written
standard library above it. This includes `agel/fixed-point`: even anonymous
recursion, bounded convergence, and the agent continuation driver add no syntax
or evaluator primitive.
