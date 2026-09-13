# Agel v0.2.67 — Keys down and up

The second rung of *Does it run DOOM?*: a process learns which key went
down and which came up, so it can hold one.

## What changed

- **`EVENT_KEY_DOWN` (6) and `EVENT_KEY_UP` (7):** every scan code from
  the machine's keyboard reaches the focused window as an event with the
  set-1 code in the low byte and bit 8 for an `e0`-prefixed key; shifts,
  controls and caps lock included. A press that means a character is
  also the `EVENT_KEY` it always was, so nothing that reads characters
  changes.
- **The workshop's line** still sees only characters; a key that means
  nothing to it and is not a window's goes nowhere. The serial console
  remains characters only, and says so in the header.
- **`<agel/window.h>`** names the kinds, `AGEL_KEY_EXTENDED` and the keys
  a game needs; `agel_window_event.key` holds sixteen bits; `keys.c`
  reports what it receives.

## Proof

`scripts/test-desktop-process.sh` runs `keys.c`, sends `a`, the up
arrow and a shift through QEMU's keyboard and reads each going down and
up, the character for `a`, and the exit on `q`. The full regression
passes.

## Not claimed

No key repeat, no layouts beyond the US one, no keys from the serial
console; DOOM does not run yet.
