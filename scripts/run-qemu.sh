#!/bin/sh
set -eu

kernel=$(./scripts/build-boot.sh --features isolated-repl | tail -n 1)
set +e
qemu-system-x86_64 \
  -machine pc,accel=tcg -m 64M -display none -monitor none -serial stdio -no-reboot \
  -device isa-debug-exit,iobase=0xf4,iosize=0x04 \
  -boot order=c,strict=on \
  -drive format=raw,file="$kernel",if=ide,index=0,media=disk
status=$?
set -e

# isa-debug-exit maps the guest's clean value 0x10 to host status 33.
test "$status" -eq 33 && exit 0
exit "$status"
