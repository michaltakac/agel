#!/bin/sh
# The AArch64 kernel laid out for the Raspberry Pi 4, under QEMU's model of
# the board: a flat image at 0x80000, entered at EL2, RAM from zero, the
# PL011 on the header, no virtio and no PSCI. The workshop comes up and
# evaluates; there is no disk, and it says so.
set -eu
if ! qemu-system-aarch64 -machine help | grep -q '^raspi4b'; then
  printf '%s\n' 'Agel Raspberry Pi 4 board: skipped, this QEMU has no raspi4b machine'
  exit 0
fi
image=$(./scripts/build-kernel.sh raspi4 --features isolated-repl | tail -n 1)
python3 ./scripts/test-native-repl.py "$image" --smoke --arch raspi4
