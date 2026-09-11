#!/bin/sh
# The C library's breadth on any of the three machines: arguments reach
# main, the heap reuses and joins blocks, the formatter's widths and flags,
# strings and numbers, streams over files with append and seek, and an
# unmodified public-domain SHA-256 whose digest agrees with the host's.
set -eu

architecture=${1:-x86_64}
writer=$(./scripts/build-program.sh writer "$architecture" | tail -n 1)
cat_program=$(./scripts/build-c-program.sh cat "$architecture" | tail -n 1)
digest=$(./scripts/build-c-program.sh digest "$architecture" | tail -n 1)
breadth=$(./scripts/build-c-program.sh breadth "$architecture" | tail -n 1)
case "$architecture" in
  x86_64)
    image=$(./scripts/build-boot.sh --features isolated-repl | tail -n 1)
    disk=$(mktemp "${TMPDIR:-/tmp}/agel-breadth.XXXXXX")
    trap 'rm -f "$disk"' EXIT HUP INT TERM
    cp "$image" "$disk"
    dd if=/dev/zero of="$disk" bs=512 seek=1024 count=1024 conv=notrunc 2>/dev/null
    kernel=$disk
    ;;
  aarch64 | riscv64)
    kernel=$(./scripts/build-kernel.sh "$architecture" --features isolated-repl | tail -n 1)
    disk=$(mktemp "${TMPDIR:-/tmp}/agel-breadth.XXXXXX")
    trap 'rm -f "$disk"' EXIT HUP INT TERM
    dd if=/dev/zero of="$disk" bs=512 count=3072 2>/dev/null
    ;;
  *) printf 'unknown architecture: %s\n' "$architecture" >&2; exit 2 ;;
esac
python3 ./scripts/install-program.py "$disk" writer "$writer" >/dev/null
python3 ./scripts/install-program.py "$disk" c-cat "$cat_program" >/dev/null
python3 ./scripts/install-program.py "$disk" c-digest "$digest" >/dev/null
python3 ./scripts/install-program.py "$disk" c-breadth "$breadth" >/dev/null
python3 ./scripts/test-native-repl.py "$kernel" --breadth --arch "$architecture" --disk "$disk"
