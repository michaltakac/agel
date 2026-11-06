# Agel state checkpoints

`agel/data-codec` encodes inert values as tagged ASCII data. Nil, booleans,
signed integers, UTF-8 strings, arbitrary symbol names, lists and maps round-trip.
An empty list remains distinct from nil, and empty maps remain empty. Functions,
capabilities, agents and other runtime handles are refused. Decoding uses the
closed Agel reader and validates tags, arity and duplicate map keys. It never
evaluates the decoded text. The limit is 65,536 encoded bytes and reader depth
128 per payload. Normal evaluator fuel, collection and call-depth limits
also apply.

`data-pack` takes a value and an atom renderer. The native renderer is the
existing `print-form`; policy, tagging, UTF-8 hex encoding and reconstruction
are Agel code. `data-read` takes encoded text. These are optional modules;
load `bytes.agel`, `data-codec.agel` and `checkpoint.agel` in that order in a
world with the standard library installed.

`agel/checkpoint` provides:

- `checkpoint-new(seed)` creates revision 0 with digest 0.
- `checkpoint-write(store, state, render, validate, append-record)` validates
  and round-trips the candidate, encodes revision/predecessor/state, computes
  CRC-32 and appends one complete record. It returns a new store only after
  the writer acknowledges the exact byte count. Callers select its state
  for subsequent dispatch after this return.
- `checkpoint-recover(text, seed, validate)` scans complete lines and selects
  the latest valid successor. It returns revision, digest, state and an issues
  count. Wrong checksums, malformed payloads, invalid states, duplicate/stale
  revisions, missing predecessors and unterminated checkpoint tails do not
  advance the recovered state. Later valid successors can continue from the
  last valid predecessor; damaged bytes remain in the append-only history.

A record is a newline, `AGEL-STATE/1 `, decimal CRC-32, a space, tagged payload
and a final newline. The payload contains the revision, prior accepted CRC
and candidate state. Its ASCII encoding cannot inject physical newlines.
The leading newline separates a new append from an earlier torn tail.
Other log lines are ignored, allowing these records to share protected
`judge-log` with existing audit records.

`evolution-valid?` checks the version-store structure, contiguous retained
version IDs, source/after agreement, base-source relationships, ordered commit
history and active object ownership. It is structural validation, not an
independent attestation that a judge or replay actually ran. The example also
replays the recovered active language form against its current behavioral
contract before using it.

A fully persisted record is the recovery commit point. If its acknowledgment
is lost, recovery can select that record even if the old controller never
received the return value. A short or interrupted record remains evidence
and cannot select its incomplete candidate. A normal write acknowledgment
and reboot are tested; arbitrary power-loss durability still depends on the
underlying OS storage protocol and device guarantees.

CRC-32 and predecessor links detect accidental corruption and stale records;
they are not cryptographic authentication. A writer with append authority can
forge a structurally valid checkpoint. Guardian-only writes, signatures and
concurrent-writer arbitration remain unfinished. `checkpoint-write` does not
reread the log to claim an exclusive revision, so recovery refuses a conflicting
successor rather than merging it.

The current native example reads `judge-log` with the existing 64 KiB read
bound. This test is for bounded histories, not indefinite streaming recovery.
Supplying truncated input can hide later records; callers must supply the
complete history. Automatic boot/service discovery, compaction, and recovery
of executable artifacts, live domain state or kernel slots remain separate
work. Recovering a source store does not migrate a running service or preserve
an in-memory closure's identity through reboot.

Run `sh scripts/test-checkpoint.sh` for hosted corruption/codec checks and a
native three-boot test. Its first process commits versions 1 and 2 and leaves
a torn successor. After reboot, another Agel process restores version 2,
refuses an unsafe revert, and commits version 3 past the damaged evidence.
After another reboot, version 3 is restored and a guarded revert to retained
version 2 commits as version 4. All judgment fixtures have zero costs and are
logged on the OS; this test invokes no model provider.
