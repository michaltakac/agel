# Agel v0.2.34 — The power fails at every write

The persistence suite had modelled a torn write once, by hand, at the place
its author thought of. This release lets the machine tear any write it is
told to and stop, and sweeps that over every write of a save on every
machine.

```text
agel-native[2]> :cut-power 7
power cut armed: sector write 7 will be torn and the machine halted
agel-native[2]> :save
power cut injected: sector 262 torn; halting
```

## What changed

- **`:cut-power N`.** The storage service in the supervisor counts sector
  writes from the moment it is armed; the N-th is torn, its first half
  reaching the disk and its second half keeping the old bytes, and the
  machine halts before anything else. It is a serial-workshop command; no
  world can reach it and the graphics build does not compile it.
- **A sweep, not a sample.** `scripts/test-power-cut.sh [aarch64|riscv64]`
  seeds a generation, then for N from 1 upward boots, checks that the
  workspace is a whole generation whose cell and generation number agree,
  arms the cut, edits, saves, and reboots after the halt. It stops at the
  first save the cut never reaches and requires at least eighteen cuts, the
  number of writes in a save. CI runs it on all three machines.
- **A finding.** The recovery record is written after the generation is
  published and reported. A cut on that write leaves a whole new generation
  and a record that fails its checksum, which reads as empty: nothing
  trusted, newest generation booted. The safe direction, now documented.

## Also in this release

The v0.2.33 CI run found the virtio driver's wait too short on the shared
runner: a flush on a slow host disk took longer than the storage domain's
half-second tick budget, and the save reported a timeout. The storage domain
now has a three-second budget on every machine and the driver's poll bound is
sized inside it; a device that never answers still costs one request and is
reported, not the driver.

## Verification

```sh
./scripts/test-power-cut.sh
./scripts/test-power-cut.sh aarch64
./scripts/test-power-cut.sh riscv64
```

## What this does not claim

The cut is a tear and an immediate stop; a drive that acknowledges a write
and loses it, or lies about a flush, is not modelled, because QEMU writes
through to the image. Kernel staging and the selector are host-side writes
outside the sweep.
