# v0.3.62 · Retain signed kernel versions across slot reuse

Kernel staging now retains the exact kernel and signature in a
content-addressed history beside the disk image. Use
`stage-kernel.py IMAGE --restore VERSION` to restage an archived pair.
The existing signature check, healthy boot, and promotion still govern
whether it becomes trusted. A corrupted archive is refused before the
slot selector changes.

The archive belongs to the host staging tool. Kernel change proposals
and semantic replay approval in Agel remain open.

The QEMU evaluation covered archive corruption, normal admission after
restore, and watchdog rollback. [Native trace](evidence/v0.3.62-kernel-history.txt)
