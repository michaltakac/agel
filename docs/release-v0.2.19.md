# Agel v0.2.19 — Native agent-proposed behavior upgrades

A compiled designer agent now produces behavior source that native Agel composes
and lowers into a replacement program, without returning to the Rust evaluator
after bootstrap. Rust remains the validating machine-code backend.

Owner/revision-bound previews, explicit code-only promotion and checked rollback
preserve committed state and queued messages. Previous code must run successfully
against current state before restoration. Validated IR is inspectable as data.

Try:

```sh
cargo run --release -q -p agel-jit --example live_upgrade
```

Watch a designer propose `+10`, a preview predict 10 without committing it, and
promotion preserve queued work. Rollback keeps state at 10; the next message
uses the original `+1` and reaches 11. The executable is also an integration test.
Failure tests cover foreign/stale/competing candidates, invalid or depleted
probes, incompatible rollback and discarded candidates.

The Agel source composer is now closed and natively compilable, replacing the
previous duplicated wrapper implementation. No language syntax is added.

This is an isolated hosted JIT feature, not graphical-OS integration or complete
self-hosting. Promotion is host-authorized; one probe is not a formal proof.
No models or subscription tokens are used. See `docs/native-code-upgrades.md`
for resource costs, trust boundaries and remaining work.
