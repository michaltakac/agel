#!/bin/sh
# Load programs from the disk into protection domains on any of the three
# machines: a hello program that writes and exits, a hostile one that is
# contained, and a name that is not there.
set -eu

architecture=${1:-x86_64}
hello=$(./scripts/build-program.sh hello "$architecture" | tail -n 1)
hostile=$(./scripts/build-program.sh hostile "$architecture" | tail -n 1)
. ./scripts/lib.sh
prepare_machine "$architecture" process 34
python3 ./scripts/install-program.py "$disk" hello "$hello" >/dev/null
python3 ./scripts/install-program.py "$disk" hostile "$hostile" >/dev/null
python3 ./scripts/test-native-repl.py "$kernel" --exec --arch "$architecture" --disk "$disk"
