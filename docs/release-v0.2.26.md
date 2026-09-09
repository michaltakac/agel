# Agel v0.2.26 — The disk leaves the supervisor

Phase 3 of the native roadmap says every risky driver belongs in its own
restartable domain. Since v0.1.5 that was true of one driver, the console;
the disk was still port I/O executed in ring 0 by the supervisor. This release
moves it.

```sh
./scripts/test-isolation.sh x86_64
./scripts/test-native-persistence.sh
```

## What changed

- **A storage driver domain.** `agel_storage_main` runs in `.user_text` at
  ring 3 and drives the primary ATA controller with single-sector LBA28 PIO,
  the same bounded polling the supervisor used to do. It exchanges one sector
  at a time through a 512-byte block area of its shared page and reports a
  status code, never text.
- **Exactly the disk's ports.** The task-state-segment I/O bitmap is rewritten
  per entry: the console driver gets COM1's eight ports, the storage driver
  gets the eight ATA command-block ports and the alternate status port, and
  every other world gets none. A world that is not the driver executing the
  driver's status read takes a general-protection fault, and CI asserts it.
- **Generation-checked handles.** The service machinery built for the console
  now covers the disk: `read_sector`, `write_sector` and `flush` refuse a
  handle from before a restart with `stale-generation`. The isolation suite
  reads the boot sector from the driver, kills it, replaces it at generation
  two, refuses the old handle and reads the same sector again.
- **Policy stays where it was.** The dual-slot workspace codec, header
  validation, torn-slot fallback and generation publication are unchanged
  supervisor code; they now call a service instead of a port. Both the serial
  and graphical workshops create the driver at boot.

## Verification

```sh
./scripts/test-isolation.sh
./scripts/test-native-repl.sh
./scripts/test-native-persistence.sh
python3 scripts/test-native-workbench.py target/boot/agel-v1.img
```

## What this does not claim

Serial input and timers remain the supervisor's. The driver exists only on
x86-64 because only that backend has storage. A driver that faults in the
middle of a multi-sector save leaves recovery to the supervisor's slot
protocol, which already tolerates torn slots. The frame pool still never
reclaims a replaced domain's frames, so a driver restarted in a loop would
exhaust it.
