# Agel v0.2.35 — Frames come back

Since v0.1.5 a driver domain could be lost and replaced at a new generation,
and since then every document has said the same thing about it: the frame
pool never reclaims, so each replacement cost frames the machine would never
see again. That was stated rather than hidden. It is now fixed.

```text
isolation[riscv64]: the console driver's 8 frames were reclaimed and its replacement built from them
isolation[riscv64]: the storage driver's 9 frames were reclaimed and its replacement built from them
```

## What changed

- **Every domain knows its frames.** The pool records what it hands out
  while a domain is being built, into a bounded ledger the domain keeps. A
  domain that would need more frames than a ledger names is refused, and a
  build that fails part-way gives back what it took.
- **A replaced domain gives them back.** On restart, the stopped domain's
  frames return to the pool before the replacement is built, so the
  replacement is built from them; the pool holds exactly as many frames
  after the restart as before the fault. Frames are zeroed when handed out,
  whichever way they came, so nothing a dead domain wrote reaches its
  successor.
- **Asserted on three machines.** The isolation self-test counts the pool
  before it kills the console driver and the storage driver and after each
  is replaced, and fails on any difference.
- **The storage DMA frame is part of the domain.** It is allocated inside
  the domain's ledger now, which the assertion found: the first run leaked
  exactly one frame per storage restart.

## Verification

```sh
./scripts/test-isolation.sh
```

## What this does not claim

Only replaced domains give frames back; the evaluator and the workshop's
domains live for the session. Reclamation is compiled where restart is: the
self-test builds that replace domains carry the ledgers and the free list,
and the interactive workshops, which never replace one, carry a one-word
ledger so the x86-64 image keeps its 254-sector budget; the first build with
ledgers everywhere was 1,257 bytes over it. The free list is bounded, and a frame that
would not fit is leaked rather than misfiled. Nothing here is a general
allocator, and no world can ask for memory: the memory group of the
contract is still outside every backend's profile.
