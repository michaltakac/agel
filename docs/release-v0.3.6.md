# v0.3.6 — Recoverable judge records with reserved result space

Native judge calls now use `/judge-ledger`, a protected 64 MiB append store,
instead of filling the ordinary filesystem's 64 KiB `judge-log`. Legacy logs
remain intact and readable. The new file lives in a separate disk region;
ordinary files, cells, programs and kernel slots keep their existing offsets.
Boot-image builds and resumed bridge disks grow to at least 128 MiB without
truncating or clearing their old contents.

## Agel policy, protected byte storage

`agel/journal-frame` writes and reads fixed 512-byte ASCII frames. It handles
payload encoding, checksums, sequence numbers, completion markers, UTF-8
reconstruction and recovery of complete records after a damaged or unfinished
record. Its incremental reader lets an OS process read one sector at a time.
The `journal` example is a hosted byte reader around that same Agel policy:

```sh
cargo run -q -p agel-stdlib --example journal -- FRAMED_FILE
```

The Rust bootstrap stores one sector at the next unused address and flushes
it before updating either of two checksummed position records. It never
rewrites a data sector. Recovery scans past occupied tail sectors, retaining
their bytes even if their record is incomplete. If neither position record
is valid, the occupied-sector prefix supplies the position. All-zero data
sectors are prohibited so they can identify unused space. Corrupt frames
remain observable in the raw file and in the reader's corruption count;
they are not silently treated as complete records.

Before each HTTP attempt, Agel calculates a conservative request/result
space bound from the request size and the provider's output limit. The
bootstrap grants a reservation only when that much space remains. Ordinary
appenders cannot consume it, and the provider releases it after recording
the outcome. A durable generation distinguishes a new token from a released
or pre-restart token. Overlapping reservations are refused. This bridge
serializes calls per journal; separate simultaneous experiments need their
own journals/disks.

Record construction, prices and framing remain Agel code. The new Rust
code is storage, byte transport, reservation enforcement and UTF-8 byte
construction. It does not choose game actions or propose self-edits.

## Interface and compatibility

The process ABI adds reserve, release and sector-append operations 23–25.
They require a writable root namespace; read-only processes cannot reserve
space. Positional writes, truncation, unlink and rename of `judge-ledger`
are rejected. The disk format and process calls for ordinary files are
unchanged. Native and POSIX judge transports carry the same framed bytes.

POSIX Agel `file-read` now accepts an optional byte offset and byte limit,
so a process can stream the ledger without loading it all. The existing
one-argument call retains its 64 KiB read bound. The hosted seed adds the
pure `text-from-bytes` UTF-8 constructor; invalid bytes are rejected. This
changes the pinned fresh-world digest because the seed gains a binding,
not because the canonical encoding changed. Existing world deltas still
require their original library/host base; no incompatible delta is guessed
or silently migrated. The fixed native evaluator does not gain this word;
the full Agel process and hosted runtime do.

The old `test-judge-bridge` commands now run the framed-ledger suite.

## Validation

The [validation record](evidence/v0.3.6-validation.txt) lists the completed
checks and the earlier compositor startup timeout.

- Workspace tests and Clippy; kernel/POSIX formatting and Clippy.
- Agel frame tests cover every torn-sector prefix, UTF-8 round trips and
  incomplete-record recovery. Metadata tests cover every torn-head prefix.
- `sh scripts/test-ledger.sh` runs native judge episodes beyond the old
  64 KiB limit, checks prefix preservation across resume, exercises interrupted
  data/head writes and first-reservation recovery, and decodes results with
  Agel. It also covers the POSIX judge path.
- Near-full fixtures prove that insufficient reservation prevents the HTTP
  call. The syscall probe verifies that ordinary writers cannot consume
  reserved result space, released tokens fail, new tokens differ, a full
  store refuses reservations and a read-only namespace cannot reserve.
- The legacy protected-log and ordinary filesystem regressions still run.

The native interruption fixtures model sector-prefix tears and interrupted
flush/metadata ordering. They are not a physical-device power-loss test.
Metadata and frame checksums detect accidental damage; they are not signatures
against a malicious raw-disk owner.

## Remaining work

Reservations prevent exhaustion of this store's logical capacity between
request and result under the tested transport bounds. They cannot prevent
device failure or a crash while a paid call is in flight. A start without a
result remains evidence of uncertain cost, requiring reconciliation before
it can support a complete cost comparison. Hosted CLI journals remain host
files rather than this native store.

Root writers can still append forged data. Guardian-only append authority,
durable whole-program activation, actual OS service version/revert handling
and paired game experiments are unfinished. DOOM has not yet been replaced.
OpenRune, the more complete RSMod-derived server candidate, installed
successfully with `--no-configuration-cache`; it still needs live mechanics
verification and an Agel client.
