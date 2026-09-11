# Agel v0.2.59 — Time and signals

Processes read a monotonic clock, sleep, and kill their own children;
the C library gains `time.h`, `signal.h`, `setjmp.h` and the environment.

## What changed

- **`clock` (17):** microseconds since the machine came up, from its
  counter: the time-stamp counter on x86-64, calibrated once at
  bring-up against the timer's second channel; the generic counter on
  AArch64; the `time` CSR on RISC-V.
- **`sleep` (18):** a state of the process table; the pass wakes the
  process once the clock has passed its time. The desktop hands the
  prompt back (`PROCESS SLEEPING`) and runs it between inputs; the
  serial workshop repeats its pass until it wakes.
- **`kill` (19):** ends one of the caller's own live children with
  `SIGKILL`, the only signal; the child's `wait` answers as for a
  stopped child, and the console reports `killed by signal 9`.
- **The C library:** `clock`, `time` (seconds since the boot), `clock_gettime`,
  `nanosleep`, `sleep`, `usleep`, `kill`, `setjmp` and `longjmp` (assembly
  for the three machines), `getenv`, `setenv`, `unsetenv`, `putenv` over a
  per-process table; the build now compiles the library's assembly too.

## Proof

`scripts/test-libc.sh` on x86-64, AArch64 and RISC-V runs `c-clock`:
it sleeps twenty milliseconds and requires the clock to have advanced at
least that, escapes three frames with `longjmp`, sets, shadows and unsets
a variable, spawns `c-nap` (thirty seconds of sleep), is refused
`SIGTERM`, kills it, reads the signal from `waitpid`, and is refused a
second `kill`. The full regression passes.

## Not claimed

No signal delivery or handlers, no `alarm`, no calendar time, no
floating point. A sleeping process holds the serial workshop's `:exec`
as any running process does.
