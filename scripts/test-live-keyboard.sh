#!/bin/sh
# Inject PC keyboard scan codes through QEMU's emulated PS/2 device and prove
# that the graphical shell commits the resulting semantic Agel form.
set -eu

image=$(./scripts/build-boot.sh --features native-graphics | tail -n 1)
serial=$(mktemp "${TMPDIR:-/tmp}/agel-live-keyboard-serial.XXXXXX")
monitor=$(mktemp "${TMPDIR:-/tmp}/agel-live-keyboard-monitor.XXXXXX")
frame=$(mktemp "${TMPDIR:-/tmp}/agel-live-keyboard-frame.XXXXXX")
trap 'rm -f "$serial" "$monitor" "$frame"' EXIT

{
  sleep 2
  for key in shift-9 a c c e n t spc c y a n shift-0 ret
  do
    printf 'sendkey %s\n' "$key"
    sleep 0.08
  done
  # Quote/eval needs formerly missing apostrophe; comparison needs '<'.
  for key in shift-9 e v a l spc apostrophe shift-9 shift-comma spc 1 spc 2 shift-0 shift-0 ret
  do
    printf 'sendkey %s\n' "$key"
    sleep 0.08
  done
  # Shifted letters and underscore must survive physical input intact.
  for key in shift-9 d e f spc shift-a shift-minus shift-b spc 4 2 shift-0 ret shift-a shift-minus shift-b ret
  do
    printf 'sendkey %s\n' "$key"
    sleep 0.08
  done
  sleep 2
  printf 'screendump %s\n' "$frame"
  sleep 1
  printf 'quit\n'
} | qemu-system-x86_64 \
  -machine pc,accel=tcg -m 64M -display none -monitor stdio \
  -serial "file:$serial" -no-reboot -vga std -boot order=c,strict=on \
  -drive format=raw,file="$image",if=ide,index=0,media=disk,snapshot=on \
  > "$monitor" 2>&1

grep -q '^AGEL_GRAPHICS_OK' "$serial"
grep -q 'live-desktop> (accent cyan)' "$serial"
grep -q 'COMMITTED REV 1' "$serial"
grep -Fq "live-desktop> (eval '(< 1 2))" "$serial"
grep -q '^#t' "$serial"
grep -Fq 'live-desktop> A_B' "$serial"
grep -q '^42' "$serial"
test "$(head -c 2 "$frame")" = P6

printf '%s\n' 'Agel live desktop: PS/2 keyboard -> normalized form -> scene commit [ok]'
