#!/bin/sh
# A model decides for the in-OS loop through the host bridge, a whole
# episode: `agel-play` boots the desktop, loads the `doom-agent-model`
# Agel program, and answers every `model-request` it makes from a policy —
# here the echo policy, a stand-in provider that needs no credentials, on
# the same path the Claude and Codex providers take — recording each step.
set -eu
wad=$(./scripts/fetch-doom-wad.sh)
image=$(./scripts/build-boot.sh --features native-graphics | tail -n 1)
doom=$(./scripts/build-c-program.sh doom x86_64 | tail -n 1)
out=target/doom-runs/echo
rm -rf "$out"
mkdir -p "$out"
cargo run -q --release -p agel-play -- --image "$image" --doom "$doom" --wad "$wad" \
  --out "$out" --policy echo --steps 4 | tee "$out/console.log"
replies=$(grep -c "agel-play: model reply" "$out/console.log")
steps=$(wc -l < "$out/steps.jsonl" | tr -d ' ')
grep -q "agel-play: step 4:" "$out/console.log"
grep -q "agel-play: done" "$out/console.log"
if [ "$replies" -lt 4 ] || [ "$steps" -ne 4 ]; then
  printf 'expected 4 model replies and 4 recorded steps, got %s and %s\n' "$replies" "$steps" >&2
  exit 1
fi
echo "A model-driven episode through the bridge: 4 steps, each a model request answered by the policy and recorded [ok]"
