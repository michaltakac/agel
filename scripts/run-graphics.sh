#!/bin/sh
# Open the native Agel vector desktop in QEMU. The compositor halts after
# drawing so the framebuffer remains visible; stop QEMU from its window or
# with Ctrl-C in this terminal.
set -eu

image=$(./scripts/build-boot.sh --features native-graphics | tail -n 1)
qemu-system-x86_64 \
  -machine pc,accel=tcg -m 64M -monitor none -serial stdio -no-reboot \
  -vga std -boot order=c,strict=on \
  -drive format=raw,file="$image",if=ide,index=0,media=disk,snapshot=on
