#!/bin/sh
# The shareware DOOM data, doom1.wad (id Software, 1993; freely
# distributed as the shareware episode), fetched once into target/ and
# checked against its digest. Prints the path. AGEL_DOOM_WAD names a copy
# already on the machine.
set -eu
project_dir=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
wad=${AGEL_DOOM_WAD:-$project_dir/target/doom1.wad}
digest=1d7d43be501e67d927e415e0b8f3e29c3bf33075e859721816f652a526cac771
digest_of() { if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1"; else shasum -a 256 "$1"; fi | cut -d' ' -f1; }
check() { test -f "$1" && test "$(digest_of "$1")" = "$digest"; }
if ! check "$wad"; then
  mkdir -p "$(dirname "$wad")"
  for url in \
    https://github.com/Akbar30Bill/DOOM_wads/raw/master/doom1.wad \
    https://distro.ibiblio.org/slitaz/sources/packages/d/doom1.wad; do
    curl -sSL --max-time 300 -o "$wad.part" "$url" 2>/dev/null || continue
    if check "$wad.part"; then mv "$wad.part" "$wad"; break; fi
  done
  rm -f "$wad.part"
fi
check "$wad" || { printf '%s\n' "doom1.wad could not be fetched or does not match its digest; put a copy at $wad or set AGEL_DOOM_WAD" >&2; exit 1; }
printf '%s\n' "$wad"
