# Agel v0.2.71 — It runs DOOM

The fifth rung of *Does it run DOOM?*: DOOM's shareware demo plays in a
window on the Agel desktop, the engine an ordinary C program in a
protection domain, the data a read-only file served by the filesystem
service, every frame a canvas the process draws and the compositor blits.

![DOOM's first level, played by its demo, in a window on the Agel desktop at v0.2.71](images/native-desktop-v0.2.71.png)

## What changed

- **The port.** `boot/posix/c/doom/` is `doomgeneric` unmodified (GPL v2,
  licence beside it); `c/doom.c` binds it to the window protocol: a
  640×400 window, a 320×200 canvas blitted at twice its size, keys from
  the window's down and up events (arrows, control to fire, alt to
  strafe, shift to run, space to use), the monotonic clock and
  `nanosleep`. `scripts/fetch-doom-wad.sh` fetches `doom1.wad` once,
  pinned by digest, and `scripts/test-doom.sh` runs the demo.
- **`PROCESS RUNNING`.** A program that keeps computing without ever
  listening or sleeping is handed the prompt back after 256 passes and
  stepped between inputs, so keys reach a game that renders as fast as
  it can.
- **Found on the way, fixed with a test each:** the loader's 128-page
  cap (the engine is 730 KiB; now 2048), `seek` refusing offsets past
  the 64 KiB filesystem file limit (any 32-bit offset now), a read that
  spans two sectors losing its first half to the second's delivery into
  the block area (assembled locally now, in the filesystem and data
  reads both), the data directory needing the filesystem mounted before
  it could be named, `fseek` flushing an empty buffer, and an exit
  status of `-1` reported as a wrapped number (eight bits now).
- `c/wadcheck.c` reads a WAD's header and directory the way the engine
  does and the test compares it with the host's view.
- Per-program compiler flags in `c/NAME.cflags`.

## Proof

`scripts/test-doom.sh` formats the region, checks the WAD is listed under
`/data` and reads as the host reads it, starts the engine with
`-timedemo demo1`, reads `PROCESS RUNNING`, waits for the thirty-fifth
frame, saves the screen and requires the window's content to be
painted, then waits for the demo's end: **timed 5026 gametics in 3562
realtics, 49.4 frames per second** under QEMU's TCG on this laptop. The
full regression, with the DOOM suite in it, passes.

## Not claimed

No sound; x86-64 and AArch64 only (RISC-V has no floating-point unit
here); on the Pi, keys wait for USB input. Saving the configuration on
exit needs a formatted filesystem region. The frame rate is emulation's
on one core, not the design's. Agel does not yet play it: that is the
next rung.
