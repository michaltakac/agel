#!/bin/sh
set -eu
cargo test -q -p agel-stdlib --test repair --test live_tools --test evolution
