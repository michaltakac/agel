# Agel v0.2.32 — Candidates that must be signed

v0.2.30 let the BIOS stage try a second kernel slot and fall back after three
bad boots. It also meant that anyone who could write the disk could stage a
kernel the machine would try. This release makes admission the running
kernel's decision, and the decision is a signature check.

```text
candidate kernel slot B admitted: signature verified against the kernel's trust key; next boot tries it
kernel: running slot A; trusted slot A; candidate slot B (unverified, boots 0)
```

## What changed

- **An admitted flag the stage requires.** The selector is version 2: a
  candidate is loaded only when the running kernel has set its admitted flag.
  Until then the stage boots the trusted slot as if nothing were staged.
- **A signature over the slot.** `scripts/stage-kernel.py` signs the
  candidate's SHA-512 with `bootstrap/kernel-signing.key` and records the
  signed length and the Ed25519 signature in the selector. On every boot the
  kernel hashes a staged candidate sector by sector through the storage
  driver domain and verifies it against `bootstrap/kernel-signing.pub`, baked
  in at build time. A valid signature admits; anything else clears the slot.
- **The verifier in the kernel.** `agel-integrity` now builds `no_std` with a
  streaming SHA-512, so the same dependency-free, RFC-vector-tested Ed25519
  code that signs portable images on the host verifies kernels on bare metal.
  The graphics image grows to within three kilobytes of its budget.
- **A development key, named as one.** The checked-in key pair demonstrates
  the mechanism; `kernel-sign keygen` makes a real one, and the kernel is
  rebuilt with its public half.

## Verification

```sh
./scripts/test-kernel-rollback.sh
cargo test -p agel-integrity
cargo build -p agel-integrity --no-default-features --target x86_64-unknown-none
```

The rollback suite now stages an unsigned candidate, a candidate with a valid
signature over the wrong bytes, and signed candidates good and hung, and reads
the kernel's refusal or admission of each before the boot budget scenario runs.

## What this does not claim

The trusted slot and the selector's own bytes are unsigned, and nothing checks
the kernel before the BIOS stage runs it: there is no root of trust before the
stage. Workspace generations and the recovery record are unsigned. The
checked-in key is public knowledge.
