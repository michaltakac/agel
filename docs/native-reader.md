# Reading Agel in Agel — v0.2.20

```sh
cargo run --release -q -p agel-jit --example text_workshop
cargo test -p agel-jit --test reader
```

The workshop seeds the reader/compiler once, drops `World`, then uses native
Agel to read both tools' own source text and compile them again. Their IR must
match the bootstrap artifacts exactly. It then reads, compiles and executes a
new program, returning `{answer 3628800 message "Ahoj z Agelu 👋"}`.

Pass a file path to run your own single closed function with integer argument 10;
[`examples/jit-text-workshop.agel`](../examples/jit-text-workshop.agel) is the
default program:

```sh
cargo run --release -q -p agel-jit --example text_workshop -- my-program.agel
```

For example, the file can contain:

```lisp
(fn (n) (dict 'input n 'square (* n n) 'greeting "Ahoj 👋"))
```

## Library and bootstrap boundary

`agel/native-reader` exports `native-read` and `native-reader-source` from one
Agel source file. `(native-read text max-bytes max-depth)` returns all forms as
inert data. It does not evaluate them or grant authority. The closed reader can
run in the seed evaluator or be lowered by the existing native Agel compiler.

Agel implements whitespace/comments, lists, quote expansion, escaped strings,
UTF-8 symbols, booleans, nil and checked signed decimal integer recognition.
Like the seed reader, out-of-range decimal tokens remain symbols. Numbers are
recognized using Agel arithmetic, including the full signed i64 minimum. Empty
lists normalize to nil, matching quoted data and the managed runtime, rather
than preserving the Rust reader's distinct empty Expr list representation.

Rust supplies only five general immutable text mechanisms in the hosted seed
and managed JIT:

| Primitive | Contract |
| --- | --- |
| `text-bytes` | UTF-8 byte length; unlike `count`, not character count |
| `text-byte` | Byte at a nonnegative, in-range offset, in O(1) |
| `text-slice` | Half-open byte interval; rejects non-character boundaries |
| `text-concat` | Concatenates two strings |
| `text-symbol` | Converts text to inert symbol data, without name lookup |

Copying spends fuel proportional to bytes. The JIT reserves text quota before
copying; the seed charges fuel and enforces its collection-length limit. These
are runtime mechanisms, not calls to Rust's reader or integer parser. Existing
`count` semantics and the frozen kernel ABI are unchanged. Since v0.2.22 the
Common Lisp reference and the Agel-written `agel/meta` evaluator implement the
same five primitives and are checked against the seed on a shared corpus, and
since v0.2.23 the freestanding evaluator implements them over its bounded text
arena as well. The reader and compiler themselves still exceed the native
world's fixed bounds, so the toolchain does not yet run in the guest.

## Limits and performance

The example accepts at most 65,536 source bytes and a syntax-depth limit of 64.
Callers also supply native fuel, allocation, edge, text and call-depth budgets.
The reader's depth setting does not override the native stack-depth limit.
Malformed input or reader limits signal `reader/syntax` on the seed and the
existing generic `Signaled` fault on the JIT. Precise source spans and error
messages remain future work; this is not yet an incremental editor reader.

Byte scanning is direct and does not repeatedly convert UTF-8 into character
arrays. However, immutable vector-backed lists and concatenated escaped-string
segments can still copy quadratically. Budgets bound work; no linear-time or
overall speedup claim is made. Host file loading and bootstrap compilation are
outside invocation quotas. The example's execution-fuel figure covers the final
program only, not reading, compiling or bootstrap.

Tests compare native and seed Agel reader output to quoted Rust-reader data,
including Unicode, escapes, comments, quotes, empty input, numeric boundaries,
malformed forms, byte/depth/fuel limits and text primitive failures. They also
check self-reading, identical rebuilt IR and execution of newly read source.

The language is more self-hosted: reader, compiler, source composer and compiled
scheduler now run as native Agel. Rust still supplies bootstrap setup, validation,
machine-code emission, primitive runtime operations, collection and host commit
points. Module/macro compilation, persistent executable closures, richer error
handling and real OS integration remain. The graphical OS and ordinary CLI do
not automatically switch to this JIT pipeline. No model calls are used.
