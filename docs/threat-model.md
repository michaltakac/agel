# Threat-model evolution

Each release records newly reachable attack surfaces and the invariant that
contains them.

## v0.0.5

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

## v0.0.6

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

## v0.0.7

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

## v0.0.8

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

## v0.0.9

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
v0.1.0 boundaries, not implied guarantees.

## v0.1.0

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

## v0.1.1

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
- **Persisted closure loses lexical authority or data:** v0.1.1 stored functions
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

## v0.1.2

- **A frozen boundary drifts by accident:** the kernel contract is a versioned
  crate with an executable reference model and an 81-step conformance corpus
  whose canonical transcript is checked in and diffed in CI. Adding, removing,
  or reordering a step changes derivation identifiers and therefore the
  transcript, so a contract change cannot be made quietly.
- **"Not implemented" is discovered by being refused:** `boot.info` publishes a
  profile bitmask, and every operation outside the profile answers
  `invalid-operation`. A backend states what it does not do instead of leaving
  a caller to infer it from an error.
- **A refusal smuggles data:** a failing response carries no result words, by
  construction rather than by convention.
- **Authority is widened by derivation:** `mint` and `attenuate` reject any
  rights their parent lacks, and reject bit patterns outside the defined set. A
  capability space can attenuate itself and then cannot restore itself.
- **A revoked handle keeps working, or looks like a caller mistake:**
  revocation is transitive over the derivation tree to a fixed point, and
  descendants are tombstoned rather than merely emptied, so a stale holder is
  told `revoked` and not `invalid-capability`.
- **An unbounded mailbox absorbs a hostile sender:** the endpoint queue has a
  fixed capacity and reports `queue-full`. Notifications coalesce, so a
  notification count is never a message count.
- **A blocking call hangs a single-threaded domain:** operations with no
  counterparty answer `would-block` or `not-found`. The contract has no
  operation that can silently fail to return.
- **The mutable language world runs privileged:** the research kernel now builds
  its own page tables, runs worlds in ring 3 in separate address spaces, and
  exposes exactly one trap gate. A world holds capability slot numbers; the
  object table is supervisor-only memory it cannot read, forge, or corrupt.
- **A world writes the supervisor that is about to judge it:** the kernel image
  is mapped without the user bit in every domain's address space, so the write
  page-faults. CI asserts the specific containment, not merely that the kernel
  survived.
- **A world disables its own preemption:** ring 3 runs with IOPL 0 and no I/O
  permission bitmap, so `cli` and every port instruction raise
  general-protection. CI asserts this.
- **A world never yields:** each entry has a tick budget charged by a 100 Hz
  timer. Exhausting it stops the domain. CI runs a deliberate infinite loop and
  requires the supervisor to survive it.
- **A stopped world is silently resumed:** a fault or overrun latches, and
  re-entering a stopped domain returns its stop reason instead of running it.
  Restart is a supervisor decision with a new generation, not an automatic retry.
- **Ring-3 code reaches supervisor-only text:** `.user_text` is the only
  user-executable range, and the isolation test rejects the image if the built
  section contains a call or an indirect branch. A dense `match` in ring-3 code
  compiles to a jump table in supervisor-only `.rodata`; the command codes are
  deliberately sparse and the check keeps that from silently regressing.
- **A NOBITS section is assumed to be zero:** the entry point zeroes `.bss`
  itself rather than depending on the emulator handing out zeroed memory.

The evaluator still runs in ring 0 in the default image; the isolation layer is
built and tested but not yet carrying the language. Passing the conformance
corpus is not evidence of isolation — a backend with no privilege separation at
all would pass it — and the isolation claims above rest on the QEMU tests, not
on a proof. The research kernel's object semantics are the shared reference
model rather than an independent second implementation; that independence is
what the seL4 backend is for. The frame allocator never reclaims, there is no
IOMMU, no SMP, no signature verification, and no hardware watchdog.

## v0.1.3

- **An isolation claim that only holds on one machine:** the same contract, the
  same 81-step corpus, the same containment driver, and the same unprivileged
  world program now run on x86-64, AArch64, and RISC-V, and CI requires all
  three transcripts to be byte-identical to the frozen one. A boundary that had
  only ever been enforced by one page-table format and one trap gate was a
  boundary with one implementation, not a boundary with a specification.
- **Portability by lowest common denominator:** the fault vocabulary is shared
  but the mapping is per-architecture and deliberately unflattened. RISC-V
  cannot distinguish a privileged instruction from an undefined one, and its
  provocation table says so. AArch64 has no integer divide exception, so it is
  not provoked with one. Making three machines agree by only testing what they
  all do would have quietly weakened every one of them.
- **A world disabling its own preemption, restated per machine:** x86-64 denies
  `cli` through IOPL and an absent I/O permission bitmap; AArch64 denies EL0
  every access to the timer and the system counter through `CNTKCTL_EL1`, and
  the test provokes exactly that; RISC-V gives a U-mode hart no way to mask a
  supervisor interrupt at all. Each is asserted on its own machine.
- **Divergence between three copies of the same logic:** the capability space,
  the shared handshake page, the tick budget, the "a stopped world stays
  stopped" rule, the conformance driver, the containment driver, and the world
  program are single-sourced. Only address spaces, register frames, trap entry,
  and the privilege transition are written three times, and each is checked by
  the same tests.
- **A trap frame written through a domain's own state:** every backend resets
  its supervisor trap stack before returning to the unprivileged level. Leaving
  the stack pointer just past a domain's saved frame would make the next trap
  overwrite that domain — which is the kind of bug that looks like containment
  right up until it is not.
- **Firmware assumed to be absent:** on RISC-V the kernel is itself a guest.
  OpenSBI holds the machine timer and constrains S-mode through physical memory
  protection, and the identity window deliberately leaves the firmware's own
  memory unmapped rather than mapping what it cannot legitimately touch.

The three backends share the reference model's object semantics rather than
being independent implementations of them; that independence is still what the
seL4 backend is for. Nothing here is a formal claim: these are QEMU tests, on
emulated machines, of a kernel whose frame allocator never reclaims and which
has no IOMMU, no SMP, no signature verification, and no hardware watchdog. The
Agel evaluator still runs privileged, and only on x86-64.

## v0.1.4

- **A kernel we wrote is the only thing that has ever enforced our boundary:**
  the same contract now runs on an unmodified seL4 kernel, in four protection
  domains, with the same corpus and the same frozen transcript. Every previous
  isolation claim rested on code from this repository being correct. This one
  does not.
- **Teaching the kernel about Agel:** the contract is answered by an ordinary
  unprivileged broker domain. seL4 gains no Agel object, no Agel syscall, and no
  patch, so the reason for choosing it survives being used.
- **A device capability spreading:** exactly one domain can reach the UART.
  Every other domain that needs to print writes into a page it shares with that
  domain and asks. The serial domain clamps the requested byte count rather than
  believing a caller.
- **An unbounded control path:** an invocation fits in the four message
  registers seL4 passes in hardware registers, with the operation and capability
  in the 52-bit label. The control path touches no shared memory at all, so
  there is no shared-memory step in it to get wrong.
- **A reply believed because it came from inside:** the world validates the
  broker's status code and turns an unrecognised one into a recognised failure.
  Another protection domain's output is input.
- **Containment that depends on our own supervisor loop:** the world faults on
  purpose at the end of its work; the recovery domain is its parent, so seL4
  delivers the fault there, and declining to reply leaves the world stopped. The
  containment is a property of the kernel and of `agel.system`.
- **An assurance claim that outruns the artifact:** `./scripts/sel4-manifest.sh`
  reads the kernel, loader, monitor, library, system description and toolchain
  out of what was actually built, and prints the verification status. CI
  regenerates it and requires the trusted base and the kernel configuration to
  match what is checked in.
- **A substituted kernel:** the SDK download is pinned by version and verified
  against a checksum for every published platform, and refuses to proceed on a
  mismatch.

The configuration is **not** a verified one. Every Microkit board ships an MCS
kernel and this board also enables hypervisor support; MCS proofs are ongoing
rather than complete. Running on an unmodified verified kernel *implementation*
is not the same as running a verified *configuration*, and only the former is
claimed here. Nothing above says anything about the correctness of the Agel code
in those protection domains, which is ordinary unverified Rust, or about the
system description, which grants the authority and has been reviewed by nobody
but its author. The seL4 backend also runs only the contract: the Agel evaluator
is not in it, and is still privileged on x86-64.

## v0.1.5

- **A driver fault is a kernel fault:** the console driver runs unprivileged in
  its own domain on all three research backends. It faults where every other
  world faults, into the supervisor, and the supervisor keeps running. CI
  provokes exactly that and requires the system to continue.
- **A device reachable by anything that asks:** the device is granted the way
  each architecture grants one — eight I/O ports through a task-state-segment
  bitmap on x86-64, one mapped page on AArch64 and RISC-V. Every other world
  executes the same instruction that touches the console and is refused by
  hardware. The grant is installed around the driver's entry alone, so it is
  per-entry rather than ambient.
- **A restart nobody notices:** a replaced service gets a new generation, and a
  handle issued before the restart is refused with the contract's
  `stale-generation`. A caller that has not noticed a restart is a caller whose
  assumptions about the service are stale too, so it fails closed rather than
  being served by a server that no longer remembers the conversation. CI checks
  both the refusal and that the refused text never reached the device.
- **A recovery plane that depends on what it must report:** the supervisor keeps
  its own direct path to the console, used for its own reports and the panic
  handler. Routing those through the driver would mean losing the ability to say
  the driver had died at exactly the moment it died. That path is deliberate,
  not a leftover, and it is the reason the supervisor is still able to print the
  restart sequence at all.

What this does not do: the driver is one driver. Timers are still the
supervisor's, and deliberately — preemption is how a world gets contained, so
moving it out is a question rather than an obvious improvement. Storage,
networking and model brokering have not been split. The frame pool still never
reclaims, so a restarted domain's frames are lost; a system that restarts
drivers in a loop would exhaust it. And the evaluator still runs privileged.

## v0.1.6

- **An evaluator bug corrupts its recovery plane:** the native evaluator now
  runs in an unprivileged domain on x86-64, AArch64, and RISC-V. Its writable
  state is a private, fixed-size stack plus one shared page; kernel state and
  other domains have no user translation.
- **A writable language value becomes executable:** evaluator code is
  read/execute, immutable constants are read-only and non-executable, and the
  private stack/shared page are read/write and non-executable. The mapping type
  has no writable-and-executable variant.
- **A domain mapping becomes ambient kernel authority:** every trap switches to
  the kernel's own page-table root before invoking supervisor policy. A reply
  reinstalls the domain root only immediately before returning to user mode.
- **A request overruns an IPC parser:** source and result bytes share one 4 KiB
  page but each payload is capped at 256 bytes. Metadata is read back as
  untrusted data and lengths are clamped before supervisor use.
- **A failed evaluation partially mutates the world:** the same three-bank
  active/previous/scratch transaction protocol used by the native workshop now
  executes inside the domain. The cross-architecture test commits definitions,
  evaluates recursion, attempts a failing redefinition, and observes the old
  value and unchanged revision.
- **A large language stack consumes every domain:** ordinary worlds retain four
  pages. Only evaluator worlds receive a fixed 512 KiB stack, needed by the
  current copy-based transactional implementation; an absent guard page makes
  exhaustion a contained fault rather than growth into another allocation.
- **A console grant leaks into the language:** the evaluator has no UART mapping
  or x86 I/O-port grant. Interactive results return to the supervisor and are
  forwarded to the independently restartable console domain.

This is still not memory-safe proof, formal verification, durable recovery, or
the full hosted agent runtime. Immutable kernel constants are readable to the
evaluator domain in this research backend, serial input remains supervisor
code, and seL4 currently hosts only the frozen kernel contract.

## v0.1.7

- **A memory-layout snapshot revives stale authority:** the native image stores
  only bounded source-cell names and source bytes. Boot reconstructs a new
  evaluator session by replay; pointers, stacks, page tables, capability slots,
  and Rust enum layouts never enter the format.
- **A failed source edit becomes the boot image:** `:save` first resets a fresh
  session and replays every staged cell in order. Any language error rejects the
  entire candidate and reconstructs the last committed workspace.
- **A torn write destroys the only development state:** two fixed slots alternate.
  The target header is invalidated first, the payload is flushed, and the new
  generation header is published last. The other complete generation is never
  overwritten by the same save.
- **Corrupt bytes are evaluated at startup:** headers carry explicit magic,
  version, generation, length, and CRC-32. Decoding rechecks all cell, name, and
  source bounds, duplicate names, and trailing bytes before replay. The test
  exercises both a checksummed but semantically invalid newest image and a
  corrupt newest payload, then simulates power loss after invalidation and a
  partial payload write, requiring automatic fallback in each case.
- **Disk absence freezes the recovery console:** ATA polling is bounded and a
  missing or failed disk degrades the workshop to volatile operation rather
  than preventing the prompt.

CRC-32 is accidental-corruption detection, not authenticity. The storage path
and editor remain supervisor Rust; the disk itself is behind a driver domain
since v0.2.26 on x86-64 and v0.2.33 elsewhere. Signed workspace images,
power-cut injection at every sector transition, and the native agent runtime
remain outside this claim.

## v0.2.0–v0.2.4 agentic desktop and graphics

- **An agent publishes half a UI mutation:** desktop edits are persistent patch
  values applied to a candidate scene. Only a structurally valid whole scene may
  become a preview, and commit swaps the complete candidate in one agent turn.
- **A delayed agent overwrites newer work:** every proposal declares its base
  desktop revision. A mismatched revision produces `ui/stale-revision` and leaves
  the current scene and preview unchanged.
- **Malformed or duplicate components poison traversal:** scenes require the
  complete node shape and globally unique identities. Recursive validation,
  compilation, and hit-testing remain under the evaluator's fuel, call-depth,
  and collection limits.
- **Negative, zero, or overflowing geometry reaches a renderer:** `<` is a
  type-checked integer primitive; Agel validates viewport extents, theme metrics,
  padding, gap, basis, every rectangle, and every display command. Existing
  checked arithmetic converts overflow into a transactional condition.
- **A failed reflow destroys the visible desktop:** the layout agent compiles a
  complete candidate frame before replacing its heap. Expected layout failure
  returns `render-rejected`; the preceding validated frame remains committed.
- **A pointer event becomes authority:** hit-testing returns an inspectable
  semantic intent and its declared requirement. It does not execute the intent,
  mint a capability, or contact an effectful service.
- **Overlapping actions are nondeterministic:** action regions retain display
  order and hit-testing checks the last region first. Identical scene, viewport,
  and theme values produce structurally identical frames.
- **Agent-authored SVG becomes markup injection:** the host renderer accepts
  only strict colors, escapes text and IDs, independently revalidates the whole
  frame, and applies command, path, dimension, scale, and output-byte bounds.
- **The native compositor becomes ambient display authority:** framebuffer
  pages are mapped only into one ring-3 domain as writable, non-executable,
  cache-disabled device memory. Ordinary worlds cannot name that mapping.
- **A malformed native command destroys the visible frame:** the build adapter,
  supervisor stream envelope, and compositor validate at separate boundaries.
  The executable test submits an unknown operation and requires the exact prior
  framebuffer digest afterwards.
- **A compositor bug freezes or erases recovery:** preemption bounds every
  entry. The test deliberately makes the compositor write supervisor memory,
  requires a contained page fault, creates a fresh domain, and requires that it
  sees the exact last-good framebuffer digest without redrawing.

The v0.2.4 keyboard-to-intent router accepts only a fixed, bounded grammar;
input cannot submit framebuffer records or waive compositor validation. PS/2
mouse bytes are drained separately rather than decoded as keyboard input. A
candidate scene becomes current only after a complete validated render returns
and produces a nonzero digest; syntax and policy rejection render only a
diagnostic in the command bar, never the rejected candidate.

There is still no pointer driver, separate unprivileged input domain,
accessibility bridge, font shaping, full native evaluator-backed scene editor,
GPU acceleration, or privileged
UI action broker. The native build adapter consumes a small Agel vector source;
it is not yet the full hosted evaluator running the standard-library UI stack.
These remain explicit later boundaries.

## v0.2.22

- **An effect hidden behind a first-class builtin:** the verifier inferred
  effects only from call-head symbols, so `(apply model-request ...)` or an
  aliased builtin carried no declared effect into promotion. Inference now
  counts any occurrence of an effect-bearing name, including inside quoted
  data, and the zero-authority canary remains the runtime backstop.
- **A policy that nothing consults:** the typed default-deny `Policy` existed
  without a caller. Every model process launch is now decided by a policy that
  admits only that provider's inference requests, before the executable
  allowlist, and every denial is audited. File effects on the copy-on-write
  workspace pass through the same decision point, with `Virtualize` meaning
  "stage in the overlay" rather than a silent allow.
- **A gate reachable only from an example:** verification, promotion and
  portable images are CLI commands. A promotion rechecks the evidence binding
  against the live world immediately before the commit; any intervening
  transaction makes it fail closed.
- **A log that disagrees with its world:** in image mode `:rollback` and
  `:restore` are refused rather than leaving an append-only image claiming to
  reconstruct a world it no longer describes. A failed save keeps the expected
  root so a concurrent writer is detected on the next commit.
- **A help postcard longer than its status line:** the graphical `:help` text
  had already outgrown the 256-byte line and was truncated silently. Its
  length is now a compile-time assertion.
- **Native forms accepted with hosted meaning but native limits:** `let`,
  variadic arithmetic and multi-form functions in the freestanding evaluator
  keep every existing bound. Bindings share the eight local slots, arithmetic
  stays checked, and the overflow of a stored multi-form body is rejected at
  definition rather than truncated.

The in-memory workspace broker is not host filesystem confinement, the model
adapters' policy is still enforced by the trusted Rust host, and the CLI's
proposal files are read from the operator's filesystem with the operator's
authority. Nothing here is a syscall boundary.

## v0.2.23

- **A heap that outgrows its rollback bank:** data values live inside the
  copied world banks, never in a separate arena, so a failed form, a rejected
  candidate and `:rollback` all discard or restore the heap together with the
  bindings that reference it.
- **Garbage that survives revisions:** a copying collector runs at every
  commit boundary with the bindings, agent states and queued messages as the
  only roots; a handle that is not reachable from them does not exist in the
  next revision. Collection failure (a live set that cannot fit the target
  arena) rejects the transaction rather than committing a partial heap.
- **A result handle that dangles:** results are rendered into the reply
  payload before collection and reported as text, so no frontend ever holds a
  heap handle across a commit.
- **Unbounded allocation inside one form:** cell and text arenas are fixed;
  exhaustion is a transactional error, and `eval` re-reads a datum only if its
  rendering fits one 256-byte payload.
- **A kernel that no longer fits its boot seed:** adding the heap pushed the
  image past the 254-sector BIOS load until the empty world became all-zero
  bytes; the build still rejects an oversized kernel, and each test script's
  build step is the first thing to read when native suites fail together.

Native data is still not shared with the hosted runtime's values, agent
messages are still not typed protocols, and the self-hosted toolchain still
does not fit the native bounds.

## v0.2.24

- **A chain anyone can recompute:** portable images were tamper-evident, not
  authenticated. A signed envelope now binds the root to an Ed25519 key, and
  `load_verified` accepts only the trusted signer; a mis-signed, foreign-signed,
  unsigned or torn primary falls back to a trusted previous generation or
  fails, never to `None`.
- **A reader downgraded past a signature:** the unsigned loader refuses a signed
  primary instead of falling back to an older unsigned generation, so a
  store that has been signed cannot serve stale state to a reader that was
  not told which key to trust.
- **Evidence nobody signed:** an A/B supervisor configured with a trusted key
  refuses unsigned promotion and verifies signed evidence over canonical
  bytes with domain separation, so evidence for another artifact or another
  supervisor cannot be replayed here.
- **Malleable signatures:** verification rejects `s >= L` and uses the strict
  equation, so each signature has one accepted encoding.
- **A key file readable by everyone:** `--keygen` writes the seed with mode
  0600 and refuses to overwrite an existing file; a mismatched `--trust-key`
  is refused at startup rather than producing commits the next start rejects.

The implementation is dependency-free and RFC-vector-tested but not
constant-time: signing keys must not be used where an adversary can time the
signer. Nothing here signs native disk slots, the seL4 manifest or kernel
images; those remain hash-checked or unsigned, as their documents say.

## v0.2.25

- **Four transcripts from one implementation:** every native backend, seL4
  included, had answered the contract by linking the same reference model, so
  byte-identical transcripts proved the boundary held and nothing about the
  semantics. A second implementation now exists, written from the contract
  document and the corpus without reading the model, and the seL4 broker runs
  it. The hosted suite requires both to reproduce the frozen transcript and to
  agree on every step, and still catches a deliberately widening variant.
- **Silent conventions where the corpus is silent:** the independent
  implementation records each unpinned choice at the point it is made, so
  the next contract minor can freeze them as corpus steps instead of
  discovering them as divergences.
- **Revocation that misses a grandchild:** the first draft of the independent
  implementation tombstoned a parent before checking its child and revoked
  four descendants where the corpus expects five. The corpus caught it. That
  is the kind of bug a single implementation cannot notice about itself.

Two implementations agreeing is not a proof of either. Both are unverified
Rust; the corpus is 81 steps; and the research kernels still run the reference
model, so on x86-64, AArch64 and RISC-V the boundary is diverse and the
semantics are not.

## v0.2.26

- **A disk that is ambient to the supervisor:** the ATA port I/O has left ring
  0. The storage driver is an unprivileged domain whose task-state-segment
  bitmap clears exactly the eight command-block ports and the alternate status
  port for the duration of its entries; every other world executing the same
  status read is refused by the processor, and CI asserts it as a contained
  general-protection fault alongside the console case.
- **A driver that decides policy:** the driver carries sectors and status codes,
  never text and never slot numbers. Which sectors are workspace slots, what a
  header means, when a generation is published and how a torn slot falls back
  are supervisor decisions made on bytes the driver merely moved.
- **A restarted driver serving an old conversation:** every request checks a
  generation-bearing handle first. The isolation suite loses the driver on
  purpose, replaces it at generation two, refuses the generation-one handle
  with `stale-generation`, and requires the replacement to read the same boot
  sector.
- **One bitmap for two drivers:** the grant is rewritten per entry, so the
  console driver never has the disk and the storage driver never has the
  console, with no per-domain bitmap to keep consistent.

Timers and serial input are still the supervisor's; the disk driver exists on
x86-64 only, since it is the only backend with storage; and a driver that
faults mid-write leaves the supervisor's slot protocol, not the driver, to
recover. The frame pool still never reclaims a replaced domain's frames.

## v0.2.27

- **Input ports ambient to the supervisor:** the interactive workshops no
  longer execute a single `in` on a serial or keyboard port from ring 0. The
  console driver answers nonblocking reads, and an 8042 driver domain whose
  bitmap clears exactly ports 0x60 and 0x64 delivers raw bytes with their
  origin flag. A world that is not the driver reading the controller's status
  port takes a general-protection fault, and CI asserts it.
- **A driver that blocks on a human:** driver reads are nonblocking by
  construction; the supervisor polls, so every driver entry stays inside its
  tick budget whether or not anyone is typing.
- **Decoding in the driver:** scan-code translation, modifier state, packet
  assembly and command parsing stay in the supervisor. The drivers move bytes
  and report a flag; a compromised driver can lie about bytes, not gain UI
  authority.
- **Echo through the supervisor's device path:** typed characters are echoed
  through the console driver like every other line, so the supervisor's
  last-resort console remains reserved for the recovery plane and panics.

Timers, networking and model brokering are still the supervisor's. The input
driver exists only on the x86-64 graphics build, and the frame pool still
never reclaims a replaced domain's frames.

## v0.2.28

- **A workshop that exists on one machine:** the interactive serial workshop
  had been x86-64 only, so the claim that the evaluator is contained on three
  machines rested on a non-interactive corpus elsewhere. It now runs on all
  three from one source, and the prompt-synchronized harness types every byte
  and checks every echo, prompt and revision on each.
- **Persistence faked where there is no disk:** the diskless machines report
  "no storage device on this machine" for `:save` and `:reload` and keep the
  in-memory editor; nothing pretends a cell survived a reboot.
- **A disk path compiled where it cannot run:** the image codec and slot
  protocol are gated to x86-64 with the device, and the few result types the
  shared workshop names are marked as unreachable there rather than silenced
  wholesale.

The AArch64 and RISC-V workshops have no storage, no graphics and no
keyboard; the recovery monitor on every machine is still in-memory policy.

## v0.2.29

- **Recovery state that forgets at reset:** the A/B monitor kept its trusted
  and candidate state in two booleans that every boot reset, so nothing a
  reboot did could ever roll anything back. The x86-64 record now lives at
  sector 288, is CRC-checked, and reads as empty rather than as anything else
  when it is absent or damaged.
- **A candidate that judges itself:** the boot counter is charged and flushed
  before the candidate's first cell is replayed, so a generation that crashes,
  hangs in replay or is powered off before it reaches the prompt cannot avoid
  the charge; after three such boots the supervisor replays the trusted
  generation without the candidate's cooperation.
- **Health evidence from the world under test:** `:verify` evaluates the
  `health` cell in an isolated candidate world that is discarded afterwards,
  through the same shared-page protocol as every other evaluation; a health
  cell that faults or exhausts its budget is contained like any other form and
  the candidate stays unverified.
- **A healthy trusted generation vouching for the candidate:** after a
  rollback the running generation is the trusted one, and its successful
  evaluations mark nothing; only the candidate generation itself can be
  verified by a healthy boot.
- **A rollback point overwritten by the next save:** the save path chooses the
  slot that does not hold the trusted generation, so promotion is what decides
  which generation the next save may not overwrite.

Not claimed: the record is unsigned and the disk is trusted to return what was
written, so a malicious disk chooses the boot plan. There is one record and two
slots, so exactly one earlier generation is ever retained. The kernel image
itself is not A/B selected, the diskless machines keep the in-memory policy
model, and the graphical workshop only reads the record.

## v0.2.30

- **A kernel that cannot be rolled back:** a workspace generation could be
  rolled back by the kernel, but a kernel image that faults at its entry or
  halts before the serial console had no one above it to notice. The BIOS
  stage is now that layer: it charges an unverified candidate's boot and
  flushes the selector before jumping to it, so nothing the candidate does or
  fails to do can avoid the charge, and after three it loads the trusted slot.
- **A candidate that vouches for itself:** verification is the running
  kernel's first successful evaluation, recorded only when the slot that
  booted is the candidate; a healthy boot of the trusted slot after a
  rollback marks nothing.
- **A rebuild that quietly boots the old kernel:** `build-boot.sh` writes the
  kernel to slot A and clears the selector, so a developer who rebuilds is
  running what they built rather than a promoted slot B from an earlier
  session.
- **Logic in 16-bit real mode:** the selector is nine plain bytes and the
  stage's decision is a handful of compares; a malformed selector (unknown
  version, a slot that is not A or B) is treated as absent by the stage and
  read as the default by the kernel, so they cannot disagree about which
  slot ran.

Not claimed: the selector and both kernel slots are unsigned, so a disk that
lies chooses the kernel; there is no cryptographic root before the BIOS stage.
Staging is a host tool; the guest cannot build a kernel. Slot B exists only
on x86-64, where the BIOS stage is. The stage does not verify the loaded
sectors, and a kernel that reaches the console and then misbehaves is judged
only by whether it evaluates a form.

## v0.2.31

- **Diverse boundary, single semantics:** on x86-64, AArch64 and RISC-V the
  object table behind the trap gate was the reference model, so three
  byte-identical transcripts said nothing a hosted run of the model had not
  already said. The research kernels now link the independent implementation,
  the same one the seL4 broker runs, and the reference model moves to the
  supervisor side of the isolation self-test, where it checks every one of
  the world's 81 answers as they are produced. A divergence between the two
  implementations now fails the boot of every native backend, not only the
  hosted suite.
- **An implementation that only agrees with itself:** every service domain
  (console, storage, input, evaluator) invokes the contract through the same
  independent object table, so the driver restarts, stale-handle refusals
  and shared-page protocol the suites already prove ran on it.

Not claimed: two implementations agreeing is still not a proof of either, and
both are unverified Rust. The reference model is now the oracle rather than
the thing behind the boundary; nothing checks the oracle except the frozen
transcript and the hosted comparison.

## v0.2.32

- **A slot anyone can fill:** since v0.2.30 whoever could write the disk could
  stage a kernel the boot stage would try. A staged candidate now carries a
  signature over the SHA-512 of its bytes, and the running kernel verifies it
  against the key it was built with before setting the admitted flag the
  stage requires. An unsigned candidate, a signature by another key, or a
  good signature over different bytes is refused and cleared, and CI stages
  each of the three.
- **Verification that reads the slot it is verifying:** the candidate is
  hashed sector by sector through the storage driver domain, so a driver that
  lies can only make a good candidate fail, never make a bad one pass: the
  signature is over what the kernel hashed, and the stage loads from the same
  disk.
- **Cryptography in the kernel:** the Ed25519 verifier and SHA-512 are the
  project's own dependency-free code, RFC-vector-tested on the host, compiled
  `no_std` with no allocator and no `unsafe`; the kernel holds only the public
  key. Field arithmetic is not constant-time, which matters for signing and
  not for verifying a public signature.

Not claimed: the trusted slot and the selector's own bytes are unsigned, so a
disk that lies about slot A still chooses the kernel, and the boot stage runs
what it loads without checking it; there is no root of trust before the BIOS
stage. The checked-in key pair is a development key that anyone with the
repository holds; it demonstrates the mechanism, and an operator who relies on
it must build with their own public key. Workspace generations and the
recovery record are still unsigned.

## v0.2.33

- **A recovery plane that existed on one machine:** the disk-backed record,
  the boot budget and the health oracle were x86-64 claims; AArch64 and
  RISC-V kept two in-memory booleans. Both now have a disk, and the whole
  persistence suite, watchdog rollback included, runs on each.
- **A device that reads and writes memory:** virtio is DMA. The driver domain
  is granted exactly one frame for that, tells the device that frame's
  physical address and nothing else, and every descriptor it builds points
  inside it; the device can read or scribble that one page and the domain's
  shared page is not it. The supervisor never hands a domain a physical
  address it did not allocate for that domain.
- **Registers on a shared page:** AArch64's transports are 0x200 bytes apart,
  so the granted page holds up to eight transports. The other seven are empty
  slots on this machine; a driver that touched one would find no device. The
  grant is a page because the translation unit is a page, and the document
  says so rather than pretending the grant is narrower.
- **A wait on a device:** the driver polls the used ring with a bounded count
  and reports a timeout status, so a device that never answers costs one
  request's budget, not the machine.
- **A stack that was too small:** the supervisor stack on the `virt`
  machines was 64 KiB; the workshop's bounded workspaces overflowed it into
  the image below, which the first storage-backed boot found as a supervisor
  trap in `memset`. It is 512 KiB now, like x86-64, and the failure is
  recorded here because a silent overflow into a writable section would not
  have trapped.

Not claimed: the driver assumes QEMU's coherent memory; on hardware with a
non-coherent DMA path the DMA frame would need cache maintenance the driver
does not do. Only modern (version 2) transports are driven. Nothing on the
virtio disk is signed, and the kernel image on these machines is an ELF QEMU
loads, with no slot selection.

## v0.2.34

- **A crash the tests chose:** the persistence suite had modelled one torn
  write, by hand, at the one place its author thought of. The supervisor's
  storage service can now be told to tear the N-th sector write and halt,
  and the suite sweeps N over every write of a save on every machine. The
  model is a real tear: the first half of the sector lands and the second
  half keeps its old bytes, then nothing further reaches the disk.
- **The write after "committed":** the sweep found that the recovery record
  is written after the generation's header is published and reported. A cut
  there leaves a whole new generation and a record whose checksum fails, and
  a record that fails its checksum reads as empty: nothing trusted, the
  newest generation booted. That is the safe direction, and it is now
  written down rather than discovered.
- **Injection as an attack surface:** `:cut-power` is a serial-workshop
  command in the supervisor; a world cannot reach it, and the graphics build
  does not compile it.

- **A wait that was too short for a real disk:** the storage domain's tick
  budget was half a second, and the shared CI runner's disk took longer than
  that to flush, so v0.2.33's own CI run failed on AArch64 with a timeout. The
  budget is three seconds now on every machine; it is still a bound, and a
  device that never answers exhausts it and is reported.

Not claimed: the cut models a tear and an immediate stop. It does not model
a write the device acknowledged and then lost, because QEMU writes through
to the image; a drive with a volatile cache that lies about flushes is
outside what this suite can show. The sweep covers workspace saves; kernel
staging and the selector are host-side writes.

## v0.2.35

- **A restart that costs frames forever:** every section above v0.1.5 has
  said that the frame pool never reclaims, so a driver restarted in a loop
  would exhaust it. Each domain now records the frames it was built from in
  a bounded ledger, a replaced domain gives them back before its replacement
  is built, and the isolation self-test on all three machines requires the
  pool to hold exactly as many frames after a restart as before the fault.
- **A frame that carries the dead domain's data:** frames are zeroed when
  handed out, whichever list they came from, so a successor built from a
  predecessor's frames starts from nothing.
- **A translation that outlives the frame:** a reclaimed frame is still
  named by the stopped domain's tables until that domain is dropped. The
  stopped domain never runs again: its stop reason is latched and every
  request checks it before entering. The assertion found the one frame the
  ledger missed, the storage DMA page allocated outside the build; it is
  inside now.

Not claimed: only replaced domains give frames back; the evaluator and the
workshop's domains live for the session, and no world can ask the kernel for
memory, the contract's memory group being outside every profile. The free
list is bounded and a frame that would not fit is leaked rather than
misfiled. Reclamation is compiled only where restart is, in the self-test
builds; the interactive workshops never replace a domain and carry none of
it, which the x86-64 image budget required.

## v0.2.36

- **A slot that is never given back:** the native actor table has eight
  slots and, until now, no way to free one; a session that spawned nine
  agents in its life was out of agents. `reap-agent` frees a slot, and the
  freed slot is the next one spawned into.
- **A handle that reaches the successor:** the reason slots were not reused
  before is the obvious one. A handle is now the slot number and the slot's
  generation; reaping moves the generation on, so every handle issued to the
  reaped agent is refused as `stale native agent`, distinct from the
  `invalid native agent` of a slot that never held one, exactly as a driver
  restart refuses `stale-generation`. The behavior a scheduler turn passes as
  `self` carries the current generation too.
- **A rectangle owned by the dead:** scene rectangles bound to a reaped
  agent become unowned rather than pointing at whoever takes the slot.

Not claimed: a generation is one byte and wraps after 256 reaps of the same
slot, at which point a handle from 256 occupants ago would be accepted; the
bound is stated rather than hidden, and the hosted runtime has no such wrap.
Actors still share the evaluator's globals and one protection domain.

## v0.2.37

- **A closure the evaluator refused to keep:** `def` of a lambda made inside
  a lexical call, and a lambda escaping a stored function, were refused
  because a stored function had nowhere to put captured values and silently
  dropping them would have changed meaning. A stored function now carries up
  to eight captured scalars, bound before its body runs, with a parameter of
  the same name shadowing a capture.
- **A capture that is not a value:** capturing a function is still refused
  with the same message as before, because a function is source plus
  captures and storing one inside another has no bound yet.
- **A bigger world:** each stored function grows by the eight captures, so a
  world is about 39 KB and the three transactional banks plus checkpoints
  stay well inside the evaluator domain's 512 KiB stack; `:limits` is
  unchanged.

- **An image that grew 45 KB from one boolean:** the empty world must be
  all-zero bytes so the banks are zero-filled rather than carried in the
  image, and a niche inside the new capture field let the compiler encode an
  enum tag as a non-zero byte there. Explicit zero tags on the two enums
  restore the property, and the budget check in the build script is what
  caught it.

Not claimed: captures are copied at creation, so a closure sees the values
its lexical context had then, not later assignments, which is what lexical
capture of immutable locals means here; nothing about authority changed.

## v0.2.38

- **A backend that ran only the contract:** the seL4 system answered the 81
  corpus steps and contained a faulting world, and that was all; the
  evaluator, the thing that runs untrusted programs, had never run there.
  The world domain now runs the same native evaluator source the research
  kernels compile into their evaluator domains, over the forms their
  isolation self-test checks, and CI requires the same answers.
- **Language state on an seL4 stack:** the evaluator's three transactional
  world banks live on the world domain's stack, so the domain is given
  512 KiB by the system description, the one place authority and resources
  are written. A world that overran it would fault to its parent, the
  recovery domain, as the deliberate fault already does.
- **A shared source, not a shared binary:** the evaluator is compiled twice
  from one file, once into each backend; nothing links the seL4 build to the
  research kernel's, and the seL4 domain reaches the console only through
  the serial domain's page.

Not claimed: the seL4 world runs a fixed corpus, not an interactive
workshop, and has no disk, no workspace and no recovery record. The
evaluator's bounds are the same as everywhere else and are not enforced by
seL4; they are the program's own.

## v0.2.39

- **A group declared and never specified:** the contract had named
  `frame.*` and `as.*` since v1.0 and defined nothing about them, so every
  backend refused them and nothing said what an answer would have meant.
  Contract v1.1 specifies the memory group over a numbered frame budget and
  a page-indexed frame window, and both hosted implementations answer 35 new
  corpus steps identically: a mapping cannot carry a right its capability
  lacks, a mapping can only lose rights in place, a share cannot widen, the
  budget pushes back, reclaiming revokes every share and the share fails
  closed, and a shared handle or the domain's own frame cannot be reclaimed.
- **A profile a backend cannot make real:** the research kernels' frame
  window is not yet backed by their page tables, and under Microkit's static
  system description a server domain cannot change another domain's
  mappings at all. Both therefore publish the v1.0 profile and refuse the
  memory group, which the corpus records as their transcript. The frozen
  transcript is now one per published profile, and the claim is stated per
  profile rather than stretched.
- **A step that answers differently under each profile:** the corpus is one
  list, so the invariants that concern memory accept `invalid-operation`
  where the group is not published and require the specified refusal where
  it is.

- **A right the machines never grant:** a mapping that is both writable and
  executable is refused as `not-permitted` after the capability has been
  found sufficient, so the contract cannot promise a page the research
  kernels' page tables would refuse to build.

Not claimed: no backend maps memory yet; the memory group is specified
semantics with two agreeing hosted implementations, compiled out of the
x86-64 workshop images to keep their budget. The seL4 backend will not
publish it under Microkit as it stands.

## v0.2.40

- **A mapping that was only bookkeeping:** v0.2.39 specified the memory
  group and no backend mapped anything. The research kernels now back the
  frame window with their page tables: after every memory operation the
  object table accepts, the supervisor reconciles the page tables with the
  window, page by page, so the two cannot disagree for longer than one trap.
- **Allocation at trap time:** a domain is built with the five frames behind
  its budget and the tables under its eight window pages, and `frame.map`
  at trap time only rewrites a leaf entry; nothing in the trap path touches
  the frame pool, and a budget exhausted is the object table's answer, not
  an allocator's.
- **Rights the tables cannot express:** `execute` maps as read-and-execute,
  because these machines have no execute-only page, and that is stated;
  `write` with `execute` is refused by the contract before it reaches a
  table, so no domain is ever handed a page it can both write and run.
- **A world that tests its own mappings:** the isolation self-test has a
  world write through a mapping and read the value back, take a page fault
  on a write to a mapping protected to read-only, and take a page fault on a
  read of a page it unmapped, on all three machines; a divergence between
  what the object table says and what the tables enforce would fail the
  boot.
- **A command code reused:** the first build gave the window-touch command
  the code of the x86-64 divide-by-zero provocation, and the divide world
  page-faulted reading the window instead. The self-test's "contained in an
  unexpected way" check caught it before anything was claimed.

Not claimed: the window is eight pages and the budget five frames per
domain, fixed at build; no domain can map another's frames, because
`frame.share` derives a capability inside one domain's space and there is
no operation that crosses domains. The seL4 backend still publishes v1.0.
The x86-64 workshop images link the implementation without the memory group
and publish v1.0.

## v0.2.41

- **Code that did not come from the kernel image:** every world so far ran
  code the kernel was built with. A process is code read from the disk,
  and the loader treats it as what it is: the image's CRC is checked
  against the table before any byte of it is believed, the ELF must be a
  static executable for this machine, every segment must lie inside the
  process window, be page-congruent with its file offset, share no page
  with another and never ask to be writable and executable, and the entry
  must lie inside the window. Anything else is refused with the reason,
  and the domain is never built.
- **A process that misbehaves:** it is a protection domain like every
  other. The hostile program writes where it was never mapped and is
  contained with the same page fault, on all three machines; a process
  that never yields is stopped by the tick budget.
- **A request protocol on a shared page:** the process's words are data.
  The supervisor bounds the write length to the block area, serves only the
  console descriptors, and answers everything else `-ENOSYS`; a process
  cannot name a path, a device or another domain, because there is nothing
  in the protocol that would.
- **Frames that come back:** a process's pages are recorded with its
  domain's frames and reclaimed when it ends, and the test loads the same
  program twice to show it.

Not claimed: the program region is unsigned and unverified beyond a CRC, so
whoever writes the disk chooses what runs, exactly as for the workspace and
the kernel's trusted slot. There is no capability set: a process can write
to the console and nothing else, and that is policy in the supervisor, not
a capability the process holds. No files, no namespaces, no C library.

## v0.2.42

- **A bigger kernel slot, the same trust:** the BIOS stage now loads 508
  sectors in four 127-sector transfers instead of 254 in two. Nothing about
  what is trusted changes: the selector is still read first, an unverified
  candidate is still charged its boot before its first instruction, and the
  kernel still hashes and verifies a staged candidate over the slot's signed
  length, which the length check now bounds at 508 sectors.
- **Images that predate the layout:** an image laid out before v0.2.42 has
  its workspace, records and slot B where this layout expects kernel slot A
  and nothing. The build rebuilds such an image as a new baseline; the kernel
  does not guess at the old layout, so an old workspace is simply absent
  rather than misread.
- **Fewer panic paths in the kernel image:** the SHA-512, curve and
  scalar-decoding code the kernel links from `agel-integrity`, and the
  contract model's object lookups from `agel-kernel-abi`, no longer index in
  ways the compiler must guard with a panic. A panic in a supervisor is a
  halt, so every one removed is a way the machine cannot stop; the change
  also removes the building machine's source paths from the image.

## v0.2.43

- **A path is not authority:** a process opens a name through the namespace
  the operator granted at `:exec`, a root entry and three rights. The
  supervisor refuses a write or a create the namespace lacks before the
  filesystem service sees the path; the service resolves the path from the
  process's root and refuses `..` there; so a file outside the namespace is
  `ENOENT` however it is spelled, and the tests spell it both ways.
- **A filesystem that cannot reach the disk:** the service is an
  unprivileged world with no device. Every sector it wants is a request the
  supervisor relays through the storage driver domain, bounded to the
  region the service owns; a sector outside it is `EACCES`, and the
  service's arithmetic cannot change that.
- **Descriptors that die with their service:** a descriptor records the
  service generation it was opened under, and `:fs-restart` makes every
  earlier one `ESTALE`. This is the same rule as driver handles, and it is
  implemented but not yet exercised by a test, because a process cannot
  outlive one `:exec`.
- **No panics in the service:** the filesystem world is written without
  indexing the compiler must guard, so a malformed directory sector is
  refused by its checks rather than by a panic that would leave the world's
  text for the kernel's and be contained as a fault.
- **The region is data:** the service believes the directory sectors it
  reads. A damaged or hostile region can make it return wrong names and
  lengths, never a sector outside the region and never anything in the
  supervisor; the superblock magic is the only integrity check.

## v0.2.44

- **C code in a process is still a process:** a C program built against
  `agel-libc` runs in the same protection domain as a Rust one, with the
  same window, the same tick budget, the same namespace. The library adds
  no authority: every function is a request the supervisor already
  bounded, and `errno` is the negated answer.
- **The C boundary is the unsafe part:** the library's `unsafe` is reading
  a NUL-terminated string and filling a caller's buffer, both of which trust
  the C program about its own memory. A C program that lies to its own
  library corrupts its own domain and nothing else; the supervisor reads the
  request words, never the program's pointers.
- **Position-independent images on x86-64:** the GOT a position-independent
  C program carries is placed in the writable segment by the linker script,
  so no section shares a page with another segment and the loader's checks
  hold for C exactly as for Rust; an image that violates them is refused
  with the reason, as `c-hello` was until the script said where the GOT
  goes.
- **No floating point:** a process gets no floating-point or vector state,
  so the C programs are compiled without SSE, NEON or the RISC-V F and D
  extensions. A program that uses them anyway takes a fault the supervisor
  contains; the library does not hide that.

Not claimed: the library is not audited against a C standard, `malloc` is
a bump arena with no reuse and no guard, and `printf` is a subset. None of
that is a supervisor concern; all of it is a program's.

## v0.2.45

- **No `fork`:** a child receives exactly the two descriptors its parent
  names and its parent's namespace or a read-only view; nothing is
  inherited by default, so a parent's open files, pipes and rights do not
  leak into a child that was not meant to have them. This is the
  requirement in `deployment-targets.md` that `fork` had to be decided
  rather than inherited, decided.
- **A child is bounded by its parent:** it cannot be given a wider
  namespace, its descriptors are copies of ones the parent held, and the
  parent's rights on a copied file descriptor are the child's; a child
  spawning a grandchild passes on no more than it has.
- **Pipes are the supervisor's:** the queue lives in supervisor memory, a
  process sees only its block area, and the end counts are the supervisor's
  arithmetic, so a process cannot forge an end or keep a stream open by
  lying about what it holds.
- **Blocking cannot hang the workshop:** a process blocked on a wait or a
  pipe is retried each pass, and when nothing live can make progress every
  blocked process is stopped and reported. A child the machine stops is
  reported at once, and its parent's `wait` answers a signal.
- **Ids are table slots:** a child id is its slot in a four-entry table for
  this `:exec`, and `wait` checks that the slot holds the caller's child;
  a reaped slot may be reused by a later spawn, which is why a parent must
  not wait twice for the same id.

Not claimed: no arguments or environment cross to a child, so the only
thing a child learns from its parent is what it reads on descriptor 0;
there is no way to stop a child from outside; and the scheduler is a round
robin with no notion of fairness beyond one entry per pass.

## v0.2.46

- **Arguments are data in the process's own page:** the supervisor writes
  the argument block into the payload area before the process first runs
  and the count into a word; the library copies it out and builds `argv`
  in its own memory with a terminator it writes itself, so a block the
  supervisor left unterminated cannot run `argv` off its end. A child's
  arguments come from its parent's payload, bounded to the area.
- **Seek is bounded:** a descriptor's offset can be moved only inside the
  file size the filesystem allows, and only on a file; the length used for
  `SEEK_END` is what the supervisor last saw for that descriptor, so two
  descriptors on one file may disagree about its end until one reads it
  again, which is a correctness limit and not a boundary one.
- **A third-party source is still a process:** the SHA-256 built
  unmodified runs with the rights of the program around it and nothing
  more; source compatibility adds no authority, only the ability to build.
- **The heap is the process's own:** a heap corrupted by a program's bug
  corrupts that program's domain; the allocator checks that a pointer it is
  handed lies in its arena at a block boundary before writing a header,
  which keeps a stray `free` from writing outside the arena, not from
  confusing the program that misused it.

Not claimed: the library is not audited against a C standard, and the
breadth is what the tests exercise.

## v0.2.47

- **Assets are data the compositor bounds:** a font atlas is read from the
  asset region, checked against its CRC-32, and mapped read-only into the
  compositor at a fixed window; the compositor checks every offset the
  atlas names against the length it was told before reading it, so a
  broken or hostile atlas draws nothing and reads nothing outside itself.
  The supervisor parses only the metrics it needs for layout, with the same
  bounds, before any frame is drawn.
- **The compositor still holds nothing but pages:** the atlases are more
  read-only pages in a domain that already had only its framebuffer as a
  device; blending reads the framebuffer it could already write. The record
  format grew three operations, each validated as the others are.
- **The seed carries its fonts:** the build installs the atlases on every
  rebuild and the graphics image refuses to boot without them, rather than
  drawing with a wrong or absent face; the serial images ignore the region.
- **Fira Sans and Fira Mono** are bundled under the SIL Open Font License
  with the license text beside them; the atlases are derived works of the
  fonts and carry no other code.

Not claimed: the desktop is not yet at its native resolution, has no icons
beyond drawn shapes, no windows a process owns, and no pointer cursor that
is more than a square; the look is a first pass at COSMIC's, not a port of
it.

## v0.2.48

- **Setting the display is the supervisor's, through two ports:** the
  Bochs interface is probed by its identifier and, when present, given the
  scene's size; the framebuffer address is the one the BIOS mode reported
  and the mapping is the same device grant to the same compositor, checked
  against the same 16 MiB limit. Without the interface nothing is written
  and the BIOS mode stays.
- **Sprites are bounded like glyphs:** the sheet is an asset the
  compositor checks by magic, count, and every sprite's offset and size
  against the sheet's length; a tint is a colour like any other record's.
- **Input outlives painting:** the queue between the input driver and the
  session holds 256 bytes and is drained between records, so a frame that
  takes long under emulation drops nothing the controller held; a burst
  larger than the queue is dropped by the queue, not misread.

Not claimed: the fallback mode scales geometry but not text; the sprites
are the project's own drawings, not COSMIC's icon theme.

## v0.2.49

- **The same loader, the same rules, on the desktop:** the graphical
  workshop runs a program exactly as the serial one does, through one
  shared module: the CRC, the ELF checks, the namespace, the frame
  reclamation and the process table are the same code. The only new
  surface is where the console output goes: a tee to the serial driver and
  to a terminal panel the supervisor owns.
- **The terminal panel is bounded:** sixteen rows of eighty-four bytes,
  scrolling, with every byte outside printable ASCII shown as `?`; a
  process cannot write records, colours or positions, only text, and it
  cannot write past the panel.
- **Reclamation is compiled into every image now:** the graphics image
  reclaims a process's frames like the serial one, so a process run twice
  costs no lasting frames; the size-era gates that left it out are gone.

Not claimed: a process cannot draw into a window of its own, and the
terminal takes no input from the keyboard for a process (a process's
descriptor 0 is what it was given at `:exec`, which is nothing).

## v0.2.50

- **A click is a typed command:** every pointer action the desktop owns
  becomes the same line the operator could have typed, handled by the
  same code, echoed on the same console. The launcher lists names from
  the program table and runs one through `:exec` with the operator's own
  namespace; nothing a click does is unavailable, or different, at the
  keyboard.
- **The clock is a driver domain:** two CMOS ports, granted to one world
  that answers six numbers; the supervisor reads it at boot and while idle
  and never touches the ports itself. A clock that does not answer leaves
  the panel showing the workspace's name.
- **Motion is coalesced, not trusted:** queued pointer packets move the
  pointer before a repaint and a press ends the run where it landed; the
  8042's queue still overflows on a burst larger than it, which loses
  motion, never authority.

Not claimed: the editor tile does nothing yet, and the launcher shows the
program table's first eight names.

## v0.2.51

- **A window is a bounded grant, not a framebuffer:** a process never
  sees pixels or the compositor. It hands the supervisor records in the
  coordinates of its own content, and the supervisor refuses any record
  that is not one of the admitted operations or reaches outside the
  content, before anything is painted; a refused request draws nothing.
  The full-screen gradient and the shadow are not admitted, so a process
  cannot paint over the desktop or darken what is around its window.
- **What is kept is the supervisor's:** the accepted records live in the
  scene, translated to the window's place at materialization, and are
  repainted with the desktop; a window outlives its process, and no
  process can draw into a window it did not ask for: the process table
  names the drawing process by its slot, and the owner is cleared when it
  ends, so a later process in the same slot is `-EBADF`.
- **Bounded:** two windows, 24 records each, eight per request, titles
  of 28 bytes, sizes from 64×48 to 1280×720; a draw is refused, never
  truncated. The frame budget is 224 records and a frame past it fails
  the paint rather than dropping records silently.
- **Closing is a typed command:** the close control becomes `:close N`,
  echoed on the console like every click.

Not claimed: a window receives no input; a process runs to its end before
the desktop reads the next input, so a window cannot react or animate;
the process side's records are trusted only after the check, and the
compositor still validates every record it receives as before.

## v0.2.52

- **Input reaches a process only through its window:** a press is
  queued for the window it landed in, in that window's coordinates, and
  only while a live process owns it; a key goes to the window that has
  the keyboard, and only while its owner lives. A process never sees the
  pointer elsewhere, the workshop's line, or another window's events;
  `event` on a window it does not own is `-EBADF`.
- **The keyboard is lent, not taken:** a window has it from opening or a
  click in it, and the workshop takes it back with a click; when the
  owner ends, keys return to the workshop by themselves. The queue is
  eight events and drops the oldest, so a process that never reads loses
  input, never memory.
- **A listening process cannot hold the desktop:** the table runs in
  passes with a fixed budget per idle turn, the desktop reads its inputs
  between them, and a process that only computes still runs to its end as
  before. On the serial workshop, where nothing delivers events, a
  process asleep on one is stopped as blocked, as a deadlock is.
- **The same table, the same rules:** `start`, `step_run` and `finish`
  are the code `exec` was, split; spawn, pipes, wait and files are served
  in a pass exactly as before, in either workshop.

Not claimed: one running program at a time; no release, motion or
modifier events; the budget of sixteen passes is a constant, not a
scheduler.

## v0.2.53

- **A held pointer belongs to one window:** after a press in a window's
  content, motion and the release go to that window's owner and nowhere
  else, until the button is up; the desktop takes no other action from
  the held pointer, so a process cannot be made to act on a drag that
  started elsewhere, and a drag that starts in a window cannot reach the
  desktop's controls.
- **A window moves only by its own header, only by the pointer:** a
  process cannot move, raise or resize its window or any other; the
  order and the places are the scene's, changed by the operator's
  presses, and a window stays on the screen below the panel.
- **Stacking is a real order:** the hit test walks the windows from the
  front, so a press lands on what the operator sees; a covered window
  receives nothing through the one above it.
- **Motion is coalesced, never lost to the process's benefit:** a window
  keeps the latest motion in place of an older one, so a process that
  reads slowly sees the pointer's position, not a stale one, and the
  queue of eight still bounds what a window holds.

Not claimed: no resize; a process's window can be dragged over another
process's window, which is the operator's doing and covers it; no
modifier keys.

## v0.2.54

- **Pixels only:** the shadow's falloff, the edges and the press states
  change what the compositor paints and nothing about what any domain
  may do; the compositor still validates every record, the shadow's
  blur is still bounded at 64, and a process still cannot request a
  shadow.
- **A press state is the scene's:** it is set by the operator's press on
  a control the desktop owns and cleared by the release; no process can
  set or read it.

## v0.2.55

- **EL2 is left, not used:** the entry drops to EL1 once and installs no
  vectors at EL2; no path returns there, and the bring-up refuses any
  exception level but EL1, so the kernel never runs with hypervisor
  privilege it does not account for.
- **A board is a compile-time fact:** every physical address comes from
  one module selected by a feature; nothing is probed, so a kernel built
  for one board cannot be talked into another's device window.
- **No disk means no disk:** the board's SD controller is not driven, the
  workshop says so at boot and refuses `:exec`, and nothing pretends to
  persist.

Not claimed: the Pi 5's addresses (unverified without the board), the
device tree (unread), the other cores (parked by the firmware and never
started), and the firmware itself, which loads the image and is trusted
as it is on every Pi.

## Surfaces the scope adds

Recorded before the code exists, because it is easier to design against a
written list than to remember one. Nothing here is implemented; see
[`deployment-targets.md`](deployment-targets.md).

### The POSIX personality

- **A path that grants itself.** Unix succeeds at `open` because of who you are.
  If the personality reproduces that, every capability boundary above it becomes
  decorative: a program that can name a resource can reach it. A name must
  resolve through a namespace capability the process was given, and a name
  outside that capability must be unreachable however it is spelled.
- **Descriptors that outlive their authority.** A file descriptor is a derived
  handle and is subject to the derivation rule — equal or weaker, never widened
  — and must fail closed with `stale-generation` when its backing service
  restarts, exactly as the console driver's handles now do.
- **`fork` as an authority copier.** A call whose default is "duplicate the
  entire authority set" is at odds with everything above it. What `fork` means
  here is a decision to be made, not a semantic to be inherited.
- **A C library as a trusted computing base.** Writing it in safe Rust bounds
  the memory-safety failures, not the logic ones, and says nothing about whether
  a program running on it is contained. Containment comes from the protection
  domain and the capability set, and the personality must not become a place
  where those are quietly widened for convenience.
- **Compatibility as a pressure to weaken.** Every POSIX program that does not
  run is an argument for an exception. The exceptions are where ambient
  authority comes back.

### Local inference

- **A model as a way in.** Weights are attacker-influenceable data parsed by a
  large amount of code. The parser is a boundary and belongs in a domain that
  holds nothing else.
- **A driver as a way around.** Any accelerated inference path that needs a
  proprietary kernel-mode driver puts unreviewable code in the privileged
  position the whole architecture exists to keep small. That is the rule that
  keeps this from drifting back to Linux as the core.
- **Inference as unbounded work.** A request with no budget and no deadline is a
  denial of service that arrived through the front door.

None of this is mitigated today: there is no POSIX layer, no filesystem, and no
local inference in this repository.
