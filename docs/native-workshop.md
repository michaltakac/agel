# The native Agel workshop

`./scripts/run-qemu.sh` now boots directly into an Agel REPL on the freestanding
kernel. Since v1.6, source crosses a bounded shared page into an unprivileged
evaluator domain, its transactional state lives on that domain's private stack,
and results are printed through a separate console-driver domain. Evaluation
uses no Rust allocator or host operating system. Since v1.7, named source cells
can be edited, committed, and reconstructed after reboot.

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
:edit NAME         read one balanced form into a named source cell
:run NAME          evaluate a staged cell
:show NAME         print a cell's exact source
:delete NAME       remove a cell from the staged workspace
:cells             list cells in replay order
:workspace         show generation, cell count, and dirty state
:save              validate, commit, and switch to the workspace image
:reload            discard staged changes and replay the disk image
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

The workspace holds at most 16 cells. Cell names are at most 24 ASCII bytes and
each cell is exactly one balanced Agel form of at most 256 bytes. The editor is
structural and intentionally small: `:edit NAME` opens a secondary prompt and
the ordinary balanced-form reader accepts as many physical lines as the form
needs. Editing stages source; `:run` changes only the live evaluator and `:save`
is the explicit durability boundary.

## v1.7 durable source workspace

The raw x86 disk reserves two 8 KiB slots after the boot seed. A workspace image
contains canonical name/source pairs, never a Rust memory dump or capability.
On `:save`, Agel resets the evaluator and replays every staged cell in order.
Definitions entered directly at the prompt are intentionally discarded; put
anything you want to retain in a named cell. The revision counter remains
monotonic across that rebuild. A reader or evaluation failure rejects the whole
candidate and reconstructs the last committed workspace. Only a valid candidate
reaches storage.

The storage path invalidates the older slot, writes and flushes its bounded
payload, publishes the generation header last, flushes again, and reads it back
for verification. Boot checks format, bounds, canonical decoding, and CRC-32,
then tries valid generations newest-first. If the newest slot is torn, corrupt,
or cannot be evaluated, the preceding slot is replayed automatically. CRC detects accidental
damage; it is not a cryptographic signature or protection from a malicious disk.

`./scripts/build-boot.sh` replaces only sectors 0 through 255 and preserves the
workspace region. Thus rebuilding or rerunning `./scripts/run-qemu.sh` keeps
your cells. `./scripts/test-native-persistence.sh` uses a temporary disk and
proves edit → save → reboot → reject a checksummed but semantically invalid
newest slot → corrupt it → simulate an invalidated/partially written slot →
recover the previous generation in every case.

`./scripts/test-native.sh` exercises the evaluator inside QEMU without input.
`./scripts/test-native-repl.sh` additionally drives the real UART reader and
isolated REPL through a stateful, recursive, rollback-producing session.

This is enough to write, organize, and retain small programs inside Agel itself.
It is not yet a self-hosted development environment: the editor and storage
codec are trusted Rust services, and the hosted macro/module/agent/effect system
is not in the VM. A native agent runtime and an editor implemented in Agel are
the next useful rungs.

## v1.6 isolation boundary

The evaluator holds no console-device grant and cannot name another domain's
stack. Its only mutable cross-boundary object is one 4 KiB shared page; source
and result payloads are each capped at 256 bytes. The supervisor switches to its
own page-table root before answering a trap, so a world's mappings never become
ambient supervisor authority. `.user_text` is read/execute, immutable constants
are read-only, stacks and the shared page are read/write, and no mapping is both
writable and executable.

The interactive serial reader and recovery commands remain supervisor code.
The AArch64 and RISC-V isolation images run the same evaluator corpus but do not
yet expose an interactive UART workshop. This is a protected language workshop,
not yet the full hosted agent runtime or a durable self-hosted environment.
