#!/bin/sh
# Power-cut injection at every sector write of a workspace save, on any of the
# three machines. The workshop's `:cut-power N` tears the N-th write and halts;
# the harness sweeps N from 1 until a save completes, rebooting after each cut
# and requiring the workspace to be a whole generation, old or new.
set -eu

architecture=${1:-x86_64}
case "$architecture" in
  x86_64)
    image=$(./scripts/build-boot.sh --features isolated-repl | tail -n 1)
    test_image=$(mktemp "${TMPDIR:-/tmp}/agel-power-cut.XXXXXX")
    trap 'rm -f "$test_image"' EXIT HUP INT TERM
    cp "$image" "$test_image"
    dd if=/dev/zero of="$test_image" bs=512 seek=256 count=33 conv=notrunc 2>/dev/null
    python3 ./scripts/test-native-repl.py "$test_image" --power-cut
    ;;
  aarch64 | riscv64)
    kernel=$(./scripts/build-kernel.sh "$architecture" --features isolated-repl | tail -n 1)
    disk=$(mktemp "${TMPDIR:-/tmp}/agel-virtio.XXXXXX")
    trap 'rm -f "$disk"' EXIT HUP INT TERM
    dd if=/dev/zero of="$disk" bs=512 count=2048 2>/dev/null
    python3 ./scripts/test-native-repl.py "$kernel" --power-cut --arch "$architecture" --disk "$disk"
    ;;
  *) printf 'unknown architecture: %s\n' "$architecture" >&2; exit 2 ;;
esac
