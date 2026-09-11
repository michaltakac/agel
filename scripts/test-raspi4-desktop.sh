#!/bin/sh
# The desktop on QEMU's Raspberry Pi 4: the graphics kernel as a flat image,
# a card holding the desktop's fonts, sprites and a C program, the frame
# read back through QEMU's screendump.
set -eu
if ! qemu-system-aarch64 -machine help | grep -q '^raspi4b'; then
  printf '%s\n' 'Agel desktop on the Raspberry Pi 4: skipped, this QEMU has no raspi4b machine'
  exit 0
fi
image=$(./scripts/build-kernel.sh raspi4 --features native-graphics | tail -n 1)
hello=$(./scripts/build-c-program.sh hello aarch64 | tail -n 1)
card=$(mktemp "${TMPDIR:-/tmp}/agel-pi-card.XXXXXX")
trap 'rm -f "$card"' EXIT HUP INT TERM
dd if=/dev/zero of="$card" bs=1048576 count=64 2>/dev/null
for asset in boot/desktop/assets/*; do
  name=$(basename "$asset")
  python3 ./scripts/install-asset.py "$card" "${name%.*}" "$asset" >/dev/null
done
python3 ./scripts/install-program.py "$card" c-hello "$hello" >/dev/null
python3 ./scripts/test-raspi4-desktop.py "$image" "$card"
