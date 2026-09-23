# Agel v0.3.2: a plan followed, and a tool the agent built for itself

Two rows of the plan. The player follows a plan step by step, each step
a pair of typed questions. And an agent asks a model to write a tool,
the desktop admits it through a judged gate cell by cell, and the tool
runs beside the agents that asked for it, painting what it reads. Any
program in the world with a step is an agent now, not only the ones the
desktop carries.

## What changed

- **Any program is an agent.** `:agents` finds agents by their cells: a
  cell `NAME-0` names a program, and `NAME-step` bound makes it an agent.
  A program joined from a file counts like one the desktop carries.
- **`:join-file NAME`, through the gate.** The forms of the file NAME
  join the world as cells `NAME-N`, each having passed the judged gate
  first: the desktop asks the judge, per form, whether this Agel cell
  may run on the desktop, where a program can read and write files and
  paint rectangles and nothing else; one no refuses the whole file
  (`GATE REFUSED FORM N OF NAME`). The desktop's own questions are
  numbered from 900001, past any program's, and go out on the same line
  the bridge answers programs on.
- **`(write "NAME" "SPEC")`.** A request the bridge serves with its
  planner: the model is asked for a program in the native evaluator's
  dialect, told the builtins, the limits, and that every definition must
  start with `NAME-` and define `NAME-needs` and `NAME-step`; the lines
  that are forms arrive as `:page NAME` lines and the desktop writes
  them to the file NAME. The model's whole answer is kept beside the
  run. Without a planner: `written NAME 0 no model`.
- **The `builder` agent** (`bd-`, three cells) asks for a chart of the
  player's route and seen share, painted as rectangles from the file
  `summary`, then types `:join-file chart`. A chart that passes the gate
  is an agent from then on: it needs `summary`, and each step paints
  the last ten route lengths as bars. The world's scene is pulled and
  shown after every `:agents` step, so a tool is seen as it runs.
- **The plan, step by step.** The player keeps the plan's current step
  (its K-th line, found by scanning the file's newlines) and every
  fifteen steps asks two typed questions: whether that step is done,
  given the state, which advances the step, and where to head for it,
  a heading for ten steps. Thirteen cells.

- **A join replays nothing.** `:join` and `:join-file` evaluate each form
  into the live world and append its cell; before, a join replayed the
  whole workspace into a fresh world, which reset every running agent's
  state. The first live run found it: the chart was admitted, the world
  replayed, the builder forgot it had built and asked the model again,
  ten times in 120 steps. A form that fails to evaluate stops the join
  there, with the cells before it kept and the failure reported.
- **The call depth is 96** (48 before), and `text-field` splits on any
  whitespace, newlines included. Recursion is the language's only loop,
  and a scan over a plan file's bytes for its K-th line ran out of depth
  at the first line; 200 overflowed the evaluator's stack instead (a
  level's frame is large), so the bridge writes each plan step as
  exactly eight fields and the player reads step K by field arithmetic,
  no scan at all. Before the split change, the last word of one line and
  the first of the next were one field.
- **The bridge stops on `STEP FAILED`** from the agents loop, as it did
  on the drive and play loops' failures.

## Measured

Two live runs (`agel-play --policy jev --agents --join builder --join
lookup --plan claude --steps 120`), Claude writing and Jev judging:

- **The first**, before the join was made live. Claude wrote fourteen
  forms (its answer, kept beside the run, opened with a code fence the
  bridge dropped). The builder typed `:join-file chart`, and the desktop
  asked Jev fourteen times whether the cell may run here: every answer
  between 530 and 740 thousandths, admitted. Then the join replayed the
  world and the builder, reset, asked again; ten joins in the run, some
  hundred gate questions, and one refusal, at 470 on a cell of the last.
  The chart never stepped: Claude declared its needs as `summary` and
  `chart-series`, and `chart-series` is a file only the chart itself
  would write. A tool admitted by the gate can still wait forever on a
  need of its own making. The player, meanwhile, took the route from
  4416 to 1984 map units, the furthest yet.
- **The second**, after, with the spec telling the model the chart's
  only need is `summary`. Claude wrote nine forms; the desktop asked Jev
  nine times and every answer was between 570 and 710 thousandths: the
  chart joined through the gate, once, in the second loop step, and
  stepped from then on beside the player, the reviewer, the lookup and
  the builder (`chart drew bars from summary` on its console each step).
  What it drew: the first ten numbers of the summary line as bars, not
  the route's history the spec asked for, which is what a model's first
  program is. The player: the route from 4416 to 1632 map units, health
  100 throughout, the level's lines seen from 6 % to 32 %, the furthest
  of any run. The plan from the lookup (the page still a bot check, the
  plan from memory) was followed by typed questions as it went.

## Validation

- `scripts/test-drive.sh`: a two-form file joins after two yeses from
  the judge, a one-form file is refused on a no, and the joined program
  is an agent the loop steps.
- `scripts/test-play-bridge.sh`: the builder episode, with a stand-in
  model that prints the chart fixture and a stand-in judge that admits
  every cell: five cells written, `:join-file chart` typed by the
  builder, the chart stepping beside the player.
- The full local regression and CI.
- By hand: the live run above.

## Not claimed

- That a model writes a working tool on the first try: the live run
  says what it wrote; a rejected file is reported and not retried, and
  the retry with the error in hand is a row.
- That the gate is a proof: it is the judge's yes, per cell, on a
  question about what the cell does, with the cell's text; a cell can
  say one thing and do another within what a program can do here, which
  is read and write files and paint. The effects a program can reach
  are the bound, not the gate.
- That the plan's steps are right: they are followed as typed questions,
  and the judge's answers decide.
