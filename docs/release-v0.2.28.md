# Agel v0.2.28 — The workshop on three machines

The AArch64 and RISC-V backends had answered the kernel contract and run the
evaluator corpus from their lowest privilege level since v0.1.6, but the
interactive workshop, the thing a person actually types into, was x86-64 only.
With serial input already flowing through the console driver domain, the
remaining difference was the disk. This release makes it optional.

```sh
./scripts/run-qemu.sh aarch64
./scripts/run-qemu.sh riscv64
```

## What changed

- **One workshop, three machines.** `isolated_repl` builds for every research
  backend. The evaluator runs in an EL0 or U-mode domain, prints through the
  console driver domain and reads through it, and the recovery monitor answers
  the same colon commands.
- **Storage is optional, not faked.** Only x86-64 creates a storage driver
  domain. Elsewhere `:save` and `:reload` answer "no storage device on this
  machine" and the named-cell editor keeps working in memory. The image codec
  and slot protocol are compiled only with the device.
- **One harness drives all three.** `scripts/test-native-repl.py --arch`
  boots the right machine, types every byte through the UART, checks every
  echo, prompt and revision through the same forty-step session, and expects
  each machine's own clean exit. CI runs it on all three.

## Verification

```sh
./scripts/test-native-repl.sh
./scripts/test-native-repl.sh aarch64
./scripts/test-native-repl.sh riscv64
./scripts/test-isolation.sh
```

## What this does not claim

The diskless machines have no persistence, no graphics and no keyboard. The
recovery monitor is still in-memory policy everywhere. Nothing about the
evaluator's bounds changed.
