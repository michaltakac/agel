# Agel v0.3.1: a lookup through the host, and a plan from it

The plan's next row: an agent on the OS asks for a page from the
internet and for a plan drawn from it, and the player steers by that
plan through a typed question. The OS has no network and runs no model;
both are the host's, reached through the same request the judge answers,
and what comes back lands in files on the OS, which is where a program
reads. The model is used once, for the one thing no typed question can
do: turning a page of prose into a list of typed steps.

## What changed

- **`(fetch "URL" "NAME")`.** A program's `model-request` with this form
  is served by the bridge, not the judge: it fetches the page with curl,
  drops scripts and styles, replaces tags with breaks, decodes a few
  entities, keeps lines of 24 characters or more, and delivers at most
  3 KiB as `:page NAME LINE` lines ahead of its reply `fetched NAME LINES
  BYTES`. The kernel keeps the lines and, once the step is done, writes
  them to the file NAME in the filesystem region and says `page: NAME
  BYTES`. `(needs (file "NAME"))` is a watch on the lookup.
- **`(plan "NAME" "GOAL")`.** With a planner configured on the bridge
  (`--plan claude` or `--plan codex`, the same providers the model policy
  uses, under the same limits and audit), the model reads the fetched
  text once and is asked for at most eight lines `N. DIRECTION -
  instruction`, DIRECTION one of north, east, south, west, door, switch,
  keep; the lines land in the file `plan`, the reply is `planned plan
  COUNT`, and the model's whole answer is kept beside the run. Without a
  planner the reply is `planned plan 0 no planner` and no file: the OS
  never pretends to have a plan.
- **The `lookup` agent** (`lk-`, three cells): needs nothing, asks for
  the E1M1 page and a plan for reaching its exit, says each answer on
  the console as `lookup: ...`, and is done. `agel-play --join NAME`
  joins any program before `:agents`.
- **The player heeds the plan.** When the file `plan` appears the DOOM
  agent reads its tail into its next request as one more typed question:
  the plan's text, and where the player should head now (north, east,
  south, west, keep). The answer is a heading for ten steps, said on the
  console as `doom: plan says WHERE`; `keep` leaves the engine's route.
  The plan is read once and asked about once; twelve cells.

- **A request is one line.** The plan's lines, embedded in the player's
  question, split the request on the serial line and the judge's reader
  stopped at an unterminated string; the kernel now writes a request's
  newlines as spaces. Any text from a file can go into a question. And
  the world kept a request's length, a reply's and an exec line's in a
  byte, so the first request over 255 bytes (the plan question, 290) went
  out cut at its length modulo 256, at a different place each run;
  v0.2.100 raised the limit and left the byte. They are sixteen bits now.
  The bridge prints the prompt of a request the judge could not read.

## Measured

One live run (`agel-play --policy jev --agents --join lookup --plan
claude --steps 120`), the lookup beside the player and the reviewer,
against doomwiki.org, Claude as the planner and Jev as the judge,
reported as it went:

- `fetched e1m1 1 59`: the page came back as a Cloudflare interstitial
  ("Just a moment... Enable JavaScript and cookies to continue"), one
  line of 59 bytes, which the kernel wrote to `e1m1`. The other DOOM
  wiki answers curl the same way; Wikipedia serves its text but has no
  walkthrough.
- `planned plan 8`: the planner read the 59 bytes and said so in its
  first line ("only a Cloudflare interstitial, no walkthrough text came
  through, so this plan is from general knowledge of E1M1, not that
  source"), then gave eight typed steps (east, door, south, east, door,
  north, east, switch), which the bridge filtered to the eight numbered
  lines and the kernel wrote to `plan`. The disclaimer is kept in
  `plan-answer.txt` beside the run and is not in the file the player
  reads, which is the honest shape: the plan's provenance is the host's
  record, not the OS's.
- The player's next request carried the plan and the question where to
  head; Jev answered `east` at 450 thousandths (north 70, east 550,
  south 90, west 30, keep 260), and the player took east for ten steps
  before returning to the engine's route.

So the whole path ran, and the one thing it was for, a page of prose
turned into typed steps by a model once, ran on a page that was not
the walkthrough. What this says: the OS's lookup is only as good as
what the host can fetch, and a page behind a bot check is not fetched
by curl; the plan a model writes from memory is a plan the OS cannot
tell from one written from the page, except by the record the bridge
keeps. Both facts go in the ledger as they are.

## Validation

- `scripts/test-drive.sh`: `:page` lines delivered ahead of a reply are
  written to the file named, and the loop says so.
- `scripts/test-play-bridge.sh`: the lookup episode, with a stand-in
  curl that serves a page from a file and answers the judge, and a
  stand-in planner that prints three steps: `fetched e1m1 3`, `planned
  plan 3`, and the player's request carrying the plan question, answered
  `north`.
- The full local regression and CI.
- By hand: the live run above, against doomwiki, Jev and Claude.

## Not claimed

- That a plan from a page is a route: the player takes one heading from
  it for ten steps and returns to the engine's route; a plan followed
  step by step, with "is this step done" as a typed question, is the
  next row.
- Any fetch beyond what curl returns in 30 seconds and 600 KB, any HTML
  beyond tag stripping, any page the judge would read; the page is for
  the planner and the files are what the OS sees.
- A tool written by a model: the row after this one.
- A fetch of a page behind a bot check: curl gets the interstitial, and
  the OS gets what curl gets. A page fetched by a browser on the host is
  a possible row, not this one.
