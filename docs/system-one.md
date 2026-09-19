# Typed judgments: System One models in Agel

Since v0.2.91 an Agel program can ask a *System One* model — a model
that answers typed questions about a state with probabilities, in one
pass, and never writes prose — and read the answer as integers. The
first such model is TypeSafe's Jev (the [introduction](https://typesafe.ai/blog/introducing-system-one-models-and-jev),
the [documentation](https://docs.typesafe.ai)); the interface is not
Jev's. It is a request grammar, an answer line, a provider behind the
`model/infer` effect boundary the text providers already use, a library
module that prints requests and reads answers, a host word that carries
a request through a process's own console, and a judge written in Agel
that answers on the same line without a network. Nothing in the language
or the kernel knows the model's name.

The reason to want one: a language model deciding a DOOM step through
the bridge takes seconds and costs cents; a judgment takes a fraction of
a second, costs a fraction of a cent, cannot answer with a word that is
not one of the options, and says how sure it is. TypeSafe's founder
showed Jev playing DOOM from structured state at ten decisions a second.
Agel's agents already ask their questions from inside the OS; this gives
them a model whose answers software can use directly, and a place for
policy: in the program, in code, gated on confidence.

## The contract

A request is an Agel form:

```text
(judge [STATE] QUESTION...)
STATE    := "text" | (state (NAME "text")...)
QUESTION := (noul ID "instructions" ["yes means" "no means"])
          | (choice ID "instructions" OPTION OPTION...)
          | (score ID "instructions" "level" "level"...)
OPTION   := name | (name "description")
```

The state is optional: a bridge that sees more than the program does (the
desktop, which has the engine's state line and the window) adds what it
sees as named fields. Names may be symbols or text. `noul` is the yes/no
primitive (a probability of yes), `choice` picks one option and reports
the distribution over all of them, `score` places the state on ordered
levels and reports a probability-weighted position.

An answer is one line, one group per question in request order, every
probability an integer in thousandths (each rounded on its own, so a
distribution may sum to 999 or 1001):

```text
ID noul YES
ID choice COUNT OPTION CONFIDENCE P...   ; P in the request's option order
ID score COUNT SCORE CONFIDENCE P...     ; SCORE in thousandths of a level
```

The line is self-describing — each group carries its count — so a program
reads it with text words alone, and the same line comes from the hosted
model as from the judge written in Agel. Confidence is what the model
reports (TypeSafe derives it from the distribution's shape); a `noul`
carries none. Where the thresholds lie is the program's decision.

Agel has integers only, which is why thousandths: the contract is exact,
deterministic and replayable, and `(< 250 confidence)` reads as it should.
A score is a position among the levels, from 0 to one less than their
count, so with three levels it runs to 2000; until v0.2.94 the provider
clamped it to 1000 as if it were a probability (the judge written in
Agel never did), and `level_thousandths` now reads it as documented.

## The pieces

**The provider** (`crates/agel-model/src/systemone.rs`, `JevProvider`,
name `jev`). It reads the request form with the language's own reader,
posts the documented JSON to `POST /v1/systemone` and reads the answers
back into the request's option and level order. The transport is `curl`,
run in the same audited process sandbox as the Claude and Codex providers
— one allowed executable, a default-deny effect policy under
`model/infer/jev/request/`, time and output limits — and the key goes to
curl on standard input as a configuration line: never in an argument
(the process list), never in curl's environment, never on disk. The
request body waits in a file in the workspace for as long as the call
takes. `429` and `529` are retried twice with a pause; any other status,
a body that is not JSON, or answers that do not fit the questions are
provider errors, delivered as `system/model-error` like any other. The
endpoint and model are configurable (`--jev-url`, `--jev-model`), so a
self-hosted server that speaks the same shapes can stand in.

**The host.** `agel-cli --enable-jev` registers the provider and grants
`model/infer` for `jev`; the key comes from `TYPESAFEAI_API_KEY` in the
environment (`.env` is ignored by Git and read by the operator's shell,
not by Agel). A request is `(model-request 'jev REQUEST-TEXT self)`,
dispatched by `:dispatch` as the others are, and its answer line arrives
as `(system/model-result id jev "LINE")`.

**The library** (`agel/judgment` in the standard library). Questions are
data with names as text — `(choice "act" "Best next move" "forward"
("fire" "shoot"))` — and `judgment-request` prints the form, escaping as
the reader reads; `judgment-parse` reads an answer line into groups
`("id" kind value confidence probabilities)` with `answer`,
`answer-value`, `answer-confidence` and `answer-probabilities` over them;
`judge-locally` is the judge written in Agel (below). `int->text`,
`read-int` and `tokens` are exported because a program reading lines
needs them. Since v0.2.92 the cycle from an agent is two words:
`(judge-request PROVIDER STATE QUESTIONS REPLY-TO)` puts the printed
request into the outbox as `model-request` does, under the agent's
`model/infer` capability, and `(judgment-of MESSAGE)` reads the
`system/model-result` message the runtime alone can deliver into answer
groups, nil for any other message; `(judgment-failure MESSAGE)` gives the
kind and text of a `system/model-error`. The behavior decides from the
groups; nothing about the model reaches it but the numbers.

**The gate** (`agel-cli --gate agel|jev`, v0.2.92). Before `:dispatch`
invokes a provider for a pending request, the host consults a gate and
records its verdict beside the answer line it was given. `agel` evaluates
`(effect-gate REQUEST)` in the world, with `REQUEST` as
`("model/infer" "provider" ID AGENT "text")` and the answer `allow`,
`deny`, or either with a text; `(make-gate RULES THRESHOLD)` in
`agel/judgment` builds such a function from rules for one `noul`
question, `run`, over the request's kind, provider and text, answering
`(allow LINE)` or `(deny LINE)` from `judge-locally`. The evaluation is a
committed input, so an image holds it and replays it. `jev` asks the
model itself one yes/no question about the request (`--gate-question`)
under its own provider process and audit, and allows at
`--gate-threshold` thousandths or more. A denial is committed as the
request's completion, `effect/denied` with the line, delivered to the
agent as `system/model-error`; a gate that cannot decide leaves the
request pending. The gate can only refuse. It is the host's, over the one
effect the host dispatches: model requests. Nothing on the OS is gated.

**The hosted runtime's word.** `(model-request TEXT)` in a process on
the OS writes the request as a block on the process's console
(`model-request N:` through `model-request end`) and waits for the line
`:model-reply N TEXT` given to the program while it reads — by the
operator at the desktop's prompt, or by a bridge on the host that carries
the block to a model and types the answer back. It holds the
`model/infer` capability like every other effect word; a program that
was not granted it cannot ask. End of input while waiting is a
`model/unavailable` signal. The Unix shape: the model is a filter on the
process's own console, and the program never learns which one answered.

**The desktop's bridge.** `agel-play --policy jev` boots the desktop with
the `doom-agent-judge` program (`--program` chooses another), and for
every `model-request` the in-OS loop makes it takes the program's
`(judge ...)` form, adds the engine's state line and the window as shades
as fields of the state, asks the provider, and types the answer line
back. For the older `doom-agent-model` program, which sends a plain
prompt, it asks one choice among the action words and answers `WORD
reason` as the other policies do. The native evaluator gained two text
words for the program's side, `(text-field TEXT N)` (the N-th
space-separated word, or nil) and `(text-int TEXT)` (its integer, or
nil), because a program of forty-eight frames cannot walk a hundred bytes
one at a time.

**The judge written in Agel.** `(judge-locally STATE QUESTIONS RULES)`
answers the same questions on the same line. A rule is `("id" "option"
WEIGHT "needle")`: when the state's text contains the needle, the option
(a choice's option, a score's level, or `"yes"`/`"no"` for a noul) gains
the weight. Every option starts at one, probabilities are the weights in
thousandths, a confidence is the lead of the first over the second, and a
question no rule touches is answered evenly with no confidence. It is
rules, not learning: as calibrated as its rules, deterministic, replayable,
and runnable where there is no network — the interface's second
implementation, so that programs and tests do not depend on a vendor, and
the place a model of Agel's own can later stand.

## DOOM, judged

[`boot/desktop/doom-agent-judge.agel`](../boot/desktop/doom-agent-judge.agel)
is the agent. Each step it sends, within the native evaluator's 200-byte
request:

```lisp
(judge (choice act "Best next move" forward back left right fire use)
       (noul foe "Is an enemy in view?")
       (score risk "How dangerous is it?" "safe" "wary" "lethal"))
```

and reads a line such as
`act choice 6 fire 330 320 15 220 10 440 0 foe noul 980 risk score 3 740 600 270 730 0`.
The policy is the program's: a choice under 250 thousandths of confidence
falls back to the scripted reflexes (back off when hurt, otherwise forward
and firing); a confident `forward` with an enemy in view (`foe` over 500)
holds fire as well; `fire`, `use` and the turns are held as named. The
reason recorded for the step is the answer line itself, so the dataset
holds the model's distribution beside what the program did with it.

Measured on the host, one typed request through `agel-cli` with three
questions, the provider process included: 0.8 s wall clock of which the
model's answer was 86 bytes; TypeSafe's endpoint answered a comparable
request in 0.9 s from this machine, `jev-1.13.0`. Through the bridge,
twelve judged DOOM steps against the live endpoint took 24 s wall clock
with the desktop's boot included (transcript in
[v0.2.91](release-v0.2.91.md)). The same three questions cost a language
model provider seconds per step in v0.2.74.

## What is proven, and where

- `crates/agel-model`: the grammar parses and is refused with a reason
  when malformed; a request becomes the documented JSON; answers are read
  in request order and written in thousandths; the provider posts the
  body with the key on standard input and nothing in its arguments,
  removes the body file, and audits the call; HTTP failures, non-JSON and
  ill-fitting answers are errors, not answers; `529` is retried a bounded
  number of times.
- `crates/agel-stdlib/tests/judgment.rs`: a printed request is the form
  the provider reads, escapes included; an answer line parses into groups
  with the accessors; the judge written in Agel answers on the same line,
  with the arithmetic checked by hand.
- `scripts/test-agel-process.sh`: a program on the OS imports
  `agel/judgment`, judges locally, then sends a request out on its console
  and reads the reply line the desktop gives it.
- `scripts/test-play-bridge.sh`: a whole judged episode, the in-OS loop
  asking typed questions through the bridge and deciding from the answer
  line, the provider's curl a stand-in that answers as the endpoint does,
  so the path runs in CI without a network or a key.
- `crates/agel-stdlib/tests/judgment.rs` (v0.2.92): an agent's
  `judge-request` lands in the outbox as the form the provider reads, the
  completed result parses through `judgment-of` and a failure through
  `judgment-failure`; `make-gate` answers the host's request with a
  verdict and the line, even, matched and denied.
- `crates/agel-cli` (v0.2.92): the Agel gate leaves requests pending until
  `effect-gate` exists, then allows one and denies one, the denial
  delivered as `effect/denied` with the line; the judged gate, with a
  stand-in curl, allows at the threshold and denies below it, audits its
  calls under `model/infer/jev/request/gate-N`, and sends the request's
  kind, provider, agent and text as the state.
- By hand with `TYPESAFEAI_API_KEY`: the live endpoint through `agel-cli
  --enable-jev`, a twelve-step judged DOOM episode through
  `agel-play --policy jev` (transcripts in [v0.2.91](release-v0.2.91.md);
  the episode needed a fix to the provider's body path that landed on
  `main` after the tag), and both gates on two agents, one asking about a
  corridor and one for the operator's key, the model allowing the first at
  830 thousandths and denying the second at 40 (transcripts in
  [v0.2.92](release-v0.2.92.md)).

## Where this goes

The ecosystem around Jev in its first week is instructive for an OS
whose agents already live inside it: browser agents where the model picks
the operation and the target from an accessibility tree and code does the
rest; a form-filling specialist (Cua's `cua-s1-forms`, 706 thousand
parameters) that scores every field of a form in one pass; guardrails
that judge a tool call before it runs; routers that pick which model a
request deserves. Each is a bounded judgment inside a deterministic loop,
which is what Agel's agents and effects are.

The order of work from here, after the gate on the host (v0.2.92) and
the desktop driven from inside by judgment (v0.2.93,
[`computer-use.md`](computer-use.md)) and the game played and labeled
by judgment (v0.2.94, [`doom.md`](doom.md)): a browser process on the OS
whose accessibility tree is a state and whose actions are a choice;
richer perception for the driving program than the status line; a use
for the labeled datasets; and a learned judge of Agel's own behind
`judge-locally`'s contract, small enough to run in a domain. None of that
is claimed today.

## Not claimed

- Jev's calibration or accuracy on any task. The provider carries the
  model's numbers; it does not vouch for them. Thresholds are the
  program's and untuned.
- Any model runs on the OS. The hosted model is reached through a host
  bridge, as the text providers are; the judge written in Agel is rules.
- A browser, a form filler, or any gate on the OS. The gate of v0.2.92 is
  the host CLI's, over model requests; the OS's effects are governed by
  capabilities alone, and the rest is the order of work above.
- Replay of the judged gate's allowances. A `system/model-result` is
  journaled like any model result and the request is not re-issued on
  replay; a denial is the request's journaled completion; the Agel gate's
  evaluations are committed inputs; but the judged gate's allowance of a
  request leaves its line in the session's `:effects` only. The bridge
  path through the console records the line in `steps.jsonl`, not in a
  journal.
- More than 200 bytes of request from the native evaluator, or a request
  whose state the program itself supplies there: the desktop adds the
  state it sees, and the program's form must fit the request area.
