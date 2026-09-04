#!/bin/sh
set -eu

kernel=$(./scripts/build-boot.sh --features monitor-selftest | tail -n 1)
output_file=$(mktemp "${TMPDIR:-/tmp}/agel-monitor.XXXXXX")
trap 'rm -f "$output_file"' EXIT HUP INT TERM

set +e
perl -e 'alarm shift; exec @ARGV' 15 qemu-system-x86_64 \
    -machine pc -m 64M -display none -monitor none -serial stdio -no-reboot \
    -device isa-debug-exit,iobase=0xf4,iosize=0x04 \
    -drive format=raw,file="$kernel" > "$output_file" 2>&1
status=$?
set -e

test "$status" -eq 33
grep -q 'active slot: A (stable)' "$output_file"
grep -q 'denied: verify candidate before promotion' "$output_file"
grep -q 'selected slot B; slot A retained for rollback' "$output_file"
grep -q 'active slot: B (candidate)' "$output_file"
grep -q 'watchdog fault: rolled back to slot A' "$output_file"
grep -q 'AGEL_MONITOR_OK' "$output_file"
printf '%s\n' "Agel recovery monitor test: deny -> verify -> promote -> rollback [ok]"
