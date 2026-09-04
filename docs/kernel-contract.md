# The Agel kernel contract, v1.0

This is the frozen boundary called for by Phase 0 of
[`docs/microkernel-research.md`](microkernel-research.md#incremental-implementation-roadmap).
It is one small semantic contract that every native backend must implement
identically: today the freestanding research kernel, later an seL4/Microkit
personality, and always the hosted reference model.

The contract lives in `crates/agel-kernel-abi`. It is `no_std` and
allocation-free, so the same definitions and the same conformance corpus link
into a hosted test binary and into a 64 KiB freestanding kernel image.

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
Memory, domain, and interrupt groups are declared in the contract and are not
yet in any backend's profile.

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
| 6–31 | *(empty)* | — | — |

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

`agel_kernel_abi::conformance::CORPUS` is 81 ordered, stateful steps. Most of
them are refusals, on purpose: an interface is defined by what it declines, in
what words, and in what order it checks. Two kernels that agree on the happy
path and disagree about whether a bad call earns `invalid-capability`,
`wrong-object-type`, or `insufficient-rights` do not have the same contract.

The corpus covers the version probe and published profile, refusal of every
unimplemented group, naming an empty or out-of-range slot, type checking before
rights checking, non-zero reserved argument words, monotonic derivation,
copy/mint/attenuate/move semantics, transitive revocation and fail-closed stale
handles, the bounded endpoint queue reaching backpressure and draining in order,
badge delivery, notification coalescing, clock monotonicity, and a capability
space irreversibly attenuating itself.

Steps are ordered and stateful. Inserting one in the middle changes every later
derivation identifier, so it is a contract change: bump the minor version and
regenerate the frozen transcript.

## How conformance is checked

```sh
./scripts/test-kernel-contract.sh   # reference model = frozen transcript
./scripts/test-isolation.sh         # ring-3 backend  = frozen transcript
```

`bootstrap/kernel-contract.trace` is the frozen canonical transcript. Both
scripts diff against it, so three artifacts share one set of bytes: the hosted
reference model, the checked-in freeze, and a real protection domain talking
through a trap gate inside QEMU. This is the same comparison discipline the
Common Lisp reference uses for the language kernel.

## Backend notes: the research kernel

The research backend implements the contract's object semantics by linking the
shared reference model, and adds the part a hosted model cannot have: the object
table lives in supervisor-only memory, the caller holds slot numbers rather than
references, and the only path to any of it from ring 3 is the `int 0x80` trap
gate. That is the honest division of labour for this phase — the research
backend's job is to put already-specified semantics behind a hardware privilege
boundary, and seL4 will be the independent second implementation.

Register convention for `int 0x80`:

```text
in   rax = operation   rdi = capability   rsi rdx r10 r8 = argument words 0..3
out  rax = status      rdi rsi rdx r10    = result words 0..3
```

`rbx` and `rbp` are absent because Rust's inline assembler reserves them.

Slot 31 is a backend convention rather than part of the contract: a send on it
is how a world hands control back to its supervisor. It sits above every slot
the corpus touches, so the corpus never observes that it exists.

## What this document does not claim

Answering the contract correctly is not evidence of isolation. The contract
states what a backend must *answer*; whether it enforces those answers with
hardware protection domains is a property of the backend, recorded in
[`docs/native-boot.md`](native-boot.md) and
[`docs/threat-model.md`](threat-model.md). A backend that implemented every
operation in one address space with no privilege separation would pass this
corpus and provide no security whatsoever.
