# Agel v0.2.72 — Agel plays

The sixth rung of *Does it run DOOM?*, in its first form: a hosted agent
plays the game on the Agel desktop through the machine's screen and
keys, decides through Agel's model-provider effect, and leaves a dataset
of every step.

![DOOM paused in its window while Agel decides, the engine's state lines in the terminal, at v0.2.72](images/native-desktop-v0.2.72.png)

## What changed

- **`crates/agel-play`**, the agent: it boots the desktop image, starts
  the engine from the workshop, and steps: pause, read the engine's
  state line, capture a frame the compositor was not mid-way through
  (two captures that agree), render the window as eighty by twenty-five
  shades, decide, unpause, hold the action's keys. `steps.jsonl` records
  the frame, state, action, reason and ASCII of every step; the frames
  are kept.
- **Policies:** scripted, for tests without a model; Claude and Codex,
  through `agel-model`'s providers (the typed, audited `model/infer`
  effect), the answer parsed into one of ten actions. Run by hand with
  Claude Code: five steps walking north from the start of E1M1, each
  with a reason, the engine's coordinates confirming the walk.
- **The engine's side:** `p` pauses; control, space, comma and period
  carry the engine's own fire, use and strafe keys (a first run fired
  nothing: the engine names them apart from the keys); one heartbeat
  line every ten seconds and one state line per pause. Every console
  line repaints the panel under the window, and thinning them took the
  timed demo from 49 to **170 frames per second** under TCG.
- **The desktop** repaints only the canvas for a window in front whose
  one record is its blit, so no moment shows the window's surface
  between two of a game's frames.
- **v0.2.71's CI failed** on the DOOM suite: the runner's emulation is
  slower and the wait for the first frames too short; the waits are
  longer now and the fewer console lines make the start faster.

## Proof

`scripts/test-play.sh` builds everything, runs eight scripted steps and
requires eight dataset lines, the engine's state in them, a fire among
the actions and the last frame on disk. `scripts/test-doom.sh` times the
demo at 170 frames per second. The full regression, both in it, passes.

## Not claimed

The loop is Rust on the host, not an Agel agent in the language; the
model reads shades, not pixels; a decision takes seconds and the game
waits paused for it; no run window on the desktop, no steering, no
speech, no learning yet. The model policies are exercised by hand, not
in CI, which has no provider.
