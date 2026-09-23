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
sysroot=$(rustc --print sysroot)
rust_lld="$sysroot/lib/rustlib/$(rustc -vV | sed -n 's/^host: //p')/bin/rust-lld"
# Invoked directly rather than through rustc, rust-lld must be told where the
# toolchain keeps libLLVM; rustc's own driver arranges this for its links.
export DYLD_LIBRARY_PATH="$sysroot/lib${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"
export LD_LIBRARY_PATH="$sysroot/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
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
# A kernel slot holds 4096 sectors (2 MiB), read by 33 conservative
# 127-sector BIOS transfers (disk layout v3, v0.2.100).
test "$(wc -c < "$kernel_bin" | tr -d ' ')" -le 2097152

# The boot seed is the BIOS stage (sector 0) and kernel slot A (sectors
# 65536-69759, past the data region; the transfers read on to 69759).
# Everything between belongs to the native dual-slot source workspace, the
# recovery record, the kernel slot selector, the filesystem region, the
# program, asset and data regions, and must survive rebuilding the kernel
# between workshop sessions. A rebuild installs the new kernel as slot A
# and clears the selector (sector 1057): it is a new baseline, and any
# candidate staged with scripts/stage-kernel.py is dropped rather than
# silently kept in front of the kernel just built. Slot B (sectors
# 69760-73983) is left as it was.
disk_bytes=37879808
if test ! -f "$disk_image"; then
  dd if=/dev/zero of="$disk_image" bs=512 count=73984 2>/dev/null
elif test "$(wc -c < "$disk_image" | tr -d ' ')" -lt "$disk_bytes"; then
  dd if=/dev/zero of="$disk_image" bs=1 count=1 seek=$((disk_bytes - 1)) conv=notrunc 2>/dev/null
fi
dd if=/dev/zero of="$disk_image" bs=512 seek=65536 count=4224 conv=notrunc 2>/dev/null
dd if=/dev/zero of="$disk_image" bs=512 seek=1057 count=1 conv=notrunc 2>/dev/null
dd if="$boot_bin" of="$disk_image" conv=notrunc 2>/dev/null
dd if="$kernel_bin" of="$disk_image" bs=512 seek=65536 conv=notrunc 2>/dev/null

# The asset region (sectors 10240-13311) holds the compositor's font atlases
# and sprite sheet, committed under boot/desktop/assets so that every build
# installs the same bytes (scripts/build-assets.sh regenerates them from the
# fonts and drawings). They are part of the seed: the graphics image refuses
# to boot without them, and a rebuild installs them fresh.
assets_dir="$project_dir/boot/desktop/assets"
for asset in fira-sans.agf fira-sans-medium.agf fira-mono.agf sprites.agi; do
  test -f "$assets_dir/$asset" || { printf '%s\n' "missing $assets_dir/$asset; run scripts/build-assets.sh" >&2; exit 1; }
  python3 "$project_dir/scripts/install-asset.py" "$disk_image" "${asset%.*}" "$assets_dir/$asset" >/dev/null
done

# The desktop's programs (sectors 13312+, the data region): the kernel
# carries their names and reads their source from here when one is loaded,
# so the sources are not in the kernel image. Installed fresh every build
# of a desktop (`--features native-graphics`) and removed by a build of
# anything else, so the persistent image's data region holds what its
# kernel reads and nothing a test of the workshop would list unasked.
for program in workbench:wb doom-agent:da doom-agent-model:dm doom-agent-judge:dj desktop-agent:dk review:rv lookup:lk builder:bd; do
  case " $* " in
    *native-graphics*)
      source="$project_dir/boot/desktop/${program%%:*}.agel"
      test -f "$source" || { printf '%s\n' "missing $source" >&2; exit 1; }
      python3 "$project_dir/scripts/install-program.py" --region data "$disk_image" "${program##*:}.agel" "$source" >/dev/null
      ;;
    *)
      python3 "$project_dir/scripts/install-program.py" --region data "$disk_image" --remove "${program##*:}.agel" >/dev/null
      ;;
  esac
done

printf '%s\n' "$disk_image"
