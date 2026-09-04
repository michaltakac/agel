#!/bin/sh
set -eu

work_dir=$(mktemp -d "${TMPDIR:-/tmp}/agel-bootstrap.XXXXXX")
trap 'rm -rf "$work_dir"' EXIT HUP INT TERM

cargo run -q -p agel-core --example conformance > "$work_dir/rust.out"
sbcl --noinform --disable-debugger --script \
  bootstrap/common-lisp/agel-reference.lisp > "$work_dir/common-lisp.out"

diff -u "$work_dir/rust.out" "$work_dir/common-lisp.out"
printf '%s\n' "Agel bootstrap conformance: Rust seed = Common Lisp reference"
