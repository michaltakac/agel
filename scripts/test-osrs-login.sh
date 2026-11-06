#!/bin/sh
set -eu
cargo build -q --release -p agel-stdlib --example osrs
python3 scripts/test-osrs-crypto.py
python3 scripts/test-osrs-login-fixture.py
python3 scripts/test-osrs-npcs.py
if test "${1:-}" = --live; then
  python3 scripts/test-osrs-login-live.py
fi
