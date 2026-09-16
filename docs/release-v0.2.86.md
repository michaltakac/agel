# Agel v0.2.86 — The console driver under back-pressure

A fix. The v0.2.84 CI run failed twice in the DOOM suite with the engine's
last lines cut off mid-line and no exit report, while `PROCESS ENDED` —
which the supervisor writes on its own path — arrived; every local run
and the v0.2.85 CI run passed. That shape is the console driver domain
dying: its transmit loop polled the UART's transmitter without bound, so
when the machine running QEMU drained the serial socket slowly, one
entry spun past the driver's tick budget, the supervisor stopped the
driver as it stops any domain that never yields, and — a stopped domain
staying stopped — every line any process printed afterwards was lost.

## What changed

- **The driver's poll is bounded** (`TRANSMIT_POLLS`, 200,000 reads of
  the line-status register on x86-64, the flag register on AArch64, the
  16550's on RISC-V): a byte the transmitter will not take in that many
  polls is answered as not written, and the driver reports how many of
  the payload's bytes went out.
- **The supervisor sends the rest again** in the next entry, with a fresh
  budget, until the payload is out; four thousand entries in a row with
  nothing leaving — seconds, the host not draining at all — is a stall
  given up on, the rest dropped, rather than the supervisor waiting on
  the host forever. The driver is never stopped for a slow host.

## Proof

Not reproduced locally: on macOS the serial socket buffers absorb a
six-second reader stall during the timedemo on the old kernel and the
new, so the experiment written for it passes on both. The evidence is
the CI failure's shape twice on the same commit, the mechanism read from
the driver, and the fix removing the unbounded wait; this release's CI
run is the test. The full regression passes on both kernels; the
monitor's provoked-stop tests, which stop the driver on purpose, are
unchanged.

## Not claimed

A host that never drains still loses text, after the stall bound, and
the loss is silent on the console (it cannot be reported there); the
driver survives it. The bound is polls, not time, so its length varies
with the machine.
