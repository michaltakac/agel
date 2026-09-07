# Self-hosting without permanent interpreter overhead

Research checked on 2026-09-07. Implemented milestone: v0.2.14.

## Decision

Being written in Rust does not make interpreted Agel execute like compiled Rust.
Self-hosting and fast execution are compatible, but interpreting an interpreter
adds work. We need to move reusable syntax decisions out of the execution path,
reduce representation/copying costs, then lower language-owned compiler output
to a small efficient execution substrate. Rust can provide that substrate while
Agel owns language semantics and transformations.

## Research informing this direction

- **Wasmi 2.0, September 1, 2026 (maintainer engineering report):** accumulator
  registers, shared code and redesigned instance access complement threaded
  dispatch. The reported 2.2× geometric mean is on Wasmi's workloads, not Agel's.
  Our takeaway: measure layout, copying and calls before focusing exclusively
  on the dispatch loop. [Report](https://wasmi-labs.github.io/blog/posts/wasmi-v2.0/).
- **GenExtension, MPLR 2026 (conference abstract):** interpreter-specific tracer
  specialization reduced tracing and resume-metadata generation time, but total
  execution time was essentially unchanged. Optimizing compilation time does
  not establish a throughput win. [Abstract](https://2026.ecoop.org/details/mplr-2026-papers/5/Generating-Interpreter-Specific-Tracers-for-Meta-Tracing-JIT-Compilers).
- **Fallin and Bernstein, PLDI 2025; 2024 public preprint:** `weval` specializes
  an interpreter with its program to derive compiled control flow. It motivates
  keeping semantics in one source and eliminating repeated dispatch, but is
  not a drop-in optimizer for today's Agel closure evaluator. We have not
  integrated it or implemented a Futamura projection.
  [Paper](https://arxiv.org/html/2411.10559v1).
- **Pulley, Bytecode Alliance:** an internal register bytecode and combined
  instructions let an optimizing pipeline reduce the work the interpreter
  executes. This informs our prospective portable VM, not a decision to add
  Wasmtime as a dependency. [Architecture](https://bytecodealliance.org/articles/wasmtime-portability).
- **SICP §4.1.7 (foundational, not recent research):** separate syntax analysis
  from execution and reuse execution procedures. This is the closest model
  for the deliberately smaller improvement implemented here.
  [Section](https://sicp.sourceacademy.org/chapters/4.1.7.html).

The implementation choices below are our engineering conclusions, not measured
claims made by those papers about Agel.

## Implemented: analyze once, execute many times

`meta-analyze` is written entirely in `stdlib.agel`. It walks quoted source once
and returns an ordinary Agel closure. That plan captures analyzed subexpressions;
execution no longer redispatches on their source syntax. Conditionals select
preanalyzed branches, and sequences reuse composed execution procedures.

```lisp
(import agel/meta)
(def plan (meta-analyze '(+ x 2)))
(def env (assoc (meta-base-env) 'x 40))
(plan env)                  ; 42
(plan (assoc env '+ -))     ; 38: dynamic bindings still work
```

`make-analyzed-agent` analyzes its behavior once at construction and stores the
original source beside its prepared behavior. Every message uses the prepared
body through the existing transactional scheduler. `make-meta-agent` remains
the reference interpreted path. Both preserve failed-turn state and outgoing
message rollback; neither constructor grants model capabilities.

Analyzed procedures use `(meta/analyzed parameters body environment)` internally,
where `body` is an Agel execution closure. `meta-apply` supports both interpreted
and analyzed procedures. Names and arguments still need runtime validation;
there is no unsafe global-binding constant folding. Analysis checks syntax in
all branches, so it may reject malformed *unexecuted* syntax earlier than the
reference interpreter. It does not run user effects or either branch at analysis
time. Division by zero in an unselected well-formed branch remains harmless.

## Implemented: share immutable host storage

Previously, cloning a closure copied its body and recursively copied its captured
lexical environments. That happened during lookup, calls and world snapshots.
Closure code now uses `Arc`; lexical frames use shared maps/parents with
copy-on-write insertion. Branches and captured scopes remain independent.
No new Rust evaluator primitive or language form was added.

This is an internal Rust API change: the hidden `Value::Closure` payload is now
`Arc<Closure>`. Lisp values, source replay and capability checks retain their
semantics. Reference counting is not a tracing collector, and this is not a
claim of constant-time whole-world transactions: other collections still copy,
and copy-on-write can copy a map when it is actually modified.

## Measurements and reproduction

```sh
cargo run --release -q -p agel-stdlib --example meta_benchmark
cargo run -q -p agel-cli < examples/analyzed-agents.agel
cargo test --workspace
./scripts/test-bootstrap.sh
```

The benchmark measures batches of 20 executions, discards one warm-up batch,
and reports the median of seven batches divided by 20. It includes source
parsing, the world transaction and commit; it is **not** isolated VM throughput.
Installation, environment setup and the outer test-world fork are outside the
timed region. Plan preparation is reported separately as a single sample;
reusing the plan amortizes that cost. The interpreted input contains quoted
source; the analyzed input is the shorter `(plan environment)`.

An illustrative local arm64 macOS release run with Rust 1.89.0:

| Workload | Interpreted | Reused analyzed plan | Evaluator steps, interpreted → analyzed |
| --- | ---: | ---: | ---: |
| Add 20 and 22 | 41.5 µs | 25.7 µs | 227 → 106 |
| Lexical capture | 183.9 µs | 80.4 µs | 959 → 413 |
| Factorial of 5 | 2,204.2 µs | 997.4 µs | 7,735 → 4,076 |

These are about 1.6–2.3× paired wall-time improvements for **these three small
workloads**, not a universal speedup. Preparation was approximately 0.17–0.32 ms
in that run. Deterministic step savings are regression-tested; noisy wall times
are deliberately not CI pass/fail thresholds. The initial pre-change benchmark
also exposed a large transaction/closure-copying cost: direct seed batches were
roughly 74–88 µs per operation versus 7–12 µs after sharing. That comparison also
spans a changed library image and must not be treated as a pure instruction
throughput measurement. All these runs use **zero model calls**.

Tests compare the shared success/failure corpora against interpreted and analyzed
Agel, while the bootstrap script checks Rust against Common Lisp. Additional
tests cover shared-storage non-aliasing, dynamic environments, analysis without
effects, failed actor sends, earlier syntax rejection and resource exhaustion.

## What comes next (not implemented by this release)

1. **Agel-owned explicit IR and lexical slots.** Replace repeated string/map
   lookups and closure-plan plumbing with inspectable resolved operands; retain
   source maps and explicit effect/call boundaries. Add allocation and realistic
   agent/desktop benchmarks before selecting a collector or tagged-value layout.
2. **Small Rust bytecode engine.** Start with validated portable instructions,
   an explicit value/call stack, bounded fuel and transactional effects. Measure
   register/accumulator bytecode and combined instructions; keep code ownership
   and validation separate from language-authored compilation policy.
3. **Bootstrapped compiler and native lowering.** Expand the Agel compiler until
   it can compile its own implementation, then verify bootstrap-stage agreement.
   Evaluate a Rust backend such as Cranelift for AOT first. A compiler written
   in Agel can emit efficient machine code without retaining this interpreter
   layer at runtime. Rust-generated machine code is not obtained automatically
   just because the first evaluator is Rust.
4. **Guarded specialization and live replacement.** Only after a sound IR and
   differential tests: specialize hot paths with dependency/version guards,
   deoptimization state and invalidation on code/shape/capability changes. Retain
   a recoverable generic path; never optimize away fuel, checked arithmetic or
   effect authorization. Scheduling/model latency is measured separately.

The current plan executor still runs on the Rust tree evaluator. This is an
analyzing evaluator, not machine-code generation, a JIT, full self-compilation,
proper tail-call elimination, or a native OS update. Current native Agel still
needs rich persistent values before it can run these hosted libraries.
