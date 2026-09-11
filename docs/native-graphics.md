# Native Agel graphics

Agel boots to a live graphical workshop in QEMU, at 1920×1080×32 since
v0.2.48 (1024×768 before it, and still when the display cannot be set). The path
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

The default launcher boots into QEMU's own native window. `--web` optionally
opens a local browser console with the real QEMU
framebuffer and a host text field. Use your Slovak/macOS layout, Option symbols,
dead keys, and Command-V paste there; Enter or **Run in Agel** submits one line
to the guest. Clicking the image focuses the field without capturing your
mouse. Stop the viewer and QEMU with Ctrl-C in the launching terminal. The disk
is persistent: `:save` publishes source cells replayed on the next boot.

`./scripts/run-graphics.sh` (equivalently `--native`) uses QEMU's direct window and serial
terminal input. The launcher validates its flags, accepts `--workbench`, `--native`,
`--web` and `--help` in any order, rejects anything else, and passes arguments
after `--` to QEMU. This physical PS/2 path uses a US layout, now including all
ASCII punctuation, uppercase, Caps Lock, independent Shift keys, and Ctrl-U/C
to clear the line (Ctrl-H backspaces). It does not inherit macOS text layout or
Option dead-key composition. QEMU's Cocoa frontend controls mouse capture;
its release shortcut is Control-Option-G by default (see QEMU's View menu and
the [QEMU frontend keys](https://www.qemu.org/docs/master/system/keys.html)).

The browser console is explicitly a host input bridge, not a native browser
inside Agel. It binds only to loopback with a random session URL, checks POST
origins, and exposes bounded text submission and framebuffer capture. It has
no arbitrary command execution endpoint. Every form still executes in Agel's
isolated evaluator, with 256 UTF-8 bytes per input. Source and the host
transcript preserve Unicode; the seed framebuffer font uses `?` for unsupported
bytes. Characters such as `≤` are not automatically new language operators.

## Typography, surfaces and assets (v0.2.47), native resolution and sprites (v0.2.48)

![The native desktop at v0.2.48](images/native-desktop-v0.2.48.png)

The compositor draws from three more record operations, each validated
like the others:

| Operation | Words | What it draws |
|---|---|---|
| `label` (6) | x, y, face, size, colour, alpha, length; text at byte 36 | anti-aliased text from a font atlas, blended over what is below |
| `surface` (7) | x, y, width, height, radius, colour, alpha | a rounded box blended with an alpha, its corners anti-aliased |
| `shadow` (8) | x, y, width, height, radius, blur, alpha | a linear falloff around a box, drawn as rings, the box itself left alone |
| `sprite` (9) | x, y, sprite, tint, alpha | a sprite from the sheet, blended by its own alpha; in `tint` when that is not black, so one white glyph serves every colour |

The faces are font atlases in the disk's **asset region** (sectors 3072
through 6143, a table like the program region's). `scripts/build-font-atlas.py`
rasterizes a TrueType font with Pillow into `AGF1`: for each pixel size,
the 96 printable ASCII glyphs with their metrics and 8-bit coverage
bitmaps. The build installs Fira Sans Regular and Medium (12 to 32 px) and
Fira Mono (12 to 20 px), bundled under the SIL Open Font License in
`boot/desktop/fonts`. At boot the graphics supervisor reads each atlas
through the storage driver domain, checks its CRC-32, maps it read-only
into the compositor's asset window, and keeps the metrics it needs to lay
text out; the compositor checks every offset an atlas names against the
length it was told before reading it. The graphics image refuses to boot
without its faces. The sprite sheet (`AGI1`, RGBA8) is drawn by
`scripts/build-sprites.py` with Pillow at four times its size and scaled
down: the arrow cursor, seven dock icons, the three window controls and a
search glyph.

Since v0.2.48 the supervisor sets the display to 1920×1080×32 through the
Bochs display interface QEMU's standard VGA exposes (two I/O ports), keeping
the linear framebuffer the BIOS mode reported; where the interface is absent
the BIOS mode stays and the scene is scaled into it, with text unscaled,
which is a limit of that fallback rather than the design. The scene is laid
out at 1920×1080; the language's drawing region is 1920×1000, above the
command field. The pointer is the cursor sprite. Painting drains the input
driver between records into a queue the session reads first, so keys typed
while a frame is painted are kept: a frame of blended surfaces is slow
under emulation, and before this the controller's buffer overran.

The scene in `boot/desktop/native-desktop.agel` uses COSMIC's tokens: its
dark palette (`#1b1b1b`, `#262626`, `#333333`, text `#dedede` and
`#9e9e9e`, accents `#e79cfe`, `#63d0df`, `#ffad00`), corner radii of 8 and
16 pixels, spacing in steps of 8. A top panel with workspaces, the workshop
window with a title bar, a sidebar of spaces and agents, two cards and three
actions, a floating dock of six tiles, and the command field in Fira Mono.
The accent intents recolour every accented record.

Drawing is bounded by a **clip rectangle** the supervisor sets in the
shared page: a keystroke redraws the command field, a pointer that moved
redraws the union of where it was and where it is, and only a committed
scene change redraws the screen, so input is not lost to painting.

What this is not yet: no windows a process owns, no launcher behind the
panel's words, no clock, and the shadow is a linear falloff, not a blur.

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

Since v0.2.10, ordinary evaluated Agel can build a live vector overlay through
`scene-clear`, `scene-rect`, and `scene-count`. The dock library, actor-driven
redraw, bounds, and persistence walkthrough are in
[`native-scenes.md`](native-scenes.md).

The graphical workshop owns the same crash-tolerant source format as the serial
workshop:

```text
:cell mathematics (def triangular (fn (n) (/ (* n (+ n 1)) 2)))
:run mathematics
(triangular 100)
:workspace
:save
```

`:cell NAME FORM` stages one bounded named form. `:run`, `:show`, `:delete`,
`:cells` and `:workspace` inspect and manipulate that source workspace. `:save` resets the
evaluator and successfully replays every staged cell before publishing an
alternating, CRC-checked raw-disk slot. A failed form or failed write restores
the preceding committed evaluator. On boot, the newest structurally valid and
semantically replayable generation wins; a broken newest generation falls back
to its twin.

This is deliberately source persistence, not a memory dump. Authority-bearing
state, device handles, evaluator stacks, and Rust layouts never cross a reboot.
See [`examples/graphical-workshop.txt`](../examples/graphical-workshop.txt) for a
complete session. `:help` prints the self-documenting command postcard; since
v0.2.22 its length is checked at build time against the status line, so it can
no longer be silently truncated.

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

1. 82 Agel vector commands produce the fixed framebuffer digest
   `0xdb1898cb6a32adc7` (31 commands and `0x71acd98bb55c3d9f` before the
   v0.2.47 restyle).
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
python3 scripts/test-graphical-console.py target/boot/agel-v1.img
python3 scripts/test-native-dock.py target/boot/agel-v1.img
```

The framebuffer backend is currently a scalar software reference renderer. It
is deterministic and bounded, not yet GPU accelerated or optimized with SIMD.
A later accelerated service can consume the same language-owned contract
without moving UI hierarchy, actions, customization policy, or agent authority
into the driver.

## What is next

The next step is a language-owned graphical editor: multiline source cells,
selectable transcript history, pointer focus through semantic hit-test requests,
and preview/commit of a scene cell without leaving the desktop. Since v0.2.27
raw input already enters through restartable driver domains: serial bytes
through the console driver and keyboard/pointer bytes through an 8042 driver
granted only ports 0x60 and 0x64. Scan-code decoding, packet assembly and
every policy about what a key means stay in the supervisor, and the drivers
hold no UI authority.
