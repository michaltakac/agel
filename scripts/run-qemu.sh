#!/bin/sh
set -eu

kernel=$(./scripts/build-boot.sh | tail -n 1)
exec qemu-system-x86_64 \
  -machine pc -m 64M -display none -monitor none -serial stdio -no-reboot \
  -drive format=raw,file="$kernel"
