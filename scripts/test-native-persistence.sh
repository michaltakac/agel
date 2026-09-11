#!/bin/sh
# Edit -> save -> reboot -> recover, on a temporary disk, on any of the three
# machines. x86-64 boots its BIOS image, which is also the disk; AArch64 and
# RISC-V boot the ELF with a virtio block device attached.
set -eu

architecture=${1:-x86_64}
case "$architecture" in
  x86_64)
    image=$(./scripts/build-boot.sh --features isolated-repl | tail -n 1)
    test_image=$(mktemp "${TMPDIR:-/tmp}/agel-persistent.XXXXXX")
    trap 'rm -f "$test_image"' EXIT HUP INT TERM
    cp "$image" "$test_image"
    # Tests never mutate the developer's workshop. Start the temporary copy with
    # both v0.1.7 slots and the recovery record blank even if the real image
    # already contains a workspace.
    dd if=/dev/zero of="$test_image" bs=512 seek=1024 count=34 conv=notrunc 2>/dev/null
    python3 ./scripts/test-native-repl.py "$test_image" --persistence
    ;;
  aarch64 | riscv64)
    kernel=$(./scripts/build-kernel.sh "$architecture" --features isolated-repl | tail -n 1)
    disk=$(mktemp "${TMPDIR:-/tmp}/agel-virtio.XXXXXX")
    trap 'rm -f "$disk"' EXIT HUP INT TERM
    dd if=/dev/zero of="$disk" bs=512 count=3072 2>/dev/null
    python3 ./scripts/test-native-repl.py "$kernel" --persistence --arch "$architecture" --disk "$disk"
    ;;
  *) printf 'unknown architecture: %s\n' "$architecture" >&2; exit 2 ;;
esac
