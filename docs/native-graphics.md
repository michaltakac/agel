# Native Agel graphics

Agel v0.2.4 boots to an actual 1024×768×32 live graphical desktop in QEMU. The path
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
in the launching terminal through the serial adapter. Stop QEMU from its window
or with Ctrl-C.

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
editor has a 48-byte input budget; titles are 1–28 ASCII letters, digits,
spaces, or hyphens. Escape clears the line and Backspace edits it.

A mutating form is decoded into a semantic intent rather than a drawing
command. The supervisor derives a complete candidate vector frame from the
immutable Agel baseline, and the ring-3 compositor revalidates every record.
Only a successful complete render with a nonzero framebuffer digest advances
the revision. Syntax or policy rejection leaves the retained scene unchanged;
only the isolated diagnostic command bar is redrawn to report the rejection.
`(rollback)` swaps the current and preceding semantic scenes and renders the
restored value without rebooting.

This bounded native parser is a bootstrap surface, not yet the full Agel
evaluator. It proves the live input/intent/transaction/display loop while
keeping the amount of pre-self-hosting Rust small and reviewable.

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

Run the headless proof with:

```sh
./scripts/test-graphics.sh
./scripts/test-live-desktop.sh
./scripts/test-live-keyboard.sh
```

The framebuffer backend is currently a scalar software reference renderer. It
is deterministic and bounded, not yet GPU accelerated or optimized with SIMD.
A later accelerated service can consume the same language-owned contract
without moving UI hierarchy, actions, customization policy, or agent authority
into the driver.

## What is next

The quickest route to working primarily inside Agel OS is to join this live
graphical loop to the existing persistent native source-cell evaluator. That
will let a user edit an Agel scene definition, preview it, commit it, reconstruct
it after reboot, and roll it back from the graphical shell. Input normalization
should then move from the supervisor into a separately restartable domain, with
pointer events represented as semantic hit-test requests rather than ambient UI
authority.
