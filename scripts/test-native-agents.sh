#!/bin/sh
# Run the exact allocator-free evaluator on the host for adversarial actor tests.
set -eu
# The evaluator's transactional banks grew at v0.2.76; a test that previews
# holds several worlds at once, so the test threads get the room the guest
# gives the evaluator domain (4 MiB) rather than the host default.
export RUST_MIN_STACK=67108864
test_dir=$(mktemp -d "${TMPDIR:-/tmp}/agel-native-agents.XXXXXX")
trap 'rm -f "$test_dir/tests"; rmdir "$test_dir"' EXIT HUP INT TERM
rustc --edition 2021 --test boot/kernel/src/native.rs -o "$test_dir/tests"
"$test_dir/tests"
rustc --edition 2021 --test boot/kernel/src/pointer.rs -o "$test_dir/tests"
"$test_dir/tests"
