#!/bin/sh
set -eu

output=$(mktemp "${TMPDIR:-/tmp}/agel-fixed-point.XXXXXX")
trap 'rm -f "$output"' EXIT HUP INT TERM

cargo run -q -p agel-cli < examples/agentic-fixed-point.agel > "$output"

if grep -q 'evaluation error:' "$output"; then
  cat "$output"
  exit 1
fi
grep -q '(720 720 {revision 3 surface (mail calendar shared-search provenance)})' "$output"
grep -q '(fixed/evolved #<agent:2> 1)' "$output"
grep -q '(fixed/preview #<agent:2> 0 1)' "$output"
grep -q '(fixed/done #<agent:2> (optimized-at 0)' "$output"
grep -q 'evolution (preview version-check commit discard message-ordered)' "$output"

printf '%s\n' 'Agel fixed points: Z -> convergence -> transactional turns -> live evolution [ok]'
