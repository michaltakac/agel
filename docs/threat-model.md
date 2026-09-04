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

## v0.9

- **Single-bootstrap semantic bugs:** a separately written Common Lisp evaluator
  and the Rust seed consume the same functional-kernel forms and must emit the
  same canonical values in CI. Agreement increases confidence; it cannot exclude
  a shared specification mistake.
- **Self-interpreter escape:** `agel/meta` receives only an explicit environment.
  It has no implicit capabilities, and the enclosing evaluator still enforces
  transaction, fuel, call-depth, and collection bounds.
- **Candidate replaces its judge:** `agel-supervisor` remains Rust-hosted and
  outside candidate images. Candidates must extend the active semantic history,
  rebuild successfully, and pass at least one isolated zero-authority health
  check.
- **Stale or forged promotion:** evidence binds both active and candidate roots.
  Staging a newer candidate invalidates older evidence. Promotion swaps whole
  images, retaining the old slot for watchdog rollback.

The Common Lisp reference and metacircular evaluator currently cover the lexical
functional kernel, not macros, modules, agents, persistence, or effects. A/B
slot state is not yet a separately bootable disk selector. These are explicit
v1.0 boundaries, not implied guarantees.

## v1.0

- **Self-editing world replaces recovery:** the freestanding recovery monitor is
  linked into the native seed, outside any mutable Agel world. Its A/B policy
  denies unverified promotion and retains the previous slot for rollback.
- **Hosted-runtime-only confidence:** CI boots the exact raw image under QEMU,
  checks the serial success token and debug-exit status, and rebuilds twice byte
  for byte. This catches linker, disk-loader, and long-mode handoff regressions.
- **Voice mistaken for authority:** text and voice are data modalities, not
  credentials. `Authorize` inputs require an opaque proof bound to the hub's
  host-owned presence authority; transcription alone can only observe or propose.
- **Slow agent blocks conversation:** bounded foreground and background lanes
  acknowledge accepted input independently of model or agent latency and apply
  explicit backpressure rather than growing without limit.
- **Native unsafety spreads inward:** privileged assembly and port I/O live only
  in the separate `boot/kernel` workspace. The language workspace continues to
  forbid unsafe Rust.

The monitor's slots are presently an executable policy model, not two persisted
boot partitions, and the boot seed does not yet contain the Agel evaluator,
allocator, interrupts, storage, networking, audio, or isolation for hostile
native binaries. QEMU emulation is a conformance target, not a proof of hardware
correctness. Cryptographic boot, real watchdog hardware, and signed system
images remain required before this is a secure autonomous OS.

## v1.1

- **Failed native evaluation destroys rollback history:** three fixed world
  banks separate active, previous, and scratch state. Evaluation mutates only
  scratch; failure discards it without touching either committed bank.
- **Unbounded syntax or computation:** source length, syntax nodes, nesting,
  names, parameters, arguments, globals, stored function bodies, call depth,
  and evaluator fuel have deterministic limits. Capacity errors abort the
  candidate transaction.
- **Arithmetic faults halt a kernel without an IDT:** parsing and arithmetic use
  checked operations. Division rejects zero and overflow before executing a
  faulting instruction.
- **Language state mutates recovery policy:** recovery state is owned by the
  native shell, not stored in an Agel world. The language has no primitive for
  serial ports, debug exit, page tables, or slot mutation.
- **Repeated promotion destroys the rollback slot:** promoting while B is
  already active is denied and clears stale candidate evidence, so retained A
  cannot be replaced by B. A fault reports the slot actually restored.
- **Persisted closure loses lexical authority or data:** v1.1 stored functions
  do not yet encode captured environments. Defining one from a nonempty lexical
  context is rejected transactionally instead of committing broken semantics.
- **Argument side effects replace the selected callee:** function values carry
  a snapshot of their fixed representation. Application does not reread a
  mutable global binding after evaluating arguments.
- **UART tests pass without testing input:** CI waits for the native-ready token,
  sends each byte only after its echo, frames every result by the next revision
  prompt, and requires QEMU's debug-exit status. This covers the actual normal
  REPL rather than only a compile-time self-test path.

Definitions last for the current VM session only. There is no native filesystem,
editor, persistent image, macro expander, agent scheduler, capability system,
interrupt table, memory protection, or compiler yet. The fixed evaluator shares
the kernel address space, so its checked implementation is a robustness boundary,
not hardware isolation from hostile native code.
