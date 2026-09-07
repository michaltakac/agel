# Agel v0.2.14 — Analyze once, share immutable code

- Added `meta-analyze`, written in Agel, producing reusable execution plans for
  the hosted functional subset. No new language primitives.
- Added opt-in `make-analyzed-agent`: prepare behavior once and reuse it for
  transactional turns while retaining inspectable source.
- Shared immutable closure code and lexical frames in the Rust bootstrap,
  removing repeated deep copies. Writes use copy-on-write to preserve isolation.
- Added paired release-mode benchmarks and differential/regression coverage for
  source semantics, dynamic environments, effect timing, state rollback and fuel.

The three small local benchmark cases use about half as many evaluator steps
and show roughly 1.6–2.3× paired wall-time gains for reused plans, with preparation
reported separately. These are not general application-throughput guarantees.
No inference is needed. Run `examples/analyzed-agents.agel` in the hosted CLI
and `cargo run --release -q -p agel-stdlib --example meta_benchmark`.

Explicit analysis can reject malformed syntax in dead branches earlier than
reference interpretation. Internally, the hidden Rust `Value::Closure` payload
now uses `Arc<Closure>`; Agel syntax and the native kernel contract are unchanged.

See `docs/self-hosting-performance.md` for dated primary research, measurements,
and the path toward an Agel-authored compiler and fast VM/native backend. This
release does not claim native code generation, a JIT or complete self-hosting.
