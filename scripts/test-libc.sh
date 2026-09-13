#!/bin/sh
# C programs built from source against agel-libc, on any of the three
# machines: printf, the heap and the string routines; open, read, write and
# close through a namespace; names removed, moved, stat-ed and listed; errno
# and main's status; a window request
# where there is no display.
set -eu

architecture=${1:-x86_64}
writer=$(./scripts/build-program.sh writer "$architecture" | tail -n 1)
hello=$(./scripts/build-c-program.sh hello "$architecture" | tail -n 1)
cat_program=$(./scripts/build-c-program.sh cat "$architecture" | tail -n 1)
chart=$(./scripts/build-c-program.sh chart "$architecture" | tail -n 1)
dir_program=$(./scripts/build-c-program.sh dir "$architecture" | tail -n 1)
clock_program=$(./scripts/build-c-program.sh clock "$architecture" | tail -n 1)
nap=$(./scripts/build-c-program.sh nap "$architecture" | tail -n 1)
heap=$(./scripts/build-c-program.sh heap "$architecture" | tail -n 1)
big=$(./scripts/build-c-program.sh big "$architecture" | tail -n 1)
canvas=$(./scripts/build-c-program.sh canvas "$architecture" | tail -n 1)
digest=$(./scripts/build-c-program.sh digest "$architecture" | tail -n 1)
. ./scripts/lib.sh
prepare_machine "$architecture" libc 1024
python3 ./scripts/install-program.py "$disk" writer "$writer" >/dev/null
python3 ./scripts/install-program.py "$disk" c-hello "$hello" >/dev/null
python3 ./scripts/install-program.py "$disk" c-cat "$cat_program" >/dev/null
python3 ./scripts/install-program.py "$disk" c-chart "$chart" >/dev/null
python3 ./scripts/install-program.py "$disk" c-dir "$dir_program" >/dev/null
python3 ./scripts/install-program.py "$disk" c-clock "$clock_program" >/dev/null
python3 ./scripts/install-program.py "$disk" c-nap "$nap" >/dev/null
python3 ./scripts/install-program.py "$disk" c-heap "$heap" >/dev/null
python3 ./scripts/install-program.py "$disk" c-big "$big" >/dev/null
python3 ./scripts/install-program.py "$disk" c-canvas "$canvas" >/dev/null
python3 ./scripts/install-program.py "$disk" c-digest "$digest" >/dev/null
# A data file larger than any file the filesystem region holds.
pattern=$(mktemp "${TMPDIR:-/tmp}/agel-pattern.XXXXXX")
python3 -c 'import sys; sys.stdout.buffer.write(bytes((i * 7) & 0xff for i in range(100000)))' > "$pattern"
python3 ./scripts/install-program.py --region data "$disk" pattern "$pattern" >/dev/null
rm -f "$pattern"
python3 ./scripts/test-native-repl.py "$kernel" --c --arch "$architecture" --disk "$disk"
