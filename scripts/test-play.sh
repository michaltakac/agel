#!/bin/sh
# Agel plays DOOM, without a model: the hosted agent boots the desktop,
# starts the engine, and steps it with the scripted policy, pausing to read
# the screen back and holding keys; the dataset it leaves is checked.
set -eu
wad=$(./scripts/fetch-doom-wad.sh)
image=$(./scripts/build-boot.sh --features native-graphics | tail -n 1)
doom=$(./scripts/build-c-program.sh doom x86_64 | tail -n 1)
cargo build -q --release -p agel-play
out=$(mktemp -d "${TMPDIR:-/tmp}/agel-play.XXXXXX")
trap 'rm -rf "$out"' EXIT HUP INT TERM
./target/release/agel-play --image "$image" --doom "$doom" --wad "$wad" --out "$out" --steps 8 --policy scripted
test "$(wc -l < "$out/steps.jsonl" | tr -d ' ')" -eq 8
grep -q '"action":"fire"' "$out/steps.jsonl"
grep -q '"state":"doom: state map 1 ' "$out/steps.jsonl"
test -f "$out/step-0007.ppm"
mkdir -p target/doom-runs
cp "$out/step-0007.ppm" target/doom-runs/last-scripted-step.ppm
printf '%s\n' 'Agel plays DOOM: eight scripted steps, the screen read back, the engine state recorded, a dataset written [ok]'
