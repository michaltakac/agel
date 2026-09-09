# Agel v0.2.29 — Recovery that survives a reboot

Since v0.1.2 the serial workshop has answered `:verify`, `:promote` and
`:fault`, and the isolation suite has proved the A/B policy on three machines.
The state behind those commands was two booleans in memory: every boot started
trusting slot A again, and nothing a reboot did could ever roll back. On the
one machine with a disk, that is now fixed.

```sh
./scripts/run-qemu.sh
agel-native[1]> :recovery-status
recovery: trusted generation 1; candidate generation 2 (unverified, boots 1)
```

## What changed

- **A record on disk.** Sector 288 holds the trusted generation, the
  candidate generation, its boot count and whether it is verified, CRC-checked
  and read as empty when absent or damaged. It is supervisor policy carried
  by the storage driver domain; no language world can reach it.
- **Boots that count.** Every boot of an unverified candidate is charged and
  flushed before its first cell is replayed. The first successful evaluation
  after boot verifies it. A candidate that fails three boots is not booted a
  fourth time: the supervisor prints the watchdog fault and replays the
  trusted generation, whatever the candidate did or failed to do.
- **A health oracle in a world.** `:verify` evaluates a cell named `health` in
  an isolated candidate world that is discarded afterwards; only a clean
  evaluation admits the candidate.
- **Promotion that names what it keeps.** `:promote` is denied until the
  candidate is verified, makes it the trusted generation, and reports which
  earlier generation is retained. `:save` chooses the slot that does not hold
  the trusted generation. `:fault` rolls back now and exhausts the candidate's
  budget so later boots keep the trusted generation until an operator
  intervenes.
- **The graphical workshop reads it.** `:recovery` shows the same record on
  the desktop, and the desktop boots by the same plan.

## Verification

```sh
./scripts/test-native-persistence.sh
./scripts/test-native-repl.sh
./scripts/test-graphical-workshop.sh
./scripts/test-isolation.sh
```

The persistence suite drives the whole cycle on a temporary disk: a failing
then passing `health` cell, promotion, a new candidate, three boots that exit
before evaluating anything, the automatic rollback on the fourth, an explicit
fault, revival by `:verify`, and promotion with the previous generation
retained.

## What this does not claim

The record is unsigned and the disk is trusted to return what was written.
One record and two slots retain exactly one earlier generation. The kernel
image is not A/B selected; only workspace generations are. The diskless
machines keep the in-memory policy model.
