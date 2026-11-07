#!/bin/sh
set -eu
cargo test -q -p agel-stdlib --test evolution
agel=$(./scripts/build-program.sh agel x86_64 | tail -n 1)
image=$(./scripts/build-boot.sh --features native-graphics | tail -n 1)
disk=$(mktemp "${TMPDIR:-/tmp}/agel-evolution.XXXXXX")
trap 'rm -f "$disk"' EXIT HUP INT TERM
cp "$image" "$disk"
python3 scripts/install-program.py "$disk" agel "$agel" >/dev/null
python3 scripts/test-evolution.py "$disk"
