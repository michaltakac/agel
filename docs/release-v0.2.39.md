# Agel v0.2.39 — Kernel contract v1.1: the memory group

Since v1.0 the kernel contract has declared `frame.allocate`, `frame.map`,
`frame.share`, `frame.reclaim` and `as.map`, `as.unmap`, `as.protect`,
`as.query`, and defined nothing about them: every backend refused them.
Contract v1.1 says what they mean, and both hosted implementations answer
them identically.

```text
memory/share-narrows: frame.share(cap=16 0x11 0x1 0x7 0x0) -> ok 0x14 0x0 0x0 0x0
memory/reclaim-frame-1: frame.reclaim(cap=16 0x0 0x0 0x0 0x0) -> ok 0x1 0x1 0x0 0x0
memory/the-shared-handle-fails-closed: frame.map(cap=17 0x3 0x1 0x0 0x0) -> revoked 0x0 0x0 0x0 0x0
```

## What changed

- **A specification.** A conformance domain has a frame, an address space
  with an eight-page window, and a budget of four more frames. Pages are
  named by index, frames by number. Allocation is an act of the capability
  space; sharing needs `grant` on the frame itself; a mapping can carry no
  right its capability lacks and can only lose rights in place; reclaiming
  unmaps, revokes every share, and returns the frame. The document has the
  table.
- **Two implementations, 37 new steps.** The reference model and the
  independent implementation both answer the whole 118-step corpus, and
  `conformance::compare` requires them to agree under either profile.
- **A transcript per profile.** A backend publishes the profile it
  implements, and the corpus records what it answers: `kernel-contract.trace`
  is the v1.1 transcript and `kernel-contract-v1.0.trace` the v1.0 one. The
  research kernels publish v1.0 until their frame window is backed by real
  page-table mappings, and the seL4 broker publishes v1.0 because Microkit's
  mappings are static. Nothing claims a group it cannot make real.

## Also in this release

The memory group is a crate feature, on by default and on in the isolation
builds, and off in the x86-64 workshop images: they publish v1.0, never
invoke the group, and linking it cost them their 254-sector budget. A
mapping that is both writable and executable is `not-permitted` on every
backend, checked after the capability has been found sufficient, because no
machine here grants both and the contract should not promise what the
machines refuse.

## Verification

```sh
./scripts/test-kernel-contract.sh
cargo test -p agel-kernel-abi
./scripts/test-isolation.sh
./scripts/test-sel4.sh
```

## What this does not claim

No backend maps memory yet. The next rung backs the research kernels' frame
window with their page tables and flips their profile to v1.1; the domain
and interrupt groups remain declared and unspecified.
