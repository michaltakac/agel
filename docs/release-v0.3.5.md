# v0.3.5 — Judge accounting reaches the OS

Every Jev provider HTTP attempt now runs through `agel/judge-accounting`.
Agel constructs the request and result records and calculates cost. The
provider transports those records to a journal before dispatch and after
response, including HTTP retries and transport failures. No authentication
header is included. Records retain the request, raw response, operation ID,
attempt number, wall-clock timestamps, usage, rate source and cost status.

For the native bridge, the journal is the guest's protected `judge-log` file.
The host sends bounded hex byte chunks and waits for a disk acknowledgement
from the OS before continuing. This works in native program requests, the
desktop's cell-admission questions and POSIX Agel process requests. A failed
start write prevents the HTTP call. A failed result write prevents response
delivery and stops the bridge; it does not silently fall back to an action.

Hosted CLI judgments, its judgment gate and offline dataset judgments use
the same provider path, appending and syncing `WORKSPACE/judge-log` on the
host. That host file has no special native-filesystem protection. A custom
trusted `JudgeJournal` can supply another byte sink; choosing a sink is a
bootstrap responsibility, not agent policy.

## Cost and evidence

The Agel policy applies the published TypeSafe rate to the official endpoint
and `jev-latest`: USD 0.042 per million input tokens, output free, with no
failed-request fee. The
[source](https://help.typesafe.ai/articles/5187202169-how-much-does-jev-cost)
was checked on November 6, 2026. Custom endpoints/models, missing or invalid
usage, transport uncertainty and overflow-sized values retain unknown cost.
Costs are exact integer nanodollars at this rate. The microdollar display is
rounded upward per call; experiments must sum nanodollars, not those rounded
display values. These are published-rate costs, not invoice reconciliation.

One live call selected `done` for an already-complete desktop task, using
Jev 1.14.0: 521 input tokens and 105 output tokens, or 21,882 nanodollars
(USD 0.000021882). Both records were read from the protected guest file
after reboot and are preserved verbatim in
[`evidence/v0.3.5-live-judge.agel`](evidence/v0.3.5-live-judge.agel).
This is a transport/accounting check, not an agent improvement result.

## Preservation and bootstrap fixes

`agel-play` now refuses to overwrite an existing run disk. `--resume` boots
that disk without formatting its filesystem and appends to its existing
history. Use a new output directory for a fresh account/environment. This
does not yet restore an agent's unsaved evaluator state.

Reboot readback exposed a pre-existing mismatch: the native effect buffer
claimed 4096 bytes while its shared observation area maps 2048. A sufficiently
large file read faulted the evaluator. Effects now fit 2048 bytes, with a
compile-time check against the mapping; write content is bounded to 1792
bytes plus the existing 256-byte path allowance. Larger native writes must
be split. This is a deliberate reduction from an unsafe advertised limit,
not a disk-format or process-ABI change.

The bridge also returns failure for failed guest steps and nonzero Agel
process exit status; previously these could terminate with a success message.

## Validation

- Workspace tests, formatting and Clippy; kernel and POSIX Agel Clippy.
- Agel accounting tests cover fractional costs, rounding, failed requests,
  missing/negative/overflow usage and unverified rates.
- Provider tests verify retry records, durable start before dispatch,
  failures before/after dispatch, unknown usage and credential exclusion.
- `sh scripts/test-judge-bridge.sh`: native and POSIX guest records after
  reboot, correct cost, refusal to overwrite a disk, prefix preservation on
  resume and no network dispatch when the guest log fills.
- Existing drive/play bridge, evolution and protected-log regressions.

## Remaining boundaries

The file still has a 64 KiB limit. A failed multi-chunk write may leave a
partial tail; crash framing/recovery, result-space reservation, scalable
storage and power-cut tests are still required. Without result reservation,
storage can fill after a paid request, leaving a start record without its
result. Such runs cannot support complete cost claims. There is no automatic
truncation or log clearing.

Root-authorized programs can still append forged records. Guardian-only
append authority and durable whole-program judge/replay/commit activation
are unfinished. Provider JSON conversion, the old game bridge and much of
the OS remain Rust bootstrap code; this release moves accounting policy into
Agel, not all of those implementations. The OSRS client and comparative
experiments remain open in [the programme](exo-programme.md).
