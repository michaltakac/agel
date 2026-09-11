# Agel v0.2.55 — A board

The AArch64 kernel boots on QEMU's Raspberry Pi 4 as a flat image entered
at EL2, the rehearsal for the Pi 5 on the bench.

## What changed

- **A `board` module** is the one place the AArch64 kernel names a
  physical address: RAM, the device window, the PL011, the GIC, the frame
  pool, the virtio transports (or none), and whether PSCI exists. QEMU's
  `virt` is the default; the `board-raspi4` feature selects the Pi 4's
  layout (BCM2711).
- **A flat image at `0x80000`.** The linker script takes its load
  address from `AGEL_LOAD`, defined by the build script for the board;
  `scripts/build-kernel.sh raspi4` builds in its own target directory and
  prints `kernel8.img`, made with whichever objcopy knows AArch64.
- **The drop from EL2.** `agel_boot` reads `CurrentEL` and, at EL2, makes
  EL1 AArch64 with the physical timer untrapped, resets `SCTLR_EL1`, and
  `eret`s to its next instruction with interrupts masked. EL2 keeps no
  vectors.
- **RAM from zero, the GIC-400, the PL011 on the header**, through the
  same page tables, the same console driver domain and the same
  preemption path, with the board's addresses.
- **No disk, said plainly:** the SD controller is not driven; the
  workshop reports `no block device driver for this board yet` and
  refuses `:exec`.
- `docs/raspberry-pi.md` records what the Pi 5 changes and the order of
  the work to reach it.

## Proof

`scripts/test-raspi4.sh` builds the image and boots it on `raspi4b`:
the kernel names its board, `AGEL_NATIVE_READY` arrives through the PL011
driver domain, `(+ 20 22)` answers 42, a function is defined and called,
and `:exec hello` is `denied: no storage device`. Where QEMU has no
`raspi4b` the test skips itself and says so. The `virt` kernels and every
other suite pass unchanged.

## Not claimed

The Pi 5's addresses are unverified without the board. The device tree
in `x0` is not read. No SD driver, no framebuffer, no other cores. Run
under QEMU only; the board is the next thing to try.
