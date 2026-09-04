# Microkernel research and Agel native architecture

Status: architecture research and recommendation, 2026-09-04

This document evaluates modern microkernel and virtual-machine projects as a
foundation for the native Agel system. It is intentionally stricter than asking
which project looks most like an "agent OS." Agel must remain live and
self-improving without allowing a language bug, model mistake, driver fault, or
malicious tool to rewrite the mechanism that contains it.

## Executive decision

Agel should use a **two-backend architecture with one small kernel contract**:

1. Build the high-assurance native system as an Agel personality above an
   **unmodified seL4 kernel**, initially using the seL4 Microkit for static
   protection-domain composition.
2. Continue Agel's small from-scratch kernel as a **research, bootstrap, and
   conformance backend**, not as the first production security boundary.
3. Put the Agel evaluator, mutable worlds, model adapters, filesystems, network
   stacks, drivers, UI, and voice pipeline in unprivileged protection domains.
   None belongs in the privileged kernel.
4. Use **Firecracker** optionally to contain or deploy an entire Agel VM on a
   Linux/KVM host. Use **rust-vmm** crates only in host-side VMM or device-model
   tooling, not as the Agel guest kernel.
5. Mine **Redox** for Rust userspace, service, driver, and scheme ideas, but do
   not adopt its whole compatibility surface as Agel's trusted base.
6. Treat the linked **Oxide OS prototype** as design inspiration only. Its own
   current issue list says it has no ring-3 userspace and all code runs in
   ring 0, so its capabilities do not yet provide hardware fault isolation.
7. Also borrow from **Hubris** (small kernel, isolated restartable services,
   operational introspection), **Zircon** (typed handles, rights, channels and
   capability routing), **Genode** (component composition), **Tock** (grants),
   **Barrelfish** (multicore as a distributed system), and **CHERI** (future
   hardware-enforced fine-grained capabilities).

This is a composition of ideas and layers, not a source-level mixture of four
kernels. Combining kernels would combine their trusted computing bases and
failure modes. The valuable common denominator is a minimal Agel kernel ABI and
an explicit system graph above it.

## What "microkernel" must mean for Agel

The label is less important than the enforced boundary. For Agel, a usable
microkernel foundation must provide:

- distinct address spaces and least-privilege authority;
- kernel-mediated, capability-aware IPC;
- bounded CPU, memory, queue, and device resource ownership;
- fault containment and restart without kernel replacement;
- a small, auditable privileged implementation;
- a precise enough object and IPC model to specify and verify;
- a path to deterministic replay, A/B system images, and recovery; and
- practical Rust and C interfaces while Agel bootstraps itself.

A small monolithic program in ring 0 is not a microkernel security architecture.
Conversely, an OS may use a somewhat larger kernel and still have excellent
process isolation. The relevant questions are what executes privileged, how
authority is represented, and what happens when each component is hostile.

## Comparison at a glance

| Project | What it actually is | Isolation and assurance | Best use for Agel | Main limitation for Agel |
|---|---|---|---|---|
| seL4 | Capability-based, policy-free microkernel | Hardware protection domains plus unusually strong machine-checked kernel properties for specific configurations | Production/reference kernel and semantic anchor | Proof coverage is configuration-specific; dynamic system policy must be built above it |
| Microkit | seL4 SDK and static component framework | Protection domains, channels, memory regions, IRQs and scheduled budgets described by a system file | First Agel/seL4 prototype and recovery composition | Static topology is not by itself a dynamic agent/process manager |
| Firecracker | Minimal VMM process using Linux KVM | VM boundary plus seccomp, namespaces, cgroups, chroot and privilege dropping | Linux-hosted deployment/test sandbox around an Agel VM | It is neither a microkernel nor a macOS host solution; it depends on Linux/KVM |
| rust-vmm | Modular Rust crates for VMM construction | Depends on the VMM, host kernel and policy using the crates | Future Agel host orchestrator or specialized VMM | Libraries do not themselves create a security boundary |
| Redox | General-purpose Rust OS with a microkernel-oriented service design | Separate userspace services exist, but the kernel and compatibility goals are much broader than seL4 | Patterns, drivers, `relibc`, schemes, possible application port | Larger surface and no comparable end-to-end formal assurance claim |
| linked Oxide OS | Small agent-native hobby/prototype kernel | Current repository says all code is ring 0 and scheduling is cooperative | Agent API sketches, demos, naming and experiments | No current hardware-enforced userspace boundary; several core subsystems are incomplete |
| Hubris | Memory-protected embedded OS from Oxide Computer | Statically defined, isolated, restartable tasks with a very small Rust kernel | Fault/restart model, IDL, static topology, postmortem/live observability | Embedded/static scope rather than a general interactive desktop OS |

Licensing must be evaluated for the exact artifacts shipped. In broad terms,
seL4's kernel is GPLv2 while its userspace headers and libraries allow
independently licensed applications; Firecracker is Apache-2.0; rust-vmm crates
commonly use Apache-2.0/BSD-3-Clause; Redox and the linked Oxide OS are MIT.
See each repository rather than treating this table as legal advice.

## seL4

### Why it is the strongest foundation

seL4 is a small capability-based kernel whose abstractions are close to what
Agel needs: threads, virtual address spaces, endpoints, notifications, frames,
interrupt objects and—under the MCS configuration—scheduling contexts. The
kernel deliberately leaves resource and system policy to user space. Its
functional-correctness proof connects an executable C implementation to an
abstract specification, with additional integrity, confidentiality and
availability results under documented assumptions.

The key architectural consequence is more important than the marketing label:
an Agel agent cannot obtain a kernel object merely by naming it. A user process
can act on an object only through a capability in its capability space, with
the applicable rights. Authority can be transferred explicitly over IPC.

### What the proof does and does not buy

The proof is not a magic property of every possible seL4 build. Agel must pin a
specific architecture, platform, kernel configuration, compiler/toolchain and
proof manifest. The official verified-configurations table shows that coverage
varies by architecture and option; MCS, SMP, virtualization and some platforms
do not all have the same proved property set.

The documented assumptions also matter:

- boot code and portions of assembly are outside parts of the high-level proof;
- hardware must behave according to its model;
- cache and TLB maintenance assumptions must hold;
- DMA-capable devices can bypass CPU page-table isolation unless an IOMMU or a
  trusted driver/device arrangement constrains them; and
- the confidentiality theorem does not eliminate timing side channels.

Changing the seL4 kernel invalidates the easy claim that Agel uses the verified
implementation. Therefore Agel should **not fork seL4 to add Lisp objects,
mailboxes, policy, or dynamic agent semantics**. Those belong in isolated
servers. If a kernel change ever becomes unavoidable, it needs its own explicit
verification program and must be reported as an unverified configuration until
that work is complete.

### Microkit fit

Microkit describes a statically composed system of protection domains (PDs),
memory regions and channels. A PD has a virtual address space, capability space,
entry point, priority and optionally a scheduling budget and period. This is an
excellent match for Agel's initial trusted root, recovery supervisor, storage
server, driver domains and one or more Agel worlds.

It is not the complete answer for open-ended dynamic agent creation. Fine-grain
logical agents normally remain language objects scheduled inside an Agel world.
When a new trust boundary is required, a resource manager can allocate kernel
objects and construct a protection domain, or the system can activate a
pre-provisioned pool. That dynamic manager is trusted policy above seL4 and must
be kept small.

### Rust and performance

The seL4 ecosystem has maintained Rust bindings and examples. C remains the
lowest-friction ABI for some generated interfaces, but the Agel runtime and most
servers can be Rust `no_std` programs. seL4 is also designed around fast IPC;
performance should still be measured with Agel's actual message sizes,
scheduling configuration and hardware rather than copied from headline
benchmarks.

### seL4 recommendation

Adopt an unchanged, verified seL4 configuration as the assurance backend.
Prototype with Microkit, and make the proof/configuration manifest a release
artifact. Keep the root task and recovery plane outside the mutable Agel world.

## Firecracker and rust-vmm

### Firecracker is a VMM, not a guest kernel

Firecracker is a Linux userspace virtual-machine monitor. It uses KVM to run one
microVM per Firecracker process and exposes a deliberately small virtual-device
model. Its security design layers the VM boundary with a jailer, namespaces,
cgroups, chroot, privilege reduction and per-thread seccomp filters.

That makes it valuable around Agel, but it does not replace an Agel kernel:

```text
Linux host
  -> Firecracker process + KVM
       -> Agel guest image
            -> seL4
                 -> Agel protection domains and worlds
```

This is useful for cloud workers, hostile experiments, reproducible CI and
disposable candidate worlds. It is not the direct development route on macOS,
where KVM is unavailable; QEMU remains the portable emulator during bootstrap.

Firecracker's published sub-125 ms boot and sub-5 MiB VMM overhead figures are
measurements under specified host and guest conditions, not guarantees for an
Agel image. We should benchmark boot-to-Agel-REPL and per-world memory ourselves.

### Snapshot caution

Firecracker snapshots are operational VM state, not an Agel transaction or a
security proof. Firecracker's snapshot documentation treats snapshot files and
the host as trusted, uses checksums for accidental corruption rather than
cryptographic authenticity, and leaves disk-state coordination to the caller.
Cloning snapshots can also duplicate guest-level identifiers or secrets.

The authoritative Agel state remains its canonical, signed event/image format.
A VM snapshot may accelerate restore only after its corresponding disk, device
and Agel image roots have been validated.

### rust-vmm fit

rust-vmm provides reusable crates such as `kvm-ioctls`, `vm-memory`, loaders,
virtio devices and seccomp utilities. These are good ingredients for a future
Agel host launcher or purpose-built orchestrator. They are not useful inside a
seL4 guest kernel, and importing a device crate is not equivalent to inheriting
Firecracker's isolation or review.

### Firecracker/rust-vmm recommendation

Use Firecracker later as an optional Linux deployment envelope. Consider
rust-vmm when Agel needs a custom host orchestrator. Do not base the native Agel
kernel ABI on either project.

## Redox OS

Redox is a substantial general-purpose OS written in Rust. It puts many
facilities in user-space daemons and exposes resources through **schemes**.
Scheme services receive request queue entries and return completion entries;
ordinary file-descriptor operations can therefore address filesystems, devices
and other resources through a uniform namespace. `relibc`, the Redox C library,
provides an important POSIX-facing compatibility layer.

This offers several useful lessons:

- a Rust OS can move drivers and high-level services out of the kernel;
- a uniform service namespace makes composition and tooling pleasant;
- queue-based user/kernel protocols need explicit lifecycle and cancellation;
- POSIX compatibility can be a library/service personality rather than the
  conceptual center of a new system; and
- usable desktop hardware support is an enormous, long-running effort.

Redox is nevertheless a different optimization point. It aims to be a usable
Unix-like general-purpose OS and therefore carries kernel mechanisms,
compatibility semantics and existing application expectations that Agel does
not initially need. Its scheme handles are an appealing service interface, but
Agel must retain capability-derived authority rather than assuming a string
path is authority.

There are three plausible relationships, in increasing order of commitment:

1. borrow protocol, driver, userspace and `relibc` ideas;
2. port an Agel runtime to Redox as another hosted environment; or
3. fork Redox as the main Agel OS.

The first two are useful; the third is not recommended now. It would make Agel
responsible for a broad compatibility OS before its own small kernel contract,
capability model and recovery semantics are stable.

## The linked Oxide OS prototype

The repository at `gkganesh12/oxide-os` describes itself as an agent-native
microkernel and implements tasks, IPC, a global capability table, networking,
storage, inference scheduling and crypto-related experiments in Rust. Its
capability table models ownership, attenuated delegation and recursive
revocation—concepts worth testing in Agel.

The repository must not currently be treated as a secure microkernel base. Its
own `TODO.md` records, among other limitations:

- no ring-3 userspace; all code runs in ring 0;
- cooperative rather than true timer preemption;
- no SMP;
- no page-table unmap/free path and a fixed small heap;
- an in-memory-only filesystem;
- virtio block DMA addresses that work in QEMU but not real hardware; and
- no TLS.

Code inspection also shows that the capability table is kernel-global and uses
numeric IDs plus a recorded task owner. That may become a legitimate kernel
object model once callers are isolated and syscall entry identifies the real
task, but while all code shares ring 0 it is a convention, not containment.
Networking, HTTP, storage, GPU scheduling, agents and crypto also currently sit
inside the kernel crate, enlarging the privileged failure domain.

The project is valuable as a compact experiment and a warning against judging
security from APIs alone. Borrow tests and agent-facing ideas only after an
independent audit. Do not fork it as Agel's security foundation at this stage.

### Do not confuse it with Hubris

Oxide Computer Company's **Hubris** is a separate project with a materially
different architecture: a very small Rust kernel, statically described tasks,
memory protection, message-passing IPC, isolated and restartable drivers, and
the Humility tool for live and postmortem inspection. It targets embedded
systems rather than a general-purpose agent workstation.

Hubris is nevertheless highly relevant to Agel. Its most important lesson is
that restart and introspection are architectural properties: the supervisor
knows task identity and generation, servers can fail without kernel failure,
and tooling can inspect a system without teaching every task a debug protocol.

## Additional important references

### L4 and minimality

Jochen Liedtke's *On micro-kernel construction* remains foundational: IPC cost
is determined by design and implementation discipline, not by an unavoidable
law that microkernels must be slow. Agel should keep the synchronous control
path short, bounded and allocation-free, while moving bulk bytes through shared
memory.

### Fuchsia Zircon

Zircon is a larger, pragmatic kernel rather than a seL4-style minimal verified
microkernel. Its object handles are still an excellent application-facing
model: handles carry explicit rights, are scoped to a process, and can be
transferred over channels. Fuchsia's component framework adds declarative
capability routing through a component tree. Agel should borrow the ergonomics,
not the kernel size.

### Genode

Genode demonstrates a capability-oriented component framework that can run on
multiple kernels, including seL4. It is strong evidence for separating an OS
personality and component contract from a single kernel implementation. Agel's
kernel-neutral ABI should pursue the same kind of architectural leverage while
remaining much smaller initially.

### Tock

Tock combines Rust kernel code, hardware process isolation and **grants**, which
let kernel capsules associate state with processes without trusting a process
to provide safe kernel memory. The precise mechanism is not directly portable
to Agel's preferred userspace-service design, but the ownership lesson is:
state must be charged to and reclaimed with the protection domain that caused
it.

### Barrelfish

Barrelfish models a multicore machine as a distributed system: communicate by
messages, replicate state deliberately and prefer split-phase operations.
Agel should use this model before adding SMP. A single global Lisp heap or
global capability lock would turn live evolution into a multicore bottleneck
and a catastrophic failure boundary.

### Theseus

Theseus explores a Rust "intralingual" OS, replaceable runtime components and
fine-grained recovery. It is useful research for live code evolution, but type
and language memory safety do not substitute for protection domains when Agel
runs generated native code, C, device drivers or compromised dependencies.

### CHERI

CHERI extends conventional architectures with hardware capabilities for memory
references and compartmentalization. It is a compelling future target for
fine-grained native Agel objects and FFI confinement, especially on capability
enabled RISC-V or Morello-class hardware. It complements rather than replaces
seL4's kernel object capabilities and system policy.

## Proposed Agel native architecture

```text
                     signed system manifest
                              |
                  immutable recovery / root PD
                   /          |             \
          world manager   capability       health + A/B
          and loader      broker/policy     supervisor
                |              |
     +----------+--------------+--------------------------+
     |                    seL4 IPC/caps                    |
     +----------+------------------+-----------------------+
                |                  |
       Agel world PD(s)       service protection domains
       - reader/evaluator     - storage + image log
       - logical agents       - network broker
       - transactional heap   - serial/display/input
       - stdlib               - audio/voice
                              - model/tool broker
                              - device drivers
                                      |
                            optional Linux service VM
                            for GPU/models/legacy stacks
```

The recovery/root domain is deliberately boring and not live-rewritable by an
ordinary agent. It verifies manifests, creates or activates worlds, retains the
previous known-good slot, routes only declared initial capabilities and observes
health. A candidate Agel world can propose a replacement for any component,
including the supervisor, but cannot approve or install itself.

### Protection-domain policy

| Domain | Contains | Must not contain |
|---|---|---|
| seL4 kernel | address spaces, threads, IPC, capabilities, IRQ and scheduling mechanisms | Lisp evaluator, filesystems, network stacks, policy, model clients |
| recovery/root | measured boot handoff, manifest verification, object allocation, A/B promotion, watchdog | natural-language planning, ordinary application code |
| Agel world | evaluator/compiler, transactional state, many logical agents, stdlib | raw device access, ambient host credentials |
| storage/image | append-only log, content addressing, atomic image publication | authority to approve source changes |
| model/tool broker | provider credentials, rate/budget policy, typed effect execution | authority to install its own callers or bypass audit |
| driver domains | one device or narrowly related device class | global policy and unrelated drivers |
| legacy/model VM | GPU stacks, large runtimes, untrusted native tools | root capabilities to the Agel system |

An Agel `agent` is not automatically a kernel thread or process. Millions of
small agents may be deterministic language objects inside one world. Create a
separate protection domain when there is a distinct trust boundary, native/FFI
code, independently budgeted workload, privileged resource, or required failure
containment.

## Minimal Agel kernel contract

The native backends should implement a small semantic contract rather than
expose every seL4 detail directly:

```text
ProtectionDomain  create/start/stop/fault/reap
AddressSpace      map/unmap/protect/query
Thread            configure/resume/suspend
Endpoint          call/reply/send/receive
Notification      signal/wait/poll
Capability        copy/mint/attenuate/move/revoke
Frame             allocate/map/share/reclaim
Interrupt         bind/ack/mask
ScheduleContext   budget/period/priority/bind/unbind
Clock             monotonic-now/deadline
BootInfo          manifest, memory and platform description
```

The contract needs versioned IDL, canonical error values and a conformance test
suite that runs against both seL4 and the research kernel. Backend-specific
power remains behind explicitly nonportable modules.

Do not put unbounded mailboxes, S-expressions, garbage collection, strings,
JSON, model tokens, filesystem paths or network policy into this ABI. Kernel IPC
transports bounded words, notifications and capability handles. Typed Agel
protocols live above it.

## Capabilities in Agel

An Agel capability value must be an opaque reference backed by a kernel-held
capability slot or a broker-held object reached through one. It must not be
authority merely because it contains an unguessable string or valid HMAC.

Required rules:

- no ambient capabilities after world construction;
- mint only equal or weaker rights and narrower resource scope;
- transfer explicitly in an IPC message;
- distinguish copying from moving;
- bind leases and budgets where authority should expire;
- audit derivation and transfer without logging bearer secrets;
- make revocation semantics explicit; and
- invalidate handles when a server restarts by including a generation.

Pure seL4 revocation should be used for kernel object derivation trees. For
high-level resources with many clients, an indirection object, epoch or broker
lease may make semantic revocation cheaper and easier to audit.

## IPC and interaction model

Use two paths:

- **bounded synchronous call/reply** for small control operations where the
  dependency and budget donation are explicit; and
- **shared-memory rings plus notifications** for bulk or streaming data such as
  model tokens, audio, display buffers, disk blocks and network packets.

Every queue has a fixed capacity, ownership rule, backpressure behavior and
recovery protocol. There are no unbounded kernel mailboxes. Protocol schemas are
versioned and generated from data that Agel can inspect. Cancellation,
deadlines, duplicate requests, server restart and client death are protocol
states, not undocumented errors.

Human text and voice remain inputs, never authority. A language model may turn
them into a typed proposal; a capability and policy decision authorize effects.

## Scheduling and resource accounting

MCS-style scheduling contexts are the right conceptual model even where a
backend implements them differently. Charge CPU time, pages, IPC buffers,
outstanding effects and persistent bytes to a world or service principal.

- Keep foreground interaction on a reserved budget independent of background
  planning and model calls.
- Propagate or donate bounded budgets along synchronous RPC chains.
- Detect priority inversion and bound critical sections.
- Give the recovery domain a reserved scheduling context and memory pool.
- Never let model output size, mailbox growth or a proof search consume an
  unbounded resource.
- On multicore, prefer per-core ownership and message passing over global locks.

## Safe self-improvement protocol

Self-improvement must replace isolated components, not patch the only executing
kernel image:

```text
natural-language intent
  -> typed immutable proposal
  -> source/IR and dependency hashes
  -> isolated build with no production capabilities
  -> static checks + model checking + tests + fuzzing
  -> candidate PD with strict CPU/memory/effect budgets
  -> replay and canary traffic
  -> independent verifier evidence
  -> supervisor atomically redirects endpoints
  -> old generation drains, then remains rollback-capable
```

Executable mappings are write-or-execute, never both. A promoted service gets a
new generation and fresh capabilities; stale clients receive a typed restart
error or are routed through a supervised proxy. Kernel replacement is an A/B
reboot operation with signed images and an external watchdog, not an `eval`.

Macros are useful for generating protocol implementations, specifications and
proof obligations. They are not themselves proofs. Models can propose lemmas,
tests and counterexamples, but a small deterministic checker decides whether
formal evidence is valid.

## Persistence and irreversible effects

Language transaction rollback cannot undo a packet already sent, a payment, a
disk write or a model call. Separate the mutable Agel world from effect servers:

1. stage a typed effect intent in the world's transaction;
2. commit the world revision and durable intent together;
3. execute through a capability-owning server with an idempotency key;
4. append the result to a tamper-evident event log; and
5. deliver a result message that replay can reproduce without re-execution.

Storage publication uses immutable objects plus a small atomic root update.
System software uses signed A/B roots, verified boot selection and a recovery
counter outside the candidate world. VM snapshots and memory checkpoints are
caches; the canonical log and image roots are the source of truth.

## Verification strategy

Verification should be layered:

1. **Kernel:** reuse seL4's proofs by pinning an eligible configuration and not
   modifying the kernel.
2. **System graph:** mechanically validate manifests for capability leakage,
   missing budgets, shared writable memory and forbidden dependency cycles.
3. **Protocols:** model check lifecycle, cancellation, restart, revocation and
   bounded-queue behavior with TLA+, Alloy or an equivalent finite model.
4. **Language:** specify the small evaluator and transaction semantics; maintain
   differential implementations and property tests.
5. **Native code:** deny unsafe code by default, isolate required C/assembly,
   fuzz every parser and IPC boundary, and use sanitizers in hosted builds.
6. **Updates:** require reproducible builds, signed manifests, evidence hashes,
   canary budgets, A/B rollback and watchdog tests.
7. **Operations:** preserve structured crash records and capability-safe
   introspection across service restarts.

The release manifest should state what is proved, tested, assumed, trusted and
explicitly out of scope. "Runs on seL4" must never be presented as proof that
Agel's user-level policy or evaluator is correct.

## Incremental implementation roadmap

### Phase 0 — freeze the boundary

- Write the versioned kernel contract and threat model.
- Add conformance traces independent of either backend.
- Mark today's ring-0 native evaluator as a bootstrap implementation, not a
  security boundary.

### Phase 1 — research-kernel isolation

- Add IDT/trap handling, user mode, page tables and separate address spaces.
- Boot a minimal recovery/root task and move the Agel evaluator into ring 3.
- Implement bounded endpoint IPC and opaque capability slots.
- Run a deliberately crashing and looping world without losing the monitor.

### Phase 2 — seL4/Microkit spike

- Select one QEMU target with strong verified-configuration coverage.
- Boot a Rust root/recovery PD, Agel world PD and serial PD.
- Pass the same kernel-contract conformance messages as the research backend.
- Record binary, configuration, proof and toolchain hashes.

Agel's current x86-64 QEMU work can continue, but the assurance spike should
choose architecture/configuration by proof coverage rather than familiarity.
RISC-V or Arm may therefore precede a production x86-64 build.

### Phase 3 — split privileged services

- Serial/input and timers first, then storage/image, networking and model/tool
  brokering.
- Put each risky driver in its own restartable domain.
- Use a Linux service VM for GPU/model stacks before writing native equivalents.

### Phase 4 — durable worlds and effects

- Canonical signed images and event logs.
- Prepare/commit/idempotency protocols with effect servers.
- Crash injection at every persistence transition.

### Phase 5 — live replacement

- Generation-aware service discovery and endpoint switching.
- Replay/canary promotion under resource limits.
- Watchdog-driven rollback to an independently retained image.

### Phase 6 — deployment and hardware

- Firecracker launcher on Linux/KVM; QEMU remains the portable development path.
- IOMMU-aware DMA ownership, measured boot and hardware watchdogs.
- SMP using per-core ownership and explicit distributed state.
- Investigate CHERI-capability hardware as a later fine-grained confinement
  target.

## Decisions and non-decisions

| Question | Decision now |
|---|---|
| Adopt seL4 for the assurance backend? | Yes; build a bounded Microkit spike |
| Fork or add Agel primitives to seL4? | No |
| Keep Agel's native kernel? | Yes, as research/bootstrap/conformance backend |
| Run the evaluator in ring 0 long term? | No |
| Use Firecracker as the guest kernel? | No; optional Linux host envelope |
| Put rust-vmm crates in the kernel? | No; possible host tooling only |
| Fork Redox now? | No; borrow patterns and consider a hosted port |
| Fork the linked Oxide OS now? | No; prototype inspiration only |
| Use Hubris design patterns? | Yes, especially restart and observability |
| One process per Agel agent? | No; isolate by trust/failure boundary |
| Let natural language or a model authorize effects? | No; it may only propose |

Open architecture decisions needing experiments or ADRs:

- first verified target architecture and exact seL4 configuration;
- Microkit-only static topology versus a small dynamic resource manager;
- the stable kernel-contract wire format and compatibility rules;
- world granularity for mutually suspicious applications;
- IOMMU and device-assignment policy on initial hardware;
- whether GPLv2 kernel distribution constraints fit all intended products;
- placement and attestation of local model/GPU services; and
- the minimum POSIX personality required for development tools.

## Hard invariants

- The component being changed cannot be the only component able to roll it back.
- No agent, model, compiler, or macro can mint its own authority.
- No mutable Agel heap is shared with the recovery plane.
- No untrusted driver or provider adapter runs privileged.
- Every resource is owned, bounded, reclaimable and observable.
- Every IPC queue has finite capacity and defined backpressure.
- Every service restart changes its generation; stale authority fails closed.
- Every irreversible effect has an idempotency and recovery story.
- No production update depends solely on model review or generated tests.
- Claims about formal assurance name the exact proved configuration and
  assumptions.

## Primary and foundational sources

### seL4

- [seL4 whitepaper](https://sel4.systems/About/whitepaper.html) — architecture,
  capabilities, verification and deployment overview.
- [Proofs and properties](https://sel4.systems/Verification/proofs.html) — the
  relationship among refinement and security properties.
- [Proof assumptions](https://sel4.systems/Verification/assumptions.html) — the
  critical boundary of formal claims.
- [Verified configurations](https://docs.sel4.systems/projects/sel4/verified-configurations.html)
  — property coverage by architecture and configuration.
- [Microkit manual](https://docs.sel4.systems/projects/microkit/manual/latest/) —
  protection domains, channels, memory regions and scheduling attributes.
- [Microkit tutorial](https://docs.sel4.systems/projects/microkit/tutorial/part1.html)
  — concrete component construction.
- [Rust on seL4](https://docs.sel4.systems/projects/rust/how-to-use.html) — current
  Rust ecosystem entry point.
- [seL4 licensing](https://www.sel4.systems/Legal/license.html) — kernel and
  userspace licensing distinctions.
- Klein et al., [*seL4: Formal Verification of an OS Kernel*](https://sel4.systems/Research/pdfs/seL4-formal-verification-operating-system-kernel.pdf).

### Firecracker and rust-vmm

- [Firecracker design](https://github.com/firecracker-microvm/firecracker/blob/main/docs/design.md)
  and [specification](https://github.com/firecracker-microvm/firecracker/blob/main/SPECIFICATION.md).
- [Firecracker FAQ](https://github.com/firecracker-microvm/firecracker/blob/main/FAQ.md)
  — host, guest and operational constraints.
- [Snapshot support](https://github.com/firecracker-microvm/firecracker/blob/main/docs/snapshotting/snapshot-support.md)
  — trust, consistency and clone caveats.
- [rust-vmm community](https://github.com/rust-vmm/community/blob/main/README.md)
  and [crate repository](https://github.com/rust-vmm/rust-vmm).

### Redox and the linked Oxide prototype

- [Redox repository](https://github.com/redox-os/redox),
  [kernel](https://gitlab.redox-os.org/redox-os/kernel), and
  [organization](https://github.com/redox-os).
- Redox book: [communication](https://github.com/redox-os/book/blob/master/src/communication.md)
  and [scheme operation](https://github.com/redox-os/book/blob/master/src/scheme-operation.md).
- [`relibc`](https://github.com/redox-os/relibc) — Redox's C library and
  compatibility boundary.
- [gkganesh12/oxide-os](https://github.com/gkganesh12/oxide-os) and its
  [technical debt](https://github.com/gkganesh12/oxide-os/blob/master/TODO.md).

### Further architecture references

- Liedtke, [*On micro-kernel construction*](https://flint.cs.yale.edu/cs428/doc/p237-liedtke.pdf).
- [Hubris](https://github.com/oxidecomputer/hubris) and the
  [Hubris documentation site](https://hubris.oxide.computer/).
- Fuchsia: [Zircon kernel](https://fuchsia.dev/fuchsia-src/concepts/kernel),
  [kernel objects](https://fuchsia.dev/fuchsia-src/reference/kernel_objects/objects),
  [handles](https://fuchsia.dev/fuchsia-src/concepts/kernel/handles), and
  [components](https://fuchsia.dev/fuchsia-src/concepts/components/v2/introduction).
- [Genode Foundations](https://www.genode.org/documentation/genode-foundations-25-05.pdf).
- [Tock SOSP paper](https://tockos.org/assets/papers/tock-sosp2017.pdf) and
  [ten-year retrospective](https://tockos.org/assets/papers/2025-sosp-tock-decade.pdf).
- Baumann et al., [*The Multikernel: A New OS Architecture for Scalable Multicore Systems*](https://www.microsoft.com/en-us/research/wp-content/uploads/2009/10/paper.pdf).
- [Theseus OS](https://github.com/theseus-os/Theseus).
- [CHERI FAQ](https://www.cl.cam.ac.uk/research/security/ctsrd/cheri/cheri-faq.html)
  and [CHERI technical report](https://www.cl.cam.ac.uk/techreports/UCAM-CL-TR-850.html).

## Bottom line

Agel's unique power should live above the kernel: homoiconic protocols,
inspectable agent graphs, transactional worlds, capability-safe tools,
proof-carrying proposals and live replacement. The kernel's job is to remain
small and unsurprising while enforcing the boundaries that an evolving Lisp
world cannot be trusted to enforce for itself.

seL4 gives Agel the best available assurance anchor. The small Agel kernel keeps
the project intellectually independent and provides a transparent bootstrap and
conformance target. Firecracker supplies an optional outer VM boundary; Redox,
Hubris, Zircon, Genode, Tock, Barrelfish, Theseus and CHERI supply tested design
ideas. That division lets Agel be radically dynamic without making its recovery
and authority foundations radically fragile.
