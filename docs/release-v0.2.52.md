# Agel v0.2.52 — A window that listens

A process waits for presses and keys in its window while the desktop
keeps running. The process table runs in passes, so the graphical
workshop gives the prompt back when a program listens and runs it between
inputs until it ends.

![The sketch program's window with a dot where the pointer pressed, at v0.2.52](images/native-desktop-v0.2.52.png)

## What changed

- **`12` event in the process protocol.** The next event queued for a
  window the process owns, packed in one word: a press with its content
  coordinates, or a key with its byte; 0 when there is none, or the
  process sleeps until there is one. `-ENODEV` without a display,
  `-EBADF` for a window not its own. Each window keeps eight events and
  drops the oldest.
- **Presses and the keyboard.** A press in a window's content is queued
  for its live owner; the window takes the keyboard from opening or a
  click until the workshop is clicked, and keys typed on the serial
  console or the PS/2 keyboard go to it rather than the workshop's line.
  A window whose process ended queues nothing.
- **The process table in passes.** `exec` is split into `start`,
  `step_run` and `finish`; a pass gives every runnable process one entry
  and every blocked one a chance to be answered, and says whether
  something moved, whether every live process waits for the desktop, or
  how the first ended. The serial workshop loops over passes as before
  and stops a process asleep on an event, which nothing there delivers,
  as blocked.
- **The graphical workshop runs a listening program between inputs.**
  `:exec` runs to the end as before unless the program listens: then the
  command answers `PROCESS LISTENING`, the prompt returns, sixteen passes
  run per idle turn, the terminal panel is repainted when the program
  wrote, and the frame, the report and a fresh prompt come when it ends.
  A second `:exec` meanwhile is `A PROCESS IS RUNNING`.
- **From C:** `agel_event(window, &event, wait)` fills an
  `agel_window_event`; `boot/posix/c/sketch.c` puts a dot where each
  press lands, clears on any key, ends on `q`.

## Proof

`scripts/test-desktop-process.sh` runs `sketch`, requires the prompt back
with `PROCESS LISTENING` and no exit report, presses in the window's
content, reads `sketch: press at 200,140` on the serial console and the
dot's colour where the press landed, sends `q` on the serial console and
reads the quit line, the exit report and `PROCESS ENDED`, and requires
the dot still there until `:close 0`. The serial C library and spawn
tests pass unchanged over the split table.

## Not claimed

No release, motion or modifier events. A process that computes without
listening holds the desktop until it ends, as before. One running program
at a time; the sixteen-pass budget is a constant, not a scheduler. Windows
still do not move or resize.
