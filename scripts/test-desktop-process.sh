#!/bin/sh
# Programs on the desktop: build the graphics image and the programs the
# test runs, install them in the image's program region, and drive the
# graphical workshop over its serial console.
set -eu
writer=$(./scripts/build-program.sh writer x86_64 | tail -n 1)
hello=$(./scripts/build-c-program.sh hello x86_64 | tail -n 1)
cat_program=$(./scripts/build-c-program.sh cat x86_64 | tail -n 1)
image=$(./scripts/build-boot.sh --features native-graphics | tail -n 1)
disk=$(mktemp "${TMPDIR:-/tmp}/agel-desktop.XXXXXX")
trap 'rm -f "$disk"' EXIT HUP INT TERM
cp "$image" "$disk"
python3 ./scripts/install-program.py "$disk" writer "$writer" >/dev/null
python3 ./scripts/install-program.py "$disk" c-hello "$hello" >/dev/null
python3 ./scripts/install-program.py "$disk" c-cat "$cat_program" >/dev/null
python3 ./scripts/test-desktop-process.py "$disk"
