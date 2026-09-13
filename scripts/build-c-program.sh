#!/bin/sh
# Build one of the C programs in boot/posix/c against agel-libc for a
# research machine and print the path of the static ELF the supervisor loads.
#   build-c-program.sh NAME [x86_64|aarch64|riscv64]
# clang compiles the C and links with lld through its own driver: any clang
# with the three backends and an lld beside it (Homebrew's llvm on macOS,
# since Apple's lacks RISC-V and lld; the clang and lld packages on Debian).
# AGEL_CLANG names the compiler explicitly.
set -eu
name=${1:?program name}
architecture=${2:-x86_64}
case "$architecture" in
  x86_64)
    target=x86_64-unknown-none
    triple=x86_64-unknown-none-elf
    # The process window is at 512 GiB: position-independent code, and
    # data reached directly rather than through a GOT. With a GOT, clang
    # references data through `add sym@GOTPCREL(%rip), %reg`, and because
    # the link is static and not `-pie`, lld 18 relaxes that to an
    # absolute 32-bit immediate, which cannot hold 0x80_1000_0000 and is
    # silently truncated (DOOM faulted on the CI runner that way). Function
    # addresses still come from the GOT, so the link keeps every GOT load
    # as a load (`--no-relax`). The kernel enables SSE for processes
    # (v0.2.70), so floating point is the compiler's default.
    arch_flags="-fPIE -fdirect-access-external-data"
    link_flags="-Wl,--no-relax"
    ;;
  aarch64)
    target=aarch64-unknown-none-softfloat
    triple=aarch64-unknown-none-elf
    arch_flags=""
    link_flags=""
    ;;
  riscv64)
    target=riscv64imac-unknown-none-elf
    triple=riscv64-unknown-none-elf
    arch_flags="-march=rv64imac -mabi=lp64 -mcmodel=medany"
    link_flags=""
    ;;
  *) printf 'unknown architecture: %s\n' "$architecture" >&2; exit 2 ;;
esac
posix_dir=$(CDPATH= cd -- "$(dirname "$0")/../boot/posix" && pwd)
if test -n "${AGEL_CLANG:-}"; then
  clang=$AGEL_CLANG
elif test -x /opt/homebrew/opt/llvm/bin/clang; then
  clang=/opt/homebrew/opt/llvm/bin/clang
else
  clang=clang
fi
rustup target add "$target" >/dev/null
(cd "$posix_dir" && cargo build -q --release --target "$target" -p agel-libc)
archive="$posix_dir/target/$target/release/libagel_libc.a"
out="$posix_dir/target/c/$architecture"
mkdir -p "$out"
# shellcheck disable=SC2086
cflags="--target=$triple $arch_flags -ffreestanding -nostdlib -fno-builtin -fno-stack-protector \
  -fno-asynchronous-unwind-tables -O2 -Wall -Wextra -Werror -I$posix_dir/libc/include"
# The library's own C and assembly, one object each.
library_objects=""
for source in "$posix_dir"/libc/c/*.c "$posix_dir"/libc/c/*.S; do
  object="$out/libc_$(basename "$source" | tr '.' '_').o"
  # shellcheck disable=SC2086
  "$clang" $cflags -c "$source" -o "$object"
  library_objects="$library_objects $object"
done
# A program may carry its own defines and flags in c/NAME.cflags, one line,
# applied to it and to every source it lists.
program_flags=""
if test -f "$posix_dir/c/$name.cflags"; then
  program_flags=$(cat "$posix_dir/c/$name.cflags")
fi
# shellcheck disable=SC2086
"$clang" $cflags $program_flags -c "$posix_dir/c/$name.c" -o "$out/$name.o"
# A program may list further sources, one per line relative to c/, in
# c/NAME.deps; third-party files are compiled as they are, without -Werror.
objects="$out/$name.o"
if test -f "$posix_dir/c/$name.deps"; then
  while read -r source; do
    test -n "$source" || continue
    object="$out/$(printf '%s' "$source" | tr '/' '_').o"
    # shellcheck disable=SC2086
    "$clang" $cflags $program_flags -Wno-error -Wno-unused-parameter -c "$posix_dir/c/$source" -o "$object"
    objects="$objects $object"
  done < "$posix_dir/c/$name.deps"
fi
# shellcheck disable=SC2086
"$clang" --target=$triple -fuse-ld=lld -nostdlib -static $link_flags -Wl,--gc-sections -Wl,-z,max-page-size=4096 \
  -Wl,-T,"$posix_dir/linker/$architecture.ld" -o "$out/$name" \
  $objects $library_objects "$archive"
printf '%s\n' "$out/$name"
