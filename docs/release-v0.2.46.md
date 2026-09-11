# Agel v0.2.46 — Breadth begins: arguments, a heap, streams, and a source built unmodified

The C library's fourth stratum is breadth: what real programs need next,
one step at a time, with a test each. This is the first step, chosen so
that a C source written for other systems builds and runs on Agel without
a change.

```text
agel-native[0]> :exec c-digest /app -- notes
968fdb320ca48afd0557fe96b6416246e0b266031146ab2a707a119c0107b684  notes
process c-digest exited with status 0
agel-native[0]> :exec c-breadth /etc -- one two
arguments: 3 [c-breadth] [one] [two]
heap: reuse, join, realloc, ENOMEM
formatter: widths, flags, precision, truncation
strings: strtol, strstr, strrchr, ctype, strcat, qsort
streams: fopen, fprintf, append, fgets, feof, lseek
breadth: 23 checks passed
process c-breadth exited with status 0
```

## What changed

- **Arguments.** `main(argc, argv)` receives what `:exec NAME [ROOT] [ro]
  [-- ARG...]` or a parent's `agel_spawn(program, argv, ...)` supplied; the
  supervisor places the block in the process's payload area before it
  first runs.
- **A heap that gives memory back.** A first-fit free list over a 256 KiB
  arena, with freed blocks joined to free neighbours, `realloc` in place
  when it can, `calloc` clearing.
- **Streams.** `FILE` over descriptors, line buffered out and block
  buffered in: `fopen` with `r`, `w`, `a` and `+`, the `fget`/`fput`
  family, `fread`/`fwrite`, and a formatter with widths, flags, precision,
  `*`, and `l`/`ll`/`z`/`h`, behind `printf`, `fprintf`, `snprintf`,
  `sprintf` and their `v` forms; `perror`; `exit` flushes.
- **Seek and append.** A `seek` request in the process protocol, `lseek`,
  `O_APPEND`.
- **`ctype.h`, `assert.h`, `memory.h`**, and the wider `string.h` and
  `stdlib.h`: `memchr`, `strncpy`, `strcat`, `strncat`, `strrchr`,
  `strstr`, `strdup`, `strerror`, `atoi`, `atol`, `strtol`, `strtoul`,
  `abs`, `labs`, `qsort`.
- **A third-party source, unmodified.** Brad Conte's public-domain
  SHA-256 under `boot/posix/c/third-party`, byte for byte as published,
  with `digest.c` around it; `cat` takes file names now.

## Proof

`scripts/test-breadth.sh [arch]` on x86-64, AArch64 and RISC-V: `cat`
with arguments and a missing name, the SHA-256 digest of a file agreeing
with the host's, and `breadth.c`'s 23 checks of the heap (reuse, joining,
`realloc`, `ENOMEM`), the formatter, strings and numbers, sorting, and
streams over files with append, `fgets`, `feof` and `lseek`. CI runs all
three.

## Not claimed

No `scanf`, `time`, `signal`, `setjmp`, `math`, environment, `getopt`,
`stat`, directories, `unlink` or `rename`, floating point in the
formatter, locale, or threads. `qsort` is quadratic; the heap is one arena
that never grows. The library is not audited against a C standard; its
breadth is what the tests exercise, and it grows a step at a time.
