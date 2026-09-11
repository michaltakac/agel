#!/bin/sh
# Build the research kernel's isolation backend for one architecture and print
# the path to the resulting image.
#
# x86-64 produces a raw BIOS disk image through ./scripts/build-boot.sh, because
# that architecture still carries the 512-byte boot stage and the 128 KiB seed.
# AArch64 and RISC-V produce ELF files that QEMU loads directly. `raspi4`
# builds the AArch64 kernel for the Raspberry Pi 4's layout and prints a
# flat `kernel8.img`, which the board's firmware (and QEMU's raspi4b) loads
# at 0x80000.
set -eu

if test "$#" -lt 1; then
  printf '%s\n' "usage: build-kernel.sh <x86_64|aarch64|riscv64|raspi4|raspi5> [extra cargo args]" >&2
  exit 2
fi
architecture=$1
shift

kernel_dir=$(CDPATH= cd -- "$(dirname "$0")/../boot/kernel" && pwd)

case "$architecture" in
  x86_64)
    exec "$(dirname "$0")/build-boot.sh" --features isolation-selftest,contract-memory "$@"
    ;;
  aarch64) target=aarch64-unknown-none-softfloat ;;
  riscv64) target=riscv64imac-unknown-none-elf ;;
  raspi4 | raspi5)
    target=aarch64-unknown-none-softfloat
    rustup target add "$target" >/dev/null
    out="$kernel_dir/target/$architecture"
    mkdir -p "$out"
    # A separate target directory: the same triple with another board's
    # layout must not share an incremental build with the `virt` kernel.
    (cd "$kernel_dir" && cargo build --release --target "$target" --target-dir "$out" \
      --features "isolation-selftest,contract-memory,board-$architecture" "$@")
    # Any objcopy that knows AArch64 ELF makes the flat image: LLVM's (the
    # Homebrew llvm on macOS, whose rust-objcopy lacks its library), GNU's,
    # or the toolchain's own.
    if test -x /opt/homebrew/opt/llvm/bin/llvm-objcopy; then
      objcopy=/opt/homebrew/opt/llvm/bin/llvm-objcopy
    elif command -v llvm-objcopy >/dev/null 2>&1; then
      objcopy=llvm-objcopy
    elif command -v gobjcopy >/dev/null 2>&1; then
      objcopy=gobjcopy
    elif command -v objcopy >/dev/null 2>&1; then
      objcopy=objcopy
    else
      objcopy="$(rustc --print sysroot)/lib/rustlib/$(rustc -vV | sed -n 's/^host: //p')/bin/rust-objcopy"
    fi
    # The firmware's name for a 64-bit image: kernel8.img on the Pi 4,
    # kernel_2712.img on the Pi 5.
    case "$architecture" in
      raspi4) image=kernel8.img ;;
      raspi5) image=kernel_2712.img ;;
    esac
    "$objcopy" -O binary "$out/$target/release/agel-boot" "$out/$image"
    printf '%s\n' "$out/$image"
    exit 0
    ;;
  *)
    printf '%s\n' "unknown architecture: $architecture" >&2
    exit 2
    ;;
esac

rustup target add "$target" >/dev/null
# Cargo resolves `.cargo/config.toml` from the working directory rather than
# from the manifest, and that file is where each target's linker script lives.
(cd "$kernel_dir" && cargo build --release --target "$target" --features isolation-selftest,contract-memory "$@")
printf '%s\n' "$kernel_dir/target/$target/release/agel-boot"
