#!/bin/sh
set -eu
cargo test -q -p agel-stdlib --test checkpoint --test evolution --test live_runtime
./scripts/build-program.sh agel x86_64 >/dev/null
./scripts/build-boot.sh --features native-graphics >/dev/null
python3 scripts/test-checkpoint.py
