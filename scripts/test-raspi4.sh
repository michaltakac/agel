#!/bin/sh
# The AArch64 kernel laid out for the Raspberry Pi 4, under QEMU's model of
# the board: a flat image at 0x80000, entered at EL2, RAM from zero, the
# PL011 on the header, the card on an SD host controller, no PSCI. Without
# a card the workshop comes up and says so; with one it loads a program
# from the card and persists a cell across two boots.
set -eu
if ! qemu-system-aarch64 -machine help | grep -q '^raspi4b'; then
  printf '%s\n' 'Agel Raspberry Pi 4 board: skipped, this QEMU has no raspi4b machine'
  exit 0
fi
image=$(./scripts/build-kernel.sh raspi4 --features isolated-repl | tail -n 1)
hello=$(./scripts/build-program.sh hello aarch64 | tail -n 1)
python3 ./scripts/test-native-repl.py "$image" --smoke --arch raspi4
# A 64 MiB card: QEMU's model wants a power of two, and a card this small
# is a standard-capacity one, addressed by byte.
card=$(mktemp "${TMPDIR:-/tmp}/agel-card.XXXXXX")
trap 'rm -f "$card"' EXIT HUP INT TERM
dd if=/dev/zero of="$card" bs=1048576 count=64 2>/dev/null
python3 ./scripts/install-program.py "$card" hello "$hello" >/dev/null
python3 ./scripts/test-native-repl.py "$image" --board --arch raspi4 --disk "$card"
