# Agel v0.2.74 — The loop in the OS

The play loop is Agel now, running in the OS. Until this release the agent
that played DOOM was a Rust program on the host; the release before named
that under "Not claimed". The perceive-decide-act cycle is an Agel program
in the desktop's own native evaluator. Rust builds what the language needs
to reach the game and nothing more.

## What changed

- **`boot/desktop/doom-agent.agel`**, the agent: loaded with `:load
  doom-agent` into the native evaluator, run with `:play STEPS [HOLD]`. Its
  forms choose every step: forward and fire where the way is open, a turn
  toward the darker (farther) half of the view when its position has not
  moved, a step back when health is low.
- **Six native words**, the least the language needs: `(look x y)` and
  `(look-mean x y w h)` read a sixty-four by twenty-five grid of shades the
  desktop samples from the played window's canvas; `(look-line)` and
  `(look-field n)` read the program's last console line and the integers in
  it, so the agent reads `doom: state map 1 x 1055 y -3611 ... health 100
  ...` as fields; `(model-request text)` and `(model-result)` ask a model
  and read its answer. They read a copy the desktop placed in the shared
  page, never process memory or a device, and a world given nothing to look
  at answers with an error.
- **`:play`** is the desktop's step: it pauses the game with the key
  `(play-pause)` names, samples the window and copies the state line into
  the evaluator, calls `(play-step)`, injects the keys the returned form
  names into the window's own event queue, holds them for the step, and
  unpauses. A command line still reaches the workshop while the game holds
  the keyboard, because it opens with a colon.
- **A model can decide** through `doom-agent-model.agel`, which asks with
  `model-request` and reads the answer with `model-result`. The desktop
  prints each request and the observation on the serial console between
  `model-request N:` and `model-request end`, and reads the answer back as
  `:model-reply N <action> <reason>`. `crates/agel-play` is now only that
  bridge: it answers each request by calling a provider through
  `agel-model`'s typed, audited `model/infer` effect (a `--policy echo`
  answers instantly, for proving the round-trip without a model). The
  request with its observation and the reply are proven; driving a whole
  model episode is by hand, not yet a tested path.
- **A collector fix:** an empty string in a loaded program made the native
  heap's copying collector call `slice::windows(0)`, which panics; the
  first program to hold one, `doom-agent`, exposed it. Empty text now
  forwards without a window.
- **`printf` fix carried:** the release before taught `printf` `%f`, `%e`
  and `%g`; the agent's reasons use none, but the library keeps them.

## Proof

`scripts/test-play.sh` loads the Agel program, starts the engine, and
`:play 8` steps it, requiring eight `play: step` lines with the keys the
language chose, the engine's state line it read, and a forward or fire
among them. No host policy and no model are in that path: the loop it
proves is the Agel program's, in the OS. The full regression passes, and
the kernel stays within its size budget. A run of the model bridge with
Claude Code on this machine is recorded alongside the release.

## Not claimed

The model call still leaves the machine through the host bridge: the
native kernel has no network and no local inference, so a model decides on
the host, as `deployment-targets.md` places it. The shades are coarse and
a decision takes seconds while the game waits paused, so this is
judgement, not reflexes. The model path completes the request-and-reply
round-trip but is not yet a tested multi-step episode, so only the
scripted in-OS loop is claimed proven. There is no separate run window
drawing the agent's reasoning yet, and no steering or speech. The trained
policy and world model of the next rungs are for reflexes and prediction.
