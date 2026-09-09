#!/bin/sh
# Boot Agel in QEMU's own graphical window.
# --web selects the optional host-layout text bridge.
# Named source cells remain on the same persistent disk across launches.
set -eu

image=$(./scripts/build-boot.sh --features native-graphics | tail -n 1)
test -n "$image" && test -f "$image"
if test "${1:-}" = "--workbench"; then
  shift
  # Keep the user's existing workshop disk and this demo's source cells separate.
  workbench_image="$(dirname "$image")/agel-workbench.img"
  test ! -L "$workbench_image"
  if test ! -e "$workbench_image"; then
    dd if=/dev/zero of="$workbench_image" bs=512 count=2048 2>/dev/null
  fi
  test -f "$workbench_image"
  dd if="$image" of="$workbench_image" bs=512 count=256 conv=notrunc 2>/dev/null
  image="$workbench_image"
fi
if test "${1:-}" = "--web"; then
  shift
  cargo build --release -q -p agel-jit --example module_workshop
  exec python3 ./scripts/graphical-console.py "$image" "$@"
fi
printf '%s\n' 'Direct QEMU input uses a US physical layout. Use --web for Slovak/macOS text composition.'
printf '%s\n' 'On a fresh empty world, type :workbench. Click a dock icon, or Tab then Enter. QEMU owns its mouse-capture/release shortcut.'
qemu-system-x86_64 \
  -machine pc,accel=tcg -m 64M -monitor none -serial stdio -no-reboot \
  -vga std -boot order=c,strict=on \
  -drive format=raw,file="$image",if=ide,index=0,media=disk
