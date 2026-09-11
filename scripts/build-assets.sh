#!/bin/sh
# Regenerate the desktop's committed assets from their sources: the font
# atlases from the fonts under boot/desktop/fonts and the sprite sheet from
# scripts/build-sprites.py. The results are committed, so every build and
# every CI run installs the same bytes and the graphics self-test's frozen
# digest means the same thing on every machine; run this after changing a
# font, a size list or a sprite, then refreeze the digest.
set -eu
project_dir=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
assets="$project_dir/boot/desktop/assets"
fonts="$project_dir/boot/desktop/fonts"
mkdir -p "$assets"
python3 "$project_dir/scripts/build-font-atlas.py" "$fonts/FiraSans-Regular.ttf" "$assets/fira-sans.agf" --sizes 12,14,16,20,24,32
python3 "$project_dir/scripts/build-font-atlas.py" "$fonts/FiraSans-Medium.ttf" "$assets/fira-sans-medium.agf" --sizes 12,14,16,20,24,32
python3 "$project_dir/scripts/build-font-atlas.py" "$fonts/FiraMono-Regular.ttf" "$assets/fira-mono.agf" --sizes 12,14,16,20
python3 "$project_dir/scripts/build-sprites.py" "$assets/sprites.agi"
