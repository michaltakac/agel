# v0.3.15: Recover Agel source versions after reboot

Agel can now encode a version store as inert data, append a linked checkpoint,
and recover the latest complete accepted successor after a restart. The
native test restores a language form and its retained history across two OS
reboots, continues past deliberately torn evidence, and persists a guarded
revert. This is a bounded source-store recovery API; boot-time service
discovery, executable/domain/kernel recovery and guardian authority remain open.

`agel/data-codec` owns tagged serialization and strict inert decoding.
`agel/checkpoint` owns revision/predecessor links, checksums, record scanning,
validation and the publication rule. `evolution-valid?` checks retained IDs,
base/source relationships, commit order and active object ownership. All are
Agel code. The existing native formatter and file byte transport are reused;
this release adds no Rust production code.

A write validates and round-trips the candidate before appending, and returns
a new store only after an exact byte-count acknowledgment. Recovery never
evaluates stored text. A bad checksum, stale revision, missing predecessor,
invalid store or incomplete record cannot advance the recovered pointer.
The previous complete state remains available; a later valid successor can
link to it while retaining the damaged bytes. A fully persisted record is
the recovery commit point, including when its acknowledgment was lost.
See [the API and protocol limits](checkpoints.md).

## Native evidence

Run `sh scripts/test-checkpoint.sh`. The [native transcript](evidence/v0.3.15-checkpoints.txt)
records three Agel processes on the same disk across three boots:

1. The first process commits source versions 1 and 2 through judgment/replay
   and checkpoint acknowledgment, then appends a deliberately incomplete
   successor record.
2. After reboot, recovery selects version 2 and reports the damaged tail.
   The recovered form skips a division-by-zero branch and evaluates correctly
   in a lexical environment. Revert to eager version 1 fails runtime replay.
   An equivalent version 3 passes the gate, is checkpointed, and recovers
   correctly past the retained damaged record.
3. After another reboot, version 3 is recovered. Guarded revert to retained
   version 2 creates and checkpoints version 4. Reading the log back restores
   version 4; all earlier source versions and gate history remain available.

Each boot also refuses overwrite of protected `judge-log`. Five local
fixture judgments have zero-cost receipts appended before/after invocation.
No model provider is invoked; this release does not add paid inference.
The older model-driven repair evidence remains in v0.3.13 and v0.3.14.
The test deliberately appends a truncated record and then performs normal
reboots; it does not simulate every power-cut point in the filesystem/device.

## Bugs and deterministic cost

The round-trip check exposed an Agel sequence bug: `keys` of an empty map
returns an empty list distinct from nil. Sequence functions previously
invoked their callback once for that list, introducing a spurious nil entry
during map serialization. They now stop at either empty representation.
The codec preserves empty lists, nil and empty maps as distinct data values.
Tests cover this, full version-store round-trips, Unicode, minimum signed
integers, arbitrary symbol names, duplicate map keys and rejected handles.

Native recovery initially [exhausted the unchanged 50-million-step process
budget](evidence/v0.3.15-fuel-failures.txt). CRC-32 now uses a byte table and a 24-bit XOR for the shifted
accumulator; text hashing reads UTF-8 bytes directly instead of converting
through hex and byte chunks. It still uses the reflected IEEE polynomial
0xedb88320. Tests compare the full 0–255 byte stream and every single-byte
input against an independent bitwise reference, plus standard and UTF-8
vectors. Native program installation/revert/reboot is regression-tested
because executable metadata uses the same checksum implementation.

For the same first native phase, deterministic work fell from 17,127,343 to
5,513,023 steps. Later phases use 27,201,254 and 49,705,977 steps. The last
phase has little headroom. These are measured evaluator steps for this
fixture, not model token costs, Exo task results or a wall-time speed claim.
The VM remains 64 MiB; neither its memory nor process fuel limit was raised.
The sequence and CRC fixes were made during repository development, not
discovered or implemented by the inner repair agent.

Hosted codec, corruption, schema and budget tests pass, as do the standard
library suite, independent CRC references, native checkpoint and native
program activation/revert/reboot tests. Clippy and formatting checks pass.

## Remaining scope

The example reads the existing protected file through its 64 KiB native read
bound. Callers must provide complete history; streaming/indexed recovery and
compaction remain unfinished. Checkpoints have a 65,536-byte encoded payload
bound. CRC-32 and predecessor links detect corruption and stale records,
not malicious forgery. Structural validation does not attest judge execution.
Concurrent-writer arbitration and guardian-only append authority are open.

Recovery is explicitly invoked by the startup script. It restores a source
store and active form pointer, then runs current behavioral canaries; it does
not automatically discover services at boot or preserve a live heap's identity
through reboot. Durable native artifact activation, domain state migration,
kernel-slot replacement, full Agel self-hosting and Exo cost/time/correctness
comparisons remain unfinished. The mechanisms are application-neutral.
