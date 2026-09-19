#!/bin/sh
# An Agel program drives the desktop from inside the OS: the desktop loads
# the desktop-agent program into its native evaluator and `:drive` steps
# it, the program's typed questions answered by this harness as the host
# bridge would. No game, no window, no model on the host.
set -eu
image=$(./scripts/build-boot.sh --features native-graphics | tail -n 1)
disk=$(mktemp "${TMPDIR:-/tmp}/agel-drive.XXXXXX")
trap 'rm -f "$disk"' EXIT HUP INT TERM
cp "$image" "$disk"
dd if=/dev/zero of="$disk" bs=512 seek=1024 count=1024 conv=notrunc 2>/dev/null
python3 ./scripts/test-drive.py "$disk"
