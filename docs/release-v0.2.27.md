# Agel v0.2.27 — Input leaves the supervisor

After v0.2.26 the disk was the supervisor's last device. It was not: every
keystroke and every serial byte was still a port instruction executed in ring
0. This release moves those too.

```sh
./scripts/test-native-repl.sh
./scripts/test-live-keyboard.sh
python3 scripts/test-native-workbench.py target/boot/agel-v1.img
```

## What changed

- **The console driver reads as well as writes.** A nonblocking read command
  answers with a byte or nothing, on every architecture's UART. The serial
  workshop polls it for input and echoes through it, so the supervisor's
  last-resort console path is reserved for the recovery plane and panics.
- **An 8042 driver domain.** On the graphics build a domain granted only ports
  0x60 and 0x64 delivers raw keyboard and pointer bytes with their origin
  flag, and performs the bounded pointer-enable handshake on request. Scan-code
  decoding, modifier state, packet assembly and every policy about what a key
  means stay in the supervisor.
- **Refused by hardware.** A world that is not the driver executing the
  driver's status read on the keyboard controller takes a general-protection
  fault, and the isolation suite asserts it alongside the console and disk
  cases.
- **Generation-checked, like everything else.** Reads go through the same
  service handles as writes and sectors, so a restarted driver refuses its old
  handle.

## Verification

```sh
./scripts/test-isolation.sh x86_64
./scripts/test-native-repl.sh
./scripts/test-native-persistence.sh
./scripts/test-live-keyboard.sh
python3 scripts/test-native-dock.py target/boot/agel-v1.img
python3 scripts/test-native-workbench.py target/boot/agel-v1.img
```

The serial suite types every byte through the driver and checks each echo;
the graphical suites inject real QEMU key and mouse events through the input
domain and assert framebuffer changes.

## What this does not claim

Timers, networking and model brokering are still the supervisor's. The
legacy privileged REPL keeps its direct serial path by design. The input
driver exists only on the x86-64 graphics build; AArch64 and RISC-V have no
interactive workshop yet. The frame pool still never reclaims a replaced
domain's frames.
