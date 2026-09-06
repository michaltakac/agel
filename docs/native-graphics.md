# Native Agel graphics

Agel v0.2.8 boots to an actual 1024×768×32 live graphical workshop in QEMU. The path
is deliberately split so that visual meaning remains language data and display
authority remains a narrow replaceable service:

```text
Agel vector source
  → bounded build validator
  → immutable 64-byte command records
  → supervisor envelope validation
  → ring-3 compositor revalidation
  → VBE framebuffer
```

Run it:

```sh
./scripts/run-graphics.sh
```

The kernel prints its containment report, draws a command surface, and remains
live. Click the QEMU window to type through the emulated PS/2 keyboard, or type
in the launching terminal through the serial adapter. Use `:shutdown`, stop
QEMU from its window, or press Ctrl-C. The disk is persistent: `:save` publishes
source cells which are replayed on the next boot.

## Live Agel forms

The first native scene language is intentionally postcard-sized:

```lisp
(accent violet)
(accent cyan)
(accent amber)
(workspace 1)
(workspace 2)
(workspace 3)
(title "MY AGENTIC WORKSPACE")
(inspect)
(rollback)
(help)
```

Both input adapters produce the same bounded byte stream. The visible line
editor has the evaluator's 256-byte input budget; titles are 1–28 ASCII letters, digits,
spaces, or hyphens. Escape clears the line and Backspace edits it.

A mutating form is decoded into a semantic intent rather than a drawing
command. The supervisor derives a complete candidate vector frame from the
immutable Agel baseline, and the ring-3 compositor revalidates every record.
Only a successful complete render with a nonzero framebuffer digest advances
the revision. Syntax or policy rejection leaves the retained scene unchanged;
only the isolated diagnostic command bar is redrawn to report the rejection.
`(rollback)` swaps the current and preceding semantic scenes and renders the
restored value without rebooting.

These visual forms are the tiny scene-control surface. Every other Lisp form is
sent through a bounded shared page to the existing native evaluator in a
separate ring-3 domain. For example:

```lisp
(def square (fn (x) (* x x)))
(square 12)
```

The result appears both in the graphical command bar and on serial. A language
error rolls back its evaluator transaction without changing the desktop.

The same evaluator now owns bounded executable agents. A behavior definition can
be persisted as a cell, replayed on boot, and instantiated from the desktop:

```lisp
:cell accumulate (def accumulate (fn (self state message) (+ state message)))
:save
(def counter (spawn accumulate 0))
(send counter 42)
(step)
(agent-state counter)
```

Agent state and mailboxes participate in native world transactions; failed
behavior turns are contained and explicitly recoverable. See
[`native-agents.md`](native-agents.md).

## Durable source cells

The graphical workshop owns the same crash-tolerant source format as the serial
workshop:

```text
:cell mathematics (def triangular (fn (n) (/ (* n (+ n 1)) 2)))
:run mathematics
(triangular 100)
:workspace
:save
```

`:cell NAME FORM` stages one bounded named form. `:run`, `:show`, `:delete`, and
`:cells` inspect and manipulate that source workspace. `:save` resets the
evaluator and successfully replays every staged cell before publishing an
alternating, CRC-checked raw-disk slot. A failed form or failed write restores
the preceding committed evaluator. On boot, the newest structurally valid and
semantically replayable generation wins; a broken newest generation falls back
to its twin.

This is deliberately source persistence, not a memory dump. Authority-bearing
state, device handles, evaluator stacks, and Rust layouts never cross a reboot.
See [`examples/graphical-workshop.txt`](../examples/graphical-workshop.txt) for a
complete session. `:help` prints the self-documenting command postcard.

## Device handoff

The 512-byte BIOS seed asks SeaBIOS for QEMU's 1024×768×32 linear VBE mode while
firmware calls are still possible. It leaves the mode-information block and an
explicit success marker in fixed low memory. Graphics failure never prevents
the serial recovery path from booting.

After enabling page-table isolation, the supervisor validates mode attributes,
pixel format, pitch, dimensions, physical address, checked byte length, and a
16 MiB upper bound. It maps only the framebuffer's physical pages into the
display domain. The mapping is user-writable, non-executable, and cache-disabled;
no ordinary world receives a translation for it.

## Agel-owned native frame

[`boot/desktop/native-desktop.agel`](../boot/desktop/native-desktop.agel) is the
default graphical shell. It is ordinary Lisp syntax containing a logical
viewport and vector operations: vertical gradients, rounded rectangles,
gradient rounded rectangles, ellipses, and resolution-independent procedural
cell text. Edit that file and rebuild to change the native desktop without
editing the compositor.

The build adapter is intentionally not presented as the full Agel evaluator. It
accepts only this postcard-sized, allocation-bounded vector form, enforces
operation arity, colors, text encoding and length, and a 256-command maximum,
then writes deterministic fixed-size records into the boot image. The full
hosted `agel/vector` library remains richer; closing that bootstrap gap is later
work.

## Compositor containment

The software rasterizer executes at ring 3. Each 64-byte record arrives through
the existing bounded shared page and is validated again. Drawing is clipped to
the declared surface, arithmetic is widened or saturating where appropriate,
and every command yields independently so the preemption budget applies.

The graphical tests prove these properties:

1. 31 Agel vector commands produce the fixed framebuffer digest
   `0x71acd98bb55c3d9f`.
2. An unknown vector operation is rejected and the digest remains identical.
3. A deliberate write to supervisor memory page-faults; a replacement display
   domain maps the same device and observes the unchanged last-good digest.
4. A semantic candidate changes the framebuffer, a rejected candidate does
   not, and rollback returns to the exact original framebuffer digest.
5. Real serial input commits several changes while invalid input is rejected.
6. QEMU-injected PS/2 scan codes become `(accent cyan)`, commit revision 1, and
   produce a real PPM framebuffer capture.
7. Ordinary Lisp definitions evaluate in the independent evaluator domain.
8. A named source cell is saved, the machine exits, the same disk reboots, and
   the restored definition evaluates to the expected result.
9. A native actor runs transactional mailbox turns in the graphical workshop;
   its persisted behavior is replayed and spawns a fresh actor after reboot.

Run the headless proof with:

```sh
./scripts/test-graphics.sh
./scripts/test-live-desktop.sh
./scripts/test-graphical-workshop.sh
./scripts/test-live-keyboard.sh
```

The framebuffer backend is currently a scalar software reference renderer. It
is deterministic and bounded, not yet GPU accelerated or optimized with SIMD.
A later accelerated service can consume the same language-owned contract
without moving UI hierarchy, actions, customization policy, or agent authority
into the driver.

## What is next

The next step is a language-owned graphical editor: multiline source cells,
selectable transcript history, pointer focus through semantic hit-test requests,
and preview/commit of a scene cell without leaving the desktop. Input
normalization should move from the supervisor into a separately restartable
domain rather than gaining ambient UI authority.
