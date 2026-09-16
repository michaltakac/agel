#!/bin/sh
# A descriptor held across a filesystem restart fails closed: build the
# `stale` program, install it in the graphics image, and restart the
# filesystem service from the desktop's serial console while it sleeps
# holding a descriptor.
set -eu
stale=$(./scripts/build-program.sh stale x86_64 | tail -n 1)
image=$(./scripts/build-boot.sh --features native-graphics | tail -n 1)
disk=$(mktemp "${TMPDIR:-/tmp}/agel-stale.XXXXXX")
trap 'rm -f "$disk"' EXIT HUP INT TERM
cp "$image" "$disk"
python3 ./scripts/install-program.py "$disk" stale "$stale" >/dev/null
python3 ./scripts/test-stale.py "$disk"
