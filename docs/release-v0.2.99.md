# Agel v0.2.99: agents side by side, and one that changes the other

The v0.2.98 notes said what the DOOM agent lacked: every change that
made it play better was made from outside, by a person reading its
metrics. This release is the mechanism for the agent to do that from
inside, in Agel, and the loop that lets agents run beside each other
with their needs declared. DOOM is the first instrument; the loop, the
needs and the reviewer are general, per [`real-work.md`](real-work.md).

## What changed

- **One loop, three shapes.** `:drive`, `:play` and the new `:agents`
  are one loop in the kernel with a mode. `:agents STEPS [HOLD]` steps
  every program in the world that defines `NAME-step` (NAME its cell
  prefix without the dash: `dj`, `rv`, `dk`), round-robin, one bounded
  step each, and ends when every agent has said `done` or the steps run
  out. With `(window)` among its needs an agent steps as the play loop
  does (the game paused, the focused window looked at, keys held for
  `hold` passes); otherwise as the drive loop does (the desktop looked
  at, a command line typed). Between steps a line at the prompt is
  heard: a sentence becomes the task the requests carry, `:stop` ends
  the run. Cooperative concurrency in one supervisor thread: a step that
  blocks the machine is the same bug it was before.
- **Needs.** `(NAME-needs)` returns a list of facts the desktop checks
  without asking anyone: `(file "NAME")` present, `(window)` a listening
  window focused, `(process)` a process running, `(done NAME)` another
  agent finished, `(after SECONDS)` the clock past a mark since the run
  began. An agent whose needs are unmet waits, and the loop says which
  need. A watch and a scheduled job are agents whose needs are their
  trigger. A fact only the judge can settle is asked by the agent in its
  own step and defined as a need, so the loop never waits on a reply.
- **`:join NAME`** joins a program the desktop carries to the world
  beside what is in it, as a sentence joins the desktop agent. The world
  holds forty cells now (sixteen before): the workbench, a player and a
  reviewer fit together.
- **The reviewer, `review` (`rv-`).** An agent whose need is the file
  `summary`, the one line the DOOM agent rewrites each step (its state
  line). It reads the line every step, counts for itself the steps in
  which the player did not move and how far the route to the exit has
  shrunk, and every twenty steps decides: a route shorter by two hundred
  units or more is progress and nothing is asked; otherwise it asks Jev
  whether the player has stalled, and if so which of the player's own
  cells should change (the follow longer or shorter, the heading's
  tolerance wider, or nothing), redefines that cell in the shared world
  (`follow-steps`, `facing-tolerance`), and appends what it changed and
  why to `notes`. Jev decides what the state is; code decides what
  follows; no model writes anything. Twelve cells.
- **The DOOM agent** exposes the two tunables as cells, writes `summary`
  beside `metrics`, and defines `dj-needs` (`(window)`, `(process)`) and
  `dj-step`, so it is an agent for `:agents`. Twenty cells.
- **`(file-read NAME FROM)`** reads from an offset, or the last `-FROM`
  bytes when FROM is negative: how a program reads the tail of a file
  larger than one reply.
- **The bridge** records `agents: step N NAME keys|do ...` lines as it
  records play and drive steps, and `agel-play --agents` joins the
  reviewer and runs `:agents` instead of `:play`.
- **Every request a step makes is relayed**, up to four within the step:
  a second question that depends on the first's answer (the reviewer's
  "what to change" after "stalled") is served before another agent's
  step can overwrite the world's one pending request.
- **A cell is under 256 bytes, and balanced.** A reviewer cell of exactly
  256 bytes with one parenthesis short loaded as "unclosed list" and the
  join said only `workspace replay rejected at cell 26`; the cell-by-cell
  harness in the notes of v0.2.98 found it. Forty cells hold the player
  (twenty) and the reviewer (fifteen) together.
- **Underneath.** The kernel image passed its 260096-byte budget with
  the loop and came back under it by keeping small helpers out of line
  (`StatusLine`, the console tee, the loop's helpers) and by making the
  empty TSS all-zero so it lives in `.bss` (its I/O bitmap base was the
  one non-zero field, set at install now): 258056 bytes. The handover a
  driving program asks for is performed by the loop that decided it, in
  one function the `:handover` command shares.

## Measured

One live run of 120 loop steps, the player and the reviewer side by
side against the endpoint (`agel-play --policy jev --agents --steps
120`), reported as it went:

| the player, 120 steps | start | end |
|---|---|---|
| route remaining, map units | 4416 | 2304 |
| health | 100 | 88 |
| seen, share of the level's lines | 6 % | 20 % |

The reviewer stepped 120 times beside it (its needs met from the first
step: the summary file exists once the player has written a line),
reviewed at its 20th, 40th, 60th and 80th steps and found progress by
arithmetic (the route shorter by two hundred units or more), asking
nothing. At its 100th step the route had not shrunk (2688 → 2720) and
the player had moved in every step, so code could not call it a stall
and asked Jev, which said stalled at 560 thousandths; asked what to
change, Jev chose `nothing` at 590 (follow-longer 210, follow-shorter
160, tolerance-wider 40). At the 120th step, the same: stalled at 740,
`nothing` at 490. The reviewer changed no cell in this run; `notes`
holds `review: progressing` four times and `review: nothing` twice. The
loop from a task's own metrics to a typed question to a redefinition is
closed and ran with nobody at the keyboard; the judge's answer, twice,
was to leave the player as it was. Whether that was right is not
claimed; the mechanism is. The same run's player, on the same route as
v0.2.98's, reached the same corridor.

Two earlier runs of the same milestone taught two things. In the first,
the reviewer asked whether the player had stalled after forty steps in
which the route did not shrink, and Jev said 460; the arithmetic already
said stalled, so the reviewer now asks the judge only when the numbers
cannot tell (progress absent but movement present) and asks only what
to change when they can. In the second, the reviewer's second question
(what to change) was lost: the world holds one pending request, and the
player's next step overwrote it before the loop relayed it; the loop
now relays every request a step makes, up to four, within the step.

## Validation

- `scripts/test-drive.sh`: `:agents` with a program that defines its
  step and needs at the prompt: it waits on a file that is not there,
  steps when the file is, and the run ends on `done`; the drive, play
  and handover cases as before, through the one loop.
- `scripts/test-play.sh`, `test-play-bridge.sh`, `test-native-workbench.py`:
  the play loop and the handover as before.
- The full local regression and CI.
- By hand: the live run above.

## Not claimed

- Any change the reviewer makes being an improvement: it changes what the
  judge says to change, and the measurement says what it did once.
- Adaptation beyond two cells: the reviewer's menu is the player's two
  tunables; a new question or a new reflex is a model's proposal through
  the gate, the plan's next rows.
- Preemption: an agent's step runs to its end; the loop is cooperative.
- A judged fact in the loop itself: the plan named `(judged QUESTION)`;
  the agent asks in its step instead, which is smaller and the same.
- Voice, touch, a network: rows in [`real-work.md`](real-work.md).
