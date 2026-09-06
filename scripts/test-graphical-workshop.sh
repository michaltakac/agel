#!/bin/sh
# Prove that the graphical shell evaluates real Agel and reconstructs a saved
# source workspace after rebooting the same disk. Both boots use a disposable
# copy with initially blank source slots; the developer image is never mounted.
set -eu

image=$(./scripts/build-boot.sh --features native-graphics | tail -n 1)
test_image=$(mktemp "${TMPDIR:-/tmp}/agel-graphical-workshop.XXXXXX")
first=$(mktemp "${TMPDIR:-/tmp}/agel-graphical-first.XXXXXX")
second=$(mktemp "${TMPDIR:-/tmp}/agel-graphical-second.XXXXXX")
trap 'rm -f "$test_image" "$first" "$second"' EXIT HUP INT TERM
cp "$image" "$test_image"
dd if=/dev/zero of="$test_image" bs=512 seek=256 count=32 conv=notrunc 2>/dev/null

boot() {
  output=$1
  shift
  set +e
  {
    sleep 2
    for command in "$@"; do
      printf '%s\n' "$command"
      sleep 1
    done
  } | qemu-system-x86_64 \
    -machine pc,accel=tcg -m 64M -display none -monitor none -serial stdio \
    -no-reboot -device isa-debug-exit,iobase=0xf4,iosize=0x04 \
    -vga std -boot order=c,strict=on \
    -drive format=raw,file="$test_image",if=ide,index=0,media=disk \
    > "$output" 2>&1
  status=$?
  set -e
  test "$status" -eq 33
}

has_line() {
  tr -d '\r' < "$1" | grep -q "$2"
}

boot "$first" \
  '(def answer 42)' \
  '(+ answer 8)' \
  ':cell durable (def durable 77)' \
  ':cell actor-behavior (def accumulate (fn (self state message) (+ state message)))' \
  ':run actor-behavior' \
  '(def counter (spawn accumulate 0))' \
  '(send counter 20)' \
  '(send counter 22)' \
  '(run 2)' \
  '(agent-state counter)' \
  ':save' \
  ':shutdown'

has_line "$first" '^AGEL_GRAPHICS_OK$'
has_line "$first" '^42$'
has_line "$first" '^50$'
has_line "$first" '^CELL STAGED - RUN OR SAVE$'
has_line "$first" '^#<native-function>$'
has_line "$first" '^#<native-agent:1>$'
has_line "$first" '^42$'
has_line "$first" '^SAVED GENERATION 1$'

boot "$second" \
  '(+ durable 1)' \
  '(def restored (spawn accumulate 40))' \
  '(send restored 2)' \
  '(step)' \
  '(agent-state restored)' \
  ':workspace' \
  ':shutdown'

has_line "$second" '^AGEL_GRAPHICS_OK$'
has_line "$second" '^78$'
has_line "$second" '^#t$'
has_line "$second" '^42$'
has_line "$second" '^GEN 1 CELLS 2 CLEAN$'

printf '%s\n' 'Agel graphical workshop: native agents -> source commit -> reboot -> replay [ok]'
