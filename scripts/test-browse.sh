#!/bin/sh
# A browser written in Agel, driven by a judge, as a process on the OS:
# the hosted runtime runs browse-agent.agel from the data region over the
# example site, and this harness answers its typed questions as the host
# bridge would. No network: the pages are installed with the image.
set -eu
agel=$(./scripts/build-program.sh agel x86_64 | tail -n 1)
image=$(./scripts/build-boot.sh --features native-graphics | tail -n 1)
disk=$(mktemp "${TMPDIR:-/tmp}/agel-browse.XXXXXX")
trap 'rm -f "$disk"' EXIT HUP INT TERM
cp "$image" "$disk"
dd if=/dev/zero of="$disk" bs=512 seek=1024 count=1024 conv=notrunc 2>/dev/null
python3 ./scripts/install-program.py "$disk" agel "$agel" >/dev/null
for page in index blue red search; do
  python3 ./scripts/install-program.py --region data "$disk" "$page.html" "examples/pages/$page.html" >/dev/null
done
python3 ./scripts/install-program.py --region data "$disk" browse.agel examples/browse-agent.agel >/dev/null
python3 ./scripts/test-browse.py "$disk"
