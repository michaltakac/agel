# v0.3.4 — Protected OS judge-log storage

`agel/judge-log` adds `judge-with-log`: append the request before dispatch,
invoke the supplied judge, then append its receipt and cost before returning.
A provider exception becomes a refusal receipt with an explicitly unknown
cost. An initial storage failure prevents dispatch. A result-storage failure
escapes to the caller, so it cannot silently activate a candidate. External
audit writes survive failure of the caller's Agel transaction.

The root filesystem entry `judge-log` now accepts only service-selected
end-of-file writes. The filesystem service rejects positional writes,
truncation, unlink, rename, replacement by rename, and formatting while this
entry exists. The desktop's append transport now requests append at the
service boundary instead of choosing an offset itself. These are byte-storage
invariants in the Rust bootstrap; receipt construction and dispatch order
are Agel code. Existing disk format and process ABI versions are unchanged.

The example `examples/logged-judge.agel` runs inside the `agel` OS process,
writing request and receipt records to `judge-log` with `file-append` and
`print-form`. Its costs and provider are fixtures, not a live-model result.
`boot/posix/journal-check` is a syscall test probe, not an agent implementation.

Validated with:

- `cargo test -p agel-stdlib`: audit ordering, failure before dispatch,
  failure after dispatch, retained external evidence and provider failure.
- `sh scripts/test-judge-log.sh`: Agel receipts, native and process appends,
  syscall overwrite/truncate/unlink/rename/replacement denial, denied format,
  reboot, repeated overwrite denial and append after reboot.
- `sh scripts/test-files.sh` and `sh scripts/test-libc.sh` on x86-64:
  ordinary filesystem, namespace, restart and C library regressions.
- Kernel and probe Clippy; formatting checks.

## Limits and next integration

This release provides protected storage and an opt-in wrapper. Existing
desktop, CLI and dataset judge entry points are not yet routed through it;
therefore this is not yet universal judge accounting. Integrating those
entry points and passing through real provider usage and pricing receipts
is required before running scored experiments. A missing cost remains
unknown and cannot support a coins-per-dollar result.

The file has the existing 64 KiB limit and no rotation protocol yet. Full
storage must stop dispatch; it must never clear the log. Multi-write records
may leave a partial tail after failure. There is no transaction-framed record
reader, hash chain or power-cut test for this file yet. These tests cover
normal reboot, not arbitrary power loss. Processes with root write access
can append forged records; the protected guardian will need a separate
namespace/append authority before the record can be trusted as canonical
evidence. A trusted host or raw disk writer can still alter the image.

The kernel storage checks are shared by all research architectures; only
x86-64 was boot-tested for this release. No game client or benchmark result
is included.
