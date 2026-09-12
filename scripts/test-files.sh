#!/bin/sh
# Files through namespaces on any of the three machines: format the
# filesystem region, make directories, run a writer at the root and a reader
# in a namespace rooted below it, restart the service, refuse a read-only
# namespace's write, list, and read the files again after a reboot.
set -eu

architecture=${1:-x86_64}
writer=$(./scripts/build-program.sh writer "$architecture" | tail -n 1)
reader=$(./scripts/build-program.sh reader "$architecture" | tail -n 1)
. ./scripts/lib.sh
prepare_machine "$architecture" files 1024
python3 ./scripts/install-program.py "$disk" writer "$writer" >/dev/null
python3 ./scripts/install-program.py "$disk" reader "$reader" >/dev/null
python3 ./scripts/test-native-repl.py "$kernel" --files --arch "$architecture" --disk "$disk"
