#!/bin/sh
# DOOM on the desktop: the engine built against agel-libc, the shareware
# data in the disk's data region, a timed demo played to its end with the
# frame rate the engine reports, and the window's pixels read back.
set -eu
wad=$(./scripts/fetch-doom-wad.sh)
image=$(./scripts/build-boot.sh --features native-graphics | tail -n 1)
doom=$(./scripts/build-c-program.sh doom x86_64 | tail -n 1)
wadcheck=$(./scripts/build-c-program.sh wadcheck x86_64 | tail -n 1)
disk=$(mktemp "${TMPDIR:-/tmp}/agel-doom.XXXXXX")
trap 'rm -f "$disk"' EXIT HUP INT TERM
cp "$image" "$disk"
dd if=/dev/zero of="$disk" bs=512 seek=1024 count=1024 conv=notrunc 2>/dev/null
python3 ./scripts/install-program.py "$disk" c-doom "$doom" >/dev/null
python3 ./scripts/install-program.py "$disk" c-wadcheck "$wadcheck" >/dev/null
python3 ./scripts/install-program.py --region data "$disk" doom1.wad "$wad" >/dev/null
python3 ./scripts/test-doom.py "$disk"
