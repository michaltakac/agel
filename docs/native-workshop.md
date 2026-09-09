# The native Agel workshop

`./scripts/run-qemu.sh` now boots directly into an Agel REPL on the freestanding
kernel, on x86-64 by default or on AArch64 and RISC-V with an argument; [`examples/native-workshop.agel`](../examples/native-workshop.agel) is a
form-by-form session to type into it. Since v0.1.6, source crosses a bounded shared page into an unprivileged
evaluator domain, its transactional state lives on that domain's private stack,
and results are printed through a separate console-driver domain. Evaluation
uses no Rust allocator or host operating system. Since v0.1.7, named source cells
can be edited, committed, and reconstructed after reboot. Since project v0.2.3,
`./scripts/run-graphics.sh` boots a separate unprivileged display domain and a
real graphical desktop alongside the serial recovery plane.

## Native subset

Atoms are signed 64-bit integers, `#t`, `#f`, `nil`, and symbols. Lists use the
same Lisp syntax and `;` begins a comment. A leading apostrophe quotes the next
form. Implemented special forms and functions are:

```text
quote  if  begin  let  def  fn
+  -  *  /  =  <  eval
list  cons  car  cdr  count
dict  get  has-key?  assoc  dissoc  keys  type-of
text-bytes  text-byte  text-slice  text-concat  text-symbol
spawn  send  step  run
agent-state  agent-pending  agent-turns  agent-faulted?
restart-agent  drop-message  agent-count
scene-clear  scene-rect  scene-count  scene-bind  scene-hit  scene-owner
agent-become
```

Since v0.2.23 atoms also include strings with the hosted escapes, and quoted
symbols, lists and maps are ordinary values: `'(compile (core) "v1")` is data
that can be bound with `def`, stored as agent state, sent as a message, taken
apart with `car`/`cdr`, compared structurally with `=`, and persisted through
source cells like everything else. `eval` re-reads a datum from its rendering,
so `(eval (cons '+ '(20 22)))` is `42`. The list, map and text builtins follow
the hosted seed: insertion-ordered persistent maps whose `assoc`/`dissoc`
share untouched entries, byte-oriented UTF-8 text mechanisms, `count` in
characters, and structural equality that is order-sensitive for maps.

Since v0.2.22 arithmetic follows the hosted seed: `+` and `*` fold any number
of integers from their identities, `-` negates one argument or folds several,
and `/` requires at least two. `=` and `<` still compare exactly two integers.
Parallel `let` evaluates every initializer in the enclosing scope and binds the
names for a sequence of body forms; a repeated name takes its last value.
`fn` accepts at most four parameters and any number of body forms, which a
persisted definition stores as one explicit `begin` sequence. Bindings from
parameters and `let` share the eight bounded local slots that `:limits`
reports. Named functions resolve globals at call time, enabling top-level
recursion. Immediate lambdas capture bounded scalar lexical parameters, so
`(((fn (x) (fn (y) (+ x y))) 40) 2)` evaluates to `42`. A lambda created inside
a lexical call cannot yet be persisted by `def`; this is rejected rather than
silently losing its captures. Function-valued captures are also deferred.
The scene and agent primitives are specified in [`native-scenes.md`](native-scenes.md),
[`native-agents.md`](native-agents.md) and [`native-workbench.md`](native-workbench.md).

## The native heap

Data lives in a bounded heap inside the transactional world: 384 cons cells
and a 2,048-byte immutable text arena, both reported by `:limits`. Allocation
only appends, so a form that would overrun either bound is rejected whole and
the committed world is untouched. At every commit boundary (an evaluated form,
a validated preview, a staged source cell) a copying collector keeps exactly
the cells and bytes reachable from global bindings, agent states and queued
messages, rewriting the handles in place; garbage from earlier revisions never
accumulates. The rollback bank keeps its own heap, so `:rollback` restores data
and bindings together. Results are rendered into the 256-byte reply before
collection, which is why a result need not itself be a root. Symbols are
interned by content within the arena. There is still no allocator: the heap is
part of the fixed world banks that live on the evaluator domain's private stack.

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
:recovery-status   show the trusted and candidate generations on disk
:verify            run the `health` cell in an isolated world; admit the candidate
:promote           make the verified candidate the trusted generation
:fault             roll back to the trusted generation now
:shutdown          leave QEMU when the debug-exit device is present
```

The serial workshop's `:verify`/`:promote`/`:fault` address the **boot recovery
plane**: on x86-64 they act on the disk-backed record described below, and on
the diskless machines on the in-memory A/B policy model. The graphical
workshop reuses the word `:promote` for a different, less privileged decision:
adopting a previewed **evaluator candidate world** after `:preview` (see
[`native-workbench.md`](native-workbench.md)). The two surfaces are compiled
from different features and never expose both meanings at once; the graphical
build reads the recovery record with `:recovery` and never changes it by hand. The graphical workshop's `:cell`,
`:preview`, `:discard`, `:source` and `:workbench` commands are documented in
[`native-graphics.md`](native-graphics.md) and the workbench guide.

## Deterministic limits

The native seed permits 128 syntax nodes, 24 global definitions, 24-byte names,
four function parameters, eight arguments/local slots, 192-byte stored bodies,
24 reader/call levels, 2,000 evaluation steps per submitted form, eight native
agents, eight messages per mailbox, 32 turns per `run`, 384 heap cells and
2,048 bytes of text. The serial
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

## v0.1.7 durable source workspace

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

This is enough to write, organize, and retain small programs inside Agel itself,
and to run fixed-memory agents beside an Agel-authored vector frame in the VM.
It is not yet a self-hosted graphical development environment: the editor and
storage codec are trusted Rust services (since v0.2.26 the ATA driver beneath
the codec is an unprivileged, restartable domain), and hosted macros, modules,
effects, model adapters, and rich agent protocols are not yet in the VM. Since v0.2.6
the graphical command surface and persistent source-cell workshop share the
real native evaluator. Project v0.2.8 adds the downward-bootstrap actor seed;
its exact transaction and containment semantics are in
[`native-agents.md`](native-agents.md).

## v0.2.29 disk-backed recovery

Sector 288 of the x86-64 disk holds a recovery record: the **trusted**
generation, the **candidate** generation, how many boots the candidate has
been given, and whether it has been verified. The record is supervisor policy
carried by the storage driver domain; no language world can reach it.

- `:save` publishes a generation as the candidate with a fresh budget. The
  previous slot is retained, so the trusted generation stays on disk as long as
  it is not the slot the next save reuses; the save chooses the slot that does
  not hold the trusted generation.
- Every boot of an unverified candidate is charged before any of it runs. The
  first form that evaluates successfully after such a boot marks the candidate
  verified and prints `candidate generation N verified by a healthy boot`.
- A candidate that fails three boots without reaching that point is not booted
  a fourth time: the supervisor prints
  `watchdog fault: candidate generation N failed 3 boots; rolling back to
  generation T` and replays the trusted generation instead. A boot that
  crashes, hangs in replay or is powered off before the first evaluation
  counts as a failure; nothing is needed from the candidate for the rollback
  to happen.
- `:verify` is explicit evidence: if the staged workspace has a cell named
  `health`, it is evaluated in an isolated candidate world that is then
  discarded, and only a clean evaluation admits the candidate. Without a
  `health` cell the operator's word is the evidence.
- `:promote` is denied until the candidate is verified. It makes the
  candidate the trusted generation and reports which earlier generation is
  retained for rollback.
- `:fault` rolls back to the trusted generation immediately and exhausts the
  candidate's budget, so later boots keep choosing the trusted generation
  until an operator verifies the candidate or saves a new one.
- `:recovery-status` reports the record and whether this boot is running the
  trusted generation after a rollback.

`./scripts/test-native-persistence.sh` proves the whole cycle on a temporary
disk: a failing then passing `health` cell, promotion, a new candidate, three
boots that exit before evaluating anything, the automatic rollback on the
fourth, an explicit fault, revival by `:verify`, and promotion with the
previous generation retained. A record that is absent or fails its CRC reads
as empty, which boots the newest generation exactly as before v0.2.29.

The record is not signed and the disk is trusted to hold what was written; a
malicious disk can present any record it likes. There is still one disk, two
slots and one record, so a rollback point survives exactly one further save.

## v0.1.6 isolation boundary

The evaluator holds no console-device grant and cannot name another domain's
stack. Its only mutable cross-boundary object is one 4 KiB shared page; source
and result payloads are each capped at 256 bytes. The supervisor switches to its
own page-table root before answering a trap, so a world's mappings never become
ambient supervisor authority. `.user_text` is read/execute, immutable constants
are read-only, stacks and the shared page are read/write, and no mapping is both
writable and executable.

Since v0.2.27 the serial reader polls the console driver domain rather than
the port; the recovery commands remain supervisor code.
Since v0.2.28 the same interactive workshop runs on AArch64 and RISC-V:
`./scripts/run-qemu.sh aarch64` or `riscv64` boots it on QEMU's `virt`
machine, with the evaluator in an EL0 or U-mode domain and every byte in and
out crossing the console driver domain. Those machines have no disk, so the
named-cell editor works in memory and `:save`/`:reload` answer "no storage
device on this machine" rather than pretending to persist. This is a protected language workshop,
not yet the full hosted agent runtime or a durable self-hosted environment.
