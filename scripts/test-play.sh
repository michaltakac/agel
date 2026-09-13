#!/bin/sh
# Agel plays DOOM, and the loop is Agel in the OS: the desktop loads the
# doom-agent Agel program into its native evaluator and `:play` steps the
# game through it. No host policy, no model.
set -eu
wad=$(./scripts/fetch-doom-wad.sh)
image=$(./scripts/build-boot.sh --features native-graphics | tail -n 1)
doom=$(./scripts/build-c-program.sh doom x86_64 | tail -n 1)
disk=$(mktemp "${TMPDIR:-/tmp}/agel-play.XXXXXX")
trap 'rm -f "$disk"' EXIT HUP INT TERM
cp "$image" "$disk"
dd if=/dev/zero of="$disk" bs=512 seek=1024 count=1024 conv=notrunc 2>/dev/null
python3 ./scripts/install-program.py "$disk" c-doom "$doom" >/dev/null
python3 ./scripts/install-program.py --region data "$disk" doom1.wad "$wad" >/dev/null
python3 ./scripts/test-play.py "$disk"
