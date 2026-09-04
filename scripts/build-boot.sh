#!/bin/sh
set -eu

rustup target add x86_64-unknown-none >/dev/null
kernel_dir=$(CDPATH= cd -- "$(dirname "$0")/../boot/kernel" && pwd)
project_dir=$(CDPATH= cd -- "$kernel_dir/../.." && pwd)
(cd "$kernel_dir" && cargo build --release "$@")
build_dir="$project_dir/target/boot"
mkdir -p "$build_dir"

if command -v gobjcopy >/dev/null 2>&1; then
  objcopy_bin=$(command -v gobjcopy)
elif command -v objcopy >/dev/null 2>&1; then
  objcopy_bin=$(command -v objcopy)
elif test -x /opt/homebrew/opt/binutils/bin/gobjcopy; then
  objcopy_bin=/opt/homebrew/opt/binutils/bin/gobjcopy
else
  printf '%s\n' "objcopy is required (Homebrew: brew install binutils)" >&2
  exit 1
fi
rust_lld="$(rustc --print sysroot)/lib/rustlib/$(rustc -vV | sed -n 's/^host: //p')/bin/rust-lld"
kernel_elf="$kernel_dir/target/x86_64-unknown-none/release/agel-boot"
kernel_bin="$build_dir/kernel.bin"
boot_object="$build_dir/boot.o"
boot_elf="$build_dir/boot.elf"
boot_bin="$build_dir/boot.bin"
disk_image="$build_dir/agel-v1.img"

"$objcopy_bin" -O binary "$kernel_elf" "$kernel_bin"
clang -target i386-none-elf -c "$project_dir/boot/bios/boot.S" -o "$boot_object"
"$rust_lld" -flavor gnu -m elf_i386 -T "$project_dir/boot/bios/linker.ld" \
  "$boot_object" -o "$boot_elf"
"$objcopy_bin" -O binary "$boot_elf" "$boot_bin"

test "$(wc -c < "$boot_bin" | tr -d ' ')" -eq 512
test "$(wc -c < "$kernel_bin" | tr -d ' ')" -le 130048
dd if=/dev/zero of="$disk_image" bs=512 count=256 2>/dev/null
dd if="$boot_bin" of="$disk_image" conv=notrunc 2>/dev/null
dd if="$kernel_bin" of="$disk_image" bs=512 seek=1 conv=notrunc 2>/dev/null

printf '%s\n' "$disk_image"
