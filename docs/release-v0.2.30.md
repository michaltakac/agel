# Agel v0.2.30 — The kernel image is A/B too

v0.2.29 made workspace generations recoverable: a candidate that failed three
boots was rolled back by the kernel. That left the one thing the kernel cannot
roll back, itself. This release puts the same policy one level down, in the
512-byte BIOS stage, where a kernel that never comes up is still somebody's
problem.

```sh
./scripts/stage-kernel.py target/boot/agel-v1.img candidate-kernel.bin
./scripts/run-qemu.sh
kernel: running slot B; trusted slot A; candidate slot B (unverified, boots 1)
agel-native[0]> (+ 1 1)
2
kernel slot B verified by a healthy boot
agel-native[1]> :kernel-promote
selected kernel slot B; slot A retained for rollback
```

## What changed

- **Two kernel slots and a selector.** Sector 289 holds nine bytes the BIOS
  stage can parse in real mode: trusted slot, candidate slot, boot attempts,
  verified flag. Slot A is the kernel `build-boot.sh` writes; slot B is
  sectors 290-543.
- **Boots charged before the candidate runs.** The stage counts a boot of an
  unverified candidate and writes the selector back to disk before jumping to
  it. A candidate that halts at its first instruction is charged all the
  same, and after three such boots the stage loads the trusted slot.
- **The kernel judges itself.** Its first successful evaluation after boot
  verifies the slot it booted from, and only that slot. `:kernel-status`,
  `:kernel-promote` and `:kernel-fault` are the operator's decisions; the
  desktop shows the same state with `:kernel`.
- **Staging is a host tool.** `scripts/stage-kernel.py` writes a kernel into
  the slot that is not trusted and proposes it with a fresh budget. A rebuild
  clears the selector, so what you built is what boots.

## Verification

```sh
./scripts/test-kernel-rollback.sh
./scripts/test-boot.sh
./scripts/test-native-persistence.sh
```

The rollback suite stages the same kernel as a candidate and promotes it after
a healthy boot, then stages a three-byte kernel that halts at its entry point,
boots it three times to a silent console while checking the selector's count
after each, and on the fourth boot reads the kernel's own report that the
stage fell back.

## What this does not claim

Nothing on the disk is signed: the selector and both slots are trusted as
written. Slot B exists only on x86-64, which is the only machine with a BIOS
stage. The stage does not verify what it loads, and a kernel that reaches the
console and then misbehaves is judged only by whether it evaluates a form.
