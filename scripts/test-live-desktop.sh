#!/bin/sh
# Drive the real persistent graphical kernel through its serial input adapter.
# The same normalized byte stream is produced by the QEMU-window PS/2 keyboard.
set -eu

image=$(./scripts/build-boot.sh --features native-graphics | tail -n 1)
output=$(mktemp "${TMPDIR:-/tmp}/agel-live-desktop.XXXXXX")
trap 'rm -f "$output"' EXIT

# Each command waits for the workshop's prompt, so boot time and paint time
# never race the input; the prompts are counted in the serial output.
prompts() { awk '/live-desktop> /{n++} END{print n+0}' "$output" 2>/dev/null || printf 0; }
await_prompt() {
  wanted=$1
  tries=0
  while test "$(prompts)" -lt "$wanted"; do
    tries=$((tries + 1))
    test "$tries" -lt 600 || exit 1
    sleep 0.1
  done
}
{
  await_prompt 1
  seen=1
  for command in \
    '(inspect)' \
    '(accent cyan)' \
    '(workspace 2)' \
    '(title "LIVE AGEL")' \
    '(rollback)' \
    '(workspace 99)'
  do
    printf '%s\n' "$command"
    seen=$((seen + 1))
    await_prompt "$seen"
  done
} | qemu-system-x86_64 \
  -machine pc,accel=tcg -m 64M -display none -monitor none -serial stdio -no-reboot \
  -vga std -boot order=c,strict=on \
  -drive format=raw,file="$image",if=ide,index=0,media=disk,snapshot=on \
  > "$output" 2>&1 &
qemu_pid=$!

tries=0
until grep -q 'WORKSPACE MUST BE 1 2 OR 3' "$output"; do
  tries=$((tries + 1))
  test "$tries" -lt 900 || break
  sleep 0.1
done
kill "$qemu_pid" 2>/dev/null || true
wait "$qemu_pid" 2>/dev/null || true

grep -q '^AGEL_GRAPHICS_OK' "$output"
grep -q 'REV 0 WS 1 VIOLET' "$output"
grep -q 'COMMITTED REV 1' "$output"
grep -q 'COMMITTED REV 2' "$output"
grep -q 'COMMITTED REV 3' "$output"
grep -q 'COMMITTED REV 4' "$output"
grep -q 'WORKSPACE MUST BE 1 2 OR 3' "$output"

printf '%s\n' 'Agel live desktop: input -> semantic intent -> commit/reject/rollback [ok]'
