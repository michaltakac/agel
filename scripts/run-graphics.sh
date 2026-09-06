#!/bin/sh
# Open the real QEMU framebuffer with host-layout text input in a browser.
# --native selects QEMU's direct PS/2 window (US physical keyboard layout).
# Named source cells remain on the same persistent disk across launches.
set -eu

image=$(./scripts/build-boot.sh --features native-graphics | tail -n 1)
test -n "$image" && test -f "$image"
if test "${1:-}" != "--native"; then
  exec python3 ./scripts/graphical-console.py "$image" "$@"
fi
printf '%s\n' 'Direct QEMU input uses a US physical layout. Use the default browser console for Slovak/macOS text input.'
qemu-system-x86_64 \
  -machine pc,accel=tcg -m 64M -monitor none -serial stdio -no-reboot \
  -vga std -boot order=c,strict=on \
  -drive format=raw,file="$image",if=ide,index=0,media=disk
