# Agel v0.2.78 — The language in a domain

The rung after programs from files: the whole language runs in the OS.
Until now the OS ran Agel through the native evaluator, a fixed-memory
interpreter with no allocator, which is why the standard library, the
metacircular evaluator and the compiler written in Agel stayed on the
host: they need a runtime with real memory. This release builds the hosted
runtime itself without `std` and loads it into a protection domain as a
process. `:exec agel -- FILE` reads an Agel file from the process's
namespace, installs the standard library from its own image, evaluates the
file as one transaction and prints each form's value. Modules, macros,
agents and the interpreter written in Agel run unprivileged in the OS for
the first time.

## What changed

- **`agel-core` builds without `std`.** The crate is `no_std` over `alloc`
  when its default `std` feature is off: the same reader, expander,
  evaluator, agents and transactional worlds. What `std` adds is
  `std::error::Error` for the error types, an `Arc<Mutex<_>>` effect
  journal where the `no_std` build has `Rc<RefCell<_>>`, and `stacker`, which
  grows the hosted thread's machine stack ahead of deep recursion. Nothing
  in the language differs between the two builds. `agel-integrity` gains an
  `alloc` feature between none and `std` (hex text and SHA-256 need an
  allocator; SHA-512 and Ed25519 never did), and `agel-stdlib` follows
  `agel-core`.
- **A pulse.** `EvaluationOptions` takes an optional `Pulse { every, hook }`:
  the evaluator calls the hook every `every` fuel ticks. A process has a
  tick budget of one second per entry and is stopped if it computes longer
  without a request; the runtime's hook asks the clock, which is a request,
  so a long evaluation is many entries. Without it, `(fib 25)` is stopped
  with `never yielded; tick budget exhausted`; with it, it finishes in a
  few seconds on QEMU's TCG, three million steps.
- **`boot/posix/agel`**, a program like `hello` or `writer`, 360 KiB with
  the standard library's source inside it. Before the runtime runs it maps
  1,024 pages at its break and moves its stack pointer there, since the
  sixteen pages a process is built with hold no deep evaluation; its heap
  is more pages at the break, handed out in power-of-two classes with a
  free list each and as whole page runs above 64 KiB. Its arguments are
  `[--no-stdlib] FILE`; the file comes from the namespace `:exec` granted,
  values go to descriptor 1 as `=> VALUE`, errors to descriptor 2, and the
  exit status is 0, 1 for a failed transaction, the error number for a file
  it could not read, 2 for no file.
- **The console harness** (`scripts/graphical_console.py`) lets a test wait
  longer than thirty seconds for a command that runs a program.

## Proof

`scripts/test-agel-process.sh` builds the program, installs it in the
graphics image's program region and drives the desktop over its serial
console. The desktop's own evaluator writes the files with `file-write` and
`file-append`. `:exec agel -- --no-stdlib core.agel` answers `=> 42` and
`process agel exited with status 0` in under a second. `:exec agel --
prog.agel` reports `agel: standard library installed, 834 steps`, then
`(fib 15)` through `agel/sequence`'s `foldl` and `map`, a `defmacro`, and a
`make-meta-agent` counter from `agel/meta-agent` that takes two messages
under `(run 2)` and answers 42: `=> 610`, `=> 385`, `=> expanded`, `=> 42`.
`(fib 25)` outlives the `:exec` (the desktop hands the prompt back and
reports the end on a later pass) and answers `=> 75025` after 3,034,811
steps. A file whose second form divides by zero exits with status 1 and
`agel: error: evaluation error: ... division by zero`, the transaction
rolled back; a file the namespace does not hold exits with `error 2`. The
same run with the pulse disabled was made once, in isolation, to confirm
the supervisor stops it. The full regression passes; the kernel is
unchanged.

## Not claimed

The runtime's effects in the OS are its console descriptors and its
namespace, nothing else: there are no file, clock or window words in the
hosted core, and the model adapters, the image and snapshot store, the
`:dispatch` and provider machinery of the hosted CLI have not moved. A
file is one transaction, not a session: nothing persists between two
`:exec`s but the files. The desktop is still run by the native fixed
evaluator, the workbench and the DOOM agent with it; the two evaluators
share nothing. A panic in the runtime (an allocation the window cannot
hold) spins until the tick budget stops it, and its message is lost. The
compiler written in Agel is in the library this process carries, and has
not been run here yet: that is the next thing to prove.
