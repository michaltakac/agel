#!/bin/sh
# Programs from files: an Agel form writes a program into the filesystem
# region, the desktop loads it by path or name, a failing form stops a load,
# and /init.agel runs at boot.
set -eu
image=$(./scripts/build-boot.sh --features native-graphics | tail -n 1)
python3 ./scripts/test-programs.py "$image"
