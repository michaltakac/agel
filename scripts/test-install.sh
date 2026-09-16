#!/bin/sh
# Programs installed from files: the hello program's ELF as hex text in the
# data region, installed into the program region by the desktop's
# `:install` and run by `:exec`.
set -eu
hello=$(./scripts/build-program.sh hello x86_64 | tail -n 1)
image=$(./scripts/build-boot.sh --features native-graphics | tail -n 1)
disk=$(mktemp "${TMPDIR:-/tmp}/agel-install.XXXXXX")
hex=$(mktemp "${TMPDIR:-/tmp}/agel-hello.XXXXXX")
trap 'rm -f "$disk" "$hex"' EXIT HUP INT TERM
cp "$image" "$disk"
python3 -c 'import binascii, sys; data = open(sys.argv[1], "rb").read(); text = binascii.hexlify(data).decode(); open(sys.argv[2], "w").write("\n".join(text[i:i + 64] for i in range(0, len(text), 64)) + "\n")' "$hello" "$hex"
python3 ./scripts/install-program.py --region data "$disk" hello.hex "$hex" >/dev/null
python3 ./scripts/test-install.py "$disk"
