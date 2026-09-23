# Agel v0.3.0: a sentence is a fact, and the game is played with instructions

The general features of [`real-work.md`](real-work.md), exercised on
DOOM and filmed. Nothing here is a DOOM feature: a sentence heard while
agents run becomes a file every agent can wait on and read; an agent
asks the judge what an instruction asks for and acts on the answer with
its own reflexes; the desktop hands a task to two agents side by side;
a reviewer says what it changes and learns from its notes across runs.
DOOM is the instrument that shows each of them working.

## What changed

- **A sentence is a fact.** A sentence heard between steps of `:agents`
  is written to the file `task` as well as becoming the task line every
  request carries (a window's requests too now, so the judge reads the
  operator's words beside the picture). `(needs (file "task"))` is a
  watch on the operator; `(file-read "task")` is how a step reads it.
- **Instructions, judged.** The DOOM agent reads `task` when it asks the
  judge and, when the file holds text, adds one typed question: what the
  operator asks for, among a look at the map, care, a fight, nothing.
  The answer is acted on with the agent's own reflexes (the map on Tab,
  care lowering the risk it backs off at and stopping the firing, a
  fight firing at anything in view), said on the console as
  `doom: instruction WHAT for: SENTENCE`, and the file is cleared. The
  agent is eleven cells now that a cell is 896 bytes; the same program,
  written for the job.
- **`review-doom`** in the desktop agent's menu: `:handover NAME STEPS
  agents` loads the player, joins the reviewer and runs `:agents`, the
  two side by side, where `play-doom` runs the player alone.
- **The reviewer says and remembers.** It says each note on the console
  as it appends it to `notes`, and on its first step reads the tail of
  `notes` and re-applies the last change it made in an earlier run, so a
  run learns from the run before it (AgentRun's notes, one line each).
- **A sentence said during a reply's wait is kept.** The loop spends
  most of its time waiting for the judge's reply, and that wait read
  every serial line and dropped the ones that were not a reply; the
  first film's fourth sentence went that way. A prose line arriving
  during a wait is kept now and heard between steps.
- **The bridge is a console.** `agel-play --attach` forwards its standard
  input to the serial line, the operator's other keyboard, so a sentence
  can be said while the desktop's own keyboard belongs to the game's
  window. The recorder says the fourth sentence that way.

## The film

[`media/agel-agents-demo.mp4`](media/agel-agents-demo.mp4) (158 s;
[a GIF](media/agel-agents-demo.gif) beside it), recorded live by
`scripts/record-demo.py` from the desktop's own mouse and keyboard and
the bridge's console, with nothing staged. What it shows, from the
serial trail alone:

1. A click on the desktop opens the workbench.
2. "list the files on the disk, then finish": the desktop agent lists
   them (`files` at 950 thousandths) and stops (`done` at 820).
3. "hi Jev, play DOOM and review yourself as you go": the desktop agent
   starts the game (`start-doom`, 570) and, once it runs, hands the
   desktop to the player and the reviewer side by side (`review-doom`,
   390 against `play-doom` at 130): `:handover doom-agent-judge 120
   agents`.
4. The player walks the route the engine reports; the reviewer notes
   `progressing` at its 20th step and `nothing` at its 40th, the judge
   having been asked and having chosen no change.
5. "check the map and be careful", said at the console while the game
   holds the keyboard: `agents: heard`, the file `task`, and the
   player's next request carries one more question. Jev answers `map`;
   `doom: instruction map for: check the map and be careful`; the
   player presses Tab, asks which quadrant of the automap is unexplored,
   walks that way for six steps and presses Tab again (four `tab` steps
   in the run).
6. The reviewer: `progressing` three times more, `nothing` once;
   `AGENTS RAN 120 STEPS`.

The player over the 120 steps: the route to the exit from 4416 to 2272
map units, health 100 throughout, ammo 50 to 27 (twenty-eight
forward-and-fire steps at what the judge called an enemy), the level's
lines seen from 6 % to 24 %, one door used. The same corridor v0.2.98
reached, with an instruction obeyed on the way and a reviewer beside
it. One run, no claim of a rate; the first recording lost the fourth
sentence to the reply's wait, which is the fix in the list above.

## Validation

- `scripts/test-drive.sh`: a sentence said during `:agents` is heard,
  carried as the next request's task line, and present in `task`
  afterwards; the desktop agent's nine options.
- `scripts/test-play-bridge.sh`, `test-native-workbench.py`: the
  handover and the desktop episode with the new menu.
- The full local regression and CI. One regression run saw the bridge
  suite's judged episode report the route on three of its four steps;
  the rerun passed, and the flake is noted rather than hidden.
- By hand: the film.

## Not claimed

- That the judge reads an instruction rightly, or that the player's
  reflex for it helps: one film, reported as it went.
- Instructions beyond the four the player knows: a new reflex is a
  model's proposal through the gate, still a row.
- Voice and touch: the sentence arrives by a keyboard or the console.
