# Agel versioning

Agel uses Semantic Versioning, but remains explicitly pre-production. Project
releases stay below `1.0.0` until the language, native runtime, recovery model,
and supported operating-system surface are ready for a stable compatibility
promise.

- `v0.0.5` through `v0.0.9` identify the hosted bootstrap milestones.
- `v0.1.0` through `v0.1.7` identify the native workshop milestones.
- Patch releases repair a milestone without claiming a new capability rung.
- `v0.2.0` begins the agentic desktop line with an Agel-authored scene and live
  change protocol. `v0.2.1` adds the Agel-authored default shell, layout compiler,
  display-list contract, and semantic hit-testing. `v0.2.2` adds Agel-authored
  vector primitives and UI-to-vector compilation plus a bounded deterministic
  SVG output service. `v0.2.3` adds the VBE boot handoff and an unprivileged
  native compositor consuming an Agel-authored bounded vector stream, including
  retained-frame recovery. `v0.2.4` adds PS/2 and serial input, a visible native
  Agel command surface, and validated live scene commit, rejection, inspection,
  and rollback. `v0.2.5` adds the tested graphical kitchen-sink program and its
  exact 2880×1800 browser-rendered screenshot. `v0.2.6` joins the graphical
  command surface to the protected native evaluator and crash-tolerant source
  workspace, including replay-validated save and reconstruction after reboot.
  `v0.2.7` adds eager-safe and bounded lexical fixed points, immutable
  convergence, and a transactional agent fixed-point driver with bounded
  tracing, explicit model transitions, and message-ordered evolution. `v0.2.8`
  is the first downward bootstrap of executable agents into the freestanding
  evaluator: bounded native mailboxes, deterministic scheduling, atomic
  behavior turns, inspection, and contained fault recovery run inside the
  graphical OS. `v0.2.9` repairs native punctuation/modifiers and adds a
  host-layout graphical console for Slovak/Unicode text composition and paste
  without mouse capture. `v0.2.10` restores QEMU's direct window as the default
  and connects committed native Agel scene data to the compositor, including
  an Agel dock library, actor-driven repaint, rollback, and reboot replay.
  `v0.2.11` adds the native agent workbench: pointer-to-agent actions, keyboard
  focus, source inspection, isolated candidate previews, turn-boundary behavior
  replacement, explicit promotion/discard, and persisted source upgrades.
  `v0.2.12` repairs failed-save recovery and process deadline/output enforcement,
  strengthens process audit identities, and removes blanket dead-code suppression.
  `v0.2.13` expands the Agel-written functional interpreter and adds source-backed
  hosted agents, shared three-evaluator conformance, and bootstrap validation fixes.
  `v0.2.14` adds reusable execution plans analyzed in Agel and opt-in analyzed
  agents, plus structurally shared immutable closure code and lexical frames
  in the hosted Rust bootstrap. No language syntax or native-kernel ABI changes.
  `v0.2.15` adds an Agel-authored integer-IR compiler and an isolated hosted
  Cranelift JIT backend with checked arithmetic, bounded validation and code
  lifetime ownership. The optional JIT crate requires Rust 1.86 or newer.
  `v0.2.16` adds a self-compiling Agel frontend, managed native closures and
  immutable collections, metered calls/recursion, and three-stage IR agreement.
  Rust still supplies runtime primitives, validation and machine-code emission.
  `v0.2.17` adds Agel-authored tail-call annotations with native trampolining,
  primitive-handle reuse, and an isolated Agel-written compiled scheduler with
  explicit peer permissions and revision-checked atomic batch commits.
  `v0.2.18` adds metered compacting collection at outermost tail boundaries,
  preserving cumulative quotas while reclaiming dead invocation-arena storage.
  `v0.2.19` adds native Agel source composition and agent-proposed compiled
  behavior upgrades with owner/revision-bound previews, code-only promotion,
  and checked rollback against current state. This remains a hosted JIT library.
  `v0.2.20` adds an Agel-written native reader, self-reading/rebuilding reader
  and compiler checks, and a source-text workshop independent of the seed
  evaluator after bootstrap. Five bounded UTF-8 mechanisms support the reader.
  `v0.2.21` adds an Agel-authored static module linker and restricted expression
  templates, plus a host-assisted graphical bridge deploying expanded behaviors
  through real OS candidate validation and source persistence. This is not an
  in-guest JIT or general procedural macro compilation.
  `v0.2.22` makes three library rungs live paths: the CLI verifies and promotes
  proposal files, persists worlds as portable images, and every model process
  launch is decided by a typed default-deny effect policy. Effect inference is
  conservative over first-class builtins, the copy-on-write workspace gains a
  policy broker, the Common Lisp reference and `agel/meta` cover maps and text,
  and the freestanding evaluator gains `let`, variadic arithmetic and
  multi-form functions. No kernel-contract, image-format or wire change.
  `v0.2.23` gives the freestanding evaluator strings, symbols, lists and maps
  as values in a bounded heap with commit-boundary copying collection, the
  hosted list/map/text builtins, structural equality and data-carrying native
  agent messages. Quoted data now persists in native globals. The shared-page
  protocol, source-cell format and kernel contract are unchanged.
  `v0.2.24` adds a dependency-free Ed25519/SHA-512 implementation, signed
  portable-image envelopes with verified loads, signed A/B promotion evidence,
  and operator key generation in the CLI. The v1 image bytes inside an
  envelope are unchanged; the kernel contract is unchanged.
  `v0.2.25` adds a second implementation of the kernel contract written from
  the contract and corpus without the reference model, held to the frozen
  transcript and to step-by-step agreement with the model, and makes the seL4
  broker run it. The contract, corpus and transcript are unchanged.
  `v0.2.26` moves the x86-64 ATA driver out of the supervisor into an
  unprivileged, restartable domain granted exactly the disk's ports, with
  generation-checked handles and a stale-handle refusal in CI. The workspace
  format, slot layout and every language surface are unchanged.
  `v0.2.27` moves serial input into the console driver domain and keyboard and
  pointer input into an 8042 driver domain granted only its two ports; the
  supervisor no longer touches an input port on any interactive path. Decoding
  and policy are unchanged.
  `v0.2.28` runs the interactive serial workshop on AArch64 and RISC-V from the
  same source as x86-64, with storage optional and reported absent, and drives
  all three with the same harness in CI. No language, format or contract
  change.
  `v0.2.29` makes the x86-64 recovery plane durable: a record at sector 288
  binds trusted and candidate workspace generations, boots of an unverified
  candidate are budgeted and a candidate that fails three is rolled back
  automatically, and `:verify` runs a `health` cell in an isolated world. The
  workspace slot format is unchanged; the record is new and reads as empty when
  absent.
  `v0.2.30` makes the x86-64 kernel image A/B: sector 289 is a selector the
  BIOS stage reads, slot B at sectors 290-543 holds a candidate kernel staged
  by `scripts/stage-kernel.py`, the stage charges each candidate boot and
  loads the trusted slot after three, and the running kernel verifies,
  promotes or gives up the candidate. The disk layout grows; earlier images
  boot slot A exactly as before.
  `v0.2.31` puts the independent kernel-contract implementation behind the
  trap gate on all three research kernels and keeps the reference model in
  the supervisor as the live oracle of their isolation self-test. No
  contract, format or language change; the frozen transcript is unchanged.
  `v0.2.32` signs candidate kernels: the selector becomes version 2 with an
  admitted flag, a signed length and an Ed25519 signature, the running kernel
  verifies a staged candidate against `bootstrap/kernel-signing.pub` before the
  boot stage may load it, and `agel-integrity` gains a `no_std` mode with a
  streaming SHA-512. Version 1 selectors read as absent.
  `v0.2.33` gives AArch64 and RISC-V a disk: a virtio block device driven from
  an unprivileged domain granted one register page and one DMA frame, so the
  dual-slot workspace and the disk-backed recovery plane exist on all three
  research machines and the persistence suite runs on each. The workspace and
  record formats are unchanged; the `virt` supervisor stack grows to 512 KiB.
  `v0.2.34` adds power-cut injection: `:cut-power N` in the serial workshop
  tears the N-th sector write and halts, and `scripts/test-power-cut.sh`
  sweeps every write of a save on all three machines. No format change.
  `v0.2.35` makes the frame pool reclaim: every domain records the frames it
  was built from, a replaced driver domain gives them back, and its
  replacement is built from them, asserted on all three machines. No format
  change.
  `v0.2.36` lets native actor slots be reclaimed: `reap-agent` frees a slot
  and moves its generation on, agent handles carry their generation and a
  stale one is refused, and a reused slot prints as `#<native-agent:N.G>`.
  Agent handles widen from one byte to two inside the evaluator; images and
  the workspace format are unchanged.
  `v0.2.37` lets stored native functions carry captured scalars: a closure
  made inside a lexical call can be persisted by `def`, and a lambda escaping
  a stored function is stored rather than refused. Function-valued captures
  are still refused. No image or workspace format change.
  `v0.2.38` runs the native evaluator inside the seL4 world protection domain,
  over the forms the research kernels' isolation self-test checks, with the
  world domain given a 512 KiB stack. The contract and the transcript are
  unchanged.
  `v0.2.39` is kernel contract v1.1: the memory group (`frame.allocate`,
  `frame.map`, `frame.share`, `frame.reclaim`, `as.map`, `as.unmap`,
  `as.protect`, `as.query`) is specified, implemented by both hosted
  implementations, and covered by 37 new corpus steps. The corpus is 118
  steps with two frozen transcripts, one per published profile; the research
  kernels and the seL4 broker publish v1.0 until the frame window is real.
  `v0.2.40` makes the frame window real on x86-64, AArch64 and RISC-V: every
  domain carries the frames behind its budget, the page tables follow the
  object table after each memory operation, and the research kernels publish
  v1.1 and reproduce its transcript. No contract change; seL4 stays v1.0.
  Minor releases may make
  deliberate breaking changes while Agel is still experimental; those changes
  must be documented and migration-tested.
- `v1.0.0` is reserved for the first production-ready Agel system.

Project releases and protocol versions are separate namespaces. In particular,
the frozen **Agel kernel contract v1.0** remains protocol v1.0: changing the
project release labels does not rewrite its wire format, conformance transcript,
or compatibility claim.

Historical Git commits retain their original subjects. Corrected release tags
are the canonical public names for those snapshots.
