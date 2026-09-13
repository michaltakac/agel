# Does it run DOOM?

The question every system gets asked, taken as a research programme
rather than a stunt: DOOM runs as an ordinary POSIX process on Agel's own
kernel, and Agel plays it, with the whole loop (the game, the agent that
plays it, the data it leaves behind and the model trained on that data)
running under the same capability discipline as everything else here.
This document records what is known, what Agel lacks today, measured, and
the order of work. Each rung is one release with its own test; the state
of each line is kept in [`roadmap.md`](roadmap.md).

## What others have done

- **A language model plays it, slowly.** *Will GPT-4 Run DOOM?* (de
  Wynter, 2024, [arXiv:2403.05468](https://arxiv.org/abs/2403.05468))
  gave GPT-4V screenshots of the game, had it describe the state in text
  and choose keystrokes. It opened doors, fought and found paths "to a
  passable degree", forgot enemies that left the screen, and did better
  with several model calls per step. Each step took seconds: the game had
  to wait for the model.
- **A tiny trained model plays it in real time.** *Playing DOOM with 1.3M
  Parameters* (2026, [arXiv:2604.07385](https://arxiv.org/abs/2604.07385))
  trained a 1.3-million-parameter encoder on 31,000 human demonstrations
  of one scenario, fed it ASCII frames and depth maps, and had it choose
  an action every 31 ms. It scored 178 frags over ten episodes where every
  large language model tested together scored 13, and was the only agent
  that engaged enemies rather than fleeing. The lesson for this
  programme: judgement from a large model, control from a small one
  trained on data the system collected itself.
- **A model *is* the game.** *Diffusion Models Are Real-Time Game
  Engines* (GameNGen, 2024, [arXiv:2408.14837](https://arxiv.org/abs/2408.14837),
  ICLR 2025) trained an agent to play, recorded it, then trained a
  diffusion model to predict the next frame from past frames and actions:
  20 frames per second on one TPU, raters near chance at telling it from
  the game. A world model of this kind is what "predict future outcomes"
  means concretely: a learned simulator of DOOM under Agel's actions.
- **Reinforcement learning from pixels.** [ViZDoom](https://vizdoom.cs.put.edu.pl/)
  is the standard research harness; curiosity-driven exploration
  ([Pathak et al., 2017](https://proceedings.mlr.press/v70/pathak17a/pathak17a.pdf))
  and asynchronous training reached super-human play there, and world
  models (DreamerV3, *Nature* 2025) learn control in imagination from
  the same kind of frames-and-actions data. Unsupervised
  reinforcement learning (surprise, curiosity, skill discovery) is the
  right first objective when a human only says "learn to play it": the
  reward is the game's own progress, and exploration is intrinsic.
- **Low-frequency agents with tools.** The 2025 "Claude plays Pokémon"
  runs showed a model steering a game through screenshots, a memory it
  writes for itself and tools it calls, at one decision every few
  seconds, for days. That is the shape of the *judgement* loop here.

## What Agel has and lacks, measured

Where the port would run: `boot/posix`, the POSIX personality, C
programs against `agel-libc` in a protection domain on the desktop. All
80 sources of `doomgeneric` (the platform-free Chocolate Doom fork, GPL
v2) compile against Agel's C headers with two shim headers; what remains
is listed here exactly.

| Need | Agel today | Gap |
|---|---|---|
| Pixels on the screen | a window holds 24 records: rectangles, gradients, ellipses, labels, surfaces, sprites | no way to show a 320×200 frame a process rendered; a **pixel surface** record backed by pages the process draws into and the compositor blits, scaled |
| Keys | `EVENT_KEY` carries a byte on press; nothing on release | DOOM holds keys; **press and release events with key codes**, and modifiers |
| The program | ~1 MB of code and data | the program region is sectors 2048–3071, 512 KiB for every program together, in a 3 MiB disk image |
| The WAD | `doom1.wad`, the shareware data, 4.2 MB, read by `fopen`/`fseek`/`fread` | files are 64 KiB at most in a 256 KiB filesystem region; a **data region** of large read-only files, or a larger `agelfs` |
| Memory | a 6 MB zone plus the WAD's cached lumps | a domain is built from at most 512 frames (2 MiB, `FrameLedger::CAPACITY`) from a pool of 14 MiB on x86-64; the process window is 16 MiB |
| Floating point | `r_main.c` and `v_video.c` use `float` in a few places; `m_config.c` parses one with `atof` | processes run without an FPU (`-msoft-float`); one function returning `float` will not compile that way, and no soft-float runtime is linked |
| C library | `printf` family, streams, heap, strings, `getopt`, directories, time | `strcasecmp`/`strncasecmp`, `fseek`/`ftell` declarations and `SEEK_*`, `remove`, `atof`, `system` (a stub), `strings.h`, `inttypes.h` |
| Time | `clock_gettime`, `nanosleep` on a monotonic clock | enough: `DG_GetTicksMs`, `DG_SleepMs` |
| Sound | none | out of scope; DOOM runs silent |
| An agent that plays | model providers exist only in the **hosted** runtime, through a typed, audited effect; the native kernel has no network and no local inference | the agent runs hosted and drives the OS through the same QMP and serial channels the tests use, on the machine's own screen and keyboard; a native agent waits on networking or local inference, both open on the roadmap |
| Speed | under QEMU's TCG a full 1080p repaint costs 0.5 s; a 320×200 blit at 1× is 64 K pixels, ~16 ms; at 2× ~64 ms | real hardware (the Pi 5, a PC) is 20–50× faster; the design keeps the blit unscaled or 2× and repaints only the window |

## The design

1. **A canvas is a record that names pages.** A process asks the
   desktop for a canvas of W×H (`CANVAS`, protocol word 22); the
   supervisor allocates the frames into the process's domain, read-write,
   and maps the same frames into the compositor read-only, where a new
   record type blits them into the window at 1× or 2×. The process draws
   into memory and asks for a `DRAW` as now; nothing it writes can reach
   outside its window, since the compositor clips the blit like every
   other record. When the process ends, the compositor's mapping is
   withdrawn before the frames go back to the pool.
2. **Keys go down and up.** The input driver world reports make and
   break codes; the desktop turns them into `EVENT_KEY_DOWN` and
   `EVENT_KEY_UP` with a key code, keeps `EVENT_KEY` for the workshop's
   line, and delivers modifiers as keys like any other.
3. **Room.** The disk image grows to 32 MiB, with a program region of
   4 MiB and a **data region** (sectors installed from the host by
   `scripts/install-data.py`, a table like the program and asset
   regions) that the filesystem service serves as read-only files under
   `/data`, so `fopen("/data/doom1.wad")` works through the namespace.
   The frame ledger becomes a bitmap over the pool rather than a list of
   512, and the pool grows to what the machine has.
4. **A floating-point unit for processes**, on x86-64: SSE enabled for
   domains, the FPU state saved and restored around every domain entry,
   and a small `math.h`. On AArch64 and RISC-V the same switch, when the
   port reaches them.
5. **The port.** `boot/posix/c/doom/`: `doomgeneric` unmodified as a
   third-party source (GPL v2, its licence kept beside it) plus one
   platform file implementing `DG_*` over `agel/window.h`: the surface,
   the keys, the clock. Built by `scripts/build-c-program.sh doom`,
   installed with the WAD, started with `:exec c-doom`. The proof is a
   demo played back to a checksum: DOOM's own `-timedemo` reports its
   frame rate, and the test requires the title screen's pixels.
6. **Agel plays.** A hosted Agel agent (`crates/agel-cli`, the
   model-provider effect) holds a `Machine` like the tests do: it reads
   the frame by screendump, sends keys by QMP, and runs the game in
   **steps**, a fixed number of tics per decision, so a model that
   answers in seconds plays a game that runs at 35 Hz. Every step is
   appended to a **dataset**: the frame, the game's own state (from a
   `/data`-style channel DOOM writes: position, health, ammo, kills, the
   level), the action, the model's stated reason. The desktop shows the
   run: the game in its window and, beside it, a window a second process
   draws with the agent's state, the last reasons, the frame time, the
   pool's frames in use and the passes per second, as the roadmap's
   visualization line.
7. **A policy trained on the dataset**, small and fast, following the
   1.3M-parameter result: imitation from the model-played episodes
   first, then intrinsic-reward reinforcement learning (curiosity or
   surprise) against the game itself, then a world model for
   predictions of what comes next. Training is orchestrated, not
   performed, by Agel, as [`deployment-targets.md`](deployment-targets.md)
   decides: the trainer is a provider, reached through the same
   capability-scoped effect. The trained policy runs as a process on the
   OS when local CPU inference exists, and hosted until then.
8. **A human in the loop.** The hosted agent's run is a world with a
   message queue: the operator's text (or, later, speech transcribed by
   a provider) is appended as a message the agent reads between steps;
   side questions to a sub-agent read the run's own state and the world
   model's predictions without touching the run. This is what the agent
   runtime already does for any agent: mailboxes, supervision, event
   history, snapshots, replay.

## The order of work

Each rung is a release with a test; none starts before the one before it
is proved. The rungs and their state are listed in [`roadmap.md`](roadmap.md).

1. A canvas (design 1), proved by a C program that draws a moving
   pattern into it and a test that reads the pixels back: **v0.2.66**.
2. Key press and release events (design 2): **v0.2.67**.
3. Room: the larger disk, the data region, the bitmap ledger (design 3).
4. The C library's missing functions and floating point (design 4).
5. DOOM runs, keyboard-playable on the desktop, `-timedemo` frame rate
   reported (design 5).
6. Agel plays it, stepping, with the dataset and the run window
   (design 6).
7. A trained policy from the dataset (design 7); a world model after.
8. Speech and steering (design 8).

## What this is not

Nothing here is a claim that Agel runs DOOM today: it does not. The
native kernel has no network and no local inference, so the agent that
plays is hosted, as [`deployment-targets.md`](deployment-targets.md)
calls Tier 2, until those lines close. Sound is out of scope. DOOM's
source is GPL v2 and stays a third-party program beside Agel's code, not
part of it.
