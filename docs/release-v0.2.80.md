# Agel v0.2.80 — The compiler in the guest

The toolchain written in Agel runs in the OS. The standard library the
loaded runtime carries has held the native reader (`native-read`), the
static linker and template expander (`native-link`) and the compiler
frontend (`native-compile`) since v0.2.20 and v0.2.21, and since v0.2.78
that library is installed in a protection domain. This release runs them
there: the process reads a module bundle from the data region, links it
to a closed function, compiles that to the native IR, and writes the
linked definition as source the desktop loads and runs. What the guest
toolchain produces is byte-equal to what the host toolchain — the same
Agel, compiled to machine code by the JIT — produces from the same
module. The roadmap row "the compiler and reader running in the guest"
is done; the machine-code backend is still Cranelift on the host, which
is rung 16's remaining item.

## What changed

- **`print-form VALUE`** in `boot/posix/agel`: the printed form of a
  value as text, pure (no capability), the inverse of `native-read`, so a
  program in the OS can write source another evaluator reads.
- **Nothing else.** The reader, linker, expander and compiler are the
  library's Agel, unchanged; the runtime that runs them is v0.2.78's; the
  words that reach the files are v0.2.79's. This rung is the proof that
  they compose.

## Proof

`scripts/test-agel-process.sh` installs `examples/jit-module-dock.agel`
(two modules: `arithmetic` exporting a `defsyntax double`, `dock`
importing it and exporting a `behavior`) in the data region, and the
desktop's evaluator writes `compile.agel`:

```text
live-desktop> :exec agel -- compile.agel
agel: standard library installed, 834 steps
=> #<module:agel/native-reader>
=> #<module:agel/native-modules>
=> #<module:agel/native>
=> ((module arithmetic (export double) (defsyntax double (x) (* x 2))) (module dock (import arithmetic) (export behavior) (def behavior (fn (self state message) (+ state (double message))))))
=> (fn (self state message) (+ state (* message 2)))
=> (agel/native-v2 (fn 3 (begin (tail-call (builtin +) ((local 0 1) (call (builtin *) ((local 0 2) (const 2))))))))
=> list
=> #<closure>
=> 118
=> 64
agel: 10 forms, 33297 steps, revision 2
process agel exited with status 0
```

The reader turned the text into forms, the linker expanded `double` and
closed `behavior` over its import, the compiler lowered it to the IR the
JIT takes, and `print-form` wrote `(def behavior (fn (self state message)
(+ state (* message 2))))` to `linked.agel` and the workbench-adapted
definition to `behavior.agel` — 33,297 steps, under a second on QEMU's
TCG. Then `:load-file /linked.agel` defines `behavior` in the desktop's
native evaluator and `(behavior nil 40 1)` answers 42: code read, linked
and compiled in the guest runs in the guest. Finally the test compiles
the same module on the host with `module_workshop --workbench` (the
bootstrap chain: seed evaluator, then the reader and linker compiled to
machine code) and asserts `behavior.agel` is byte-equal to its output.
Two toolchains, one Agel source, one answer. The full regression passes;
the kernel is unchanged.

## Not claimed

The IR is produced in the guest and not executed there: the backend that
turns it into machine code is Cranelift, on the host, and the desktop's
native evaluator runs the linked *source*, not the IR. Nothing is signed:
a compiled definition is a file, loaded with the operator's authority like
any file. The module dialect is the bounded static one of v0.2.21 — no
procedural macros, no dynamic linking. The reader, linker and compiler
are interpreted here by the hosted evaluator, as the seed does on the
host before it compiles them; a self-compiled toolchain in the guest
would need the backend there.
