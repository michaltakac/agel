#!/bin/sh
# Open the live native Agel workshop in QEMU. Click the window to type through
# PS/2, or type in this terminal through serial. Named source cells are written
# to the persistent disk image by :save and replayed on the next launch.
set -eu

image=$(./scripts/build-boot.sh --features native-graphics | tail -n 1)
qemu-system-x86_64 \
  -machine pc,accel=tcg -m 64M -monitor none -serial stdio -no-reboot \
  -vga std -boot order=c,strict=on \
  -drive format=raw,file="$image",if=ide,index=0,media=disk
