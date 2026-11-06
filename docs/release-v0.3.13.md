# v0.3.13: Model-written repairs activate inside Agel OS

The general repair policy now runs inside a native Agel process and connects
real inference to the native activation gate from v0.3.12. Agel constructs
the prompt, parses a diagnosis and source AST, checks predicted outputs,
requests an independent text judgment, replays recorded and held-out cases,
compiles ELF canaries, installs them and selects the new program version.
The controller and kernel stay running. This path has no game-specific policy.

The [real run transcript](evidence/v0.3.13-native-repair.txt) records a resource
scheduler that rounded partial batches down. Codex proposed
`(fn (n size) (/ (+ n (- size 1)) size))`; Jev approved its text with score
900/1000. Metacircular replay checked eight cases, including predictions.
Six distinct argument tuples then produced the expected native output and
exit status. Version 2 became active. Metacircular evaluation scored the frozen source 3/6 and the
candidate 6/6 on the same cases; retained controller data stayed unchanged.
This is a small repair experiment, not an Exo performance result or a proof
that an agent can independently redesign the runtime.

## Running it

`sh scripts/test-native-repair.sh` runs deterministic, unpaid provider fixtures
through the same native transport and activation path. `--live` uses the
existing authenticated Codex CLI and `TYPESAFEAI_API_KEY` for Jev. Each run
uses a fresh private disk and writes its mode, serial transcript and extracted
OS ledger under `target/native-repair-{fixture|live}-TIMESTAMP/`.

The successful real run took 89.18 seconds from starting its VM/broker,
excluding builds, preparation and failed development runs. It used the
existing 64 MiB VM. The [subsequent fixture](evidence/v0.3.13-fixture-queued.txt) verifies the corrected queued
console transport; the published real transcript predates that transport fix.

## Agel policy and byte transport

`agel/repair` exposes a pure session API alongside its hosted actor. Both use
the same diagnosis, prediction, judgment, retry and replay policy. The native
example explicitly defers approved activation to a later exchange, so inference
and compilation do not retain their temporary allocations in one phase.
The activation hook remains opt-in; an ordinary evolution edit still has its
previous metadata-only behavior.

`agel/inference` builds provider-neutral envelopes and decodes replies.
The Rust broker forwards those bytes to existing providers. It shares the
acknowledged native journal adapter with the older hosted runner. Agel still
owns journal records, framing and published-rate arithmetic. The provider
adapters and native runtime remain Rust bootstrap infrastructure; removing
all their policy remains unfinished.

A fast fixture exposed a reply arriving before the process entered its
console read. The desktop now accepts generic `:stdin TEXT` input for the
running process regardless of reader readiness. It queues literal bytes,
refuses a full or closed input buffer, and preserves ordinary foreground
input and `:eof`. The queue is bounded at 4096 bytes; workshop commands retain
their 896-byte limit. The broker caps decoded replies at 1900 bytes so their
hex envelope fits. This is byte routing, independent of models and games.

Native compilation initially ran out of contiguous allocation space while
holding flat byte lists; the [fixture failure](evidence/v0.3.13-memory-failure.txt)
records an 81,920-byte failed allocation with cached pages still available.
Agel now decodes executable bytes in bounded chunks, feeds CRC incrementally,
and emits/installs/runs one canary at a time. The sector writer accepts those
bounded byte chunks. These memory and transport fixes were implemented during
repository development; the inner model did not discover or implement them.

## Judge accounting

Every real Jev HTTP attempt in this native path reserves guest capacity and
waits for its start record before dispatch and result record before delivery.
A failed storage acknowledgment stops delivery. The protected append-only
ledger is extracted and decoded after the VM stops, including failed runs.
Agel also appends session inference requests/results to protected `judge-log`;
unknown costs remain nil instead of zero.

The successful real run used one Jev call: 603 input tokens, 68 output tokens,
25,326 nanodollars ($0.000025326). A prior run paid for the same-sized judgment
before failing during native allocation. The [retained real records](evidence/v0.3.13-real-judge-records.agel)
therefore account for two real Jev calls totaling $0.000050652. Three real
Codex proposals occurred across development attempts; their dollar cost is
unknown. Fixture usage and priced amounts are synthetic and excluded.
The Jev rate is $0.042/M input tokens with free output, rechecked November 7
against the [published pricing](https://help.typesafe.ai/articles/5187202169-how-much-does-jev-cost).
All-in cost comparisons remain unavailable.

## Validation and limits

Focused repair, evolution and program-store tests pass, including audit order,
pure sessions, deferred activation and UTF-8 envelope boundaries. The native
fixture completes through real ELF execution and protected ledger decoding.
The prior native activation/revert/reboot and ledger recovery/reservation
regressions also pass. Provider/runner tests and clippy checks pass.

The native backend still supports an integer subset with fixed argument
images, not arbitrary dynamic arguments or the complete Agel runtime. A new
child uses the selected version; this does not migrate an already-running
service domain. Program table slots remain limited, and orphaned staged
images remain available after refusals. Canonical active pointers and history
are not automatically recovered after reboot. Domain/kernel-slot replacement,
recovery guardians, general runtime self-extension, native game transport and
the Exo task/cost/time comparisons remain open. No superiority claim follows
from this release.
