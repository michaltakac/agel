#!/bin/sh
set -eu
cargo build -q -p agel-osrs-example --example osrs
python3 scripts/test-osrs-wire.py "$@"
