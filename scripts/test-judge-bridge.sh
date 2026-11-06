#!/bin/sh
set -eu
image=$(./scripts/build-boot.sh --features native-graphics | tail -n 1)
agel=$(./scripts/build-program.sh agel x86_64 | tail -n 1)
cargo build -q --release -p agel-play
python3 scripts/test-judge-bridge.py "$image" "$agel"
