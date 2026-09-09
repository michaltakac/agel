# Agel portable image format v1

Agel v0.0.7 persists *causes*, not host representation. An image is the ordered
sequence of successful inputs that created a world:

```text
grant → evaluate → claim-model → complete-model
```

Failed transactions never enter the image. Model completion stores the exact
success or structured failure, so reconstruction never invokes a provider.
Effect keys are regenerated for the fresh world and matched locally.

## Canonical binary envelope

All integers are unsigned big-endian. Byte strings have a `u64` length prefix.
Strings are UTF-8. The envelope contains:

```text
"AGELIMG\\0" | format:u16 | history-limit:u64 | budget:7*u64
entry-count:u64 | (entry-bytes | entry-digest:32)* | root:32
```

The initial root is SHA-256 over the format version, history limit, and resource
budget under the `agel/image-chain/v1` domain. Each entry root hashes the prior
root plus the canonical entry bytes. The decoder caps the total image at 64 MiB,
each field at 16 MiB, and the entry count at one million before allocating.

This format is deliberately independent of Rust enum layout and debug output.
Unknown format or entry tags fail closed. v1 has no implicit migrations.

## Reconstruction and authority

`Image::rebuild` starts an empty world and applies entries in order. A grant is
reissued by the new world exactly where it originally appeared. Consequently,
old capability handles cannot authorize actions after restart. The image root
remains the same because it commits to semantic inputs, not ephemeral world IDs.

Use `ImageSession` to ensure only successful state transitions are appended. It
does not expose mutable access to its `World`; this prevents unrecorded commits.

## Crash-safe store

`ImageStore::save` requires the previously observed root. It writes `NAME.new`,
syncs it, rotates the current file to `NAME.previous`, renames the new image, and
syncs the containing directory on Unix. `load` validates the primary and falls
back to the previous image if the primary is absent, torn, or corrupt.

The sequence protects one local writer from process or machine interruption.
The root check detects a stale caller but is not a cross-process lock; deployments
with concurrent writers must serialize commits above this API.

## Signed images (v0.2.24)

`Image::encode_signed` wraps the unchanged v1 image in an envelope:
`"AGELSIG\0"`, a `u16` envelope version, the signer's 32-byte Ed25519 public
key, and a 64-byte signature over `"agel/image-root/v1\0" || root`. The v1
format itself is untouched, so the image bytes inside are exactly what an
unsigned store would write. `ImageStore::save_signed` uses the same
temporary-file, previous-sidecar and directory-sync sequence as `save`, so a
signed generation is atomic and the previous generation remains recoverable.
The optimistic root check accepts a current file in either form, which is how
a store is upgraded to signed in place.

`ImageStore::load_verified(&trusted)` accepts only an envelope whose signer is
exactly the trusted key and whose signature verifies against the root inside.
A torn, corrupt, unsigned, foreign-signed or mis-signed primary falls back to
the previous generation under the same rule; if no trusted generation exists
the result is an error, never a silent `None`. The unsigned `load` refuses a
signed primary outright rather than falling back to an older unsigned
generation, so a reader that has not been told which key to trust cannot be
served stale state by a store that has since been signed.

Ed25519 and SHA-512 are implemented in `agel-integrity` with no dependencies
and checked against the RFC 8032 vectors; verification uses the strict
equation and rejects `s >= L`. The arithmetic is not constant-time, so signing
keys belong on the operator's machine, not on a host an adversary can time.
Signatures authenticate the root against a key; they do not encrypt anything
and do not prevent an adversary who holds the seed from signing whatever they
like.

## The CLI as an image writer (v0.2.22)

`cargo run -p agel-cli -- --image PATH` runs the ordinary REPL over an
`ImageSession`. If the file exists it is loaded (falling back to the
`.previous` sidecar), rebuilt by replay, and its root printed; otherwise a new
image is started and the standard library's source becomes its first committed
input. Every successful transaction, provider grant, model claim and model
completion is appended and the file is atomically replaced; a failed save is
reported and retried against the same expected root on the next commit, so a
concurrent writer is detected rather than overwritten. `:image` shows the path,
entry count and root.

With `--signing-key FILE` (a hex seed written by `--keygen FILE`, readable
only by its owner) every save is a signed envelope and every load is verified
against that key's public half; `--trust-key FILE` may name the public key
explicitly and must match the signing key. `:image` reports the signer.

`:rollback` and `:restore` are refused in image mode. An image is an
append-only log of committed inputs; rewinding the live world without rewinding
the log would leave a file that no longer reconstructs the world it claims to.
`:snapshot` remains available for inspection. Provider grants are not
duplicated on restart: a reconstructed image already replayed them.

## Try it

```sh
cargo run -q -p agel-image --example portable_image
cargo run -q -p agel-cli -- --image target/agel-world.image
```

The output shows the entry count, stable root, restored value, and encoded size.
Copying those bytes to another machine with the same Agel image-format/runtime
version reconstructs the same language-visible state with new local authority.
