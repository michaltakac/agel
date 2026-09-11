#!/bin/sh
# Boot Agel in QEMU's own graphical window.
# --workbench boots a separate persistent demo disk; --web adds the optional
# host-layout text bridge; --native (the default) uses QEMU's direct window.
# Named source cells remain on the same persistent disk across launches.
set -eu

usage() {
  cat <<'USAGE'
Usage: ./scripts/run-graphics.sh [--workbench] [--native | --web] [-- QEMU-ARGS...]
  --workbench  boot target/boot/agel-workbench.img, a separate persistent demo disk
  --native     QEMU's direct window and serial input (default; US physical layout)
  --web        also open the loopback browser console for host-layout text entry
  --help       show this message
USAGE
}

workbench=false
web=false
while test $# -gt 0; do
  case "$1" in
    --workbench) workbench=true ;;
    --native) web=false ;;
    --web) web=true ;;
    --help | -h) usage; exit 0 ;;
    --) shift; break ;;
    *) printf 'unknown option: %s\n' "$1" >&2; usage >&2; exit 2 ;;
  esac
  shift
done

image=$(./scripts/build-boot.sh --features native-graphics | tail -n 1)
test -n "$image" && test -f "$image"
if $workbench; then
  # Keep the user's existing workshop disk and this demo's source cells separate.
  workbench_image="$(dirname "$image")/agel-workbench.img"
  test ! -L "$workbench_image"
  if test ! -e "$workbench_image"; then
    dd if=/dev/zero of="$workbench_image" bs=512 count=6144 2>/dev/null
  fi
  test -f "$workbench_image"
  dd if="$image" of="$workbench_image" bs=512 count=512 conv=notrunc 2>/dev/null
  # The asset region travels with the seed: the fonts the desktop is set in.
  dd if="$image" of="$workbench_image" bs=512 skip=3072 seek=3072 count=3072 conv=notrunc 2>/dev/null
  image="$workbench_image"
fi
if $web; then
  cargo build --release -q -p agel-jit --example module_workshop
  exec python3 ./scripts/graphical-console.py "$image" "$@"
fi
printf '%s\n' 'Direct QEMU input uses a US physical layout. Use --web for Slovak/macOS text composition.'
printf '%s\n' 'On a fresh empty world, type :workbench. Click a dock icon, or Tab then Enter. QEMU owns its mouse-capture/release shortcut.'
exec qemu-system-x86_64 \
  -machine pc,accel=tcg -m 64M -monitor none -serial stdio -no-reboot \
  -vga std -boot order=c,strict=on \
  -drive format=raw,file="$image",if=ide,index=0,media=disk "$@"
