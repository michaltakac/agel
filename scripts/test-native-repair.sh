#!/bin/sh
set -eu
cargo test -q -p agel-stdlib --test repair --test evolution --test program_store --test live_runtime
cargo build -q --release -p agel-model --example guest_inference
cargo build -q -p agel-stdlib --example journal
./scripts/build-program.sh agel x86_64 >/dev/null
./scripts/build-boot.sh --features native-graphics >/dev/null
python3 scripts/test-native-repair.py "$@"
