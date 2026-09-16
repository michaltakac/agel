#!/bin/sh
# The language in a protection domain: build the `agel` program (the hosted
# runtime and standard library without `std`), install it in the graphics
# image's program region, and run Agel source files with it from the
# desktop's serial console.
set -eu
agel=$(./scripts/build-program.sh agel x86_64 | tail -n 1)
hello=$(./scripts/build-program.sh hello x86_64 | tail -n 1)
image=$(./scripts/build-boot.sh --features native-graphics | tail -n 1)
disk=$(mktemp "${TMPDIR:-/tmp}/agel-language.XXXXXX")
trap 'rm -f "$disk"' EXIT HUP INT TERM
cp "$image" "$disk"
python3 ./scripts/install-program.py "$disk" agel "$agel" >/dev/null
python3 ./scripts/install-program.py "$disk" hello "$hello" >/dev/null
python3 ./scripts/test-agel-process.py "$disk"
