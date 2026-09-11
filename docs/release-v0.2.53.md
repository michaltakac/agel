# Agel v0.2.53 — Windows that move

Windows move by their header, come to the front when clicked, and hand a
process the pointer's motion and release after a press in its content.

![The sketch window moved by its header, the dot with it, at v0.2.53](images/native-desktop-v0.2.53.png)

## What changed

- **Drag by the header.** A press in a window's header takes hold of it;
  while the button is held the window follows the pointer, clamped to
  the screen below the panel, and the place it left and the place it
  reaches are repainted together with the shadow's margin. The release
  lets go.
- **Raise on press.** The scene keeps the windows' order back to front;
  the frame paints them in that order, the hit test walks it from the
  front, and a press on a window (header or content) brings it to the
  front. A new window opens in front.
- **Motion and release events.** After a press in a window's content the
  pointer is the window's until the release: the owner receives
  `EVENT_MOTION`, coalesced to the latest position, and `EVENT_RELEASE`,
  in content coordinates clamped to sixteen bits. `<agel/window.h>`
  names them.
- The pointer decoder reports the button's state, so a release is seen.
- `sketch.c` moves the dot with the pointer until the release fixes it.

## Proof

`scripts/test-desktop-process.sh` presses in the sketch window, drags,
requires the dot to follow with no trace behind, reads
`sketch: release at 260,180`, drags the window by its header and requires
the dot to have moved with it and the old place repainted, ends the
program with `q`, opens a chart window over the sketch and requires the
dot covered, clicks the sketch and requires it back in front, and closes
both.

## Not claimed

No resize, maximize or minimize; no motion events without a press; no
modifier keys. Windows cannot be moved over the panel or off the screen.
