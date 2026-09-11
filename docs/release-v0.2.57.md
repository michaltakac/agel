# Agel v0.2.57 — A Pi 5 image, unrun

The Raspberry Pi 5's layout, built from the board's documented addresses
and never executed: no emulator models the board, so the image waits for
it.

## What changed

- **Two device windows.** The AArch64 identity map takes two gibibyte
  device windows instead of one, because the BCM2712's peripherals span
  two gibibytes from 64 GiB: the SD host controller in the first, the
  debug UART, the mailbox and the GIC-400 in the second. The `virt` and
  Pi 4 layouts name the same gibibyte twice.
- **`board-raspi5`:** RAM from zero, the image at `0x80000`, the PL011 on
  the 3-pin debug connector at `0x10_7d00_1000`, the GIC-400 at
  `0x10_7fff_9000`, the SD slot's controller at `0x10_00ff_f000`, no
  virtio, no PSCI. The entry, the drivers and the workshop are the Pi 4's.
- **`build-kernel.sh raspi5`** prints `kernel_2712.img`;
  `docs/raspberry-pi.md` gives the `config.txt` and what to expect on the
  UART.

## Proof

Compile-only: the Pi 5 layout is linted with the REPL and the isolation
self-test features in the regression. The Pi 4 board test and every other
suite pass unchanged over the two-window identity map.

## Not claimed

The image has never run. Its addresses are from documentation; the board
will confirm or correct them, starting with the first byte on the UART.
