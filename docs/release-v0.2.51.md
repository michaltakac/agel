# Agel v0.2.51 — Windows a process can own

A process asks the desktop for a window and draws into it. The supervisor
checks every record against the window's content before it is painted,
keeps what passes, and repaints the window with the desktop, so it
outlives its process.

![Two windows a C program asked for, at v0.2.51](images/native-desktop-v0.2.51.png)

## What changed

- **Two requests in the process protocol.** `10` window (width, height,
  title) answers a window's number; `11` draw hands over up to eight
  64-byte compositor records in the block area, relative to the window's
  content, `DRAW_CLEAR` emptying the window first. `-ENODEV` where there
  is no display, `-EBUSY` when the two windows are taken or the process
  has one, `-EINVAL` for a bad size or a record the window does not
  permit, `-ENOSPC` past 24 records, `-EBADF` for a window the process
  does not own.
- **A `Display` trait** in the process table, beside the console:
  `open`, `draw`, `release`. The graphical workshop implements it over
  the scene's windows; the serial workshop passes none.
- **Every record is checked** before anything is painted: an admitted
  operation (rectangle, gradient rectangle, ellipse, label, surface,
  sprite), 24-bit colours, alphas at most 255, a face and sprite index the
  atlases carry, and the whole shape inside the content, a label measured
  in its face. One failing record refuses the request and nothing of it
  is drawn.
- **The desktop keeps the records** and translates them to the window's
  place when the frame is materialized; a window has COSMIC's surfaces, a
  header with the title in Fira Sans Medium, a close control from the
  sprite sheet and a shadow. Opening paints the box with its shadow once;
  each draw repaints the box alone.
- **Closing is a typed command:** the close control lightens under the
  pointer and a click on it becomes `:close N`, which can also be typed.
  A click elsewhere on a window is the window's. `(rollback)` leaves
  windows and the terminal alone.
- **From C:** `<agel/window.h>` declares `agel_window`, `agel_draw` and
  inline record builders; `boot/posix/c/chart.c` draws a bar for each
  number on its command line and, given `outside`, first asks for a
  rectangle past the edge and reports the refusal.
- The frame's record budget grows from 160 to 224.

## Proof

`scripts/test-desktop-process.sh` runs `chart` twice, requires the
refusal of the rectangle outside, both windows' bars on the screen (the
bars' colour appears inside the windows' boxes and nowhere there before),
closes the second by clicking its close control (`:close 1` echoed,
`WINDOW CLOSED 1`) and the first by typing `:close 0`, and requires the
bars gone and a third `:close 0` answered `NO SUCH WINDOW`.
`scripts/test-libc.sh` requires `chart: no display (errno 19)` and exit
status 3 on x86-64, AArch64 and RISC-V.

## Not claimed

A window receives no input, so a process cannot react to a click or a key
in it, and a process runs to its end before the desktop reads the next
input, so a window cannot animate. Windows do not move, resize or stack by
click; the workshop is not itself a window. Two windows, 24 records each.
