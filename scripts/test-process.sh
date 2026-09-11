#!/bin/sh
# Load programs from the disk into protection domains on any of the three
# machines: a hello program that writes and exits, a hostile one that is
# contained, and a name that is not there.
set -eu

architecture=${1:-x86_64}
hello=$(./scripts/build-program.sh hello "$architecture" | tail -n 1)
hostile=$(./scripts/build-program.sh hostile "$architecture" | tail -n 1)
case "$architecture" in
  x86_64)
    image=$(./scripts/build-boot.sh --features isolated-repl | tail -n 1)
    disk=$(mktemp "${TMPDIR:-/tmp}/agel-process.XXXXXX")
    trap 'rm -f "$disk"' EXIT HUP INT TERM
    cp "$image" "$disk"
    dd if=/dev/zero of="$disk" bs=512 seek=256 count=33 conv=notrunc 2>/dev/null
    kernel=$disk
    ;;
  aarch64 | riscv64)
    kernel=$(./scripts/build-kernel.sh "$architecture" --features isolated-repl | tail -n 1)
    disk=$(mktemp "${TMPDIR:-/tmp}/agel-process.XXXXXX")
    trap 'rm -f "$disk"' EXIT HUP INT TERM
    dd if=/dev/zero of="$disk" bs=512 count=2048 2>/dev/null
    ;;
  *) printf 'unknown architecture: %s\n' "$architecture" >&2; exit 2 ;;
esac
python3 ./scripts/install-program.py "$disk" hello "$hello" >/dev/null
python3 ./scripts/install-program.py "$disk" hostile "$hostile" >/dev/null
python3 ./scripts/test-native-repl.py "$kernel" --exec --arch "$architecture" --disk "$disk"
