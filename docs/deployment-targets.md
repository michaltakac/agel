# Scope, deployment targets, and the POSIX personality

Status: requirements, 2026-09-04

Agel is a **Unix-like agentic operating system built on a microkernel**. Agents
are first-class values in a homoiconic Lisp, authority is capability-derived,
and the mechanisms that contain an evolving world are enforced by hardware
protection domains rather than by the language.

Two decisions define the scope:

1. **Agel does inference, not training.** Model inference is a first-class
   capability of the system. Training and fine-tuning are out of scope for the
   OS itself.
2. **Linux application compatibility comes from a POSIX personality written in
   safe Rust**, running unprivileged above the kernel contract — the Redox
   approach, with capability-derived authority instead of ambient paths.

This document states what those mean, what they rule out, and what the tiers
are. It supersedes an earlier draft that explored putting Linux underneath Agel
in order to reach CUDA; that exploration is in git history and is not the
project's direction.

## Why training is out of scope

Worth writing down, because the reason is not "we did not get to it".

The GPU stack that makes training worth doing on NVIDIA hardware cannot be
written by us. The open kernel modules are interface layers over
`nv-kernel.o_binary`; the GSP firmware is a signed binary running on a
coprocessor on the card, driven by an undocumented RPC protocol; and the entire
user-space CUDA stack — the runtime, the driver API, cuBLAS, cuDNN, NCCL, the
compiler — is proprietary and built for Linux.

So a training-capable Agel node would have to run Linux as its kernel. That
trade is available and it is not the one this project makes: it would move the
trusted computing base from tens of thousands of lines to tens of millions and
end every assurance claim the microkernel work exists to support.

Training is therefore something Agel may **orchestrate** rather than perform.
Agel already has the right boundary for that: a capability-scoped, typed,
audited, idempotency-keyed request to an external system that computes and
returns a result, with a transactional outbox so a crash cannot double-claim. A
training cluster is a provider, reached the way a model API is. Nothing about
that requires Linux underneath Agel.

## Where inference runs

Inference is in scope, which raises the same question in a smaller form.

| Path | Status | Notes |
|---|---|---|
| **External providers** | implemented | capability-scoped adapters with a typed effect boundary and an audit log; this is how Agel gets model access today |
| **CPU inference in an Agel domain** | not started | safe Rust over quantized weights. Slower than an accelerator, and free of proprietary blobs. The honest baseline for a self-contained node |
| **Accelerated local inference** | open question | anything using CUDA reintroduces the Linux dependency in full. Open stacks and non-NVIDIA accelerators are the directions worth investigating; neither is a commitment |

The rule that keeps this from drifting: **local inference must not require a
proprietary kernel-mode driver.** A path that does is a path back to Linux as
the core, and that decision has been made in the other direction.

## The POSIX personality

Agel is Unix-like, and Unix-like software should run on it. The mechanism is the
one Redox demonstrates: a C library and system-interface layer written in Rust,
running unprivileged, translating POSIX into the system's native operations.

### It is a personality, not the centre

This is already the project's recorded position on Redox — POSIX compatibility
is a library and service personality rather than the conceptual centre of a new
system. The kernel contract stays free of paths, file descriptors and process
semantics. The POSIX layer is an ordinary set of unprivileged components above
it, exactly as the console driver is.

### A path is not authority

This is the requirement that distinguishes an Agel POSIX layer from a
reimplementation of Unix, and it is the one most easily lost.

In Unix, `open("/etc/passwd")` succeeds because of who you are. In Agel it must
succeed because of what you hold. A process receives a namespace capability when
it is created; `open` resolves a name *through that capability*, and a name it
does not cover is unreachable however it is spelled. No ambient root, no path
that grants itself, no descriptor that outlives the authority that produced it.

Concretely:

- every POSIX process starts from an explicit capability set, never an inherited
  ambient one;
- file descriptors are handles derived from capabilities, under the same
  derivation rule as everything else — equal or weaker, never widened;
- a descriptor whose backing service restarts fails closed with the contract's
  `stale-generation`, the way any other handle does;
- `fork` has to be decided rather than inherited: a call that duplicates an
  entire authority set by default is at odds with everything above it.

### Two levels of compatibility, costing very differently

| Level | What it means | Effort |
|---|---|---|
| **Source compatibility** | POSIX software recompiled against Agel's Rust C library | large but bounded; this is what Redox's `relibc` does |
| **Binary compatibility** | unmodified Linux ELF binaries, glibc and all, through a Linux syscall emulation layer | much larger, and the surface is the whole Linux ABI |

Source compatibility is the target. Binary compatibility is not ruled out and is
not planned; it should be treated as a separate project, decided on its own
merits, and never assumed by anything depending on the POSIX layer.

### The safety claim, stated carefully

"In safe Rust" means the POSIX layer contains no `unsafe` of its own beyond a
small, named, reviewed set at the hardware and contract boundary. It does not
mean the layer is correct, and it does not mean a POSIX program running on it is
contained: containment comes from the protection domain the program runs in and
the capabilities it was given, not from the language its libc was written in.

## Target hardware

| | Deployment | seL4 assurance coverage | Conformance backend |
|---|---|---|---|
| **AArch64** | primary | functional correctness, integrity, availability, confidentiality | yes |
| **x86-64** | supported | weakest: C-level functional correctness only | yes |
| **RISC-V** | not a target | comparable to AArch64 | yes; a third machine keeps the contract honest |

Dropping the training requirement removes a tension an earlier draft had
introduced. No deployment target now forces the weakest verified architecture,
so the assurance target and the deployment target can be the same machine again:
**AArch64 is primary**, x86-64 is supported, RISC-V keeps the contract portable.

Development happens under QEMU on whatever the developer has. This repository is
built on a MacBook Pro with no discrete GPU, and every native backend runs
emulated.

## Tiers

| | Tier 1 — workstation / node | Tier 2 — hosted | Tier 3 — development |
|---|---|---|---|
| Kernel | Agel's, on a microkernel | someone else's; Agel as a guest or a process | any |
| Model access | local inference and external providers | external providers | external providers |
| POSIX software | yes, recompiled against the Agel libc | as available | not required |
| Purpose | the system Agel is for | reaching users before the native stack is finished | building and testing Agel |

Tier 2 is a real deployment shape rather than an embarrassment: it is how the
language and agent runtime reach people while the native stack is built. It
should be described as what it is — Agel hosted on someone else's kernel — and
never as bare metal.

## Requirements, as testable statements

1. The kernel contract contains no POSIX concept: no paths, no file descriptors,
   no process semantics, no ambient names.
2. The POSIX layer runs unprivileged, in protection domains, above the contract.
3. A POSIX process reaches exactly the resources its capability set covers, and
   a name outside it is unreachable however it is spelled.
4. A file descriptor is a derived handle: equal or weaker than what produced it,
   never widened, failing closed across a service restart.
5. POSIX software builds against the Agel C library without patching, for a
   stated and growing subset of the standard.
6. Local inference requires no proprietary kernel-mode driver.
7. Model access — local or external — is a capability, and a world without it
   cannot obtain one.
8. Everything except hardware-specific driver work is developable and testable
   under QEMU on a machine with no accelerator.

## What exists today

Exact, because the gap is large:

- **The language and runtime:** homoiconic reader and evaluator, transactional
  worlds, hygienic macros, modules, conditions and restarts, agents with
  isolated heaps and typed protocols, supervision, event logs, snapshots and
  replay, capability-scoped effects, evidence-carrying upgrades, A/B images, a
  tamper-evident log, and a standard library written in Agel.
- **Model access:** external providers, through a typed and audited effect
  boundary.
- **The kernel contract:** frozen at v1.0, with a reference model, an 81-step
  conformance corpus, and four backends producing byte-identical transcripts —
  three research kernels on x86-64, AArch64 and RISC-V, and one on an unmodified
  seL4 kernel under Microkit.
- **Isolation:** protection domains with separate address spaces,
  write-xor-execute, preemption, and containment of worlds that fault, execute
  privileged instructions, touch ungranted devices, or never yield; plus one
  restartable driver domain with generations and fail-closed stale handles.

Not started: the POSIX personality in any form, local inference, a filesystem, a
network stack, storage drivers, and moving the Agel evaluator out of the
supervisor. That last is the next rung and the precondition for the POSIX list,
because a POSIX process is a protection domain running an Agel-hosted program,
and the evaluator has to be able to live in one first.
