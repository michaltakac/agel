#!/bin/sh
# Boot the interactive native workshop on the serial console.
# Usage: ./scripts/run-qemu.sh [x86_64|aarch64|riscv64]
set -eu

architecture=${1:-x86_64}
case "$architecture" in
  x86_64)
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
    ;;
  aarch64)
    kernel=$(./scripts/build-kernel.sh aarch64 --features isolated-repl | tail -n 1)
    exec qemu-system-aarch64 \
      -machine virt -cpu cortex-a72 -m 128M -display none -monitor none -serial stdio -no-reboot \
      -kernel "$kernel"
    ;;
  riscv64)
    kernel=$(./scripts/build-kernel.sh riscv64 --features isolated-repl | tail -n 1)
    exec qemu-system-riscv64 \
      -machine virt -m 128M -display none -monitor none -serial stdio -no-reboot -bios default \
      -kernel "$kernel"
    ;;
  *) printf 'unknown architecture: %s\n' "$architecture" >&2; exit 2 ;;
esac
