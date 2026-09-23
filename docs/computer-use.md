# Computer use: an Agel program drives the desktop from inside the OS

Since v0.2.93 a program in the OS can drive the desktop the way an
operator does: it reads the desktop's own state, decides one command line
a step, and the desktop types it. The decision comes from a typed
judgment — which of the desktop's commands comes next for a task, and
whether the task is done — carried out to a System One model through the
same host bridge the DOOM agents use, or answered by whoever sits at the
serial console. It is the second rung of the order of work in
[`system-one.md`](system-one.md), after judgments in the language and
before the game and the browser, and it is small on purpose: what exists
is the loop, the perception and the lever, on the desktop that exists.

## The loop: `:drive STEPS [HOLD]`

`:drive` is to the desktop what `:play` is to a window. It needs a loaded
program with a `drive-step` and no window at all. Each step:

1. The desktop writes what it is into the look area the program reads
   with the `look` words. The look line is the desktop's status:
   `win N focus F | SLOT TITLE [hidden] [max] [ended]... | run yes|no |
   last: LINE` — every window by slot with its title and state, which one
   has the focus, whether a process runs, and the last line the terminal
   finished. The shades are the focused window's canvas, dark when there
   is none.
2. `(drive-step)` is evaluated. If the step made a `model-request`, the
   desktop relays the block on the serial console exactly as the play loop
   does (`model-request N TEXT`, `look-line: …`, the shades,
   `model-request end`), waits for `:model-reply N TEXT`, delivers it and
   asks again.
3. The value is text: a command line, `wait`, or `done`. The desktop
   reports `drive: step N do LINE reason R` on the console and in the
   terminal, then types the line as the operator would have — `:fs-ls /`,
   `:help`, `:maximize 0`, a form — through the same dispatcher, and
   reports what the command answered as `drive: STATUS`, which is the last
   line the program sees next. `wait` types nothing; `done` ends the run
   (`DRIVE DONE AFTER N STEPS`); the run otherwise ends after its steps
   (`DROVE N STEPS`).
4. If a process runs, it runs for `HOLD` passes, as in `:play`.

Three lines are refused whatever the program says: `:drive` and `:play`,
which would nest this loop, and `:shutdown`. The refusal is reported as
`drive: REFUSED` and the run goes on. Everything else the desktop accepts
from the keyboard it accepts from the program; there is no separate
permission, because the program is the operator's, loaded by the
operator, and runs only while the operator's `:drive` does.

## The program: `desktop-agent`

[`boot/desktop/desktop-agent.agel`](../boot/desktop/desktop-agent.agel)
(`:load desktop-agent`) asks two questions a step:

```lisp
(judge (choice act "Next desktop command for the task" help files kernel workspace maximize close wait done)
       (noul done "Is the task complete?"))
```

and reads the answer line with `text-field` and `text-int`. Its policy is
in code: `done` above 500 thousandths ends the run; a choice under 200
thousandths of confidence waits; otherwise the chosen name becomes the
desktop's command (`files` is `:fs-ls /`, `close` is `:close 0`, and so
on). The task itself is not in the program: the host names it when it
answers, so one program serves any task the menu can reach. The menu is
the program's and is deliberately narrow; a program with a wider one is a
matter of more lines, not of the kernel.

## The bridge: `agel-play --scene desktop --task TEXT`

`agel-play` boots the desktop without the game, loads the program
(`desktop-agent`, or `--program`), sends `:drive STEPS HOLD`, and answers
each relayed block. With `--policy jev` the judge is TypeSafe's System One
model: the program's `(judge …)` form gains the fields `task` (the
operator's text), `desktop` (the look line, introduced as what it is),
`history` (the commands typed so far in this run, oldest first, because a
System One model remembers nothing between calls and the desktop's line
shows only the last answer; without it the live run asked for the help
eight times) and `frame` (the shades), and the answer line goes back
typed. Every step is
recorded in `steps.jsonl` with the command the program decided on
(`keys`), the judge's line (`reason`) and a screenshot, as the DOOM runs
are.

## Summoned by a sentence (v0.2.96)

The loop above needed the operator to load the agent and type `:drive`.
Since v0.2.96 a sentence typed at the desktop's prompt does both: the
desktop keeps it as the operator's intent, loads the workbench first if
the world is empty, joins the desktop agent's six cells beside whatever
is loaded (the workbench's nine share no names with them), and drives
for eight steps, relaying the sentence as a `task:` line with every
request block. The judge on the other side reads it: the bridge takes
the block's task over the one it was started with, and the attached
bridge (`agel-play --attach SOCKET`, `run-graphics.sh --agent`) keeps the
run's history per sentence, as the booted bridge does per run, because a
judge remembers nothing. Live, the sentence "show me the help, then
finish" typed at the desktop's keyboard summoned the agent, which showed
the help and said done in two judged steps; without the history the
judge asked for the help eight times, the same finding as v0.2.93's.

The cell table did not grow for this: twenty-four cells overflowed the
kernel's stack at boot, so the workbench and the agent merged forms to
fit sixteen together, with one to spare for the operator's own cell.

## Starting the game, and handing over to its agent (v0.2.97)

"Can you play DOOM for a minute?" typed at the prompt did nothing in
v0.2.96: the agent's menu had no way to start or play the game, and the
desktop's own disk had no game installed. Now:

- `run-graphics.sh` boots its own persistent disk, `agel-desktop.img`,
  refreshed from the seed's boot sectors and assets every run, with the
  game and its data, the hosted runtime and the browser's site installed
  when they are built (the script says what is missing otherwise).
- The agent's menu is `help files start-doom play-doom maximize close wait done`:
  `start-doom` is `:exec c-doom -- -iwad /data/doom1.wad -mb 8 -warp 1 -skill 2`,
  and `play-doom` is `:handover doom-agent-judge 60`. Named `doom` and
  `play` at first, the judge started the game and then chose `wait` eight
  times: the names say what they do now.
- `:handover NAME STEPS` is a desktop command: the program named is
  loaded over the world — a load replaces a world that holds nothing but
  programs the desktop carries, and keeps one with the operator's own
  cells — the game is given up to a bounded number of passes to print its
  first frame, and `:play STEPS` runs. A driving program cannot run a loop
  inside its own, so when the agent decides on a handover the drive loop
  ends with `HANDOVER NAME STEPS` as its status and the desktop's console
  loop performs it after; the same status is what a typed `:handover`
  returns.
- `:handover` gives the keyboard to the front-most window a live process
  owns before it plays, since a person may have clicked elsewhere; and
  `play-doom` with no window open starts the game first — the look line's
  first number is the window count, and that check is the program's.
- The attached bridge answers the game's requests as the booted one does:
  a request without a `task:` line is the game's, judged with the game's
  fields and the last eight steps as history; a request with one is the
  desktop's.

The whole path is on video, recorded live by `scripts/record-demo.py`:
[`media/agel-jev-demo.mp4`](media/agel-jev-demo.mp4), with the console's
transcript in [v0.2.97](release-v0.2.97.md).

## Agents side by side: `:agents STEPS [HOLD]` (v0.2.99)

`:drive` and `:play` are one loop with a mode now, and `:agents` is its
third shape: every program in the world that defines `NAME-step` (NAME
its cell prefix without the dash) is an agent, and each step the loop
reads `(NAME-needs)`, a list of facts the desktop checks without asking
anyone — `(file "NAME")` present, `(window)` a listening window
focused, `(process)` a process running, `(done NAME)` another agent
finished, `(after SECONDS)` the clock past a mark since the run began —
and steps each agent whose needs are met, once, round-robin. With
`(window)` among the needs the step is a play step (the game paused,
the window looked at, keys held for `hold` passes); otherwise a drive
step (the desktop looked at, a command line typed, refusals as for
`:drive`). The loop says `agents: step N NAME waits FACT` for an agent
held back, `agents: step N NAME keys FORM` or `... do LINE reason R` for
one that stepped, and ends with `AGENTS DONE AFTER N STEPS` when every
agent has said `done`, or `AGENTS RAN N STEPS`. Between steps a line at
the prompt is heard: a sentence becomes the task the requests carry,
and `:stop` ends the run. `:join NAME` puts a program the desktop
carries beside what is in the world, as a sentence joins the desktop
agent; forty cells hold the workbench, a player and a reviewer.

A watch is an agent whose need is its trigger; a scheduled job is one
whose need is `(after SECONDS)`. A fact only the judge can settle is
asked by the agent in its own step and defined as a need, so the loop
never waits on a reply. The first agent that changes another is
`review` (`rv-`), described in [`doom.md`](doom.md) and
[`release-v0.2.99.md`](release-v0.2.99.md); the general model these
belong to is [`real-work.md`](real-work.md).

## What is proven, and where

- `scripts/test-drive.sh` (v0.2.96 additions): a blank region is
  formatted at boot so `:fs-ls` answers; Tab and a lone word on an empty
  world are told what to do; a sentence with the agent loaded drives with
  the sentence as the request's `task:` line and ends on the judge's
  done. `scripts/test-native-workbench.py`: a click on the empty desktop
  opens the workbench without a rollback; a sentence joins the agent
  beside it (fifteen cells) and the workbench still answers its forms.
- `scripts/test-drive.sh`: in the OS, with this harness answering as the
  bridge would — `:drive` refuses to run without a program; the program
  sees an empty desktop (`win 0 focus none | run no | last: …`), asks, is
  told `files` with confidence and types `:fs-ls /`; the next look line
  carries the desktop's answer to it as `last: drive: …`; an unsure
  answer waits; `:kernel` is typed; the judge's yes on `done` ends the run
  after four steps with the judge's line as the reason; a further run ends
  as driven; and a program that decides on `:shutdown` is refused.
- `scripts/test-play-bridge.sh`: the desktop driven through the bridge
  with the provider's curl a stand-in — three steps, each a typed reply
  read into `:fs-ls /`, three records, and the request bodies carrying
  `task` and `desktop` as fields.
- By hand with `TYPESAFEAI_API_KEY`: the live endpoint driving the
  desktop through a three-part task — the help, the listing, the kernel
  report — in four judged steps, the fourth the judge's `done` at 780
  thousandths, 8 s with the boot (transcript in
  [v0.2.93](release-v0.2.93.md)).

## Not claimed

- Perception beyond the status line. The program sees window slots,
  titles and states, the focus, whether something runs, and one line of
  the terminal; it does not read the terminal's history, a window's text,
  or the panel, and the shades are luminance, not characters. A richer
  observation is the obvious next step and is not here.
- Any action beyond the desktop's own command lines. No keys are sent to
  windows from `:drive` (that is `:play`'s), no pointer moves, no text is
  typed into a process's console.
- Any judgment of the commands themselves. The desktop refuses three
  lines and types the rest; the gate of v0.2.92 is the host CLI's over
  model requests and does not sit here.
- Calibration: the thresholds in `desktop-agent` are untuned, and the
  stand-in in CI answers the same line every step. Whether a model
  completes a task on this desktop is measured only by hand.
- A task the program itself holds, a task carried over several `:drive`
  runs, or any memory between runs beyond the world the program is.
