# Agel v0.2.58 — Names

The POSIX personality's fourth stratum gains the operations on names:
`unlink`, `rmdir`, `rename`, `stat`, `mkdir`, `opendir`, `readdir` and
`closedir`, and the C library gains `scanf` and `getopt`.

## What changed

- **Two service commands.** The filesystem service removes a file or an
  empty directory and moves an entry to a name that does not exist,
  resolving both paths from the root it is given as `open` does; the
  root cannot go, and a directory cannot be moved under itself.
- **Four process requests.** `unlink`, `rename`, `stat` and `readdir`,
  each bounded by the namespace's rights in the supervisor before the
  service sees a path: removing and moving need `write`, asking needs
  `read`. `readdir` walks a directory opened through the namespace, the
  descriptor's offset counting the children given.
- **The C library:** `<unistd.h>` `unlink`, `rmdir`, `getopt`;
  `<stdio.h>` `rename`, `ungetc`, `sscanf`, `fscanf`, `scanf` (`%d %i %u
  %x %X %o %s %c %% %n`, widths, `l`/`ll`/`h`, one character of pushback
  on streams); `<sys/stat.h>` `stat` and `mkdir`; `<dirent.h>`
  `opendir`, `readdir`, `closedir`. The library's C is now every file
  under `libc/c`.
- **`dir.c`** parses its options, stats the writer's notes, makes a
  directory, moves the file into it, lists both directories, is refused
  `rmdir` on the full one, removes the file and the directory, sees the
  file gone, and scans a string.

## Proof

`scripts/test-libc.sh` on x86-64, AArch64 and RISC-V runs `c-dir` in a
namespace rooted at `app` with `-v -o out.txt notes` and requires every
line, then in a read-only namespace at `etc`, where `mkdir` is refused
with `EACCES`. The full regression passes.

## Not claimed

No timestamps, no permissions beyond the namespace's three rights, no
`chdir`, no `truncate`, no `%[` in the scanner, no floating point.
Removed bytes remain on the disk until overwritten.
