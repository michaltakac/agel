# The v1.1 native Agel workshop

`./scripts/run-qemu.sh` now boots directly into an Agel REPL implemented in the
freestanding kernel. Source arrives through COM1, is parsed into a bounded arena,
and is evaluated without a Rust allocator or host operating system.

## Native subset

Atoms are signed 64-bit integers, `#t`, `#f`, `nil`, and symbols. Lists use the
same Lisp syntax and `;` begins a comment. A leading apostrophe quotes the next
form. Implemented special forms and functions are:

```text
quote  if  begin  def  fn
+  -  *  /  =  <  eval
```

Arithmetic and comparison currently take exactly two integers. `fn` accepts at
most four parameters and one body form; use `begin` for multiple actions. Named
functions resolve globals at call time, enabling top-level recursion. Immediate
lambdas capture bounded scalar lexical parameters, so
`(((fn (x) (fn (y) (+ x y))) 40) 2)` evaluates to `42`. A lambda created inside
a lexical call cannot yet be persisted by `def`; this is rejected rather than
silently losing its captures. Function-valued captures are also deferred.
Quoted syntax is valid for the current transaction and can be passed to `eval`,
but v1.1 does not persist quoted graphs in globals.

## Transaction protocol

The session owns three fixed world banks: active, previous, and scratch. Each
form starts by copying active into scratch and evaluates only there. Success
rotates scratch into active and retains the old active world as previous. Any
reader, capacity, arithmetic, fuel, or evaluation error leaves both committed
worlds untouched. `:rollback` swaps in previous once and advances the revision;
revision numbers never move backward.

This makes the following safe:

```lisp
(def answer 42)
(begin (def answer 99) (/ 1 0))
answer ; still 42
```

Redefinition is live. Because the preceding world is retained, entering
`:rollback` immediately after redefining a function restores its executable old
definition without rebooting the VM.

## Console commands

```text
:help              native forms and commands
:revision          monotonically increasing world revision
:rollback          restore the preceding committed world
:defs              list current global definitions
:limits            show every fixed native resource bound
:recovery-status   inspect the independent boot recovery state
:verify            admit recovery candidate B
:promote           select a verified recovery candidate
:fault             simulate watchdog rollback to A
:shutdown          leave QEMU when the debug-exit device is present
```

## Deterministic limits

The native seed permits 128 syntax nodes, 24 global definitions, 24-byte names,
four function parameters, eight arguments/local slots, 192-byte stored bodies,
24 reader/call levels, and 2,000 evaluation steps per submitted form. The serial
input buffer is 256 bytes. These are explicit resource policy, not accidental
allocation failures. `:limits` renders the table directly from the constants the
evaluator enforces, so the console, this document, and the implementation cannot
drift apart.

`./scripts/test-native.sh` exercises the evaluator inside QEMU without input.
`./scripts/test-native-repl.sh` additionally drives the real UART reader and
normal REPL through a stateful, recursive, rollback-producing session.

This is enough to write and run programs inside Agel itself. It is not yet an
editor or self-hosted development environment: definitions disappear at power
off, and the hosted macro/module/agent/effect system is not in the VM. Native
persistent images and an in-OS editor are the next useful rung.
