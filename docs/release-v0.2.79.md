# Agel v0.2.79 — Effects for the runtime in the OS

v0.2.78 put the hosted runtime in a protection domain with no effects
beyond printing. This release gives it the desktop's own effect
vocabulary — `file-read`, `file-write`, `file-append`, `file-list`,
`clock`, `console-log`, `exec` — through the process protocol, each behind
a capability the evaluator checks, so the hosted agent runtime's
capability model now governs real effects in the OS: an agent spawned
without a capability cannot write a file, and one spawned with it can.
The roadmap row "the hosted agent runtime running inside a protection
domain" is done.

## What changed

- **Host words in `agel-core`.** `HostWord { name, capability, call }`
  is a word the embedding supplies: a name, the capability kind a caller
  must hold (`""` for none) and a function pointer. `World::install_host`
  binds a table's words as `Builtin::Host(index)`; `EvaluationOptions::host`
  carries the table the evaluation may apply, so a world's bindings never
  hold a pointer and an evaluation without the table answers
  `host/unavailable`. Before a word runs the evaluator checks that the
  caller — the agent whose turn it is, or the evaluation's own set — holds
  a capability of the word's kind whose scope permits the word's first
  text argument (a path), or `*` when there is none; otherwise
  `capability/denied`, in the caller's transaction, before anything
  happens. A word's error is a condition like any other.
- **The words in `boot/posix/agel`.** The file words are the namespace
  `:exec` granted (`file-read` up to 64 KiB, `file-write` replaces,
  `file-append` appends, `file-list` names a directory, `/` being the
  root); `clock` is whole seconds since the machine came up; `console-log`
  writes a line to descriptor 1 as it runs, before the transaction's
  values print; `exec NAME` runs a program from the table as a child with
  this process's namespace and console and answers its exit status. The
  evaluation holds `file/read`, `file/write`, `clock/read`,
  `console/write` and `process/run` for every scope; an agent holds only
  the capabilities it was spawned with, taken from that set with
  `request-capability`.

## Proof

Hosted: `crates/agel-core/tests/language_core.rs` (`host_words`): a word
applies through the table, its error is a condition, the same binding
without a table is `host/unavailable`; a word needing `vault/read` is
denied, then allowed once the capability is issued; an agent spawned
without capabilities fails its turn on the word and stops, one spawned
with the capability runs.

In the OS, `scripts/test-agel-process.sh`: a file the desktop's evaluator
wrote runs under `:exec agel -- --no-stdlib effects.agel`: `file-write`
answers 22, `file-append` 3, `file-read` the joined text, `file-list "/"`
the names, `console-log` a line on the console, `(type-of (clock))` `int`,
`(exec "hello")` prints `hello from a loaded process` and answers 42, the
child's status. A `scribe` agent spawned bare has its turn fail on
`file-write` and is `stopped`; a `keeper` spawned with a `file/write`
capability from `request-capability` writes `agent.txt`, which the
process then reads back — seventeen forms, 82 steps, status 0. The
desktop's evaluator then reads `out.txt` and `agent.txt` with its own
`file-read`: the files are the filesystem's. The full regression passes;
the kernel is unchanged.

## Not claimed

Effects run as they are reached and are not rolled back with a failed
transaction: a file written before a later form fails stays written, as
with the desktop's evaluator; only the world's state rolls back. The
capability check is by kind and scope, and the namespace is the bound on
what a path can name; a capability's scope narrower than the namespace
narrows what the word accepts, but nothing widens it. Model requests made
in the OS stay pending in the world; answering them is a host bridge's
job, as it is for the desktop's evaluator, since the OS has no network.
Images (the signed replay logs of `agel-image`) remain a host tool: a file
is still one transaction and nothing persists between two `:exec`s but the
files. The compiler written in Agel has still not been run here.
