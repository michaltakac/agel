# Real work on an agentic OS

Why this document: DOOM is the instrument, not the point. A person asked
that goal-following, instructions, decisions and self-change be built
generally, so a person can drive agents to do real, important, possibly
confidential work through the same OS, by mouse, touch, voice or a
sentence at a prompt, and so that the agents can shape the OS to the
work. This document is the research behind that, the general model it
leads to, a ranking of where such an OS is worth using, and the
examples and experiments the next milestones build. Nothing here is a
claim about the code; the ledger of what exists is in
[`parallel-agents.md`](parallel-agents.md) and [`roadmap.md`](roadmap.md).

## What others found

**AgentRun (Grep.ai, September 2026).** A harness for repetitive
knowledge work in regulated settings, built on Pi's loop and on Jev.
Its findings, as they bear on us:

- A tool call is two things: a *decision* (which action, which
  arguments) and an *action* (the code that carries it out). Nothing
  about the decision needs a text generator. Followed through a whole
  procedure, a job is mostly decisions and mechanical steps, with a few
  places where someone has to go and look. So a program has three kinds
  of step: an agent where something must be found or done in the world,
  a typed question where something must be decided, plain code where the
  step is mechanical; and a handful of shapes that hold them (chain, map
  and reduce, loop until, escalate).
- **Jev decides what a thing is; code decides what follows.** Every
  decision names the questions and probabilities it rests on, so it is
  auditable, and the answers are kept, so a change to the code that
  reads them replays over a thousand past cases in minutes with no
  model call.
- **The agent does the job once the expensive way, writes down what it
  would do faster, then writes the program** (in their DSL) and routes
  the ordinary cases to it, keeping itself for the odd ones. Cost per
  alert fell in steps, one per workflow version, from $2.89 to $0.25;
  accuracy rose, because each version removed a place the agent could
  wander. Learning notes are one to three sentences about the *job*,
  not the case.
- Every step has one model, one input contract, one output schema, so
  it is evaluable alone; a candidate goes live only after beating the
  current one on a development set, with a reserved set the author never
  sees. Model governance is answered by construction: which model, on
  what inputs, under which policy version, and show me.
- Four agents with one job each: research (finds), judge (decides the
  hard case), report (renders, never re-decides), operator (the only one
  that writes outside the sandbox, under the tightest grants, every
  action on the trace).

**Pi (earendil-works).** An agent harness: a loop with tool calling and
events, extensions that hook the lifecycle, skills that load on demand,
and a durable runtime (Pico) for conversations, tasks and documents.
What bears on us:

- The loop is an event sequence with named boundaries (`before_agent_start`,
  `tool_call`, `tool_result`, `turn_end`, `agent_before_settle`), and
  extensions register handlers that observe, transform, block or
  continue. Tools declare whether they may run in parallel; one
  sequential tool in a batch makes the batch sequential.
- Durability: a session commits entries, task records and documents
  atomically; only committed state is observable; external effects never
  run inside the commit. Every task is a durable state machine attached
  to a conversation; conversations fork.
- Skills are instructions with supporting files, advertised by name and
  description and loaded only when the task calls for them.
- No built-in permission system: isolation is the container's job.

## What this OS already is, and what it lacks

Agel has the seam AgentRun describes, in the language: a judgment is a
typed question (`agel/judgment`, `judge-request`, the `(judge ...)` form
the desktop relays), effects are declared and gated (`--gate agel`, the
judged gate of v0.2.92), agents are behaviors over typed protocols with
mailboxes, deterministic scheduling, isolated heaps and transactional
failure (`agel/agents`), and every effect is a committed input in the
image's log, replayable. The desktop is a live world of cells a program
can redefine. Pi's durability invariants are close to what the
world-file and the image log already give, and Pi's lifecycle hooks are
what the desktop's loops lack: nothing observes a step but the loop
that runs it.

What it lacks, for real work: a general notion of a *task* with a goal,
instructions, what it needs, its steps, its metrics and its notes;
agents that run beside each other and say what blocks what; triggers on
facts (a file appeared, a reply arrived, the clock passed a time, a
window closed); the agent changing its own cells from its own metrics;
programs installed from inside; the route of a job written down as a
program of cheap steps once an agent has done it the expensive way; and
input beyond a keyboard and a mouse.

## The general model

One vocabulary, for DOOM and for an alert review alike. Every word is
an Agel value or form; nothing is a special case in the kernel.

- **A task** is `(task NAME GOAL INSTRUCTIONS)`: a goal sentence, the
  instructions as a list of typed steps, a metrics file, a notes file.
  The desktop makes one from a sentence at the prompt, a click on a
  dock item, a spoken sentence, or another task.
- **A step** is one of three kinds, AgentRun's seam: `(decide QUESTION)`
  asks Jev a typed question over the task's state and stores the answer
  and its probability beside it; `(do FORM)` is code or an effect, the
  thing that carries a decision out; `(find AGENT GOAL)` hands a sub-goal
  to an agent that must go and look (a browser, a file walk, a process).
  Shapes hold them: `(chain ...)`, `(each LIST STEP)` runs a step over
  every item, `(until PREDICATE BOUND STEP)`, `(escalate TO)`.
- **An agent** is a program in the world that defines `NAME-step` and
  `NAME-needs`. The desktop's scheduler steps every agent whose needs are
  met, one bounded step each, round-robin, and hears the prompt between
  steps. This is cooperative concurrency in one supervisor thread; a
  step that blocks the machine is a bug, as before.
- **A need** is a fact the desktop can check without asking anyone:
  `(file NAME)`, `(window TITLE)`, `(process)`, `(done AGENT)`,
  `(after MS)`, `(reply N)`; or `(judged QUESTION)`, a typed question the
  desktop asks Jev once and caches as a fact. That last one is how the
  System One model says what may run beside what: an agent whose needs
  include `(judged (noul "May this step run while the operator's step
  is running?"))` waits or runs on the answer, and the answer is kept.
- **A watch** is an agent whose needs are its trigger: a program that
  says `(needs (file "inbox/new"))` runs when the file appears, and one
  that says `(needs (after 3600000))` runs on the hour. Hooks and
  scheduled jobs are the same thing as agents, which keeps the kernel
  small and the audit trail one trail.
- **Metrics and notes** are files the task appends: one line per step
  (state, decision, answer, probability), and one to three sentences
  about the job when a step learned something. They are what the
  reviewing agent reads.
- **Self-change** is an agent (`review`) whose needs are the metrics
  file, whose step reads it back, asks Jev typed questions about it (is
  progress stalled; which reflex fired most; should a bound be longer;
  is a question missing), and redefines cells of the task's program with
  `eval`, writing what changed and why into the notes. A change no typed
  question can answer, such as a new question or a new reflex, goes to a
  model once, through the judged gate, as a proposal of cells. This is
  AgentRun's "fire itself" in the language: the expensive agent runs the
  job once, the program it writes takes the ordinary cases.
- **Sync and async** are declared by needs, decided by Jev where they
  are not declared, and enforced by the scheduler. An operator step,
  one that writes outside the OS, is sequential by declaration, the way
  AgentRun's operator agent runs under the tightest grants.
- **Input** is one channel with many sources: a click on a dock item
  that names a task, a tap on a touch display (the same absolute
  pointer the vmmouse path made), a spoken sentence transcribed on the
  host and typed to the prompt by the bridge, a sentence typed. Each
  becomes `(task ...)` the same way. Voice and touch are not built; they
  are rows.

## Where this OS is worth using, ranked

Ranked by fit to what the OS is: no network by default, every effect
logged and replayable, a judge that answers typed questions in
milliseconds, a language that can rewrite itself, a person at the
screen who can take over. Highest first.

1. **Confidential document work on a machine that is offline by
   choice.** Contracts, medical records, personnel files, legal
   discovery: a folder of documents in the data region, a procedure as
   typed questions, an agent that sifts and picks and writes the record,
   nothing leaving the machine but what the operator step is granted to
   send. The absence of a network stack is the feature. Fit: highest;
   the judge's questions are the rubric a reviewer reads.
2. **Case triage at volume, the AgentRun shape.** Alerts, tickets,
   claims, applications: thousands of cases, each a little different,
   95 % clearable by the same questions, the odd one escalated to a
   person or a bigger model. Fit: high; the OS adds the audit trail and
   the replay for free, and the scheduler runs the cases side by side.
3. **Operations watches.** A log, a metrics file, a queue: a watch
   agent whose need is the fact, a typed question on what it sees, an
   action under a grant. DOOM's metrics file is an instance. Fit: high
   for the mechanism, medium for the market until the OS has a network.
4. **A person's own desk.** "Every morning, list what changed in these
   folders and draft the note"; "when the download finishes, sort it";
   the browser's forms filled by a judged agent. Fit: medium; it is the
   demo the video already shows, and it needs voice and touch to be what
   a person expects.
5. **Playing and driving software** (games, simulations, legacy GUIs)
   as a way to learn a job the expensive way and write it down. Fit:
   medium; the instrument, not the work.

Not a fit today: anything that needs the network from the OS (fetch is
through the host's bridge only), files beyond the region's limits, or a
model in the OS (inference is a host provider).

## Examples and experiments

Each is a program of cells over the general model, with a measurement.
The first two exist as rows in the plan; the rest are what follows.

- **DOOM, reviewed by itself** (v0.2.99). The judged agent plays; the
  `review` agent reads `metrics`, asks Jev whether progress stalled and
  which reflex fired most, and redefines the follow's length or the
  goal's tolerance; the notes say what changed. Measure: route length
  before and after the agent's own change, nobody at the keyboard.
- **A folder triaged, offline** (v0.3.x). Twenty-four records in the
  data region, a procedure as typed questions written once (could this
  record stop the case; is it our subject), an agent that opens only
  the records the questions leave open, a record that names the one
  that decided it and why the others stayed closed. Measure: records
  opened, questions asked, answer kept per record, the replay of the
  verdict under a changed rule with no agent run.
- **A watch on the desk.** An agent whose need is a file appearing in
  the region, a typed question on its first line, an action; and one
  whose need is the clock, hourly. Measure: the trigger fires once per
  fact, never twice, and the action is on the trail.
- **A job learned and written down.** The desktop agent does a task
  the expensive way (every step a judged choice), writes notes about
  the job, and then writes the task as a chain of `decide`/`do` steps
  into cells; the next run of the same sentence routes to the program
  and asks the judge a third of the questions. Measure: questions per
  run, steps per run, the same outcome.
- **Sync decided by the judge.** Two agents, one of which writes a
  file the other reads; neither declares the order; a `(judged ...)`
  need asks whether the reader may run before the writer is done, and
  the scheduler holds it. Measure: the answer kept, the order held.

## Principles that hold across all of it

1. Jev decides what a thing is; code decides what follows; a model
   writes only what no typed question can answer, once, through the
   gate.
2. Everything an agent does is a committed input in the log; the
   decision names its questions and probabilities; a rule change is a
   replay, not a rerun.
3. An agent that can change its cells can change the job; the OS
   changing itself is staged behind gates ([`parallel-agents.md`](parallel-agents.md)).
4. One vocabulary for every source of intent: a sentence, a click, a
   tap, a voice, another agent.
5. Nothing is claimed until its row is done, with a measurement.
