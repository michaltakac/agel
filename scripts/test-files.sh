#!/bin/sh
# Files through namespaces on any of the three machines: format the
# filesystem region, make directories, run a writer at the root and a reader
# in a namespace rooted below it, restart the service, refuse a read-only
# namespace's write, list, and read the files again after a reboot.
set -eu

architecture=${1:-x86_64}
writer=$(./scripts/build-program.sh writer "$architecture" | tail -n 1)
reader=$(./scripts/build-program.sh reader "$architecture" | tail -n 1)
case "$architecture" in
  x86_64)
    image=$(./scripts/build-boot.sh --features isolated-repl | tail -n 1)
    disk=$(mktemp "${TMPDIR:-/tmp}/agel-files.XXXXXX")
    trap 'rm -f "$disk"' EXIT HUP INT TERM
    cp "$image" "$disk"
    # A blank workspace, records and filesystem region.
    dd if=/dev/zero of="$disk" bs=512 seek=1024 count=1024 conv=notrunc 2>/dev/null
    kernel=$disk
    ;;
  aarch64 | riscv64)
    kernel=$(./scripts/build-kernel.sh "$architecture" --features isolated-repl | tail -n 1)
    disk=$(mktemp "${TMPDIR:-/tmp}/agel-files.XXXXXX")
    trap 'rm -f "$disk"' EXIT HUP INT TERM
    dd if=/dev/zero of="$disk" bs=512 count=3072 2>/dev/null
    ;;
  *) printf 'unknown architecture: %s\n' "$architecture" >&2; exit 2 ;;
esac
python3 ./scripts/install-program.py "$disk" writer "$writer" >/dev/null
python3 ./scripts/install-program.py "$disk" reader "$reader" >/dev/null
python3 ./scripts/test-native-repl.py "$kernel" --files --arch "$architecture" --disk "$disk"
