# Agel v0.2.82 — A session in the OS

The loaded runtime keeps a world between lines. Until now `agel` ran one
file as one transaction and ended; a second `:exec` started from nothing.
Now a process can read what the operator types — descriptor 0 is the
console for a process started from the workshop, and the desktop gives a
reading process its lines — and `agel` without a file is a session: each
line a transaction in the same world, values as they commit, an error
leaving the world as it was, until `:eof`.

## What changed

- **Console input for processes.** A process the operator started has a
  readable descriptor 0. A `read` on it takes what was typed, a block at
  a time, or blocks; a run reports `PROCESS READING` and the desktop hands
  the prompt back. While a live process reads and nothing typed is
  waiting for it, the next line the operator types is the program's
  (`LINE GIVEN TO THE PROGRAM`), with its newline, into a 1 KiB buffer per
  run; a line of `:eof` alone ends the input, and the read answers 0, as a
  pipe's does when its last writer closed. The serial workshop, which has
  no line to give, ends the input at once. Descriptors 1 and 2 stay
  write-only; a child spawned with a pipe as its 0 reads the pipe.
- **`agel` as a session.** Without a file the runtime prints `agel:
  session; each line is a transaction, :eof ends it`, reads lines, and
  evaluates each with `evaluate_with` in the one world: `=> VALUE` per
  form, `agel: error: ...` for a failed transaction with the world
  unchanged, `agel: end of input at revision N` and status 0 at the end.
  Lines are kept up to 4 KiB.
- **No rollback history in the process.** A world's history of whole
  states (64 by default, one clone per transaction) filled the 16 MiB
  process window three transactions in with the standard library
  installed, and the failed allocation panicked into a spin the
  supervisor stopped as an exhausted tick budget. The process now builds
  its world with `World::new(0)`: a transaction rolls back through its
  own copy, and nothing is kept.
- **Panics and exhaustion say so.** `Process::report_panics` in the
  process ABI has a panic written to descriptor 2 (`panic: panicked at
  ...`) and the process exit 101 instead of spinning; `agel` asks for it
  at entry. When the window is full, `agel` reports `out of memory: the
  process window is full after N free pages` and exits 12.

## Proof

`scripts/test-agel-process.sh`: `:exec agel -- --no-stdlib` answers
`PROCESS READING`; `(def x 40)` typed at the desktop's prompt is given to
the program and answers `=> 40`; `(+ x 2)` answers `=> 42`; `(begin (def x
0) (/ 1 0))` fails with `division by zero` and `x` is still 40; `:eof`
answers `END OF INPUT`, the process reports `end of input at revision 3`
and exits 0. With the library, nine transactions in one session:

```text
live-desktop> :exec agel
agel: standard library installed, 834 steps
agel: session; each line is a transaction, :eof ends it
PROCESS READING
live-desktop> (def squares (list 1 4 9))
LINE GIVEN TO THE PROGRAM
live-desktop> => (1 4 9)
live-desktop> (+ 1 2)
LINE GIVEN TO THE PROGRAM
live-desktop> => 3
...
live-desktop> (+ z w v)
LINE GIVEN TO THE PROGRAM
live-desktop> => 15
live-desktop> :eof
END OF INPUT
live-desktop> agel: end of input at revision 9
process agel exited with status 0
```

The out-of-memory path was run once by hand: two hundred copies of a
32 KiB text ends with `agel: out of memory: the process window is full
after 6 free pages` and status 12. The full regression passes; the kernel
grows by four kilobytes and stays inside its slot.

## Not claimed

The session is live, not persistent: `:eof` ends the world, and nothing
survives to the next `:exec` but the files; an image of the session in
the filesystem is the rung after. A line is a transaction, so a form
cannot span lines. The process owns every line while it reads: the
desktop's own commands are unreachable until `:eof`, as a terminal's are
while a program reads. A world with the library is several megabytes,
and an agent's turn clones it; the window holds a few such copies, not
many.
