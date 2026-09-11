#!/bin/sh
# A/B kernel slots with a boot budget enforced by the BIOS stage.
#
# On a temporary copy of the workshop image: the same kernel is staged as a
# candidate in slot B, verifies itself by a healthy boot, and is promoted; a
# kernel that halts before printing anything is staged as the candidate in
# slot A, is given three boots that never reach the serial console, and on the
# fourth the boot stage loads the trusted slot and the kernel reports the
# rollback; an explicit fault and a fresh candidate follow.
set -eu

image=$(./scripts/build-boot.sh --features isolated-repl | tail -n 1)
test_image=$(mktemp "${TMPDIR:-/tmp}/agel-kernel-ab.XXXXXX")
trap 'rm -f "$test_image"' EXIT HUP INT TERM
cp "$image" "$test_image"
# Blank the workspace slots, the recovery record, the selector and slot B.
dd if=/dev/zero of="$test_image" bs=512 seek=512 count=546 conv=notrunc 2>/dev/null
python3 ./scripts/test-native-repl.py "$test_image" --kernel-rollback target/boot/kernel.bin
