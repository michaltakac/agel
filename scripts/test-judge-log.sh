#!/bin/sh
set -eu
agel=$(./scripts/build-program.sh agel x86_64 | tail -n 1)
probe=$(./scripts/build-program.sh journal-check x86_64 | tail -n 1)
image=$(./scripts/build-boot.sh --features native-graphics | tail -n 1)
disk=$(mktemp "${TMPDIR:-/tmp}/agel-judge-log.XXXXXX")
trap 'rm -f "$disk"' EXIT HUP INT TERM
cp "$image" "$disk"
python3 scripts/install-program.py "$disk" agel "$agel" >/dev/null
python3 scripts/install-program.py "$disk" journal-check "$probe" >/dev/null
python3 scripts/test-judge-log.py "$disk"
