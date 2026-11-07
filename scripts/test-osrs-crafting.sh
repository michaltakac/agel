#!/bin/sh
set -eu
cargo test -q -p agel-osrs-example --test osrs_interfaces
cargo build -q --release -p agel-osrs-example --example osrs
python3 scripts/test-osrs-crafting.py "$@"
