# Agel v0.2.45 — Processes that make processes: the fourth POSIX stratum

A process can now start another. There is no `fork`: a child is started by
name and receives exactly what its parent names, one descriptor as its
standard input, one as its standard output, the console as its standard
error, and the parent's namespace or a read-only view of it. Pipes join
them; `wait` collects them.

```text
agel-native[0]> :exec c-pipeline
HELLO FROM THE PARENT
child 1 exited with 22
hostile process about to write where it may not
process hostile faulted: page-fault at 0x9000002c touching 0x10; contained
child 1 stopped by signal 11
spawn nothing: errno 2
wait for no child: errno 10
process c-pipeline exited with status 0
```

## What changed

- **A process table and a scheduler.** `:exec` serves up to four processes,
  the one the operator started and every descendant, round-robin: one
  entry per runnable process per pass, blocked ones retried. A process
  blocks on `wait`, on a read of an empty pipe with a writer, or a write to
  a full pipe with a reader. When nothing live can progress, what is left is
  stopped and reported as blocked forever. Frames return as each process
  ends; a child the machine stops is reported on the console as it happens.
- **`spawn`, `pipe`, `wait`** in the process protocol. `spawn` names a
  program, two descriptors (or none) and a flag for a read-only namespace;
  `pipe` answers two descriptors; `wait` answers an exit status or a signal.
- **Pipes.** Four 512-byte queues in the supervisor with read-end and
  write-end counts: the last write end closing ends the stream, a write
  with no reader is `EPIPE`.
- **The C library.** `pipe` in `<unistd.h>`, `waitpid` with the `W*` macros
  in `<sys/wait.h>`, and Agel's own `agel_spawn` in `<spawn.h>`.
- **Programs.** `boot/posix/c/pipeline.c` and `shout.c`.

## Proof

`scripts/test-spawn.sh [arch]` runs the pipeline on x86-64, AArch64 and
RISC-V: the child's uppercased output, its byte count as the status the
parent reports, the fault report and the signal the parent sees for the
hostile child, `ENOENT` for a missing program, `ECHILD` for a child that is
not the caller's, and the whole run a second time to show every process's
frames came back. CI runs all three.

## Not claimed

No arguments or environment cross to a child. Four processes, four pipes,
round robin without priorities. A child's root is its parent's; only the
rights can be narrowed. No `kill`, no catchable signals, no process groups,
no in-place `exec`, no blocking console input. Deadlock is detected only
when nothing at all can run.
