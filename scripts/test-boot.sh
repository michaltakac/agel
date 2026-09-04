#!/bin/sh
set -eu

kernel=$(./scripts/build-boot.sh --features selftest | tail -n 1)
output_file=$(mktemp "${TMPDIR:-/tmp}/agel-boot.XXXXXX")
first_image=$(mktemp "${TMPDIR:-/tmp}/agel-image.XXXXXX")
trap 'rm -f "$output_file" "$first_image"' EXIT HUP INT TERM

cp "$kernel" "$first_image"
kernel=$(./scripts/build-boot.sh --features selftest | tail -n 1)
cmp "$first_image" "$kernel"

set +e
perl -e 'alarm shift; exec @ARGV' 15 qemu-system-x86_64 \
  -machine pc -m 64M -display none -monitor none -serial stdio -no-reboot \
  -device isa-debug-exit,iobase=0xf4,iosize=0x04 \
  -drive format=raw,file="$kernel" > "$output_file" 2>&1
status=$?
set -e

test "$status" -eq 33
grep -q 'AGEL_BOOT_OK' "$output_file"
grep -q 'recovery monitor is outside the mutable agent world' "$output_file"
printf '%s\n' "Agel QEMU boot self-test: reproducible BIOS seed -> long mode -> Rust HAL [ok]"
