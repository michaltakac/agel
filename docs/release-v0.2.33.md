# Agel v0.2.33 — A disk on every machine

Since v0.2.28 the interactive workshop has run on AArch64 and RISC-V, and
since then those machines have answered `:save` with "no storage device on
this machine". The recovery plane of v0.2.29, the boot budget, the health
oracle, the watchdog rollback, was an x86-64 claim. This release gives the
`virt` machines a disk, and the claim becomes three machines wide.

```sh
./scripts/run-qemu.sh riscv64
agel-native[0]> :save
workspace generation 1 committed: 1 cells; evaluator rebuilt from cells; previous slot retained
```

## What changed

- **A virtio block driver in a domain.** The supervisor scans the machine's
  virtio-mmio transports for a block device, maps that one page of registers
  and one DMA frame into a driver domain, and tells the driver the frame's
  physical address through the shared page. The driver negotiates the modern
  feature bit and flush, keeps one four-entry queue in its frame, and moves
  single sectors by polling with a bounded count. It reaches nothing else.
- **One disk path.** The workspace codec, the dual slots, the recovery record
  and the `:verify`/`:promote`/`:fault` commands are no longer x86-64 code
  with a fallback: they are the same code on all three machines, and the
  in-memory A/B policy model is now only what the self-tests exercise.
- **The persistence suite on three machines.** `test-native-persistence.sh
  aarch64` and `riscv64` run the whole cycle on a temporary virtio disk:
  edit, save, reboot, semantic rejection, corruption, torn write, health
  cell, promotion, three silent boots, automatic rollback, fault, revival.
- **The isolation suite too.** On every machine the storage driver reads the
  boot sector from an unprivileged domain, faults, is replaced, refuses its
  stale handle, and a world that was not granted the device faults on it.
- **A bigger stack.** The `virt` supervisors kept 64 KiB of stack; the
  workshop's bounded workspaces overflowed it. They have 512 KiB now, like
  x86-64.

## Verification

```sh
./scripts/test-native-persistence.sh aarch64
./scripts/test-native-persistence.sh riscv64
./scripts/test-native-repl.sh aarch64
./scripts/test-isolation.sh
```

## What this does not claim

The driver assumes QEMU's coherent memory and modern transports; the scripts
pass `-global virtio-mmio.force-legacy=false`. The AArch64 register grant is a
page holding eight transport slots, seven of them empty. Nothing on the
virtio disk is signed, and these machines' kernels are ELFs QEMU loads, with
no A/B slot selection.
