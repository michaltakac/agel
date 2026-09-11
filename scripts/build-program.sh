#!/bin/sh
# Build one of the loadable programs in boot/posix for a research machine and
# print the path of the static ELF the supervisor loads.
#   build-program.sh NAME [x86_64|aarch64|riscv64]
set -eu
name=${1:?program name}
architecture=${2:-x86_64}
case "$architecture" in
  x86_64) target=x86_64-unknown-none ;;
  aarch64) target=aarch64-unknown-none-softfloat ;;
  riscv64) target=riscv64imac-unknown-none-elf ;;
  *) printf 'unknown architecture: %s\n' "$architecture" >&2; exit 2 ;;
esac
posix_dir=$(CDPATH= cd -- "$(dirname "$0")/../boot/posix" && pwd)
rustup target add "$target" >/dev/null
(cd "$posix_dir" && cargo build -q --release --target "$target" -p "$name")
printf '%s\n' "$posix_dir/target/$target/release/$name"
