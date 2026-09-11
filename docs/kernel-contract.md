# The Agel kernel contract, v1.0

This is the frozen boundary called for by Phase 0 of
[`docs/microkernel-research.md`](microkernel-research.md#incremental-implementation-roadmap).
It is one small semantic contract that every native backend must implement
identically: today the freestanding research kernel, later an seL4/Microkit
personality, and always the hosted reference model.

The contract lives in `crates/agel-kernel-abi`. It is `no_std` and
allocation-free, so the same definitions and the same conformance corpus link
into a hosted test binary and into a 128 KiB freestanding kernel image.

## What is in it, and what is deliberately not

Kernel IPC carries **bounded words, notifications, and opaque capability
handles**. There are no S-expressions, no strings, no filesystem paths, no
model tokens, no JSON, no garbage collection, and no network policy anywhere in
this boundary. Typed Agel protocols live above it. A `Request` is a fixed
40-byte frame: an operation code, a capability slot, and four argument words. A
`Response` is a canonical status and four result words, and a failing response
carries no result words at all, so a refusal cannot smuggle data.

## Objects

| Object | Purpose |
|---|---|
| `CNode` | The capability space itself; required to derive, move, or revoke |
| `ProtectionDomain` | An address space, a capability space, and threads |
| `AddressSpace` | What frames are mapped into |
| `Thread` | A schedulable execution context |
| `Endpoint` | Bounded synchronous call/reply and asynchronous send/receive |
| `Notification` | A coalescing binary signal carrying a badge word |
| `Frame` | A page that can be mapped, shared, and reclaimed |
| `Interrupt` | A hardware source that can be bound, acknowledged, masked |
| `ScheduleContext` | An MCS-style budget, period, and priority |
| `Clock` | Monotonic time and deadlines |

A slot's type is not advisory: invoking an operation defined for a different
object type is `wrong-object-type`, never a coincidentally similar action on the
wrong object.

## Rights

`read write execute send receive grant control`

Rights are the only thing that makes a handle authority. A derived capability
may never hold a right its parent lacked, and the check is a total function on
a bitmask rather than a review convention. A capability space can attenuate
itself and then cannot restore itself; the corpus proves this.

## Operations

```text
0x0000 nop
0x01xx pd.create/start/stop/fault/reap
0x02xx as.map/unmap/protect/query
0x03xx thread.configure/resume/suspend
0x04xx endpoint.call/reply/send/receive
0x05xx notification.signal/wait/poll
0x06xx cap.copy/mint/attenuate/move/revoke
0x07xx frame.allocate/map/share/reclaim
0x08xx irq.bind/ack/mask
0x09xx sched.budget/period/priority/bind/unbind
0x0axx clock.monotonic-now/deadline
0x0b00 boot.info
```

## Status values

```text
ok                  invalid-operation   invalid-capability  wrong-object-type
insufficient-rights invalid-argument    would-block         queue-full
not-found           already-exists      revoked             stale-generation
budget-exhausted    resource-exhausted  faulted-domain      not-permitted
```

The set is closed, and a backend must not collapse distinct failures into one.
The difference between "you do not hold that right" and "policy forbids this"
is exactly what an audit needs, and the difference between
`invalid-capability` and `revoked` is what tells a stale holder that its
authority was taken rather than never existed.

## Profiles

`boot.info` returns the contract version, the domain's slot count, the endpoint
queue capacity, and a **profile bitmask** naming the operation groups the
backend implements. Anything outside the published profile answers
`invalid-operation`. "Not implemented" is therefore a published fact rather than
something a caller discovers by being refused in an undocumented way.

The v1.0 profile is `core | capability | endpoint | notification | clock`.
The v1.1 profile, since contract v1.1, adds `memory`: `frame.*` and `as.*`.
The domain and interrupt groups are declared in the contract and are not yet
in any backend's profile.

The corpus is one and the profile decides the transcript: a backend that
publishes v1.0 answers every memory step with `invalid-operation`, and its
transcript is frozen in `bootstrap/kernel-contract-v1.0.trace`; a backend
that publishes v1.1 answers them, and its transcript is
`bootstrap/kernel-contract.trace`. Both hosted implementations reproduce both.
The three research kernels publish v1.1 since v0.2.40: every domain is built
with the physical frames behind its budget and the page tables under its
window, and after each memory operation the object table accepts, the
supervisor makes the page tables say what the object table says. The seL4
broker publishes v1.0, because under Microkit's static system description a
server domain cannot change another domain's mappings, and it does not claim
a group it cannot make real. The group is a crate feature, on for the hosted
crate and the isolation builds and off in the x86-64 workshop images, which
publish v1.0 and would otherwise exceed their 254-sector budget.

## The memory group

A conformance domain is built with one frame and one address space, the
frame window of `CONFORMANCE_FRAME_WINDOW` pages, and a budget of
`CONFORMANCE_FRAME_BUDGET` more frames. Pages are named by their index in
the window, never by an address; frames are numbered, 0 for the one the
domain is built with and then in allocation order, lowest free number first.

| Operation | Subject | Arguments | Answer |
|---|---|---|---|
| `frame.allocate` | cnode, `control` | destination slot, rights ⊆ `read write execute grant` | derivation id of the new root frame capability; `resource-exhausted` when the budget is spent |
| `frame.map` | a frame | page, rights ⊆ `read write execute` | the page; `insufficient-rights` if the capability lacks a requested right, `not-permitted` for `write` with `execute`, `already-exists` if the page is mapped |
| `frame.share` | a frame, `grant` | destination slot, rights, badge | derivation id of the child; it may not widen |
| `frame.reclaim` | a frame's root capability | none | pages unmapped, capabilities revoked; `not-permitted` through a shared handle or for the domain's own frame |
| `as.map` | address space, `control` | frame slot, page, rights | as `frame.map`, naming the frame by slot |
| `as.unmap` | address space, `control` | page | the frame number; `not-found` if unmapped |
| `as.protect` | address space, `control` | page, rights | the rights; a mapping can only lose rights in place |
| `as.query` | address space, `control` | page | the frame number and the mapping's rights; `not-found` if unmapped |

A mapping belongs to the address space, not to the capability it was made
through: revoking the capability does not unmap, `as.unmap` and
`frame.reclaim` do. Reclaiming unmaps every page the frame occupies, revokes
every capability derived from the reclaimed root, returns the frame to the
budget, and empties the root's slot. Checks are made in the same order as
everywhere else: the subject's existence, its type, its rights, reserved
words, then the arguments.

## The conformance domain

Every backend constructs the same starting capability space before a corpus run:

| Slot | Object | Rights | Badge |
|---|---|---|---|
| 0 | *(empty)* | — | — |
| 1 | `CNode` | `control` | 0 |
| 2 | `Endpoint`, queue capacity 4 | `send receive grant` | 0 |
| 3 | `Notification` | `send receive` | 1 |
| 4 | `Frame` | `read write` (deliberately no `execute`) | 0 |
| 5 | `Clock` | `read` | 0 |
| 6 | `AddressSpace` (since v1.1) | `control` | 0 |
| 7–31 | *(empty)* | — | — |

Two harness properties make a run reproducible on any backend. Derivation
identifiers come from a monotonic counter starting at 1, allocated in the order
the domain above is constructed, so the corpus can assert them. And the
conformance clock is a **logical tick counter**, not wall time: it starts at
zero and advances by one on each successful read. A real deployment binds that
capability to a hardware time source instead.

Blocking operations never block in the harness, because a single-domain harness
has nobody to block on. `endpoint.call` answers `would-block`,
`endpoint.reply` answers `not-found`, and `notification.wait` answers
`would-block` when nothing is pending. These are specified answers, not hangs.

## The corpus

`agel_kernel_abi::conformance::CORPUS` is 118 ordered, stateful steps. Most of
them are refusals, on purpose: an interface is defined by what it declines, in
what words, and in what order it checks. Two kernels that agree on the happy
path and disagree about whether a bad call earns `invalid-capability`,
`wrong-object-type`, or `insufficient-rights` do not have the same contract.

The corpus covers the version probe and published profile, refusal of every
unimplemented group, naming an empty or out-of-range slot, type checking before
rights checking, non-zero reserved argument words, monotonic derivation,
copy/mint/attenuate/move semantics, transitive revocation and fail-closed stale
handles, the bounded endpoint queue reaching backpressure and draining in order,
badge delivery, notification coalescing, clock monotonicity, the memory
group (mapping the domain's frame, protecting a mapping downward, allocating
to the budget's edge, sharing without widening, reclaiming and the share
failing closed), and a capability space irreversibly attenuating itself.

Steps are ordered and stateful. Inserting one in the middle changes every later
derivation identifier, so it is a contract change: bump the minor version and
regenerate the frozen transcript.

## How conformance is checked

```sh
cargo test -p agel-kernel-abi        # reference model = frozen transcript
./scripts/test-kernel-contract.sh    # the same, as a diff you can read
./scripts/test-isolation.sh          # three machines = frozen transcript
./scripts/test-isolation.sh aarch64  # or just one
./scripts/test-sel4.sh               # an unmodified seL4 kernel
```

`bootstrap/kernel-contract.trace` is the frozen canonical transcript of the
v1.1 profile and `bootstrap/kernel-contract-v1.0.trace` of the v1.0 profile.
The hosted reference model and the independent implementation reproduce both.
An unprivileged protection domain on each of x86-64, AArch64, and RISC-V,
talking to its kernel through that machine's trap gate, reproduces the v1.1
transcript with a frame window its page tables make real; a protection domain
on seL4 talking to a *server* through a protected procedure reproduces the v1.0
transcript, the profile it publishes. This is the same comparison discipline
the Common Lisp reference uses for the language kernel.

The host test also stands up a deliberately non-conformant backend — one that
widens rights on `cap.mint` — and requires the corpus to catch it at
`derive/mint-cannot-widen`. A comparison that has never been seen to fail is not
yet evidence of anything.

## Backend notes: the research kernel

The research backend implements the contract's object semantics by linking the
independent implementation (`agel_kernel_abi::independent`, since v0.2.31;
the reference model before that), and adds the part a hosted implementation
cannot have: the object table lives in supervisor-only memory, the caller holds
slot numbers rather than references, and the only path to any of it from an
unprivileged world is a trap gate. The isolation self-test keeps the reference
model in the supervisor and checks every one of the world's 118 answers against
it, so on each machine the frozen transcript is two implementations agreeing
live across a hardware privilege boundary. The seL4 broker answers with the
same independent implementation; see the seL4 notes below.

Since v0.2.40 the research kernels' memory group is real. A domain is built
with the five physical frames behind its budget and with the page tables
under its eight-page window, so a memory operation at trap time never
allocates. After every `frame.*` or `as.*` invocation the object table
accepts, the supervisor reconciles the page tables with the object table's
window: a page the table says is mapped becomes a leaf entry carrying the
mapping's rights (`execute` implies `read`, and `write` with `execute` never
gets this far), a page it says is unmapped becomes an absence. The isolation
self-test has a world map its frame and write through the mapping, protect
it to read-only and take a page fault on the next write, and allocate a
budget frame, write through it, unmap it, and fault on the next read, on all
three machines.

It builds for three architectures from one source. The shared driver, the
capability space, the shared handshake page, the tick budget, and the rule that
a stopped world stays stopped live in architecture-neutral code; address spaces,
register frames, trap entry, and the privilege transition are per-architecture.

| | x86-64 | AArch64 | RISC-V |
|---|---|---|---|
| Unprivileged level | ring 3 | EL0 | U-mode |
| Trap instruction | `int 0x80` | `svc #0` | `ecall` |
| Operation register | `rax` | `x8` | `a7` |
| Capability register | `rdi` | `x0` | `a0` |
| Argument words | `rsi rdx r10 r8` | `x1`–`x4` | `a1`–`a4` |
| Result words | `rdi rsi rdx r10` | `x1`–`x4` | `a1`–`a4` |
| Translation | 4-level, 4 KiB | 3-level Sv39-shaped, 4 KiB | Sv39, 4 KiB |
| Preemption | PIT at 100 Hz | EL1 physical timer via GICv2 | SBI timer at 100 Hz |
| Platform | BIOS seed, raw 128 KiB disk | QEMU `virt`, ELF | QEMU `virt`, ELF over OpenSBI |
| Leaving the emulator | debug-exit device | PSCI `SYSTEM_OFF` | `virt` test device |

`rbx` and `rbp` are absent from the x86-64 convention because Rust's inline
assembler reserves them.

Slot 31 is a backend convention rather than part of the contract: a send on it
is how a world hands control back to its supervisor. It sits above every slot
the corpus touches, so the corpus never observes that it exists.

## Two implementations, one transcript

`agel_kernel_abi::model` is the reference model. `agel_kernel_abi::independent`
is a second implementation written from this document, the crate's type
definitions, the corpus and its frozen transcript, and deliberately not from
the model's source. They share only the contract types and the well-known slot
and profile constants. Both reproduce both frozen transcripts byte for byte;
`conformance::compare` requires them to agree on all 118 steps under either
profile; and
the hosted test suite checks that a widening variant of the independent
implementation is still caught at `derive/mint-cannot-widen`.
`./scripts/test-kernel-contract.sh` diffs both hosted transcripts against the
freeze, and `./scripts/test-isolation.sh` requires the same agreement live on
three machines, with the independent implementation behind the trap gate and
the reference model in the supervisor.

Where the corpus does not pin a choice, the independent implementation
documents the one it made at the point it is made: reserved argument words are
checked after type and rights; a tombstoned slot is a free derivation target;
`endpoint.call` queues nothing when it cannot complete; the derivation tree is
walked on a snapshot taken before any slot is revoked. Each of those is a
candidate for a future corpus step rather than a silent convention.

## Backend notes: seL4

The seL4 backend is where the contract earns its shape. seL4 is unmodified and
knows nothing about Agel, so the contract cannot be answered by the kernel: it
is answered by an ordinary unprivileged **broker** protection domain, and an
invocation is a protected procedure call to it. Since v0.2.25 the broker
answers with the independent implementation, so the frozen transcript this
backend must reproduce is the agreement of two implementations behind two
different kinds of boundary, not one implementation behind two. That is exactly what
[`microkernel-research.md`](microkernel-research.md) requires — Lisp objects,
mailboxes and policy belong in isolated servers, not in a kernel whose value is
that nobody changed it.

Since v0.2.38 the world domain also runs the native Agel evaluator, the same
source the research kernels compile into their evaluator domains, over the
same forms their isolation self-test checks: factorial, a definition, a
transaction rolled back by a division by zero, and a closure with a captured
value. It runs on the world domain's own stack and holds no authority; its
answers leave through the serial domain like everything else. The contract
is untouched by this, which is the point: the evaluator is a program above
the contract, not a kernel object.

The whole invocation fits in the four message registers AArch64 seL4 passes in
hardware registers, because the 52-bit message label carries the operation code
and the capability slot:

```text
request   label = operation | (capability << 16)     words = arguments
reply     label = status                             words = values
```

Four protection domains, and the system description is the security
architecture:

| Domain | Priority | Holds | Cannot |
|---|---|---|---|
| `serial` | 200 | the only device capability in the system | reach any other domain except by being called |
| `broker` | 150 | every object the contract defines | reach the device, or its callers |
| `recovery` | 100 | parent of `world`, so seL4 delivers its faults here | run any Agel code; it holds no contract capability |
| `world` | 50 | two channels, one page | everything else |

Protected procedures run toward higher priority, so those numbers are also the
call graph, and it is checked by the kernel rather than by convention. The world
finishes by faulting on purpose; `recovery` reports the containment and declines
to reply, which leaves the world stopped. Containment there is a property of
seL4 and of `agel.system`, not of a supervisor loop this project wrote.

What this does **not** claim: a verified configuration. Microkit ships MCS
kernels and MCS proofs are ongoing. [`sel4-manifest.md`](sel4-manifest.md)
records the exact kernel, configuration and toolchain, and states the
verification status in those terms.

### What the architectures do not agree about

The containment tests use a shared vocabulary of fault names, but the mapping to
each machine is deliberately not flattened:

- x86-64 distinguishes `divide-error`, `invalid-opcode`, `general-protection`
  and `page-fault`, so it is provoked four ways.
- AArch64 has no integer divide exception at all, so it is provoked three ways.
  Its "privileged instruction" case reads the physical timer's control
  register — the first move a world would make towards disabling its own
  preemption — which `CNTKCTL_EL1` denies EL0.
- RISC-V genuinely cannot distinguish a privileged instruction from an
  undefined one: both raise *illegal instruction*. Its provocation table says so
  rather than inventing a distinction the architecture does not make.

## What this document does not claim

Answering the contract correctly is not evidence of isolation. The contract
states what a backend must *answer*; whether it enforces those answers with
hardware protection domains is a property of the backend, recorded in
[`docs/native-boot.md`](native-boot.md) and
[`docs/threat-model.md`](threat-model.md). A backend that implemented every
operation in one address space with no privilege separation would pass this
corpus and provide no security whatsoever.
