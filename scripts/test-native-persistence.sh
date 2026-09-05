#!/bin/sh
set -eu

image=$(./scripts/build-boot.sh --features isolated-repl | tail -n 1)
test_image=$(mktemp "${TMPDIR:-/tmp}/agel-persistent.XXXXXX")
trap 'rm -f "$test_image"' EXIT HUP INT TERM
cp "$image" "$test_image"
# Tests never mutate the developer's workshop. Start the temporary copy with
# both v0.1.7 slots blank even if the real image already contains a workspace.
dd if=/dev/zero of="$test_image" bs=512 seek=256 count=32 conv=notrunc 2>/dev/null
python3 ./scripts/test-native-repl.py "$test_image" --persistence
