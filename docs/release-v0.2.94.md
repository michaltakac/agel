# Agel v0.2.94: DOOM by judgment, measured and labeled

The third rung of the order of work: the game as the experiment. The
judge program uses more of what the model says, the judge sees the run's
history, a recorded episode can be handed back to the model to label
step by step, and a scorer reads what the engine's state lines say. One
forty-step run of each policy is reported, honestly, in
[`doom.md`](doom.md); nothing is claimed about play in general.

## What is new

- **`doom-agent-judge`** uses the model's risk score — a lethal risk
  while hurt backs off — and turns when a forward step did not move the
  player, the turn counting only after a forward; the reason recorded is
  still the model's answer line. Its state lives in one form so the
  program fits the desktop's cell table.
- **History for the game's judge.** The bridge sends the last eight
  steps, keys and state line each, with the reading that an unchanged
  position under forward means a wall and a changed angle means the turn
  is done. Without that reading an earlier run turned right nineteen
  times at one wall.
- **`agel-play --judge-dataset steps.jsonl`**: the model as a labeler.
  Every recorded step is judged after the fact — was holding those keys a
  good move, how was the player faring — with the same provider and
  audit, into `judged.jsonl` beside the dataset, with a summary. Nothing
  is booted.
- **`scripts/doom-score.py`**: distance, steps without moving, kills,
  health, ammo and the keys held, read off a dataset; the mean of the
  model's labels when a judged file is given.
- **A provider fix.** A `score` is a position among levels, from 0 to
  one less than their count; the provider clamped it to 1000 as if it
  were a probability, so a three-level question could never report its
  third level. `level_thousandths` reads it as documented. The judge
  written in Agel never had the bound.

## The experiment

Forty steps of E1M1 each, by hand, the judged run 56 s with the boot:

| forty steps | scripted | judged (`jev-1.13.0`) |
|---|---|---|
| distance, map units | 202 | 783 |
| steps without moving | 33 | 5 |
| kills | 0 | 0 |
| health | 100 → 100 | 100 → 100 |
| ammo | 50 → 46 | 50 → 50 |
| keys held | left ×15, up+fire ×13, right ×12 | up ×37, left ×2, right ×1 |
| the model's "good move", mean | 372 | 607 |
| the model's "faring", mean, thousandths of a level | 1112 | 1558 |

The judged run walked the corridor, the model answering `forward` all
forty times and never seeing an enemy; the three turns were the
program's reflex at the wall. The scripted run turned against a wall for
most of its steps and fired at nothing. Neither killed anything. The
model grades its own run higher; it is the same model grading both.

```text
$ agel-play --policy jev --steps 40 --out target/doom-runs/jev-40
…
$ agel-play --policy jev --judge-dataset target/doom-runs/jev-40/steps.jsonl
agel-play: judged step 1: (up) good 590 faring 1350
…
agel-play: judged 40 steps: mean good 607 thousandths, mean faring 1558 thousandths of a level; the judgments are in target/doom-runs/jev-40/judged.jsonl
$ python3 scripts/doom-score.py target/doom-runs/jev-40/steps.jsonl
target/doom-runs/jev-40/steps.jsonl: 40 steps, 39 with a state line
  distance 783 map units, still 5 steps, kills 0, health 100 -> 100, ammo 50 -> 50
  held: (up) x37, (left) x2, (right) x1
```

## Validation

- `crates/agel-model`: `level_thousandths` beside `thousandths`, a
  score of 1.443 levels written as 1443.
- `scripts/test-play-bridge.sh`: the echo episode's dataset judged with
  a stand-in curl — four steps in, four judgments out with `good` 800 and
  `faring` 1200, the bodies carrying the keys held — and the scorer over
  the same dataset; the judged and desktop episodes as before.
- By hand: the two forty-step runs and their labels above. The full
  local regression and CI.

## Not claimed

- Any result about play: one run each, the same map, no enemies met,
  no kills by either; the differences are one sample.
- Any use of the labels. `judged.jsonl` is written and summarised;
  nothing trains on it or reads it back into a policy.
- The model's labels as ground truth: they are its opinion of moves it
  or the scripted agent made, from a state line and a frame.
- Fire when an enemy is seen: the program holds fire with forward when
  `foe` passes 500, and in this run it never did.
