# Threat-model evolution

Each release records newly reachable attack surfaces and the invariant that
contains them.

## v0.5

- **Staged source:** proposals may be malformed, stale, effectful, or authored
  by a compromised model. The external verifier binds source and base digests,
  denies undeclared effects, runs without authority, and atomically promotes.
- **Time travel after external effects:** rollback may restore pending language
  state. A monotonic effect journal outside that state prevents a second claim.
- **Restored bearer authority:** snapshot restore invalidates existing handles
  by advancing the capability epoch.
- **Forged completion:** completions are bound to a world/request/provider/
  prompt-derived effect key.

The verifier is a deterministic gate, not a theorem prover. Unknown semantic
behavior is constrained by zero-authority canaries and executable tests; later
releases add stronger effect interposition and model checking.

## v0.6

- **Ambient host authority:** a child process could inherit credentials or an
  attacker-controlled environment. The process boundary clears the environment
  and restores only a short, named login/configuration allowlist.
- **Arbitrary execution:** an effect request cannot select a different binary;
  the exact executable must have been allowlisted when its provider was enabled.
- **Runaway tools:** wall-clock and captured-output limits are enforced, and a
  timed-out Unix child process group is terminated.
- **Unreviewed filesystem mutation:** proposed file changes can live in a
  copy-on-write overlay, be inspected as a deterministic diff, then explicitly
  committed or discarded. Virtual paths reject parent traversal.
- **Invisible effects:** every process allow/deny and terminal outcome receives
  a SHA-256-bound typed intent and is visible through the provider audit log and
  the REPL's `:effects` command.

This is userspace interposition, not a complete sandbox against malicious native
code. Today the trusted Rust host can call operating-system APIs outside this
crate, and provider safety additionally relies on each CLI's own read-only or
restricted mode. Native syscall mediation, mount/network policy, quotas across
process descendants, and a separate supervisor remain required before Agel can
run hostile machine code.

## v0.7

- **Host-layout lock-in:** images store canonical committed inputs rather than
  Rust memory layouts. Decoding is bounded and versioned; reconstruction uses
  the public language semantics.
- **Silent history edits:** each entry commits to the previous digest and its
  length-delimited bytes. The final root also binds format version, resource
  budget, and history policy. Mutation, insertion, deletion, and reordering are
  detected.
- **Crash during save:** bytes are written and synced to a same-directory
  temporary file, the current image becomes a recovery sidecar, the new image is
  atomically renamed into place, and the directory is synced on Unix.
- **Stale overwrite:** callers provide the root they loaded. A mismatched root
  rejects the save. The current implementation assumes a single writer between
  check and rename; cross-process locking is still required for concurrent
  writers.
- **Persisted bearer authority:** capability grants are replayed in order into a
  fresh world. Old capability objects and world-bound model effect keys are not
  serialized or accepted in the restored world.

The chain is tamper-evident, not authenticated. Anyone who can rewrite the file
can recompute it. Signed roots, encrypted secrets, bounded compaction, and remote
replication remain later trust layers.

## v0.8

- **Library privilege creep:** standard facilities are ordinary Agel modules
  installed in one transaction. They receive no hidden host authority and can be
  omitted with `--no-stdlib`.
- **Unbounded functional traversal:** recursive sequence functions remain under
  evaluator fuel, call-depth, and collection limits. Exhaustion aborts the whole
  candidate transaction.
- **Swarm amplification:** worker creation is explicit, workers receive no
  ambient capabilities, pool messages are protocol checked, mailbox/event growth
  is collection-bounded, and `(run n)` caps turns at the caller's chosen number.
- **Partial dispatch:** a pool turn either queues one typed worker message and
  rotates its worker list, or rolls both operations back. Empty pools fail before
  an agent is spawned.

`any` payloads in `agel/swarm` are a convenience type, not authority. Applications
with a stable domain protocol should define narrower message types around the
generic pool.
