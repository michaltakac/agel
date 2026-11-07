# v0.3.12: Agel installs and activates native program versions

An Agel process can now compile a candidate, register its ELF bytes in the
real OS program table, execute a contained canary, select the version for
subsequent calls and revert. The controller and kernel remain running during
the change. Earlier executable images remain installed under distinct names.
This is application-neutral; it has no RuneScape-specific code.

`agel/program-store` owns table validation, allocation, CRC-32, duplicate-name
refusal and publication ordering. `agel/bytes` implements hex decoding and
CRC-32. `agel/native-program` connects them to the Agel compiler/backend and
`agel/evolution`. The generic `evolution-edit-with-activation` hook runs only
after text approval and metacircular replay, and before commit. It retains
activation receipts in the version and history. A hook that returns no valid
receipt or throws refuses the change. The older `evolution-edit` API retains
its metadata-only behavior; native programs explicitly supply the new hook.

Rust adds bounded sector transport, region geometry, process namespace
enforcement and captured child output. It does not decide program names,
placement, checksums, replay expectations or active versions in this path.
The older Rust desktop installer remains available; removing that path and
the other Rust OS policies is unfinished.

## Native evidence

Run `sh scripts/test-native-evolution.sh`. The [QEMU transcript](evidence/v0.3.12-native-programs.txt)
records one Agel controller doing all candidate compilation and registration:

1. Version 1 executes `(40)` and prints 41.
2. Version 2 prints 42 without restarting the controller or kernel.
3. A source regression fails metacircular replay; version 2 remains active.
4. Another source passes replay but has zero native fuel. Its actual process
   exits 112 without the expected output. Native replay refuses activation;
   version 2 still executes and prints 42.
5. Revert creates version 3 from version 1's source. It prints 41, with the
   controller's session state preserved.
6. After the controller exits, both earlier binaries execute directly. After
   reboot, both still execute and the judge log remains readable.

The native gate compares the captured full decimal output and exit status,
avoiding low-byte exit-code collisions. The process protocol gives canaries
no file namespace rights. An independent probe verifies denial of file reads,
file creation, program-region reads/writes and flush in that child. Console,
clock and ordinary process mechanisms are not removed, so this is not a
claim of zero authority over every OS facility.

Five deterministic local fixture judgments are appended, with zero costs, to
the OS's protected `judge-log` before/after invocation. The test checks all
five start/result pairs, refuses overwrite, and finds the log after reboot.
No paid model is invoked here. The real model repair actor from v0.3.11 is
not yet connected to this native activation path.

## Byte boundary and validation

Process operations 26–29 read/write/flush sectors and report geometry. Indices
are relative to the program region; out-of-range requests fail. The loaded
Agel runtime exposes `program-region`, `program-sector-read`,
`program-sector-write` and `program-flush` behind `program/read` and
`program/write` capabilities. Root namespace rights also constrain the raw
protocol, so an unprivileged binary cannot bypass them by avoiding Agel.

`exec-canary` creates a child with the no-files flag and captures stdout
through a pipe, bounded to 4096 UTF-8 bytes. The Agel native-program adapter
sets native fuel and interprets the returned status/output pair. Programs
are appended as new table entries; publication follows data flush and a
check that the table has not changed since staging.

Hosted tests cover CRC against an independent bitwise implementation,
installation ordering, retaining old names, invalid names, I/O failure,
stale tables, table exhaustion, native-output mismatch, revert, and activation
hook ordering/errors. Existing repair, evolution and live-tool tests pass.
The existing x86-64 pipe/spawn/fault regression passes. Native kernel and
process Clippy, hosted Clippy, formatting and Python compilation pass.

## Limits

The backend remains the existing x86-64 integer subset. Each emitted image
has fixed entry arguments; this API refuses calls without an activated image
for those arguments. It has not added dynamic service arguments, collections,
full signed-64-bit native semantics or a new runtime backend. This release's
native execution evidence is x86-64 only.

Selecting an installed version starts a fresh child process. This is real
program activation, not in-place replacement of a running service domain.
Domain state migration, domain-generation changes and kernel-slot activation
remain unfinished. The controller's canonical active pointer/history is not
automatically made durable by this module; the reboot test proves retention
of executable bytes and judge records, not recovery of that pointer.

The inherited table holds at most 15 entries in a 4 MiB region. Exhaustion
refuses new installs while keeping existing entries. Failed canaries leave
staged images and receipts; no garbage collector or compaction is introduced.
The table is a single sector without a new power-cut-safe publication scheme.
The stale-table check is not atomic multi-writer synchronization.

Raw program-write authority is broad: it can rewrite the program region,
including bootstrap programs, independently of this library's retention
policy. A separate guardian must eventually own it. The native fixture is
a trusted controller, with untrusted child execution contained by the kernel.
This does not establish autonomous OS self-improvement or an Exo win.
