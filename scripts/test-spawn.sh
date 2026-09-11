#!/bin/sh
# Processes that make processes, on any of the three machines: a C parent
# makes a pipe, spawns a child with the pipe's read end as its standard
# input, feeds it, waits for it, then spawns a program that faults and a
# name that does not exist and sees what a parent sees of each.
set -eu

architecture=${1:-x86_64}
hostile=$(./scripts/build-program.sh hostile "$architecture" | tail -n 1)
pipeline=$(./scripts/build-c-program.sh pipeline "$architecture" | tail -n 1)
shout=$(./scripts/build-c-program.sh shout "$architecture" | tail -n 1)
case "$architecture" in
  x86_64)
    image=$(./scripts/build-boot.sh --features isolated-repl | tail -n 1)
    disk=$(mktemp "${TMPDIR:-/tmp}/agel-spawn.XXXXXX")
    trap 'rm -f "$disk"' EXIT HUP INT TERM
    cp "$image" "$disk"
    dd if=/dev/zero of="$disk" bs=512 seek=1024 count=1024 conv=notrunc 2>/dev/null
    kernel=$disk
    ;;
  aarch64 | riscv64)
    kernel=$(./scripts/build-kernel.sh "$architecture" --features isolated-repl | tail -n 1)
    disk=$(mktemp "${TMPDIR:-/tmp}/agel-spawn.XXXXXX")
    trap 'rm -f "$disk"' EXIT HUP INT TERM
    dd if=/dev/zero of="$disk" bs=512 count=3072 2>/dev/null
    ;;
  *) printf 'unknown architecture: %s\n' "$architecture" >&2; exit 2 ;;
esac
python3 ./scripts/install-program.py "$disk" hostile "$hostile" >/dev/null
python3 ./scripts/install-program.py "$disk" c-pipeline "$pipeline" >/dev/null
python3 ./scripts/install-program.py "$disk" c-shout "$shout" >/dev/null
python3 ./scripts/test-native-repl.py "$kernel" --spawn --arch "$architecture" --disk "$disk"
