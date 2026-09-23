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
printf '%s\n200' '{"model":"stand-in","answers":{"foe":{"type":"noul","noul":0.98},"risk":{"type":"score","score":0.74,"confidence":0.6,"legend":{"0":"safe","1":"wary","2":"lethal"},"probabilities":{"0":0.27,"1":0.73,"2":0.0}}},"usage":{"input_tokens":1,"output_tokens":1}}'
FAKE
chmod +x "$out/curl"
TYPESAFEAI_API_KEY=stand-in cargo run -q --release -p agel-play -- --image "$image" --doom "$doom" --wad "$wad" \
  --out "$out" --policy jev --curl-bin "$(pwd)/$out/curl" --steps 4 | tee "$out/console.log"
replies=$(grep -c "agel-play: model reply .*: foe noul 980 risk score 3 740 600 270 730 0" "$out/console.log")
steps=$(wc -l < "$out/steps.jsonl" | tr -d ' ')
# An enemy in view: the program goes forward firing; the engine's state
# line carries the way to the exit, the seen share of the map and the rays.
fired=$(grep -c "agel-play: step .*: (up ctrl)" "$out/console.log")
goals=$(grep -c "agel-play: step .*: (up ctrl) \[doom: state map 1 .* goal [0-9]* dist [0-9]* path [0-9]* seen [0-9]* free [0-9]* [0-9]* [0-9]* door [01]" "$out/console.log")
grep -q "agel-play: done" "$out/console.log"
if [ "$replies" -lt 4 ] || [ "$steps" -ne 4 ] || [ "$fired" -lt 4 ] || [ "$goals" -lt 4 ]; then
  printf 'expected 4 typed replies, 4 recorded steps, 4 forward-firing decisions and 4 goal reports, got %s, %s, %s and %s\n' "$replies" "$steps" "$fired" "$goals" >&2
  exit 1
fi
echo "A judged episode through the bridge: 4 steps, each a typed (judge ...) request answered on one line and decided by the program toward the exit [ok]"

# The desktop driven through the bridge: no game, no window. The program
# asks which of the desktop's commands comes next for the task and whether
# the task is done; the stand-in answers "list the files" each step, with
# the task not yet done, so the run goes its whole length.
out=target/doom-runs/desktop
rm -rf "$out"
mkdir -p "$out"
cat > "$out/curl" <<'FAKE'
#!/bin/sh
cat > /dev/null
for arg in "$@"; do case "$arg" in @*) cp "${arg#@}" "$(dirname "$0")/body-$$";; esac; done
printf '%s\n200' '{"model":"stand-in","answers":{"act":{"type":"choice","choice":"files","confidence":0.7,"probabilities":{"help":0.05,"files":0.7,"start-doom":0.05,"play-doom":0.05,"review-doom":0.05,"maximize":0.05,"close":0.05,"wait":0.03,"done":0.02}},"done":{"type":"noul","noul":0.1}},"usage":{"input_tokens":1,"output_tokens":1}}'
FAKE
chmod +x "$out/curl"
TYPESAFEAI_API_KEY=stand-in cargo run -q --release -p agel-play -- --image "$image" --scene desktop \
  --task "list the files in the region" --out "$out" --policy jev --curl-bin "$(pwd)/$out/curl" --steps 3 | tee "$out/console.log"
replies=$(grep -c "agel-play: model reply .*: act choice 9 files 700 50 700 50 50 50 50 50 30 20 done noul 100" "$out/console.log")
listed=$(grep -c "agel-play: step .*: :fs-ls /" "$out/console.log")
steps=$(wc -l < "$out/steps.jsonl" | tr -d ' ')
grep -q "agel-play: done" "$out/console.log"
grep -q '"task":"list the files in the region"' "$out"/body-* 
grep -q '"desktop":"The Agel desktop' "$out"/body-*
if [ "$replies" -lt 3 ] || [ "$listed" -lt 3 ] || [ "$steps" -ne 3 ]; then
  printf 'expected 3 typed replies, 3 listings and 3 recorded steps, got %s, %s and %s\n' "$replies" "$listed" "$steps" >&2
  exit 1
fi
echo "The desktop driven through the bridge: 3 steps, each a judged choice of the desktop's own commands, typed by the program [ok]"

# The model as a labeler: the echo episode's dataset judged step by step
# after the fact, the stand-in answering as the endpoint does; nothing is
# booted. Four steps in, four judgments out, and the summary.
out=target/doom-runs/judged
rm -rf "$out"
mkdir -p "$out"
cat > "$out/curl" <<'FAKE'
#!/bin/sh
cat > /dev/null
for arg in "$@"; do case "$arg" in @*) cp "${arg#@}" "$(dirname "$0")/body-$$";; esac; done
printf '%s\n200' '{"model":"stand-in","answers":{"good":{"type":"noul","noul":0.8},"faring":{"type":"score","score":1.2,"confidence":0.5,"legend":{"0":"losing","1":"even","2":"winning"},"probabilities":{"0":0.1,"1":0.6,"2":0.3}}},"usage":{"input_tokens":1,"output_tokens":1}}'
FAKE
chmod +x "$out/curl"
TYPESAFEAI_API_KEY=stand-in cargo run -q --release -p agel-play -- --out "$out" --policy jev \
  --curl-bin "$(pwd)/$out/curl" --judge-dataset target/doom-runs/echo/steps.jsonl | tee "$out/console.log"
judged=$(wc -l < target/doom-runs/echo/judged.jsonl | tr -d ' ')
grep -q "agel-play: judged 4 steps: mean good 800 thousandths, mean faring 1200" "$out/console.log"
grep -q '"keys_held":"(up)"' "$out"/body-*
grep -q '"good":800' target/doom-runs/echo/judged.jsonl
if [ "$judged" -ne 4 ]; then
  printf 'expected 4 judged steps, got %s\n' "$judged" >&2
  exit 1
fi
python3 scripts/doom-score.py target/doom-runs/echo/steps.jsonl | grep -q "4 steps"
echo "A recorded episode judged after the fact: 4 steps, each a typed judgment of the move and the state [ok]"

# The browser written in Agel through the bridge: the hosted runtime and
# the example site installed, the agent's script written with the task,
# the process asking on its own console and the stand-in answering that
# the task is done at once. One step, one record, the process exited.
out=target/doom-runs/browse
rm -rf "$out"
mkdir -p "$out"
cat > "$out/curl" <<'FAKE'
#!/bin/sh
cat > /dev/null
for arg in "$@"; do case "$arg" in @*) body="${arg#@}";; esac; done
cp "$body" "$(dirname "$0")/body-$$"
# The page's options are in the request: every one gets a probability,
# done all of it, as the endpoint answers.
probabilities=""
for key in $(sed -n 's/.*"act":{"criteria":{\([^}]*\)}.*/\1/p' "$body" | grep -o '"[^"]*":' | tr -d '":'); do
  if [ "$key" = done ]; then p=0.9; else p=0.0; fi
  probabilities="$probabilities\"$key\":$p,"
done
printf '{"model":"stand-in","answers":{"act":{"type":"choice","choice":"done","confidence":0.9,"probabilities":{%s}},"done":{"type":"noul","noul":0.95}},"usage":{"input_tokens":1,"output_tokens":1}}\n200' "${probabilities%,}"
FAKE
chmod +x "$out/curl"
agel=$(./scripts/build-program.sh agel x86_64 | tail -n 1)
TYPESAFEAI_API_KEY=stand-in cargo run -q --release -p agel-play -- --image "$image" --scene browse --agel "$agel" \
  --task "find the price of the \"blue\" widget" --out "$out" --policy jev --curl-bin "$(pwd)/$out/curl" --steps 3 | tee "$out/console.log"
grep -q "agel-play: model reply 1: act choice " "$out/console.log"
grep -q "agel-play: step 1: done" "$out/console.log"
grep -q "agel-play: done" "$out/console.log"
grep -q '"task":"find the price of the \\"blue\\" widget"' "$out"/body-*
grep -q '"page":"page: Widget & Co (/data/index.html) form: /data/search.html' "$out"/body-*
steps=$(wc -l < "$out/steps.jsonl" | tr -d ' ')
if [ "$steps" -ne 1 ]; then
  printf 'expected 1 recorded step, got %s\n' "$steps" >&2
  exit 1
fi
echo "The browser written in Agel through the bridge: the page as the judge's state, the answer typed to the process, the run recorded [ok]"
