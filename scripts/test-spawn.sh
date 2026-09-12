#!/bin/sh
# Processes that make processes, on any of the three machines: a C parent
# makes a pipe, spawns a child with the pipe's read end as its standard
# input, feeds it, waits for it, then spawns a program that faults and a
# name that does not exist and sees what a parent sees of each.
set -eu

architecture=${1:-x86_64}
hostile=$(./scripts/build-program.sh hostile "$architecture" | tail -n 1)
pipeline=$(./scripts/build-c-program.sh pipeline "$architecture" | tail -n 1)
shout=$(./scripts/build-c-program.sh shout "$architecture" | tail -n 1)
. ./scripts/lib.sh
prepare_machine "$architecture" spawn 1024
python3 ./scripts/install-program.py "$disk" hostile "$hostile" >/dev/null
python3 ./scripts/install-program.py "$disk" c-pipeline "$pipeline" >/dev/null
python3 ./scripts/install-program.py "$disk" c-shout "$shout" >/dev/null
python3 ./scripts/test-native-repl.py "$kernel" --spawn --arch "$architecture" --disk "$disk"
