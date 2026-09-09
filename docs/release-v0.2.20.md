# Agel v0.2.20 — The reader reads itself

Agel now has a closed, Agel-written reader that executes as native code. It
reads source text into inert syntax data, including Unicode, comments, quotes,
escaped strings and signed integer boundaries. Five small, bounded UTF-8
primitives supply runtime access; parsing policy stays in Agel.

The new workshop drops the Rust evaluator after bootstrap, then reads and
rebuilds both the reader and compiler from text. Exact IR equality checks their
agreement with the bootstrap artifacts. A fresh text program is then read,
compiled and executed through the rebuilt tools.

```sh
cargo run --release -q -p agel-jit --example text_workshop
```

It returns factorial 10 (`3628800`) and a Unicode greeting. Pass a file containing
one closed `(fn (n) ...)` to run your own program with input 10.

Tests cover seed/native conformance, self-reading, recompilation, malformed
syntax, UTF-8 boundaries and resource limits. Copying is metered; vector-list
and escaped-string copying costs remain, and detailed source spans are future
work. This strengthens self-hosting without claiming the Rust runtime/backend
or graphical OS are already self-hosted. No models or subscriptions are used.

See `docs/native-reader.md` for the API and remaining bootstrap dependencies.
