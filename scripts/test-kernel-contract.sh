#!/bin/sh
set -eu

work_dir=$(mktemp -d "${TMPDIR:-/tmp}/agel-kernel-contract.XXXXXX")
trap 'rm -rf "$work_dir"' EXIT HUP INT TERM

cargo run -q -p agel-kernel-abi --example contract_conformance > "$work_dir/model.trace"
diff -u bootstrap/kernel-contract.trace "$work_dir/model.trace"

printf '%s\n' "Agel kernel contract: reference model = frozen v1.0 transcript"
