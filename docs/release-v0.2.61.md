# Agel v0.2.61 — Files beyond one block

A file grows to 64 KiB over a pool of blocks the filesystem service
zeroes as it hands them out and takes back as files shrink or go.

## What changed

- **`agelfs` version 2.** The superblock holds a bitmap of the region's
  sixty-three 4 KiB data blocks; an entry holds up to sixteen block
  numbers. A write into a byte with no block takes a free one, zeroes it
  on the disk, and records it in the bitmap and then the entry before
  the byte lands. A cut gives back the blocks past the new length; a
  removal gives back all of them. A write past 64 KiB is `EFBIG`; a
  region with no free block, `ENOSPC`.
- **The service's read, write and truncate** walk the block list; the
  supervisor's side is unchanged, and so is the process protocol.
- A region formatted as `AGELFS1` is `EIO` until formatted again; the
  shape changed and nothing converts it.

## Proof

`scripts/test-libc.sh` on x86-64, AArch64 and RISC-V runs `c-big`: fifty
thousand bytes written and read back intact, the file cut to 3,000 and
grown to 10,000 with the first bytes kept and the growth zero, a byte
past 64 KiB refused with `EFBIG`, three files of 64 KiB written and the
fourth refused with `ENOSPC` (the writer's two files and the big one
take the other five blocks), everything removed, and 64 KiB written
again. Every earlier file test passes over the new shape. The full
regression passes.

## Not claimed

No recovery of a block leaked by a power cut between the bitmap and the
entry, no free-space accounting beyond the bitmap, no conversion from
the first shape.
