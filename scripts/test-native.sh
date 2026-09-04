#!/bin/sh
set -eu

kernel=$(./scripts/build-boot.sh --features native-selftest | tail -n 1)
output_file=$(mktemp "${TMPDIR:-/tmp}/agel-native.XXXXXX")
trap 'rm -f "$output_file"' EXIT HUP INT TERM

set +e
perl -e 'alarm shift; exec @ARGV' 15 qemu-system-x86_64 \
  -machine pc,accel=tcg -m 64M -display none -monitor none -serial stdio -no-reboot \
  -device isa-debug-exit,iobase=0xf4,iosize=0x04 \
  -boot order=c,strict=on \
  -drive format=raw,file="$kernel",snapshot=on > "$output_file" 2>&1
status=$?
set -e

test "$status" -eq 33
grep -q 'AGEL_NATIVE_OK' "$output_file"
printf '%s\n' "Agel native evaluator: arithmetic -> code/eval -> functions -> atomic rollback [ok]"
