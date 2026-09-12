#!/bin/sh
set -eu
. ./scripts/lib.sh

kernel=$(./scripts/build-boot.sh --features monitor-selftest | tail -n 1)
output_file=$(mktemp "${TMPDIR:-/tmp}/agel-monitor.XXXXXX")
trap 'rm -f "$output_file"' EXIT HUP INT TERM

boot_x86_headless "$kernel" "$output_file"
grep -q 'active slot: A (stable)' "$output_file"
grep -q 'denied: verify candidate before promotion' "$output_file"
grep -q 'selected slot B; slot A retained for rollback' "$output_file"
grep -q 'denied: candidate B is already active; slot A remains rollback' "$output_file"
grep -q 'active slot: B (candidate)' "$output_file"
grep -q 'watchdog fault: rolled back to slot A' "$output_file"
grep -q 'AGEL_MONITOR_OK' "$output_file"
printf '%s\n' "Agel recovery monitor test: deny -> verify -> promote -> rollback [ok]"
