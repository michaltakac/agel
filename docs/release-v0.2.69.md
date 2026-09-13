# Agel v0.2.69 — Room on the disk

The rest of the third rung of *Does it run DOOM?*: a 32 MiB image, a
4 MiB program region, and a data region of large read-only files the
filesystem service serves under `/data`.

## What changed

- **The image is 32 MiB** (65,536 sectors); the program region is
  sectors 2048–10239 (4 MiB, from 512 KiB), the asset region moves to
  10240–13311, and the **data region** is 13312–65535: a table sector
  like the program region's, filled by
  `scripts/install-program.py --region data`. The virtio disks of the
  other machines grow only when something is installed past their end.
- **`/data` in every root.** The filesystem service reads the data table
  once and serves its files as the root's `data` directory: `open`,
  `read`, `stat` and `readdir` work through the namespace, from a root
  at `/` or at `data` itself and never from below; every write, create,
  truncate, unlink or rename there is `EACCES`. The supervisor's sector
  relay permits reads of the data region and no writes, so the service
  could not alter it if it tried. Entries are numbered from `0x8000`,
  above anything `agelfs` holds.
- A fresh root lists `data/`; `:fs-ls /data` lists the files.

## Proof

`scripts/test-libc.sh` installs a 100,000-byte pattern in the data
region and `c-digest` reads it back through `/data/pattern` with the
SHA-256 the host computes, on all three machines; `:fs-mkdir data/new`
and a writer rooted at `/data` are refused with `EACCES`.
`scripts/test-files.sh` sees `data/` in every root listing. The full
regression passes.

## Not claimed

`agelfs` files are still 64 KiB in a 256 KiB region; the data region is
read-only and installed from the host, never written by the machine;
fifteen files at most, named in sixteen bytes.
