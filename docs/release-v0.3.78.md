# v0.3.78 · Reverify the signed kernel slot before promotion

After the admitted Agel trial passes, the x86 workshop reads the candidate
slot again and verifies its signature before persisting the trusted slot
selection. If the read or verification fails, it exhausts the candidate's
boot budget and leaves the existing trusted slot selected.

The QEMU test changes a signed slot byte after the candidate boots. The
in-memory candidate reaches the interactive prompt, but promotion refuses
the changed disk image. The next boot returns to the trusted slot.

This check protects promotion. The BIOS continues to rely on the existing
admission record when selecting a candidate for its trial boot.
