# Agel v0.2.83 — World files

A session's world outlives the process. `agel --world NAME` reads the
world from a file in its namespace at the start and writes it back after
every transaction, so a second `:exec` has what the first defined — its
bindings, macros, modules, agents and their mailboxes — and continues at
the revision it reached. The file is the canonical encoding of v0.2.81,
now decodable, and a delta over the freshly installed library: nine
kilobytes for a session where the whole world is a megabyte, since the
filesystem's files hold 64 KiB.

## What changed

- **The canonical encoding decodes.** `canon.rs` gains a `Decoder` that
  refuses anything the encoder never writes (a wrong marker, a length past
  the end, a sequence claiming more items than bytes remain, text that is
  not UTF-8, a name that is not a builtin, a kind, a policy or a status),
  naming the byte offset. Every type that encodes now decodes:
  values, closures with their environments (rebuilt from the outside in,
  each closure with its own copy of what it captured), capabilities,
  agents, events, protocols, macros, modules, model records. A builtin is
  named by the name a fresh world binds it to (`+`, `spawn`, …) or
  `host/N` for a host word, rather than a Rust variant name, so the digest
  prefix is `agel-world-canonical-v2` and the pinned vector changed.
- **World files in `agel-core`:** `World::to_canonical` / `from_canonical`
  for the whole state, and `to_canonical_over(&base)` /
  `from_canonical_over(base, bytes)` for a delta: of the bindings, macros
  and modules only the entries the base lacks or holds differently, the
  rest whole. A versioned header carries the revision, the capability and
  authority counters and the world's identity, kept so the capabilities
  it issued still permit in the restored world; another version is
  refused. The restored world has no history and a fresh effect journal.
- **`agel --world NAME`.** After the library is installed the process
  keeps a copy of that world as the base; a file that exists is applied
  to it (`agel: world read from NAME at revision N`), a missing one
  starts new (`agel: new world, kept in NAME`), any other error ends the
  process. After every committed transaction — each line of a session,
  or the file run — the delta replaces the file; a file that will not fit
  or cannot be written is reported and the world stays in the process.

## Proof

`crates/agel-stdlib/tests/world_files.rs`: a session with the library —
`squares` from `map`, a closure with a `let`, a `defmacro`, an agent with
a queued message, a user module — encodes whole and comes back with the
same content digest and the same answers; truncated bytes are refused;
the delta is under a twentieth of the whole (9,142 against 1,054,144
bytes) and restores the same session; a capability the saved world issued
still permits after restoring. `scripts/test-agel-process.sh`: `:exec
agel -- --world kept.agel` starts new, `(def x 40)`, `(spawn "w")`,
`(send w 'hi)`, `:eof`; `:fs-ls /` lists `kept.agel`; the next `:exec
agel -- --world kept.agel` reads it at revision 4, `(+ x 2)` answers 42
and `(recv w)` answers `hi`, ending at revision 6. The full regression
passes; the kernel is unchanged. The console harness the Python suites
share now skips what a running program prints between the echoes of a
typed command, which was the timing flake the play suite showed under
load in three full regressions.

## Not claimed

A world file is the operator's file: unsigned, unchecked beyond its own
consistency, applied with the process's authority. A delta is over the
library the process carries: a runtime with a different library gives a
different world, and a definition removed from the library's names is not
expressible and stays. The effect journal is not kept: model requests
pending at the end are lost with their outcomes. The file is written
whole after every transaction; a session that writes more than 64 KiB of
state stops being kept and says so. Images (`agel-image`) remain the
signed, replayable form on the host; a world file is a snapshot.
