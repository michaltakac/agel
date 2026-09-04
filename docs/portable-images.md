# Agel portable image format v1

Agel v0.7 persists *causes*, not host representation. An image is the ordered
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

## Try it

```sh
cargo run -q -p agel-image --example portable_image
```

The output shows the entry count, stable root, restored value, and encoded size.
Copying those bytes to another machine with the same Agel image-format/runtime
version reconstructs the same language-visible state with new local authority.
