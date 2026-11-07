#!/bin/sh
set -eu
cargo test -q -p agel-stdlib --test osrs_interfaces
cargo build -q --release -p agel-stdlib --example osrs
python3 scripts/test-osrs-mining.py
