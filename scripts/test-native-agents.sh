#!/bin/sh
# Run the exact allocator-free evaluator on the host for adversarial actor tests.
set -eu
test_dir=$(mktemp -d "${TMPDIR:-/tmp}/agel-native-agents.XXXXXX")
trap 'rm -f "$test_dir/tests"; rmdir "$test_dir"' EXIT HUP INT TERM
rustc --edition 2021 --test boot/kernel/src/native.rs -o "$test_dir/tests"
"$test_dir/tests"
