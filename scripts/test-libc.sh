#!/bin/sh
# C programs built from source against agel-libc, on any of the three
# machines: printf, the heap and the string routines; open, read, write and
# close through a namespace; errno and main's status.
set -eu

architecture=${1:-x86_64}
writer=$(./scripts/build-program.sh writer "$architecture" | tail -n 1)
hello=$(./scripts/build-c-program.sh hello "$architecture" | tail -n 1)
cat_program=$(./scripts/build-c-program.sh cat "$architecture" | tail -n 1)
case "$architecture" in
  x86_64)
    image=$(./scripts/build-boot.sh --features isolated-repl | tail -n 1)
    disk=$(mktemp "${TMPDIR:-/tmp}/agel-libc.XXXXXX")
    trap 'rm -f "$disk"' EXIT HUP INT TERM
    cp "$image" "$disk"
    dd if=/dev/zero of="$disk" bs=512 seek=1024 count=1024 conv=notrunc 2>/dev/null
    kernel=$disk
    ;;
  aarch64 | riscv64)
    kernel=$(./scripts/build-kernel.sh "$architecture" --features isolated-repl | tail -n 1)
    disk=$(mktemp "${TMPDIR:-/tmp}/agel-libc.XXXXXX")
    trap 'rm -f "$disk"' EXIT HUP INT TERM
    dd if=/dev/zero of="$disk" bs=512 count=3072 2>/dev/null
    ;;
  *) printf 'unknown architecture: %s\n' "$architecture" >&2; exit 2 ;;
esac
python3 ./scripts/install-program.py "$disk" writer "$writer" >/dev/null
python3 ./scripts/install-program.py "$disk" c-hello "$hello" >/dev/null
python3 ./scripts/install-program.py "$disk" c-cat "$cat_program" >/dev/null
python3 ./scripts/test-native-repl.py "$kernel" --c --arch "$architecture" --disk "$disk"
