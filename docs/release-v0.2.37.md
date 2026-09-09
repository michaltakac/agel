# Agel v0.2.37 — Closures that can be kept

The native evaluator has had lexical closures since v0.2.15, but a closure
made inside a lexical call could not be given a name: `def` refused it
rather than drop its captures, and a lambda escaping a stored function was
an error. Stored functions now carry their captures.

```text
agel-native[50]> (def add40 ((fn (x) (fn (y) (+ x y))) 40))
#<native-function>
agel-native[51]> (add40 2)
42
agel-native[52]> (def make-adder (fn (n) (fn (m) (+ n m))))
#<native-function>
agel-native[53]> ((make-adder 5) 6)
11
```

## What changed

- **Captures in stored functions.** A stored function keeps up to eight
  captured scalars, the same bound as the local slots, and binds them before
  its body runs; a parameter of the same name shadows a capture.
- **Escaping lambdas are stored.** A lambda returned from a stored function
  becomes a stored function with its captures, instead of "ephemeral value
  escaped a stored function".
- **Function-valued captures still refused.** Capturing a function is
  refused with the same message as before rather than silently dropped.

## Also found on the way

The first build with captures was 45 KB larger. A captured local carries a
`bool`, and with that niche available inside a stored function Rust encoded
the empty binding's tag there as a non-zero byte, so the empty world was no
longer all-zero and the three world banks became read-only data instead of
a zero fill. The stored-value and runtime-value enums now carry an explicit
zero tag, the shared capture binder is one out-of-line function, and the
x86-64 graphics image is the same 127,209 bytes as before.

## Verification

```sh
sh scripts/test-native-agents.sh
./scripts/test-native-repl.sh
```

## What this does not claim

Captures are copied when the closure is made. Eight is the bound. Nothing
about the workspace format changed: cells store source, and a closure
defined at the prompt is discarded like every other prompt definition.
