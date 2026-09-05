# Native Agel graphics

Agel v0.2.3 boots to an actual 1024×768×32 graphical desktop in QEMU. The path
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

The kernel prints its containment report on the terminal and then halts with
the graphical frame visible. Stop QEMU from its window or with Ctrl-C.

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

The graphical boot test proves three distinct properties:

1. 31 Agel vector commands produce the fixed framebuffer digest
   `0x71acd98bb55c3d9f`.
2. An unknown vector operation is rejected and the digest remains identical.
3. A deliberate write to supervisor memory page-faults; a replacement display
   domain maps the same device and observes the unchanged last-good digest.

Run the headless proof with:

```sh
./scripts/test-graphics.sh
```

The framebuffer backend is currently a scalar software reference renderer. It
is deterministic and bounded, not yet GPU accelerated or optimized with SIMD.
A later accelerated service can consume the same language-owned contract
without moving UI hierarchy, actions, customization policy, or agent authority
into the driver.

## What is next

This frame is real but not interactive. The next native graphics rung is a
separate keyboard/pointer domain that reports normalized input events to a
semantic intent router. Live native scene replacement must then use the same
preview, validation, commit, and rollback protocol as the hosted desktop rather
than granting a natural-language agent direct framebuffer or input authority.
