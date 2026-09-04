# Deployment targets and capability tiers

Status: requirements, 2026-09-04

Agel's primary deployment target is an **NVIDIA DGX machine, or several tied
together, running Agel on bare metal**, where the GPUs are used for fine-tuning
and training as well as inference. That is the configuration in which Agel is
expected to have its full capability set.

Everything else — inference-only nodes, virtual machines, laptops — is a
reduced tier. Those tiers are useful and must keep working, but they are not
what the system is designed around.

This document states what that requires, what it costs, and which previously
recorded decisions it changes.

## The tiers

| | Tier 1 — training node | Tier 2 — inference node | Tier 3 — development |
|---|---|---|---|
| Hardware | DGX, single or clustered | any 64-bit machine, bare metal or VM | any developer machine |
| Local GPU | required; training and fine-tuning | optional; inference only | none |
| Model access | local training and inference, plus external providers | local inference and/or external providers | external providers only |
| Runs under a hypervisor | no; Agel owns the machine | permitted | expected |
| Runs CUDA itself | no; a peer node does | no | no |
| Purpose | the system Agel is for | serving, edge, cheap capacity | building and testing Agel |

Tier 3 is what this repository has been developed on: a MacBook Pro with no
NVIDIA GPU, where every native backend runs under QEMU. Nothing about the work
so far has exercised Tier 1 hardware, and no claim in this repository should be
read as if it had.

## What Tier 1 actually requires

### The CPU is not one architecture

DGX is not a single ISA, and the split runs straight through the current
generation:

| System class | CPU | ISA |
|---|---|---|
| GB200 / GB300 NVL72, DGX Spark, DGX Station GB300 | Grace, Arm Neoverse V2 | aarch64 |
| DGX H100 / H200 / B200 | Intel Xeon | x86-64 |

So **both x86-64 and AArch64 are deployment targets**, not one target and one
research backend. On Grace systems the CPU and GPU share memory over NVLink-C2C
rather than PCIe, which makes the CPU side of the machine part of the
accelerator rather than a host attached to one.

RISC-V is not a DGX target. It stays in the tree as a portability and
conformance backend — its value is that a third independent machine keeps the
kernel contract honest — but nothing is expected to deploy on it.

### CUDA cannot be native, and this is not a matter of effort

NVIDIA's open kernel modules cover the *interface* layers only. The GSP firmware
and the entire user-space CUDA stack — the runtime, the driver API, cuBLAS,
cuDNN, NCCL, the compiler — remain proprietary binaries built for Linux. The
open modules themselves link against `nv-kernel.o_binary`.

There is therefore no path, at any level of effort, by which Agel writes its own
driver and runs CUDA natively on its own kernel. Anyone who tells you otherwise
has not looked at what is actually open.

**The GPU plane is Linux.** The only question is what Linux is *underneath*.

### The shape: separate the nodes, not the address spaces

The obvious reading of "bare metal" is that Agel boots the machine and Linux
becomes a contained domain with the GPUs assigned to it through the IOMMU. That
works on paper. It also drags in a VMM that is not production-ready, an IOMMU
path seL4 does not verify, and a multi-million-line driver stack sharing a
machine with the authority plane.

The product this is being built for is a **cluster of DGX Spark nodes**, and
that makes a better answer available: put the boundary between machines rather
than inside one.

```text
  Agel node                          training node(s)
  ----------                         ----------------
  Agel kernel owns the machine       NVIDIA DGX OS (Ubuntu, CUDA, NCCL)
  no Linux, no blobs, no VMM         the whole proprietary stack
  holds authority and policy         holds compute, holds no authority
  decides what runs and when   --->  runs it, returns content-addressed results
```

One Spark runs Agel. The others run NVIDIA's own OS and do the training. Agel
schedules, admits, budgets and records; it never executes CUDA and never links
a proprietary blob.

Why this is better than a contained Linux domain, rather than merely easier:

- **The node that holds authority is genuinely clean.** No Linux anywhere on it,
  no VMM, no dependence on unverified IOMMU handling for the core claim. The
  microkernel story holds completely on the machine where it matters.
- **It is the boundary Agel already has.** Agel's model-provider path is exactly
  this shape: a capability-scoped, typed, audited, idempotency-keyed request to
  an external system that computes and returns a result, with a transactional
  outbox and an effect journal so a crash cannot double-claim. A training node is
  the same relationship, larger and slower. This is a new provider, not a new
  concept.
- **Much of it is testable without DGX hardware.** The orchestration boundary is
  a protocol and an effect type, not a driver. It can be built and tested on a
  developer machine, which moves a large part of Tier 1 from "impossible here"
  to "mostly possible here".

### The cost of the split: one node's GPU goes dark

A GB10 is one package. The Grace CPU and the Blackwell GPU share memory over
NVLink-C2C and cannot be bought separately. So a Spark running Agel is a Spark
whose GPU does nothing — no training, and **no local inference either**, because
inference needs CUDA for exactly the same reasons training does.

That is not a rounding error. In a two-node product it is half the accelerator
capacity, bought and then left dark. It also lands on the node that would most
like fast local inference, since the agent's own reasoning loop runs there.

The agent does not strictly *need* local inference — Agel's model path already
treats inference as a capability-scoped request to an external system, and a
peer Spark over a 200 Gb/s link is a perfectly good model endpoint. Inference
takes tens to hundreds of milliseconds; a direct link adds a fraction of one.
Latency is not the problem. The problem is the bill.

So the boundary belongs between machines, but *which* machine Agel occupies is
an open product decision, not a technical one:

| Option | Authority plane | GPU waste | Cost |
|---|---|---|---|
| **A. Agel on a dedicated Spark** | clean; no Linux, no VMM, no IOMMU dependency | one whole GB10 idle | 50% of a two-node product, ~25% of a four-node one |
| **B. Agel on a small non-GPU controller node** | clean | none; every Spark computes | a second board type in the product, and a port to it |
| **C. Agel as a process on DGX OS** | Linux is the trusted base | none | gives up the property the native work exists for |
| **D. Agel on a Spark with a contained Linux domain for its own GPU** | Linux and a VMM on the authority node | none | reintroduces the unverified IOMMU path and an unfinished VMM |

Option A is defensible at four or more nodes and hard to justify at two. Option
B is what appliances normally do — a controller alongside the accelerators — and
is the only one that keeps both a clean authority plane and full GPU
utilisation. Option C is a legitimate staging step that ships sooner. Option D
is the design this document originally proposed, and its weaknesses do not
improve by being confined to one node.

**None of this blocks the software.** The orchestration protocol, the remote
compute capability, the training effect type and the content-addressed
checkpoint log are identical whether Agel runs on a dedicated Spark, on a
controller board, or as a Linux process. The decision changes the bill of
materials and the isolation claim; it does not change what has to be built next.

### The isolation depends on the part of seL4 that is not verified

Assigning a GPU to a domain means constraining what that GPU's DMA engines can
reach, which means the IOMMU on x86-64 or the SMMU on Arm. Two facts have to be
put next to each other:

- seL4's verified-configurations table lists **address translation for devices
  (IOMMU) as unverified in every configuration**, along with kernel startup and
  the debug interfaces.
- seL4's own proof assumptions already state that DMA-capable devices can bypass
  CPU page-table isolation unless an IOMMU or a trusted device arrangement
  constrains them.

So the mechanism a DGX deployment would lean on hardest is precisely the
mechanism seL4 carries no proof about. On top of that, `libvmm` — the current
seL4 virtual-machine monitor — describes itself as in development and not ready
for production, with IOMMU support still being worked on.

This does not make the architecture wrong. It makes one specific claim
unavailable: a DGX Agel node **cannot** be described as deriving its GPU
isolation from seL4's proofs. It derives it from hardware the proofs exclude and
from a VMM that is not finished. That has to be written in the release manifest
in those words, the way `docs/sel4-manifest.md` already writes the MCS caveat.

### Sharing state between nodes without sharing a filesystem

The obvious way to connect the nodes is a shared POSIX filesystem. It is the
wrong way: it would mean Agel needs an NFS client, which means a TCP/IP stack
and a filesystem client on the node whose whole value is being small.

Content addressing avoids it. Agel's tamper-evident log already holds digests
and committed inputs rather than bulk bytes. A training job's dataset, weights
and checkpoints stay on the training nodes' own storage; Agel records *what*
they are — digest, size, which job produced them, which grant admitted it — and
names them by hash when it wants one used. The bytes move between Linux nodes,
which already have every tool for that; the decisions move through Agel, which
is the only thing that should be deciding.

This also answers a question left open in
[`microkernel-research.md`](microkernel-research.md): how checkpoints enter the
tamper-evident log without the log becoming a bulk data store. They do not. The
log holds their names.

### A GPU is a resource to be owned, not a device to be opened

NVIDIA hardware already partitions in a way that maps onto Agel's capability
model rather than fighting it:

- **MIG** splits one datacenter GPU into as many as seven instances with
  separate memory paths, L2 banks, memory controllers and DRAM buses. That is a
  hardware-enforced resource split, and it is the natural unit for a capability
  that says "this much GPU, and no more".
- **Confidential computing** on Hopper and later protects a workload's data from
  the hypervisor and other tenants, which is the direction Agel wants for a
  candidate world running someone else's fine-tune.

The requirement that follows: an Agel GPU capability names a *partition with a
budget*, not "the GPU". A world that is granted inference capacity must not be
able to start a training run, and a training job must not be able to grow past
what it was granted.

### Training is a different kind of effect from inference

Agel already has a careful story for model inference: a transactional outbox, an
idempotency-keyed effect journal, exact-result replay, and a trusted adapter
that owns process execution. A training run breaks most of the assumptions
underneath it.

| | Inference call | Training run |
|---|---|---|
| Duration | seconds | hours to weeks |
| Resource | one request | whole GPUs, exclusively, for the duration |
| Result | a response that can be replayed | a checkpoint, non-reproducible in practice |
| Failure | retry | resume from a checkpoint, or lose the work |
| Scope | one process | many processes across many nodes |

So a training job is not a big inference call. It is a long-lived, resource-owning,
externally-checkpointed effect, and the prepare/commit/idempotency shape has to
be extended rather than reused:

- the effect is *admitted* with a resource grant and a deadline, not just
  authorized;
- progress is a sequence of durable checkpoints, each an entry in the same
  tamper-evident log that already carries images and model results;
- replay reproduces the *decision to train and the checkpoints referenced*, not
  the arithmetic;
- cancellation and preemption are protocol states, because a job that cannot be
  stopped is a resource leak with a schedule.

### Several machines are one machine

GB200 NVL72 presents 72 GPUs on a single NVLink fabric, deliberately so that a
rack behaves like one accelerator. Multi-node training additionally uses
InfiniBand or RoCE with GPUDirect RDMA, which is remote DMA straight into GPU
memory.

This is Barrelfish's lesson arriving with teeth: the machine is already a
distributed system, and the interconnect is a DMA path between nodes. Agel's
existing rule — per-core ownership and message passing rather than global locks
— extends to per-node, and the fabric has to be treated as a device class with
its own trust boundary rather than as a faster network.

## What this changes about decisions already recorded

| Previously recorded | Now |
|---|---|
| "x86-64 is the weakest verified target, so the assurance spike must be AArch64 or RISCV64" | Still true about the proofs, but x86-64 is a **required deployment target** for Xeon-based DGX. Assurance and deployment now pull in different directions, and that is a permanent condition rather than a temporary one |
| RISC-V as a candidate assurance target | Demoted to portability and conformance only; nothing deploys there |
| "Use a Linux service VM for GPU/model stacks before writing native equivalents" (Phase 3) | The Linux GPU domain is **permanent**, not a stepping stone. There is no native equivalent to write |
| Firecracker as an optional outer envelope | Unchanged for Tier 2 and CI, but irrelevant to Tier 1: a DGX node is not a guest |
| The recovery plane is small and outside the mutable world | Unchanged, and now load-bearing in a new way: it is also outside a multi-million-line GPU driver stack |

## The tension, stated plainly

"Absolutely full capabilities, including CUDA training on bare metal" and "a
small, auditable trusted computing base" cannot both be maximised on the same
node. The NVIDIA stack is enormous, proprietary, and must be able to program DMA
engines. Nothing about running it on top of a microkernel makes it small.

The resolution Agel takes is to separate *size* from *authority*:

- The GPU domain is allowed to be enormous. It computes.
- The GPU domain is not allowed to be authoritative. It cannot approve a change
  to the system, cannot mint a capability, cannot reach the recovery plane, and
  cannot read another world's memory except where a capability was explicitly
  granted.
- The recovery and authority plane stays small enough to audit, and stays
  outside Linux.

This is the same invariant the project already holds for model output — a model
may propose, a capability decides — applied to the hardware the model runs on.
A compromised or crashed GPU domain must cost the system its training run, not
its ability to say no.

## Requirements, as testable statements

Tier 1:

1. Agel boots a DGX-class machine on both x86-64 and aarch64 without a host OS.
2. GPUs are assigned to a Linux domain through the IOMMU or SMMU, and that
   assignment is described in the system manifest rather than discovered.
3. The full NVIDIA stack runs unmodified inside that domain, at native speed,
   for training and fine-tuning as well as inference.
4. A GPU capability names a bounded partition, not a device.
5. A training job is admitted with a resource grant and a deadline, checkpoints
   into the tamper-evident log, and is cancellable.
6. The GPU domain crashing or being compromised loses work, and loses nothing
   else: the recovery plane, the authority plane, and other worlds survive.
7. Multi-node work treats the fabric as a device class with a trust boundary,
   not as a fast network.
8. The release manifest states which isolation properties rest on unverified
   IOMMU handling.

Tier 2:

9. Inference-only Agel runs on any 64-bit machine, bare metal or virtualized,
   with no local GPU required, using external providers through the existing
   capability-scoped adapters.
10. A Tier 2 node cannot be persuaded to think it is Tier 1: the absence of a
    training capability is a property of what it was granted, not of a
    configuration flag it could change.

Tier 3:

11. Everything except the GPU plane is developable and testable on a machine
    with no NVIDIA hardware, which is how this repository is built today.

## What exists today against this

Very little, and it is worth being exact about it.

- Tier 3 is real: the language, the runtime, the kernel contract, the three
  research backends and the seL4 spike all run on a developer machine under
  QEMU.
- Tier 2 is partly real: external model providers already work through
  capability-scoped adapters with a typed effect boundary and an audit log.
  Local inference does not exist.
- Tier 1 does not exist at all. No GPU code, no IOMMU work, no VMM, no Linux
  domain, no training effect type, no multi-node anything. The nearest thing in
  the tree is a research kernel that can contain a world that misbehaves on
  three architectures, which is a precondition rather than a start.

The roadmap in [`microkernel-research.md`](microkernel-research.md) is written
against this document; the phases that change are marked there.
