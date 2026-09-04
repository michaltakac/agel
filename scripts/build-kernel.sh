#!/bin/sh
# Build the research kernel's isolation backend for one architecture and print
# the path to the resulting image.
#
# x86-64 produces a raw BIOS disk image through ./scripts/build-boot.sh, because
# that architecture still carries the 512-byte boot stage and the 64 KiB seed.
# AArch64 and RISC-V produce ELF files that QEMU loads directly.
set -eu

if test "$#" -lt 1; then
  printf '%s\n' "usage: build-kernel.sh <x86_64|aarch64|riscv64> [extra cargo args]" >&2
  exit 2
fi
architecture=$1
shift

kernel_dir=$(CDPATH= cd -- "$(dirname "$0")/../boot/kernel" && pwd)

case "$architecture" in
  x86_64)
    exec "$(dirname "$0")/build-boot.sh" --features isolation-selftest "$@"
    ;;
  aarch64) target=aarch64-unknown-none-softfloat ;;
  riscv64) target=riscv64imac-unknown-none-elf ;;
  *)
    printf '%s\n' "unknown architecture: $architecture" >&2
    exit 2
    ;;
esac

rustup target add "$target" >/dev/null
# Cargo resolves `.cargo/config.toml` from the working directory rather than
# from the manifest, and that file is where each target's linker script lives.
(cd "$kernel_dir" && cargo build --release --target "$target" --features isolation-selftest "$@")
printf '%s\n' "$kernel_dir/target/$target/release/agel-boot"
