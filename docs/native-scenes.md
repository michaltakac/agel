# Agel-authored live native scenes

The v0.2.10 milestone connects the language world to the framebuffer. In the
bootable OS, ordinary functions and actors can add vector shapes while running.
The window is QEMU's normal display; `--web` is an optional host text-input
bridge. Neither mode runs the kernel in JavaScript.

## Three primitives, ordinary libraries

```lisp
(scene-clear)
(scene-rect 318 610 372 72 20 3423048)
(scene-count)
```

`scene-rect` takes integer `x y width height radius rgb`; RGB is a decimal
24-bit color. It appends a rounded rectangle to a retained overlay and returns
the new count. `scene-clear` empties that overlay. There are at most 12 records;
all rectangles must fit the 1024×684 logical drawing region above the reserved
command bar. Radius is at most half either dimension. No arbitrary drawing
opcode, memory address, or device capability is exposed to Agel source.

[`boot/desktop/dock.agel`](../boot/desktop/dock.agel) builds a dock from these
primitives as ordinary native Agel functions. It is a visual dock prototype,
not yet a clickable application launcher. Follow
[`examples/native-dock.txt`](../examples/native-dock.txt) to stage its source
cells, save the dock, repaint it through a native actor, and roll back its turn.
The example requires no model inference and no host rebuild.

## Commit, display, and recovery

Rectangles belong to the same fixed world as definitions and actor mailboxes.
A rejected form or behavior turn cannot leave provisional rectangles behind.
`:rollback` restores the preceding language scene and actor state together.
The older `(rollback)` command concerns the shell's accent/workspace/title;
use `:rollback` for language-generated content.

After a command, the supervisor reads committed records through the evaluator's
bounded shared page, checks counts, revision consistency, geometry, and colors,
and sends the complete validated candidate to the isolated compositor. The
compositor revalidates records before drawing. The supervisor retains the last
rendered scene; a failed render attempts to redraw it and reports the failure.
This is atomic language state, not a hardware-atomic framebuffer swap or a
distributed transaction across disk and display. A hardware domain failure
can still require recovery.

Source cells rebuild the drawing data on save/reload and boot. Raw evaluator
memory and device handles are never written to the source workspace. Tests
compare actual QEMU framebuffer pixels above the diagnostic bar before/after
an actor turn, after rollback, after invalid geometry, and after reboot.

## What remains for “say it and the OS changes”

1. Pointer events, semantic hit testing, focus, and a push-to-talk hotkey in
   the native window. The present dock shapes have no click actions.
2. A microphone capture service and transcription adapter. A temporary native
   host companion can bridge audio while guest audio/network drivers mature;
   it should not replace the OS's display window.
3. An intent agent that receives the transcript, reads the current scene/source
   revision, and asks an explicit model provider for a bounded code proposal.
4. A candidate evaluator, validation, preview, and revision-checked promotion
   of that proposal, retaining the last working source and frame.
5. For animated SVG wallpapers: a bounded SVG subset/importer, an animation
   clock, per-frame CPU/allocation budgets, and an Agel wallpaper service.
   Loading arbitrary SVG or executing SVG scripts is not implemented here.

OpenAI documents Whisper and other models via the
[speech-to-text API](https://developers.openai.com/api/docs/guides/speech-to-text).
Codex's [authentication documentation](https://developers.openai.com/codex/auth)
distinguishes subscription sign-in from API access. This project must use a
supported speech adapter with its own configured credentials, or a separately
installed local transcription service; it must not repurpose Codex login
credentials as a Whisper API key. This release adds no recording or speech
requests and has no speech-related charge.

## Boot targets

QEMU already boots a freestanding disk image into Agel's kernel and graphical
display. VirtualBox is a possible additional test target, not a prerequisite
for that model. It requires a tested boot/device configuration. Physical NUC
and Raspberry Pi support likewise needs board/firmware and real device-driver
coverage; passing QEMU's emulated-device tests is not hardware certification.
See [deployment targets](deployment-targets.md) for the project's direction.
