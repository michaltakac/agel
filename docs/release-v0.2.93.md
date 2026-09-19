# Agel v0.2.93: an Agel program drives the desktop from inside the OS

Computer use, on the OS Agel boots: a program in the native evaluator
reads the desktop's own state, asks a judge which of the desktop's
commands comes next for a task and whether the task is done, and the
desktop types what it decided. The design is in
[`computer-use.md`](computer-use.md); it is the second rung of the order
of work in [`system-one.md`](system-one.md).

## What is new

- **`:drive STEPS [HOLD]`** on the desktop: the loaded program's
  `(drive-step)` is asked once a step for a command line, `wait` or
  `done`, with the desktop's own state in the look line — every window by
  slot with its title and state, the focus, whether a process runs, the
  last line the terminal finished — and the focused window's shades. A
  `model-request` the step makes is relayed on the serial console as the
  play loop relays it, and the step is asked again with the answer. The
  line is typed through the desktop's own dispatcher and its answer
  reported as `drive: STATUS`, the last line the program sees next.
  `:drive`, `:play` and `:shutdown` are refused. No window is needed.
- **`desktop-agent`** (`:load desktop-agent`): two typed questions a step,
  a choice among eight of the desktop's commands and a yes/no on
  completion; policy in code, confidence and completion thresholds
  included; the task is the host's to name.
- **`agel-play --scene desktop --task TEXT`**: the bridge without the
  game. With `--policy jev` the judge sees the task, the desktop's line,
  the run's history (what was typed so far, since a judge has no memory)
  and the shades, and the answer line goes back typed. `steps.jsonl`
  records each decision as the DOOM runs are recorded.

## Transcripts

Live, the key in the environment, one task, eight steps allowed. The
model asked for the help, then the listing, then the kernel report, and
then said the task was complete; 8 s wall clock with the desktop's boot:

```text
$ agel-play --scene desktop --policy jev --steps 8 \
    --task "Show the help, then list the files in the region, then show the kernel report; when all three have been shown, the task is complete."
agel-play: 8 steps by the jev Agel program (desktop-agent) into target/doom-runs/desktop-live
agel-play: model reply 1: act choice 8 help 1000 1000 0 0 0 0 0 0 0 done noul 40
agel-play: step 1: :help []
agel-play: model reply 2: act choice 8 files 820 100 850 30 10 0 0 10 0 done noul 60
agel-play: step 2: :fs-ls / []
agel-play: model reply 3: act choice 8 kernel 920 30 10 930 0 0 0 20 10 done noul 50
agel-play: step 3: :kernel []
agel-play: model reply 4: act choice 8 done 840 20 0 40 0 0 10 60 870 done noul 780
agel-play: step 4: done []
agel-play: done; the dataset is target/doom-runs/desktop-live/steps.jsonl
```

Without the history in the state, the same task ran all eight steps
choosing `help` at 990 every time: the desktop's line shows only the last
answer, and a System One model remembers nothing between calls. That run
is why the bridge carries the history; it is in this release's git
history, not claimed as a result.

In the OS, the harness answering as the bridge would
(`scripts/test-drive.sh`): the program sees
`win 0 focus none | run no | last: …`, is told `files` at 700 and types
`:fs-ls /`; sees `last: drive: …` next; waits when told `help` at 150;
types `:kernel` at 900; and ends on `done` at 950 with the judge's line as
its reason, `DRIVE DONE AFTER 4 STEPS`. A decision of `:shutdown` is
refused and the run goes on.

## Validation

- `scripts/test-drive.sh`: the in-OS loop above, plus `:drive` with no
  program loaded and a run that ends as driven.
- `scripts/test-play-bridge.sh`: the desktop driven through the bridge
  with a stand-in curl, three steps typed as `:fs-ls /`, three records,
  the request bodies carrying `task` and `desktop`.
- `crates/agel-play`: the block parser and the serial reader as before.
- The kernel's lints on x86-64 with native graphics; `kernel.bin` is
  254193 bytes of the 260096 budget.
- By hand: the live transcript. The full local regression and CI.

## Not claimed

- Perception beyond the status line and the focused window's shades: no
  terminal history, no window text, no panel.
- Actions beyond the desktop's command lines: no keys to windows, no
  pointer, no text into a process's console.
- Any gate on what the program types beyond the three refusals.
- Calibration or task success in general: one task, one run, by hand.
- A task the program holds itself, or memory across runs.
