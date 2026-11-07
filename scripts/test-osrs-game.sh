#!/bin/sh
set -eu
cargo build -q --release -p agel-osrs-example --example osrs
python3 scripts/test-osrs-game.py
