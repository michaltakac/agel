#!/bin/sh
set -eu
image=$(./scripts/build-boot.sh --features native-graphics | tail -n 1)
seed=$(mktemp "${TMPDIR:-/tmp}/agel-ledger-seed.XXXXXX")
trap 'rm -f "$seed"' EXIT HUP INT TERM
cp "$image" "$seed"
agel=$(./scripts/build-program.sh agel x86_64 | tail -n 1)
probe=$(./scripts/build-program.sh ledger-check x86_64 | tail -n 1)
cargo build -q --release -p agel-play
cargo build -q -p agel-stdlib --example journal
python3 scripts/test-ledger.py "$seed" "$agel" "$probe"
