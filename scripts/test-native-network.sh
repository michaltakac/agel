#!/bin/sh
set -eu
mkdir -p target
rustc --edition 2021 --test boot/posix/agel/src/heap_runs.rs -o target/heap-runs-test
./target/heap-runs-test
cargo test -q -p agel-stdlib --test byte_stream --example byte_bridge
cargo build -q --release -p agel-stdlib --example byte_bridge
./scripts/build-program.sh agel x86_64 >/dev/null
./scripts/build-boot.sh --features native-graphics >/dev/null
python3 scripts/test-native-network.py "$@"
