# Agel v0.2.68 — Room in memory

The third rung of *Does it run DOOM?* begins with memory: a domain may
hold as many of the pool's frames as it needs, a reclaimed domain gives
every frame back, and the x86-64 pool is as large as the other machines'.

## What changed

- **The frame ledger is a bitmap over the pool.** A domain's record of
  its frames was a list of 512, so a process could never hold more than
  2 MiB and a larger one was refused with `LedgerFull`; it is now one
  bit per frame of the pool, the same size for every domain, and the
  error is gone. A DOOM-sized process, its heap and its canvas fit.
- **The pool's free list is a bitmap too.** It held 192 frames and
  silently leaked what a larger domain gave back, so every `:exec` of a
  program with a heap lost frames for good; every reclaimed frame is now
  handed out again, lowest first, zeroed.
- **The x86-64 pool is 46 MiB** (from 2 MiB to 48 MiB of the 64 MiB QEMU
  is given), as AArch64's and RISC-V's are, and the supervisor's own
  identity window now reaches the pool's end rather than a fixed 16 MiB:
  the first regression caught a frame above it faulting the supervisor
  as it was zeroed.

## Proof

`scripts/test-isolation.sh` still finds the console and storage drivers'
frames reclaimed and their replacements built from them on three
machines, and the pool holding as many frames after as before. The full
regression passes.

## Not claimed

The disk is still 3 MiB with a 512 KiB program region and 64 KiB files;
that is the rest of this rung. No process can hold more than its 16 MiB
window.
