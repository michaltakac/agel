#!/bin/sh
# Drive the interactive native workshop over its serial console under QEMU.
# x86-64 boots the BIOS disk image; AArch64 and RISC-V boot the ELF the
# isolation backend already produces, now with the workshop feature.
set -eu

architecture=${1:-x86_64}
case "$architecture" in
  x86_64) kernel=$(./scripts/build-boot.sh --features isolated-repl | tail -n 1) ;;
  aarch64 | riscv64)
    kernel=$(./scripts/build-kernel.sh "$architecture" --features isolated-repl | tail -n 1) ;;
  *) printf 'unknown architecture: %s\n' "$architecture" >&2; exit 2 ;;
esac
# Without --disk the harness attaches a blank scratch disk, snapshot-on, so
# every machine runs the same session against a disk-backed workshop.
exec python3 ./scripts/test-native-repl.py "$kernel" --arch "$architecture"
