# Agel v0.2.25 — Two implementations of the kernel contract

Every backend of the kernel contract had been the same code. The research
kernels on x86-64, AArch64 and RISC-V and the broker on seL4 all linked the
reference model, so the byte-identical transcripts CI demanded showed that
four boundaries held and nothing about whether the semantics behind them were
right. This release adds the second implementation the contract document had
promised and puts it behind the boundary Agel did not build.

```sh
./scripts/test-kernel-contract.sh
./scripts/test-sel4.sh
```

## What changed

- **`agel_kernel_abi::independent`.** Written from `docs/kernel-contract.md`,
  the crate's type definitions, the 81-step corpus and its frozen transcript,
  deliberately without reading the reference model. It shares only the
  contract types and the well-known slot and profile constants.
- **Held to the same bytes.** It reproduces `bootstrap/kernel-contract.trace`
  exactly; `conformance::compare` requires it to agree with the reference
  model on every step; and a deliberately widening variant is still caught at
  `derive/mint-cannot-widen`. The contract script now diffs both hosted
  transcripts against the freeze.
- **The seL4 broker runs it.** The broker protection domain answers protected
  procedure calls with the independent implementation, so the seL4 transcript
  is the agreement of two implementations behind two different kinds of
  boundary. The research kernels keep the reference model behind their trap
  gates.
- **Unpinned choices are written down.** Where the corpus does not fix an
  ordering or a value, the implementation says which choice it made where it
  makes it, so the next contract minor can freeze it as a step.

The corpus earned its keep during the work: the first draft revoked four
descendants where five were expected, because it tombstoned a parent before
walking to its child. A single implementation cannot notice that about itself.

## Verification

```sh
cargo test -p agel-kernel-abi
./scripts/test-kernel-contract.sh
./scripts/test-sel4.sh
./scripts/sel4-manifest.sh
```

## What this does not claim

Two unverified implementations agreeing on 81 steps is evidence, not proof.
The research kernels still run the reference model, so the boundary is diverse
on three architectures while the semantics there are not. The contract,
corpus, transcript and profile are unchanged; the memory, domain and interrupt
groups remain outside every backend's profile.
