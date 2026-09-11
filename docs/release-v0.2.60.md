# Agel v0.2.60 — The heap grows

A C program's heap lives on pages the supervisor maps at its break and
grows on demand; the library gains `chdir`, and files can be truncated.

## What changed

- **`brk` (20):** fresh zeroed pages at the process's break, which
  starts a guard page past the image; at most 64 per request; `-ENOMEM`
  when the window, the pool or the ledger is exhausted. The frame ledger
  is 512 frames on every build (it was 160 on the serial ones).
- **The C library's heap** is those pages: the first `malloc` takes 64,
  a request nothing fits takes more, extending the last free block or
  following it; `free` still joins neighbours. `sbrk` is there for
  programs that manage their own break.
- **`chdir` and `getcwd`** in the library, over a working directory the
  namespace does not have: folded, checked with `stat`, joined to every
  relative path before the request.
- **`ftruncate` (21)** through a new service command that zero-fills what
  grows; `truncate` opens, sets and closes.
- `heap.c` proves each.

## Proof

`scripts/test-libc.sh` on x86-64, AArch64 and RISC-V runs `c-heap`: it
fills and checks sixteen 64 KiB blocks, frees them, takes one megabyte
and writes both ends, is refused 64 megabytes with `ENOMEM`, reads the
break, changes into `app` and reads the notes, is refused a directory
that is not there, truncates a file of a hundred bytes to ten, grows it
to twenty and reads the new bytes as zeros, and truncates it to nothing.
The full regression passes.

## Not claimed

No `mmap`, no shrinking of the break, and page-table frames are still
outside the ledger, as before.
