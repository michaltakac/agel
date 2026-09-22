# Agents with a goal, side by side

The demo at v0.2.97 showed Jev playing DOOM when asked, and showed what
that play was: sixty judged steps, fifty-four of them forward, into a
wall and then along it. The agent had no goal, no map, nowhere to look
anything up, and the desktop's loop ran nothing else while it played. A
person watching said so. This document is the plan that answers it, and
the ledger of what each milestone actually delivers. Every row starts as
open; a row is marked done only with the proof beside it.

## What was wrong, mechanically

- **No goal.** The DOOM agent's questions were "best next move", "an
  enemy in view?" and "how dangerous?". Nothing in the program or the
  questions knew where the level's exit was, or that reaching it was the
  point. Forward was the confident answer to a question that had no
  better one.
- **No map.** The engine reported the player's position and angle, but
  not what it had seen of the level, nor where anything was. The game's
  own automap (the Tab key) was never pressed.
- **No lookup.** The OS has no network. The browser written in Agel reads
  pages from the data region. A walkthrough of the level, which the
  internet has in dozens of copies, was out of reach.
- **Nothing beside it.** `:play` and `:drive` are loops the supervisor
  runs to their end; while one runs, no other program steps and no other
  sentence is heard. A second agent building a tool for the first could
  not exist.
- **No metrics of its own.** The steps were logged on the host by the
  bridge. On the OS, the agent kept nothing it could look at.

## The principle for deciding

Jev, TypeSafe's System One, answers typed questions: a choice among named
options, a yes/no, a score on a named scale, each with a confidence. Every
decision in these milestones that can be posed that way is posed that
way, and the program in Agel holds the policy around the answers. A large
language model is used for exactly two things, both of which produce a
typed artifact from free text and neither of which is on the step path:

1. reading a fetched walkthrough and writing a plan of at most eight
   typed steps, once per task;
2. writing an Agel program when a sentence asks for a tool, once per
   sentence, with the result going through the judged gate on effects
   before it runs.

Geometry (which way the exit is, how far, whether the player moved) is
arithmetic in the program. Neither judge nor model is asked what a number
already says.

## The milestones

### v0.2.98 — a goal for the game

The engine knows its level. At each state line it now reports the exit:
the midpoint of the level's exit line, as a heading in the same 0–255
units as the player's angle, a distance in map units, and the percentage
of the level's lines the player has seen, which is what the automap
draws. The judged agent steers toward the heading with arithmetic and
asks Jev only what the picture holds: what is directly ahead (open, a
door, a wall, an enemy, a drop), whether an enemy is in view, how
dangerous it is. A wall ahead on the goal's heading starts a wall-follow
of bounded length; a door ahead is used; being stuck turns. Every
twentieth step, or when stuck twice, the agent presses Tab, asks Jev
which quadrant of the automap has unexplored space, sets a detour
heading, and presses Tab again. The agent appends one line per step to
`/data/metrics` on the OS: step, x, y, distance to the exit, health,
action, confidence. A measured run reports the distance at the start and
at the end and whether the level changed.

### v0.2.99 — agents side by side, and what blocks what

`:agents` replaces the one-loop-at-a-time desktop. Each program in the
world that defines `NAME-step` and `NAME-needs` is an agent; the desktop
steps the ones whose needs are met, one step each, round-robin, and
keeps hearing the prompt between steps. A need is a fact the desktop can
check without asking anyone: a file present in the data region, a window
listening, a process running, another agent finished, a reply arrived.
A second sentence at the prompt while the first agent plays summons a
second agent beside it. The first tool built this way is the metrics
window: an Agel program that reads `/data/metrics` and paints its
distance-to-exit and health as bars in its own window, needing only that
the file exists. This is cooperative concurrency in one supervisor
thread: an agent's step is bounded, and a step that blocks the machine
is the same bug it was before.

### v0.3.0 — a lookup through the host, a plan from it, a tool it wrote

A program asks `(fetch URL)` through the same request the judge answers;
the bridge on the host fetches, reduces the page to text, and delivers
at most a bounded number of lines, which the desktop writes to a file in
the data region. With a model provider configured, the bridge asks the
model once for a plan of typed steps from the page and delivers it as
lines the program reads; without one, the walkthrough stays a page and
the agent keeps the engine's heading. The play agent's questions gain
"which step of the plan is this?" and "is this step done?". A sentence
that asks for a tool ("build yourself a chart of your play") reaches the
model through the bridge, and the program it returns is loaded through
the judged gate, into cells, as any typed program is.

## Ledger

| Item | State | Proof or reason |
| --- | --- | --- |
| The engine reports the exit's heading, distance and the seen fraction | open | |
| The judged agent steers to the exit; Jev answers what is ahead | open | |
| The map is checked on Tab and Jev names the unexplored quadrant | open | |
| Metrics appended per step on the OS | open | |
| `:agents`, needs, and a second sentence beside a running agent | open | |
| A metrics window painted by an Agel program from the file | open | |
| A fetch through the bridge into the data region | open | |
| A plan of typed steps from a page, through a model, once | open | |
| A tool written by a model, gated, loaded, run | open | |
