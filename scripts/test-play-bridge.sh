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

# A System One judge decides for the in-OS loop: the `doom-agent-judge`
# program sends typed questions as a `(judge ...)` form, the jev policy
# carries them with the state line and the window to the provider, and the
# answer line comes back for the program's own policy to read. Here the
# provider's curl is a stand-in that answers as TypeSafe's endpoint does,
# so the whole typed path runs without a network or a key.
out=target/doom-runs/jev
rm -rf "$out"
mkdir -p "$out"
cat > "$out/curl" <<'FAKE'
#!/bin/sh
# Reads the configuration on stdin as curl would, then answers one judgment.
cat > /dev/null
printf '%s\n200' '{"model":"stand-in","answers":{"act":{"type":"choice","choice":"fire","confidence":0.33,"probabilities":{"forward":0.32,"back":0.01,"left":0.22,"right":0.01,"fire":0.44,"use":0.0}},"foe":{"type":"noul","noul":0.98},"risk":{"type":"score","score":0.74,"confidence":0.6,"legend":{"0":"safe","1":"wary","2":"lethal"},"probabilities":{"0":0.27,"1":0.73,"2":0.0}}},"usage":{"input_tokens":1,"output_tokens":1}}'
FAKE
chmod +x "$out/curl"
TYPESAFEAI_API_KEY=stand-in cargo run -q --release -p agel-play -- --image "$image" --doom "$doom" --wad "$wad" \
  --out "$out" --policy jev --curl-bin "$(pwd)/$out/curl" --steps 4 | tee "$out/console.log"
replies=$(grep -c "agel-play: model reply .*: act choice 6 fire 330 320 10 220 10 440 0 foe noul 980 risk score 3 740 600 270 730 0" "$out/console.log")
steps=$(wc -l < "$out/steps.jsonl" | tr -d ' ')
fired=$(grep -c "agel-play: step .*: (ctrl)" "$out/console.log")
grep -q "agel-play: done" "$out/console.log"
if [ "$replies" -lt 4 ] || [ "$steps" -ne 4 ] || [ "$fired" -lt 4 ]; then
  printf 'expected 4 typed replies, 4 recorded steps and 4 fire decisions, got %s, %s and %s\n' "$replies" "$steps" "$fired" >&2
  exit 1
fi
echo "A judged episode through the bridge: 4 steps, each a typed (judge ...) request answered on one line and decided by the program [ok]"
