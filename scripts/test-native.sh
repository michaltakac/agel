#!/bin/sh
set -eu
. ./scripts/lib.sh

kernel=$(./scripts/build-boot.sh --features native-selftest | tail -n 1)
output_file=$(mktemp "${TMPDIR:-/tmp}/agel-native.XXXXXX")
trap 'rm -f "$output_file"' EXIT HUP INT TERM

boot_x86_headless "$kernel" "$output_file"
grep -q 'AGEL_NATIVE_OK' "$output_file"
printf '%s\n' "Agel native evaluator: arithmetic -> code/eval -> functions -> atomic rollback [ok]"
