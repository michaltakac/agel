#!/bin/sh
set -eu
cargo build -q -p agel-stdlib --example osrs
python3 scripts/test-osrs-wire.py "$@"
