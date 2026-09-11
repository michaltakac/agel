# Agel v0.2.42 — The disk layout grows so the kernel can

The POSIX personality's first stratum, in v0.2.41, brought the x86-64 serial
workshop image to within three kilobytes of its kernel slot. The next stratum,
a filesystem service and namespaces, did not fit. This release makes room, and
it is its own release because the layout is a compatibility boundary: an image
laid out before it is rebuilt, not read.

## What changed

- **Kernel slots hold 508 sectors.** The 512-byte BIOS stage loads a slot in
  four conservative 127-sector transfers instead of two, to physical
  `0x10000` as before, and the build rejects a kernel over 260,096 bytes
  instead of 130,048. The stage is still 512 bytes.
- **Everything after the slots moved.** Kernel slot B is sectors 512–1019, the
  workspace slots 1024–1055, the recovery record 1056, the kernel slot
  selector 1057, and the program region 2048–3071. Sectors 1536–2047 are
  reserved for the filesystem the next stratum adds. The image is 3,072
  sectors (1.5 MiB); the virtio disks of the AArch64 and RISC-V machines use
  the same layout from sector 1024 on. [`docs/native-boot.md`](native-boot.md)
  has the table.
- **Old images are rebuilt.** `./scripts/build-boot.sh` grows an image that is
  too small and installs the new seed; the kernel never guesses at the old
  layout, so a workspace saved under it is absent rather than misread.
  `scripts/stage-kernel.py` and `scripts/install-program.py` write the new
  sectors.
- **Fewer panic paths in the kernel image.** The SHA-512, curve and
  scalar-decoding code the kernel links from `agel-integrity`, and the
  contract model's object lookups from `agel-kernel-abi`, are written without
  indexing the compiler has to guard with a panic. A panic in the supervisor
  is a halt; the image also no longer carries the building machine's source
  paths, so its size does not depend on where it was built.

- **A build directory removed from the repository.** v0.2.41 committed
  `boot/posix/target`, the POSIX workspace's build output, by mistake; it is
  removed and ignored.

## Proof

Every suite that touches the disk runs against the new layout: the serial
workshop on all three machines, persistence and power-cut injection on all
three, kernel rollback with the BIOS stage charging a candidate's boots,
process loading on all three, the graphical workshop, and the isolation
self-test with its virtio disks. `cargo test` covers the two crates whose
code changed.

## Not claimed

The layout is not self-describing: nothing on the disk says which layout it
is, and the kernel does not detect an old one. The kernel slot budget is a
BIOS-stage limit, four transfers, not a measured property of firmware beyond
QEMU's; a machine whose firmware refuses 127-sector transfers is not a target
this release tests. The x86-64 workshop images still publish contract v1.0;
turning the memory group on in them now that they have room is a separate
step.
