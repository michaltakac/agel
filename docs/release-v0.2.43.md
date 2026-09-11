# Agel v0.2.43 — Files through namespaces: the second POSIX stratum

A process can now open, read, write and close files. It reaches them only
through the namespace the operator granted when it was started, served by a
filesystem that runs unprivileged and reaches the disk only by asking the
supervisor for sectors it owns. The requirement this release is measured
against is in [`docs/deployment-targets.md`](deployment-targets.md): a path
is not authority.

```text
agel-native[0]> :fs-format
formatted
agel-native[0]> :fs-mkdir app
directory ready: app
agel-native[0]> :fs-mkdir etc
directory ready: etc
agel-native[0]> :exec writer
writer: wrote etc/secret and app/notes
process writer exited with status 0
agel-native[0]> :exec reader /app
reader: notes: notes for the app
reader: etc/secret: error 2
reader: ../etc/secret: error 13
process reader exited with status 0
agel-native[0]> :fs-restart
filesystem restarted: generation 2
agel-native[0]> :exec writer /app ro
writer: open etc/secret: error 13
process writer exited with status 13
```

## What changed

- **A filesystem service world.** `agel_fs_main` is an unprivileged world
  with no device. It owns disk sectors 1536 through 2047 and reads or
  writes one by filling three words of its shared page and yielding; the
  supervisor relays the sector through the storage driver domain, refuses
  any sector outside the region, and resumes it. The on-disk shape is a
  superblock, four directory sectors of 32 entries, and one 8-sector extent
  per entry. It is written without a panic path.
- **A namespace per process.** `:exec NAME [ROOT] [ro]` grants the directory
  a process sees as `/` and whether it may write and create. The supervisor
  refuses a write or create the namespace lacks before the service is
  asked; the service resolves paths from that root and refuses `..` there.
  A file outside the namespace is `ENOENT` however the path is spelled.
- **Descriptors.** `open`, `read`, `write` and `close` join the process
  protocol. A descriptor is a supervisor record of the entry, an offset,
  its rights and the service generation it was opened under; `:fs-restart`
  replaces the service and makes every earlier descriptor `ESTALE`.
- **Workshop commands.** `:fs-format`, `:fs-mkdir PATH`, `:fs-ls [PATH]`
  and `:fs-restart`.
- **Programs.** `writer` and `reader` under `boot/posix`, and the process
  side of the protocol moved into a shared `agel-process-abi` crate with
  `open`, `read`, `close` and a small reporter.

## Proof

`scripts/test-files.sh [arch]` runs on x86-64, AArch64 and RISC-V: an
unformatted region refused, format, two directories, the writer at the root,
the reader rooted at `app` (its file readable, the sibling directory
unnameable, `..` refused), the service restarted and the reader run again,
the writer in a read-only namespace refused with `EACCES`, listings of the
root and a directory, an `:exec` with a missing root refused, and a reboot
after which the reader still finds its file. The reader also requires a read
past the end to answer 0 and a read on a closed descriptor to answer
`EBADF`. CI runs all three.

## Not claimed

`ESTALE` is implemented and not exercised: a process cannot outlive one
`:exec`, so no test holds a descriptor across `:fs-restart`. Files hold one
4,096-byte extent; there is no `unlink`, `rename`, `seek` or `stat`, no
timestamps, and no integrity beyond the superblock magic, so a damaged
directory sector is read as it is. The namespace is a root and three rights,
not a capability tree. The service is one world serving one request at a
time, and the descriptor table lives in the supervisor until a C library
moves what it can into the process. The graphics image carries none of this.
