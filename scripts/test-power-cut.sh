#!/bin/sh
# Power-cut injection at every sector write of a workspace save, on any of the
# three machines. The workshop's `:cut-power N` tears the N-th write and halts;
# the harness sweeps N from 1 until a save completes, rebooting after each cut
# and requiring the workspace to be a whole generation, old or new.
set -eu

architecture=${1:-x86_64}
. ./scripts/lib.sh
# The temporary copy starts with both workspace slots and the recovery
# record blank even if the developer's image already holds a workspace.
prepare_machine "$architecture" power-cut 34
python3 ./scripts/test-native-repl.py "$kernel" --power-cut --arch "$architecture" --disk "$disk"
