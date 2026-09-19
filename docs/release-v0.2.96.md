# Agel v0.2.96: a desktop for a person, an agent for a sentence

A report from using the graphical desktop by hand: every click rolled a
transaction back, the filesystem answered with an error number, plain
words at the prompt failed as unbound symbols, and most of the desktop
expected Agel forms typed at the bottom. This release makes the desktop
usable as a desktop, and makes the agents that already live in it
summonable by a sentence. The design notes are in
[`native-graphics.md`](native-graphics.md) and
[`computer-use.md`](computer-use.md).

## What is fixed

- **Clicks on an empty world.** The desktop's background click
  evaluated `(point X Y)`, which only the workbench defines, so a fresh
  boot rolled back on every click and repainted everything. Now a click
  with no workbench loaded opens the workbench on an empty world, or says
  the loaded program does not answer clicks; nothing is evaluated. Tab is
  guarded the same way. With the workbench in, the click is what it was,
  and echoed on the console.
- **The filesystem at boot.** A blank region (no magic in its
  superblock) is formatted before anything reads it, so `:fs-ls` answers
  on a fresh disk; a formatted region is left alone.
- **A word at the prompt.** A lone word that names nothing answers
  `UNBOUND WORD - A SENTENCE SUMMONS THE AGENT, :HELP LISTS COMMANDS`
  instead of a rollback; `:help` now begins with what to do.
- **The flash.** The frame is drawn whole only when the scene under the
  command bar changed since the last whole frame, by a digest of its
  records; a status that changed alone draws the bar.

## What is new

- **A sentence summons the agent.** A line that opens with a letter and
  has a space is the operator's intent. The desktop keeps it, loads the
  workbench first if the world is empty, joins the desktop agent's six
  cells beside the world's (the workbench's nine share no names with
  them), and drives for eight steps, relaying the sentence as a `task:`
  line with every request block. The agent's commands and `:drive`'s
  refusals are unchanged.
- **`agel-play --attach SOCKET`**: the judge beside a person. It attaches
  to a running desktop's serial socket, prints what the console says,
  answers every request the desktop relays — the block's task over the
  bridge's own — keeping the run's history per sentence, and types
  nothing but `:model-reply` lines. `run-graphics.sh --agent` starts the
  window with the console on a socket and the bridge attached
  (`TYPESAFEAI_API_KEY` in the environment).
- **The programs merged forms** so the workbench (nine cells) and the
  desktop agent (six) fit the sixteen-cell table together. A
  twenty-four-cell table was tried first and hung the machine at boot: the
  workspace is copied on the kernel's stack in several frames, and the
  larger one overflowed it. The table stays at sixteen; the roadmap has
  the row.

## Transcript

Live, the key in the environment: a headless desktop booted with its
console on a socket, the bridge attached, and the sentence typed through
QEMU's own keyboard, as at the window:

```text
$ agel-play --attach …/serial --scene desktop --policy jev
agel-play: attached to …/serial; the jev policy answers what the desktop asks
…
live-desktop> show me the help, then finish
model-request 1 (judge (choice act "Next desktop command for the task" help files kernel workspace maximize close wait done) (noul done "Is the task complete?"))
look-line: win 0 focus none | run no | last: formatted
task: show me the help, then finish
…
agel-play: model reply 1: act choice 8 help 990 1000 0 0 0 0 0 0 0 done noul 90
drive: step 1 do :help reason "act choice 8 help 990 1000 0 0 0 0 0 0 0 done noul 90"
drive: click the desktop or :workbench | a sentence summons the agent | …
…
agel-play: model reply 2: act choice 8 done 550 240 0 0 0 0 140 10 610 done noul 390
drive: step 2 do done reason "act choice 8 done 550 240 0 0 0 0 140 10 610 done noul 390"
DRIVE DONE AFTER 2 STEPS
```

Without the run's history in the attached bridge, the same sentence had
the judge ask for the help eight times, the finding of v0.2.93 again; the
history is sent now.

## Validation

- `scripts/test-drive.sh`: `:fs-ls` answers on a fresh boot; Tab and a
  lone word on an empty world are told what to do; a sentence with the
  agent loaded drives with the sentence as the request's task and ends on
  the judge's done; the earlier checks as before.
- `scripts/test-native-workbench.py`: a click on the empty desktop opens
  the workbench without a rollback and a second `:workbench` is refused;
  a sentence joins the agent beside the workbench (fifteen cells, the
  dock's state starting over on the replay) and the workbench still
  answers its forms.
- `crates/agel-play`: the block parser reads the `task:` line and a
  process's own block form.
- `scripts/test-programs.py` expects the new status for a lone unbound
  word; `scripts/test-native-dock.py`, which compares whole screens across
  a reboot, is why the boot-time format reports on the serial console
  only and leaves the terminal panel empty.
- By hand: the live transcript. The full local regression and CI.

## Not claimed

- A judgment of the sentence itself, or of the commands the agent types
  beyond the three refusals.
- A desktop that never flickers: only that an unchanged scene is not
  redrawn.
- More than one agent, or an agent beside any program but the workbench
  (the others share names with it, and the table has one cell to spare).
- Any agent without a judge on the host: without the bridge the run
  waits and says `model-reply: none`.
