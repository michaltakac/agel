# Agel v0.2.91: typed judgments, a System One model as a provider

An Agel program can now ask a System One model — a model that answers
typed questions about a state with calibrated probabilities in one pass
and never writes prose; TypeSafe's Jev is the first — and read the answer
as integers. The design is in [`system-one.md`](system-one.md). Nothing
Jev-specific entered the language or the kernel: the model is a provider
behind the `model/infer` boundary the text providers use, the request is
an Agel form, the answer is a line, and a judge written in Agel answers
on the same line without a network.

## What is new

- **The `jev` provider** (`crates/agel-model/src/systemone.rs`). The
  request grammar `(judge [STATE] QUESTION...)` with `noul`, `choice` and
  `score` questions is read with the language's reader, sent as the
  documented JSON to `POST /v1/systemone`, and the answers come back in
  the request's option and level order as one line in thousandths:
  `ID noul YES`, `ID choice COUNT OPTION CONFIDENCE P...`,
  `ID score COUNT SCORE CONFIDENCE P...`. Transport is `curl` in the same
  audited process sandbox as Claude and Codex, with the key handed to it
  on standard input as a configuration line — not in an argument, not in
  its environment, not on disk. `429`/`529` are retried twice; anything
  else that is not a fitting answer is a provider error. `--jev-url` and
  `--jev-model` choose another endpoint or model.
- **`agel-cli --enable-jev`**, the key from `TYPESAFEAI_API_KEY` in the
  environment, and the usual `model/infer` grant, outbox, `:dispatch` and
  `system/model-result`.
- **`agel/judgment`** in the standard library: `judgment-request` prints a
  request from questions as data, `judgment-parse` reads an answer line
  into `("id" kind value confidence probabilities)` groups with
  `answer`, `answer-value`, `answer-confidence` and
  `answer-probabilities`; `judge-locally` is a rule-weighted judge written
  in Agel on the same contract, deterministic and networkless.
- **`model-request` in the hosted runtime**: a program on the OS writes
  its request as a block on its own console and waits for the
  `:model-reply N TEXT` line the desktop gives it, behind `model/infer`.
- **`doom-agent-judge.agel`** and **`agel-play --policy jev`**: the DOOM
  agent sends three typed questions each step; the bridge adds the state
  line and the window, asks the provider, and types back the answer line;
  the program's policy gates on confidence and falls back to its reflexes.
  `--program` chooses another program; the older `doom-agent-model` gets a
  single choice among the action words on the same policy. The native
  evaluator gained `text-field` and `text-int` to read the line.

## Transcripts

The live endpoint through the CLI, the key in the environment
(`jev-1.13.0` answered; 0.8 s wall clock for the whole exchange, provider
process included):

```text
agel[3]> ((choice "act" "Best next move for the player" "forward" "back" "left" "right" ("fire" "shoot the current weapon") "use") (noul "foe" "Is an enemy in view?") (score "risk" "How dangerous is the situation?" "safe" "wary" "lethal"))
agel[4]> (("game" "DOOM E1M1") ("state_line" "doom: state map 1 x 1056 y -3616 angle 90 health 100 armor 0 ammo 50 kills 0") ("frame" "an open corridor ahead, a zombie soldier at the far left"))
agel[7]> (ask)
agel[9]> dispatching request #1 to jev...
request #1 completed (86 bytes)
agel[12]> (("act" choice "fire" 400 (300 20 70 90 510 10)) ("foe" noul 950 nil nil) ("risk" score 830 700 (190 790 20)))
agel[14]> "fire"
agel[15]> 400
agel[16]> 950
agel[17]> 830
```

A program on the OS, judging locally and then through its console
(`scripts/test-agel-process.sh`, the harness typing the reply as the
bridge would). The file is one transaction, so its values — the local
judge's first, then the reply's — print once it commits, after the reply:

```text
live-desktop> :exec agel -- judge.agel
agel: standard library installed, 3481 steps
model-request 1:
(judge "an imp ahead" (choice "act" "Best next move" "forward" "back" "fire") (noul "foe" "Is an enemy in view?"))
model-request end
PROCESS READING
live-desktop> :model-reply 1 act choice 3 back 400 200 600 200 foe noul 100
LINE GIVEN TO THE PROGRAM
=> "fire"
=> 900
=> "back"
=> 400
=> 100
process agel exited with status 0
```

The in-OS loop judged through the bridge (`scripts/test-play-bridge.sh`,
the provider's curl a stand-in answering as the endpoint does):

```text
agel-play: 4 steps by the jev Agel program (doom-agent-judge) into target/doom-runs/jev
agel-play: model reply 1: act choice 6 fire 330 320 10 220 10 440 0 foe noul 980 risk score 3 740 600 270 730 0
agel-play: step 1: (ctrl) []
agel-play: model reply 2: act choice 6 fire 330 320 10 220 10 440 0 foe noul 980 risk score 3 740 600 270 730 0
agel-play: step 2: (ctrl) [doom: state map 1 x 1056 y -3616 angle 64 health 100 armor 0 ammo 50 kills 0]
```

## Validation

- `cargo test --workspace`: the provider's grammar, JSON, answer order,
  thousandths, key handling, audit, failures and retries
  (`crates/agel-model`); the library's printing, parsing and local judge
  (`crates/agel-stdlib/tests/judgment.rs`); the CLI's flags.
- The kernel's native evaluator tests: `text-field` and `text-int`.
- `scripts/test-agel-process.sh`: the in-OS program above.
- `scripts/test-play-bridge.sh`: the echo episode as before, and the
  judged episode with the stand-in curl.
- The live endpoint by hand, as transcribed. The full local regression
  (81 suites) and CI.

## Not claimed

- Jev's calibration or accuracy on any task; the provider carries the
  model's numbers and the thresholds in the DOOM program are untuned.
- A model on the OS. The hosted model is reached through the host bridge
  or the CLI; the judge written in Agel is rules, not learning.
- A browser, a form filler, an effect gate, or replay of judgments —
  the order of work in `system-one.md`, none of it here.
- A live DOOM episode as a tested path: the judged episode in CI uses
  the stand-in; the endpoint needs a key and is run by hand.
- Requests from the native evaluator larger than its 200-byte request
  area, or with a state of the program's own; the desktop adds the state.
