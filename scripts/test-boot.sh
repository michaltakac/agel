#!/bin/sh
set -eu
. ./scripts/lib.sh

kernel=$(./scripts/build-boot.sh --features selftest | tail -n 1)
output_file=$(mktemp "${TMPDIR:-/tmp}/agel-boot.XXXXXX")
first_image=$(mktemp "${TMPDIR:-/tmp}/agel-image.XXXXXX")
trap 'rm -f "$output_file" "$first_image"' EXIT HUP INT TERM

cp "$kernel" "$first_image"
kernel=$(./scripts/build-boot.sh --features selftest | tail -n 1)
cmp "$first_image" "$kernel"

boot_x86_headless "$kernel" "$output_file"
grep -q 'AGEL_BOOT_OK' "$output_file"
grep -q 'recovery monitor is outside the mutable agent world' "$output_file"
printf '%s\n' "Agel QEMU boot self-test: reproducible BIOS seed -> long mode -> Rust HAL [ok]"
