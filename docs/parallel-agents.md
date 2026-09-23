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

## The principle for adapting

Not every failure of a task is known at its first step. DOOM is the
experiment for exactly this: a person cannot list its failure modes in
advance, and the work of v0.2.98 was one person reading the agent's own
metrics after each run and changing the program by hand — a heading
through walls, a room that was a number, a judge that called the far end
of a room a wall. The system the plan is for does that itself. An agent
keeps its metrics; when they say it is failing (no movement, no progress
on the goal, the same answer for many steps), it asks the judge typed
questions about what to change (which reflex, which threshold, whether
the goal is wrong, whether the picture needs a question it is not
asking), and changes its own cells — the language is live, and a cell is
a form. A change the judge cannot answer as a typed question, such as a
new question to ask or a new reflex to write, goes to a model once,
through the judged gate, as a proposal of cells. What was changed and
why is a line in the metrics beside the steps it changed. Nothing here
is claimed until its ledger row is done.

## The milestones

The order matters, and the first draft of this plan had it wrong: it
put a goal for the game first and adaptation as a later row. A person
then pointed out that every change v0.2.98 made was made from outside,
in C and Rust, by someone reading the agent's metrics at a keyboard,
and that an OS on a device on the internet has no such person. DOOM is
the instrument; adaptation is the milestone. What the game reveals is
what has to be adaptable, from inside, in Agel.

### v0.2.98 — a goal for the game (shipped as a sensor)

The engine reports the way to its exit and what is ahead on every state
line (a route over a grid learned with its own line traversal, three
rays, a door flag, the seen share of the map), and `doom-agent-judge`
steers on it with arithmetic, asking Jev only what the picture holds.
Honest label: a better sensor and a hand-tuned policy. The map reasoning
this put into C is the wrong direction for the plan, and is kept only
as the ground truth the next milestones read; the C side is a sensor,
not a policy, and the route belongs in Agel, over facts the engine
writes once (its lines and sectors as a file) or reports raw.

### v0.2.99 — the agent changes itself, from inside

The loop that adapts needs a place to stand, and everything it needs
exists: `file-read` of its own metrics, `model-request` for typed
questions, `def` and `eval` to redefine its own cells live, `file-append`
to record what changed and why. Each run, the agent reads its metrics
back, asks Jev typed questions about them (is progress stalled; which
reflex fired most; should the follow be longer, the goal wrong, the
question missing), and redefines its own cells accordingly, with the
change written beside the steps it changed. A change no typed question
can answer, such as a new question to ask or a new reflex to write,
goes to a model once, through the judged gate, as a proposal of cells.
Nobody is at the keyboard. The measure is the same live run, before and
after the agent's own change, and the change itself in the file.

### v0.2.100 — room

The disk layout, the kernel's load address and the evaluator's limits
were the first version's; [`release-v0.2.100.md`](release-v0.2.100.md)
raises them so that a program is written for the job, not for the cell.

### v0.3.0 — a sentence is a fact, and the game is played with instructions

The general features, exercised on DOOM and filmed. A sentence heard
while agents run is written to the file `task`, so it is a fact any
agent's needs can wait on and any step can read: the desktop's one input
channel reaching every agent. The DOOM agent reads it and, when it holds
text, asks Jev one more typed question, what the operator asks for (a
look at the map, care, a fight, nothing), and acts on the answer with
its own reflexes; the desktop agent's menu gains `review-doom`, a handover
that joins the reviewer and runs `:agents` instead of `:play`. The
reviewer says its notes on the console as it writes them, and on joining
reads its notes' tail and re-applies the last change it made, so a run
learns from the run before it (AgentRun's notes, one line each). The
film: a click, three sentences, the game started by the desktop agent,
played by the judged agent beside the reviewer, an instruction typed
mid-run and followed, the reviewer's notes as captions, everything the
agents did, tried and changed, from the serial trail alone.

### v0.3.1 — agents side by side, and what blocks what (done at v0.2.99)

`:agents` replaces the one-loop-at-a-time desktop. Each program in the
world that defines `NAME-step` and `NAME-needs` is an agent; the desktop
steps the ones whose needs are met, one step each, round-robin, and
keeps hearing the prompt between steps. A need is a fact the desktop can
check without asking anyone: a file present, a window listening, a
process running, another agent finished, a reply arrived. A second
sentence at the prompt while the first agent plays summons a second
agent beside it. The first tool built this way is the metrics window: an
Agel program that reads `metrics` and paints its route length and health
as bars in its own window, needing only that the file exists. This is
cooperative concurrency in one supervisor thread: an agent's step is
bounded, and a step that blocks the machine is the same bug it was
before.

### v0.3.2 — a lookup through the host, a plan from it, a tool it wrote

A program asks `(fetch URL)` through the same request the judge answers;
the bridge on the host fetches, reduces the page to text, and delivers
at most a bounded number of lines, which the desktop writes to a file in
the data region. With a model provider configured, the bridge asks the
model once for a plan of typed steps from the page and delivers it as
lines the program reads; without one, the walkthrough stays a page and
the agent keeps the engine's route. A sentence that asks for a tool
("build yourself a chart of your play") reaches the model through the
bridge, and the program it returns is loaded through the judged gate,
into cells, as any typed program is.

### Changing the OS itself, in stages

An agent that can only change its cells cannot change the OS. The change
protocol in [`architecture.md`](architecture.md) (a proposal, an isolated
build, tests, a canary, an atomic promotion) is a future protocol; today
the supervisor changes only through a commit and a rebuild. The stages,
each its own milestone with its own proof:

1. **Programs from inside.** A gated effect installs a program into the
   program region from the OS (the region is a name table the kernel
   owns; nothing can write it today). The engine's map reasoning becomes
   the first program an agent could replace.
2. **Services and the compositor as replaceable domains.** The kernel
   already replaces a faulted compositor with a fresh one; a proposal
   from inside, gated and judged, replaces a domain with a new image.
3. **The kernel image last.** The size budget and the boot layout make it
   the hardest; the protocol's build and canary stages have to exist
   first.

A model is needed only where a change is free text; its output goes
through the judged gate as a proposal, never straight to a domain.

## Ledger

| Item | State | Proof or reason |
| --- | --- | --- |
| The engine reports the next waypoint toward the exit, the route's length, the seen fraction and three rays | done | v0.2.98: `scripts/test-play-bridge.sh` reads `goal`, `dist`, `path`, `seen`, `free` and `door` off every state line |
| The judged agent steers to the exit on the engine's route and rays; Jev answers what the picture holds | done | v0.2.98: `doom-agent-judge`, the bridge test; one live run of 120 steps halved the route to the exit and opened its first door |
| A sentence heard mid-run is a fact (`task`) any agent reads; the player asks the judge what it asks for and acts | done | v0.3.0: `scripts/test-drive.sh`, the film |
| The reviewer says its notes and re-applies the last change on the next run | done | v0.3.0: `review`, the film |
| The map is checked on Tab and Jev names the unexplored quadrant | done | v0.2.98: after three stalled forwards; not needed in the measured run, seen in earlier cuts |
| Metrics appended per step on the OS | done | v0.2.98: `metrics` in the filesystem region, the state line and the answer per step |
| The agent reads its own metrics and changes its cells when they say it is failing, by typed questions, nobody at the keyboard | done | v0.2.99: `review` reads the player's `summary`, asks Jev whether it stalled and what to change, redefines `follow-steps` or `facing-tolerance`, notes it; one live run: six reviews, two questions, `nothing` chosen twice, no cell changed; the apply path tested |
| The route as an Agel program over facts the engine writes, not C | open | the route is in `doom.c` |
| `:agents`, needs, and a second sentence beside a running agent | done | v0.2.99: `scripts/test-drive.sh`; a line at the prompt between steps becomes the task, `:stop` ends the run |
| A metrics window painted by an Agel program from the file | open | |
| A fetch through the bridge into the data region | open | |
| A plan of typed steps from a page, through a model, once | open | |
| A tool written by a model, gated, loaded, run | open | |
| A program installed into the program region from inside, through a gated effect | open | the region is written only by host scripts |
| A domain replaced from inside by a gated proposal | open | the kernel replaces only a faulted compositor, with its own image |
| The kernel image replaced from inside | open | the change protocol is a description |
