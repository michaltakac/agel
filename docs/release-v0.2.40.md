# Agel v0.2.40 — The frame window is real

v0.2.39 specified the contract's memory group and no backend mapped
anything; the research kernels published v1.0 and refused it. They publish
v1.1 now, and the mappings are page-table entries.

```text
isolation[aarch64]: a world mapped its frame, wrote through the mapping, and read the value back
isolation[aarch64]: a mapping protected to read-only refused the write: page-fault
isolation[aarch64]: an allocated frame was written through the window, unmapped, and the page then faulted: page-fault
```

## What changed

- **Frames and tables built with the domain.** Every domain carries the five
  physical frames behind its budget and the page tables under its eight
  window pages, so a memory operation at trap time never allocates.
- **Reconciliation after every operation.** After each `frame.*` or `as.*`
  invocation the object table accepts, the supervisor makes the page tables
  say what the object table's window says, page by page: a mapping becomes
  a leaf entry with the mapping's rights, an unmapping becomes an absence.
  `execute` maps as read-and-execute; `write` with `execute` is refused by
  the contract before it reaches a table.
- **Asserted on three machines.** The isolation self-test has a world write
  through a mapping and read the value back, fault on a write to a
  read-only mapping, and fault on a read of a page it unmapped. The
  research kernels reproduce the v1.1 transcript; the supervisor's oracle
  publishes v1.1 too.
- **A caught mistake.** The first build reused the x86-64 divide-by-zero
  provocation's command code for the window touch; the divide world
  page-faulted reading the window, and the self-test refused the run.

## Verification

```sh
./scripts/test-isolation.sh
./scripts/test-kernel-contract.sh
./scripts/test-sel4.sh
```

## What this does not claim

The window is eight pages and the budget five frames per domain, fixed at
build. No operation crosses domains. seL4 still publishes v1.0, and the
x86-64 workshop images publish v1.0 with the group compiled out.
