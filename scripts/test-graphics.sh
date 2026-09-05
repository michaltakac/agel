#!/bin/sh
# Boot the real VBE path headlessly and require a deterministic frame from the
# unprivileged compositor, rejection without mutation, and fault containment.
set -eu

image=$(./scripts/build-boot.sh --features graphics-selftest | tail -n 1)
output=$(mktemp "${TMPDIR:-/tmp}/agel-graphics.XXXXXX")
trap 'rm -f "$output"' EXIT

set +e
qemu-system-x86_64 \
  -machine pc,accel=tcg -m 64M -display none -monitor none -serial stdio -no-reboot \
  -vga std -device isa-debug-exit,iobase=0xf4,iosize=0x04 \
  -boot order=c,strict=on \
  -drive format=raw,file="$image",if=ide,index=0,media=disk,snapshot=on \
  < /dev/null > "$output" 2>&1
status=$?
set -e

# QEMU's debug-exit device maps the guest's clean 0x10 to host status 33.
if test "$status" -ne 33; then
  cat "$output" >&2
  exit 1
fi

grep -q 'graphics\[x86_64\]: 1024x768x32, 31 Agel vector commands, digest 0x71acd98bb55c3d9f' "$output"
grep -q 'graphics\[x86_64\]: malformed frame rejected; last good frame retained' "$output"
grep -q 'graphics\[x86_64\]: compositor fault contained and replaced' "$output"
grep -q 'graphics\[x86_64\]: live Lisp scene commit/reject/rollback \[ok\]' "$output"
grep -q '^AGEL_GRAPHICS_OK' "$output"

printf '%s\n' 'Agel native graphics: VBE -> ring-3 vector compositor -> retained/recovered frame [ok]'
