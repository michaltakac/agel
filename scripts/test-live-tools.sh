#!/bin/sh
set -eu
cargo test -q -p agel-stdlib --test live_tools --test evolution
cargo build -q --release -p agel-stdlib --example osrs
python3 scripts/test-osrs-combat.py --live-tools
