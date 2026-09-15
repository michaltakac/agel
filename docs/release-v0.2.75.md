# Agel v0.2.75 — The agent keeps its own log

The next rung toward self-hosting. v0.2.74 put the decision in Agel; this
release lets the Agel program do the rest of what its task needs inside
the OS: keep its log, read it back, tell the time, print, and start a
program. The desktop is the substrate the words reach through; nothing
about the task is on the host any more.

## What changed

- **Seven effect words** in the native evaluator: `file-read`,
  `file-write`, `file-append` and `file-list` are the filesystem region
  (paths from the root, 1,024 bytes per write, 2,048 per read); `clock`
  is seconds into the day where the machine has a clock driver;
  `console-log` prints a line on the console; `exec` asks the desktop to start a
  program as `:exec` would, once the form that asked has committed.
- **A synchronous port.** The evaluator writes an effect request into its
  shared page and yields; the desktop performs it against the filesystem
  service, the clock driver or the console, writes the answer in place,
  and the world resumes inside the word. The serial workshop and any
  world with no desktop answer "no service" and go on.
- **`doom-agent.agel` logs itself:** one line per step to `play.log`, the
  engine's state line and the program's reason, appended with
  `file-append`. The dataset of a run is the program's own file.
- **`:run NAME`** evaluates a cell the same way a typed form does, with
  effects answered and a requested program started.

## Proof

`scripts/test-play.sh`: after eight steps the test reads `play.log` back
through `(file-read ...)` and `(file-list)` from the same evaluator and
sees it in `:fs-ls`. The full regression passes on all three machines; the
serial workshop's `:limits` names the new bound.

## Not claimed

The words are available to every form the desktop evaluates, as `:fs-ls`
is to every operator; a capability a world must hold to write is the next
step. An effect is not undone when its form fails afterward. `exec` is
exercised by hand, not by a test. The runtime is still the fixed-memory
evaluator with sixteen cells; the runtime that holds more is the rung
after this one.
