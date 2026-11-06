#!/bin/sh
set -eu
cargo test -q -p agel-stdlib --test evolution --test program_store
agel=$(./scripts/build-program.sh agel x86_64 | tail -n 1)
probe=$(./scripts/build-program.sh program-io-check x86_64 | tail -n 1)
image=$(./scripts/build-boot.sh --features native-graphics | tail -n 1)
python3 scripts/test-native-evolution.py "$image" "$agel" "$probe"
