# Agel v0.2.31 — Two implementations on every machine

v0.2.25 added a second implementation of the kernel contract, written from
the document and the corpus without reading the reference model, and ran it
in the seL4 broker. The three research kernels kept linking the reference
model, so their byte-identical transcripts proved that the hardware boundary
held and nothing about the semantics. This release closes that gap.

```text
isolation[riscv64]: unprivileged corpus matches the reference model
isolation[riscv64]: the world answered with the independent implementation behind a trap gate; the supervisor checked all 81 steps against the reference model
```

## What changed

- **The independent implementation behind the trap gate.** On x86-64,
  AArch64 and RISC-V the object table in supervisor-only memory is now
  `agel_kernel_abi::independent`, the same implementation the seL4 broker
  answers with. Every domain, including the console, storage, input and
  evaluator drivers, invokes the contract through it.
- **The reference model as the live oracle.** The isolation self-test keeps
  the reference model on the supervisor side and compares each of the world's
  81 answers as it is produced. A divergence between the two implementations
  fails the boot of every native backend.
- **Smaller images.** The independent implementation is the more compact of
  the two; the graphics image shrank by about four kilobytes.

## Verification

```sh
./scripts/test-isolation.sh
./scripts/test-kernel-contract.sh
./scripts/test-sel4.sh
```

## What this does not claim

Two implementations agreeing is not a proof of either; both are unverified
Rust and the corpus is 81 steps. The reference model is now the oracle rather
than the thing behind the boundary, and nothing checks the oracle except the
frozen transcript and the hosted comparison.
