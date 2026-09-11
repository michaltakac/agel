#!/bin/sh
# Two implementations, two profiles, two frozen transcripts.
#
# The reference model and the independent implementation each print the
# corpus transcript for the full v1.1 profile and for the v1.0 profile a
# narrower backend publishes; every one of the four must equal the frozen
# file for its profile byte for byte.
set -eu

work_dir=$(mktemp -d "${TMPDIR:-/tmp}/agel-kernel-contract.XXXXXX")
trap 'rm -rf "$work_dir"' EXIT HUP INT TERM

for implementation in contract independent; do
  cargo run -q -p agel-kernel-abi --example "${implementation}_conformance" > "$work_dir/$implementation-v1.1.trace"
  diff -u bootstrap/kernel-contract.trace "$work_dir/$implementation-v1.1.trace"
  cargo run -q -p agel-kernel-abi --example "${implementation}_conformance" -- --profile v1.0 > "$work_dir/$implementation-v1.0.trace"
  diff -u bootstrap/kernel-contract-v1.0.trace "$work_dir/$implementation-v1.0.trace"
done

printf '%s\n' "Agel kernel contract v1.1: reference model = independent implementation = frozen transcripts for both profiles"
