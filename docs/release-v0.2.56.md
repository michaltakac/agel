# Agel v0.2.56 — The card

The Raspberry Pi's SD card is driven: programs load from it and the
workspace persists on it, under QEMU's model of the Pi 4.

## What changed

- **An SD Host Controller driver domain**, by programmed I/O: the same
  storage protocol every driver speaks, over the controller's register
  page, every access a 32-bit word as the Pi's Arasan controller
  requires. Reset, power, the identification sequence (`GO_IDLE_STATE`,
  `SEND_IF_COND`, `SD_SEND_OP_COND`, `ALL_SEND_CID`, `SEND_RELATIVE_ADDR`,
  `SEND_CSD`, `SELECT_CARD`), the capacity from a version 1 or 2 CSD,
  byte or sector addressing as the card is, single-block reads and writes
  through the buffer port, requests past the capacity refused, a silent
  controller timed out.
- **The board names two controllers** (EMMC2, where the Pi 4's slot is
  wired; the first, where QEMU puts the card); the supervisor reads each
  present-state register once and grants the page of the one with a card.
- The virtio driver, the DMA page and the memory fence are compiled out
  of the board build; nothing else changes on the `virt` kernels.

## Proof

`scripts/test-raspi4.sh` boots the board without a card (the workshop
says it has no disk), then makes a 64 MiB standard-capacity card with
the `hello` program installed, boots with it, runs the program from the
card, stages and saves a cell, boots again on the same card and requires
`workspace generation 1 restored: 1 cells replayed`, the cell's value,
the healthy-boot verification and a clean workspace. The full
regression passes.

## Not claimed

The real controller: the sequence is the standard one and QEMU's model
accepts it; the board will say. No multi-block transfers, no DMA, no
card removal while running. The Pi 5's addresses remain unverified.
