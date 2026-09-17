# Review: v0.2.20 through v0.2.88

Reviewed on 2026-09-17 against `c768403` (v0.2.88). The range contains 553
changed files, 112,870 added lines and 2,654 removed lines. Of these, 178
files and 66,677 added lines are under the vendored DOOM directory.

This is a cross-system review with targeted reproductions and fixes, not
an independent security audit or a claim that every vendored line has been
verified. The review followed the runtime/compiler, world persistence,
effects/model boundary, kernel memory and process services, boot/install
paths, native applications and host bridge. Existing tests were used as
conformance evidence, not as proof of universal correctness. No language
redesign or release-version change is included.

## Findings fixed

| Area | Before | Change and regression coverage |
| --- | --- | --- |
| Effect approval | An allowed workspace write/delete committed **all** unrelated virtualized changes. | Apply approval only to the named path. Mixed virtualized writes/deletes remain rollbackable. `agel-effects` regression. |
| Program installation | An oversized replacement could overwrite live programs while compacting, then fail the capacity check. | Validate the table and names, reject empty programs, and preflight the complete layout before writing. Four Python tests include byte-for-byte preservation on failure. |
| Agel x86 backend | An IR tail call inside an inlined `let` operand could discard an enclosing addition's continuation. The reproducer returned 0 instead of 5. | Carry physical-frame tail context through inline emission. Host regression plus guest result 5; the million-iteration tail loop still works. |
| Domain reclamation | Pages added after domain creation were recorded, but intermediate page tables were not; repeated process creation could exhaust frames. | Record all allocations during domain extension, including partial failures. Reclamation tests force new table boundaries on x86-64, AArch64 and RISC-V. |
| World decoding | Recursive values had no decoding-depth bound; sorted canonical maps accepted duplicate/out-of-order keys. | Central depth guard, bounded environment chains and shared ordered-map decoding. Existing canonical bytes/digests remain unchanged. Extremely deep snapshots can now be rejected. |
| File writes | Independent append descriptors used stale offsets; writing beyond EOF could expose retained bytes in the gap. | Resolve append offset in the serialized filesystem service; zero-fill holes before writing, preserving the shared-buffer payload. Native regressions on all three architectures. |
| Descriptor authority | Console writes ignored descriptor write rights; `O_RDWR` could bypass a missing namespace read right; read-only spawn could add read authority. | Check requested rights independently and attenuate inherited namespace rights. Native stdin-write regression and code-path review. |
| Kept worlds | A known oversized world was truncated/written even though the loader could not read it back. | Reject saves above the 64 KiB read limit before opening/truncating the previous save. Guest restart regression. This is not an atomic-replacement or power-loss guarantee. |
| Hosted tail calls | Tail calls through the `apply` builtin still consumed Rust/evaluator call depth. | Unwrap forwarded applications before entering a frame, including `apply` applied to itself. A 5,000-step loop passes with call depth 8; non-tail recursion remains bounded. |
| Verification keys | Small-order Ed25519 public keys were accepted at construction. Such keys do not provide the expected signature assurance. | Reject points whose eightfold multiple is the identity. Order-1/2/4 regressions and existing RFC vectors pass. This does not replace an independent cryptographic review. |
| Model/game bridge | Separate serial reads/cursor updates could lose data; stale echoes could satisfy repeated characters; EOF could loop forever; Rust debug strings were used as JSON. | Consume complete lines under one lock, track echoes from before each write, report monitor EOF, and use `serde_json` for host-side protocol/dataset encoding. Socket tests and a complete echo-policy episode. |

## KISS and DRY

The changes remove the broker's clone/rollback/reconstruction path and the
obsolete data-only frame-ledger push API. File reads use the existing
file-request validation helper. Canonical map validation lives in one
reader rather than repeated loops. Serial submission shares one echo
routine. The hosted bridge uses an established JSON codec instead of a
second handwritten escaping/parser implementation; no JSON dependency was
added to the freestanding kernel or language core.

The architecture-specific page-table formats remain separate. Combining
them into a generic framework would add indirection at a hardware boundary
without eliminating their distinct invariants. Similarly, the small Agel
backend gains a tail-context argument, not another optimization pass.

## Remaining boundaries and next work

These are follow-up design/inspection findings, not claims that this patch
solves all persistence, isolation or compiler semantics:

1. **Compiled execution:** `agel/native-x86` remains an integer-subset
   research backend, distinct from the managed Rust JIT. Its closure static
   links refer to stack frames; escaping closures and captured frames passed
   through frame-reusing calls need an explicit lifetime strategy or rejection.
   Emitted code also needs fuel, allocation bounds, arity/type checks and
   numeric-overflow conformance before arbitrary agent-produced programs are
   accepted as equivalent to hosted evaluation.
2. **Effect identity and replay:** the model journal is useful, but it is
   not a journal for arbitrary file/process/clock effects. Specify what is
   transactional, what can escape rollback, and how dispatch identity survives
   durable restore. Canonical files are trusted state, not authenticated
   capabilities; world-ID allocation, counters and delta/base identity deserve
   a separate import/restore invariant review.
3. **Filesystem identity:** descriptors retain directory entry indices;
   unlink/reuse needs stable object identity or generation checking. Cached
   `SEEK_END` lengths are stale after another descriptor changes the file.
   Zero-length pipe reads and partial heap-growth failures also need dedicated
   POSIX conformance cases. Keep the documented POSIX subset explicit.
4. **Durable replacement:** host program repacking and guest kept-world saves
   are not crash-atomic. The preflight fixes protect against known validation
   failures, not I/O interruption. A staged payload plus committed generation
   is preferable to truncation-in-place when extending persistence guarantees.
5. **Resource limits:** source/file readers should report limit exhaustion
   explicitly rather than silently return a prefix. Bound host bridge transcript
   retention for long episodes. Benchmark world cloning/history retention before
   selecting persistent collections or a new collector.
6. **Assurance:** the handwritten cryptographic implementation and native
   `unsafe` boundaries need specialist review beyond regression tests. Keep
   research kernels, the seL4 backend, and verified configurations clearly
   distinguished; using seL4 does not verify the surrounding Agel services.

## Research and priorities

The recommendation is to preserve the built-in agent model: explicit
capabilities, isolated worlds, deterministic scheduling and inspectable
model effects. Harden those mechanisms before adding compiler tiers or
turning self-hosting into the primary success metric.

- **Agent authority:** the May 2026 paper [Toward Securing AI Agents Like
  Operating Systems](https://arxiv.org/abs/2605.14932) argues for applying OS
  isolation and mediated communication to agents. [CaMeL](https://arxiv.org/abs/2503.18813)
  separates untrusted data from control/authority. Applied to Agel, this suggests
  carrying provenance and capability scope through tool results and enforcing
  policy at dispatch. A model's explanation or a retrieved document must not
  itself grant authority. These are design recommendations, not a claim that
  Agel implements either research system.
- **Small OS mechanisms:** [LionsOS](https://trustworthy.systems/projects/LionsOS/)
  emphasizes simplicity and explicit component interfaces. Retain Rust for
  protection and resource accounting, with replaceable Agel policy above it.
  Check assurance statements against the exact [Microkit 2.3.0 manual](https://docs.sel4.systems/projects/microkit/manual/2.3.0/)
  and [verification roadmap](https://backend.sel4.systems/roadmap.html), rather
  than treating all architectures/configurations as equally verified.
- **Effects and resources:** August 2026 [Cambria](https://arxiv.org/abs/2608.27798)
  explores abstract resource types in parametrized effect handlers. Its useful
  direction for Agel is opaque, scoped resource handles with interchangeable
  implementations. It does not justify adopting a new type system wholesale.
  July/August 2026 [Yarrow](https://arxiv.org/abs/2607.15876) formalizes the
  difficult interaction between regions and nonlocal/multi-shot effects;
  Agel's simpler handler/turn model should remain simple unless a concrete
  use case pays for those additional lifetime rules.
- **Memory:** [Perceus](https://www.microsoft.com/en-us/research/publication/perceus-garbage-free-reference-counting-with-reuse/)
  is an older, relevant reference for ownership-aware reuse, not evidence that
  reference counting solves arbitrary cyclic agent graphs. Measure transaction
  copies and retained state; prototype collector roots and failure behavior
  before committing to a collector or estimating its size in lines of code.
- **Compiler trust:** [Wheeler's DDC](https://dwheeler.com/trusting-trust/)
  requires independent compiler assumptions and a staged executable comparison.
  Agel's host/guest corpus equality is differential conformance coverage.
  The earlier research notes are corrected to avoid presenting it as DDC or a
  universal semantic proof.

Practical order: close compiled execution limits and closure lifetimes;
specify/replay all externally observable effects; measure memory costs;
add provenance that separates evidence from authority; then expand backend
coverage and self-hosting. Recent papers are research inputs, not a reason
to expand the trusted core speculatively.

## Validation

All checks below passed locally:

- `cargo test --workspace`: 217 tests passed, including existing language,
  JIT, persistence, model, supervisor and cryptographic suites.
- Workspace formatting, strict Clippy (`--all-targets -- -D warnings`),
  and documentation generation with warnings denied.
- Freestanding core/stdlib/integrity Clippy, POSIX release Clippy, and 16
  kernel feature/architecture combinations, including Raspberry Pi 4/5 builds.
- `python3 scripts/test-install-program.py`: four installer regressions.
- `scripts/test-isolation.sh`: matching 118-step contract transcripts,
  contained faults, preemption/restart and full frame reclamation on all
  three architectures.
- `scripts/test-breadth.sh` on x86-64, AArch64 and RISC-V: 39 checks per
  architecture, including append, sparse writes and stdin permissions.
- `scripts/test-files.sh`, `test-spawn.sh`, `test-native-persistence.sh`
  and `test-power-cut.sh` on all three architectures.
- `scripts/test-kernel-rollback.sh` and `scripts/test-install.sh`.
- `scripts/test-agel-process.sh`: guest runtime/compiler, kept-world restart
  and oversized-save preservation, six emitted programs including the new
  nested-call regression and million-iteration tail loop.
- `scripts/test-play-bridge.sh`: four model requests/replies and recorded
  game steps using the local echo policy; all JSONL records parse.
- `git diff --check`.

The Raspberry Pi checks are builds, not physical-board tests. The seL4 SDK
suite and the entire graphical CI matrix were not rerun in this review.
The power-cut suite covers the native workspace generation protocol, not
crash-atomic replacement of POSIX kept-world files or host program repacking.
