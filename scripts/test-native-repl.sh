#!/bin/sh
set -eu

kernel=$(./scripts/build-boot.sh | tail -n 1)
exec python3 ./scripts/test-native-repl.py "$kernel"
