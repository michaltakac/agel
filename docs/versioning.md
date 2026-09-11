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
  `v0.2.41` starts the POSIX personality: the serial workshop loads a static
  ELF from the disk's program region (sectors 1024–2047, a table plus images)
  into a fresh protection domain and serves its `write` and `exit` requests
  through the shared page. Two programs under `boot/posix` and
  `scripts/test-process.sh` prove it on all three machines. The disk layout
  grows; earlier images have an empty program region.
  `v0.2.42` grows the disk layout so the kernel can grow: a kernel slot holds
  508 sectors loaded by four BIOS transfers, slot B moves to 512–1019, the
  workspace slots to 1024–1055, the records to 1056 and 1057, the program
  region to 2048–3071, and 1536–2047 is reserved for the filesystem; the image
  is 3,072 sectors. Images from before are not read and are rebuilt. The
  hashing and curve code linked into the kernel and the contract model's
  object lookups no longer carry panic paths.
  `v0.2.43` is the POSIX personality's first stratum of files: an
  unprivileged filesystem service owning disk sectors 1536–2047 through
  supervisor-relayed sector requests, a namespace granted per process at
  `:exec NAME [ROOT] [ro]`, and `open`, `read`, `write`, `close` on
  descriptors derived from it, with `ESTALE` after `:fs-restart`;
  `scripts/test-files.sh` proves it on all three machines, and the programs
  share the `agel-process-abi` crate.
  `v0.2.44` is the POSIX personality's C library: `agel-libc`, a `no_std`
  Rust static archive with a C ABI and headers, so C programs build from
  source with clang and lld and run as processes; `printf`, a bump heap,
  the string routines, `open`/`read`/`write`/`close` and `errno`;
  `scripts/build-c-program.sh` and `scripts/test-libc.sh` on all three
  machines, with `lld` added to CI.
  `v0.2.45` is the POSIX personality's processes that make processes:
  `spawn` by name with exactly the descriptors the parent names and its
  namespace (never `fork`), pipes as supervisor queues with end counts, and
  `wait` that blocks; `:exec` serves a four-process table with a round-robin
  scheduler and stops what can never progress. `agel-libc` gains `pipe`,
  `waitpid` and `agel_spawn`; `scripts/test-spawn.sh` proves a C pipeline
  on all three machines.
  `v0.2.46` begins the C library's breadth: arguments to `main` from
  `:exec NAME [ROOT] [ro] [-- ARG...]` and from `agel_spawn`'s `argv`, a
  free-list heap, `stdio` streams with a full integer formatter, `lseek`
  and `O_APPEND`, `ctype`, and the wider `string` and `stdlib`; an
  unmodified public-domain SHA-256 builds and its digest agrees with the
  host's, on all three machines (`scripts/test-breadth.sh`).
  `v0.2.47` begins the desktop's move to COSMIC's design language: a font
  atlas format and an asset region on the disk (sectors 3072–6143) that the
  graphics supervisor maps read-only into the compositor; anti-aliased,
  alpha-blended text in Fira Sans and Fira Mono; alpha-blended rounded
  surfaces with anti-aliased corners and soft shadows; and a desktop scene
  restyled with COSMIC's dark palette, radii and spacing: a top panel, a
  workshop window with a sidebar and cards, a floating dock and a command
  field. The image grows to 6,144 sectors; the frame budget to 160 records.
  `v0.2.48` puts the desktop at its native resolution: the supervisor sets
  1920×1080×32 through the Bochs display interface and keeps the BIOS mode
  as the fallback; a sprite sheet in the asset region gives the desktop an
  arrow cursor, dock icons and window controls through a sprite record; the
  scene is laid out at 1080p and the language's drawing region is 1920×1000;
  painting drains the input driver between records so a slow frame loses
  no keystrokes.
  `v0.2.49` runs programs on the desktop: the graphics image gains the
  process loader and the filesystem service, the graphical workshop gets
  `:exec` and the `:fs-*` commands through a workshop module both
  workshops share over a console trait, and a terminal panel in the
  workshop window shows what processes write; `scripts/test-desktop-process.sh`
  runs a Rust and two C programs there and checks the panel changed.
  `v0.2.50` makes the desktop respond: hover states on the dock and the
  panel, an Applications launcher that lists the program region and runs
  a name as a typed `:exec`, dock tiles that clear the terminal, list the
  root, cycle the accent or print the help, and a clock in the panel from
  a CMOS driver domain that holds only its two ports; pointer packets are
  coalesced into one repaint. The desktop test clicks the launcher and the
  dock.
  `v0.2.51` gives a process a window: the process protocol's `window` and
  `draw` requests, a `Display` trait the graphical workshop implements,
  every record checked against the window's content before it is painted
  and kept by the supervisor so the window is repainted with the desktop
  and outlives its process, a close control and `:close N`,
  `<agel/window.h>` in the C library and a `chart` program; the desktop
  test draws two windows and closes them, the C library test requires
  `-ENODEV` without a display on all three machines.
  `v0.2.52` lets a window listen: the `event` request with presses in the
  content and keys while the window has the keyboard, the process table
  split into `start`, `step_run` and `finish` so the graphical workshop
  runs a listening program between inputs with the prompt returned, the
  serial workshop unchanged in behaviour, `agel_event` in the C library
  and a `sketch` program; the desktop test presses in its window, reads
  the press on the console, sees the dot, and ends it with a key.
  `v0.2.53` makes windows move: a press in a header takes hold of the
  window and it follows the pointer, a press on a window brings it to the
  front through an order the scene keeps, and after a press in a
  window's content the owner receives the pointer's motion and release;
  the desktop test drags the dot, drags the window and raises it from
  under another.
  `v0.2.54` adds depth: the shadow's rings fall off by the square of the
  distance, windows and the launcher get a one-pixel lighter edge, and
  the control under a held button darkens until the release; the
  graphics digest is refrozen and the desktop test measures the pressed
  tile.
  `v0.2.55` boots the AArch64 kernel on a board: a `board` module holds
  every physical address, `board-raspi4` lays the kernel out for the
  Raspberry Pi 4 as a flat image at `0x80000`, the entry drops from EL2
  to EL1, and `scripts/test-raspi4.sh` boots it on QEMU's `raspi4b` to
  the workshop, which evaluates and reports that it has no disk.
  `v0.2.56` drives the card: an SD Host Controller driver domain by
  programmed I/O, the board's two controllers probed once for a card, and
  the board test loads a program from the card and persists a cell across
  two boots.
  `v0.2.57` adds the Raspberry Pi 5's layout, compile-only: two device
  windows in the identity map, the BCM2712's addresses for the debug
  UART, the GIC-400 and the SD host controller, `kernel_2712.img` from
  `build-kernel.sh raspi5`; linted with every release, never run, waiting
  for the board.
  `v0.2.58` gives the POSIX personality names: `unlink`, `rmdir`,
  `rename`, `stat`, `mkdir`, `opendir`, `readdir` and `closedir` over new
  service commands and process requests, each bounded by the namespace;
  `sscanf`, `fscanf` and `scanf`; `getopt`; a `dir` program exercises
  them on all three machines, in a full and a read-only namespace.
  `v0.2.59` gives processes time and signals: a monotonic clock from each
  machine's counter, `sleep` as a state the process table wakes, `kill`
  of one's own child, and the C library's `time.h`, `signal.h`,
  `setjmp.h` and environment; `clock.c` proves each on all three
  machines.
  `v0.2.60` lets the heap grow: a `brk` request maps pages at the
  process's break, the C library's heap lives on them and asks for more,
  the frame ledger is 512 on every build; `chdir` and `getcwd` in the
  library; `ftruncate` through a service command that zero-fills growth;
  `heap.c` proves each on all three machines.
  `v0.2.61` lets files grow past one block: `agelfs` version 2 with a
  block bitmap in the superblock and sixteen block numbers per entry,
  blocks zeroed when taken and freed on cut or removal, files of 64 KiB
  in a region of 63 blocks; `big.c` proves it on all three machines.
  `v0.2.62` runs the desktop on QEMU's Raspberry Pi 4: the framebuffer
  from the firmware's mailbox, the compositor domain on AArch64 with the
  framebuffer as normal uncached memory, the x86-only keyboard
  controller, clock and kernel slots made conditional, input from the
  serial console; `scripts/test-raspi4-desktop.sh` reads the frame back
  and runs a program from the card.
  `v0.2.63` gives windows their controls: maximize and restore, minimize
  to a pill in the panel, a corner that resizes, each a typed command
  underneath, and a resize event to the owner; the README is rewritten
  to read as a summary rather than a history.
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
