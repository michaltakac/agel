#!/bin/sh
# Re-evaluate the Agel kitchen sink, require byte-identical vector output, and
# verify that the checked-in browser screenshot has the promised dimensions.
set -eu

svg=$(mktemp "${TMPDIR:-/tmp}/agel-kitchensink.XXXXXX")
trap 'rm -f "$svg"' EXIT

cargo run -q -p agel-vector -- \
  --program examples/kitchensink.agel \
  --output "$svg" >/dev/null

digest() {
  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | cut -d' ' -f1
  else
    sha256sum "$1" | cut -d' ' -f1
  fi
}

test "$(digest "$svg")" = c7265e746eef554088d435ee626b533b23e123f07eef6c285c69e4252a2b333f
test "$(digest output/playwright/agel-kitchensink.png)" = 9788826406dd2c191fa63b1436ef65d7d6e74414724a8e210d68814ee549185c
dimensions=$(od -An -tx1 -j 16 -N 8 output/playwright/agel-kitchensink.png | tr -d ' \n')
test "$dimensions" = 00000b4000000708

printf '%s\n' 'Agel kitchen sink: language frame -> deterministic SVG -> 2880x1800 screenshot [ok]'
