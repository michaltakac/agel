#!/bin/sh
set -eu
cargo build -q --release -p agel-stdlib --example osrs
python3 scripts/test-osrs-combat.py "$@"
