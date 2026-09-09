# Agel v0.2.24 — Signed roots and evidence

Until now every trust boundary in Agel was hash-checked but unsigned: anyone
who could rewrite an image file could recompute its chain, and promotion
evidence was a value the supervisor compared, not one it could attribute.
This release adds the first signatures, without adding a dependency.

```sh
cargo run -q -p agel-cli -- --keygen keys/agel.hex
cargo run -q -p agel-cli -- --image target/agel-world.image --signing-key keys/agel.hex
```

## What changed

- **Ed25519 and SHA-512 in `agel-integrity`.** Written against RFC 8032 with
  no third-party code and no `unsafe`, checked against the RFC test vectors,
  with strict verification that rejects `s >= L`. Field and scalar arithmetic
  are not constant-time; signing keys belong where nobody can time them.
- **Signed image envelopes.** `Image::encode_signed` wraps the unchanged v1
  image with the signer's public key and a signature over the root under a
  domain tag. `ImageStore::save_signed` reuses the atomic temporary-file and
  previous-sidecar sequence; `load_verified` accepts only the trusted signer,
  falls back to a trusted previous generation, and reports an error rather
  than `None` when nothing trustworthy exists. The unsigned loader refuses a
  signed primary instead of downgrading to an older unsigned generation.
- **Signed promotion evidence.** `PromotionEvidence::sign` produces evidence
  bound to a key over canonical, domain-separated bytes. A supervisor built
  with `trust(key)` refuses unsigned promotion and verifies `promote_signed`
  against exactly that key; a candidate image cannot change the policy.
- **Operator keys in the CLI.** `--keygen FILE` writes a fresh seed from the
  operating system's random source with mode 0600 and prints the public key;
  `--signing-key FILE` signs every commit and verifies every load;
  `--trust-key FILE` must match it. `:image` reports the signer.

## Verification

```sh
cargo test -p agel-integrity -p agel-image -p agel-supervisor -p agel-cli
cargo run -q -p agel-supervisor --example ab_upgrade
```

Tests cover the RFC 8032 vectors, SHA-512 vectors across a block boundary,
tampered messages and signatures, the malleable `s + L` twin, non-canonical
keys, signed store upgrade in place, foreign-signer and tamper fallback,
unsigned-reader refusal, forged root signatures, trusted-supervisor refusals of
unsigned, foreign, tampered and swapped evidence, stale signed evidence, and
the CLI's key generation, verified restart and mismatched-key refusals.

## What this does not claim

Native workspace disk slots, the seL4 release manifest and kernel images are
still unsigned. Signatures authenticate a root to a key; they do not encrypt,
and a stolen seed signs anything. Key distribution and rotation are the
operator's problem: there is one trusted key per store or supervisor and no
revocation mechanism yet.
