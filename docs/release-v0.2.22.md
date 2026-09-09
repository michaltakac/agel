# Agel v0.2.22 — Library rungs become live paths

Three rungs of the bootstrap ladder existed only as libraries with one example
each: the verification gate, portable images, and the typed effect policy. This
release makes each one a path the running system actually takes, fixes the
security gaps found while doing so, widens the diverse-bootstrap corpus, and
grows the freestanding evaluator toward the hosted language.

```sh
cargo run -q -p agel-cli -- --image target/agel-world.image
```

```text
(def transform (fn (x) (+ x 1)))
:propose examples/upgrade-proposal.agel
:promote
(transform 41)
:image
```

## What changed

- **Verifier: conservative effect inference.** Effects were inferred only from
  call-head symbols, so `(apply model-request ...)`, a `let`-bound alias, or a
  module export aliasing the builtin passed with no declared effect. Any
  occurrence of an effect-bearing name now counts, including quoted data.
  `Verifier::check_promotion` exposes the evidence recheck for hosts that
  submit the source themselves.
- **CLI: the upgrade gate.** `:propose FILE [EFFECT ...]` reads a proposal
  file (`;effect` and `;test EXPR => EXPECTED` lines plus source), verifies it
  in a zero-authority canary and prints evidence; `:proposal`, `:promote` and
  `:discard` complete the loop. Promotion rechecks the binding against the live
  world immediately before the commit and any intervening commit fails closed.
- **CLI: portable images.** `--image PATH` runs the REPL over an image session:
  every committed input, provider grant, model claim and completion is appended
  to the tamper-evident image and the file is atomically replaced. Restart
  reconstructs the world by replay without re-invoking a provider. `:rollback`
  and `:restore` are refused in image mode because an append-only log cannot
  rewind honestly; `:image` reports the root and entry count.
- **Effects: the policy is consulted.** `ProcessSandbox::with_policy` decides
  every `process/run` intent before the executable allowlist; each model adapter
  installs a default-deny policy admitting only its own provider's inference
  requests. `WorkspaceBroker` routes `file/read` and `file/write` intents on the
  copy-on-write workspace through a policy, where `Virtualize` stages an overlay
  change for explicit commit or rollback. Every decision is audited.
- **Diverse bootstrap: maps and text.** The Common Lisp reference implements
  insertion-ordered persistent maps with structural equality and the five
  byte-oriented UTF-8 text mechanisms; `agel/meta` exposes the text mechanisms;
  the shared corpus checks all three evaluators on both, including required
  failures.
- **Freestanding evaluator: `let`, variadic arithmetic, multi-form functions.**
  Parallel `let` with last-binding-wins, `+`/`*` folds from their identities,
  unary and n-ary `-`, n-ary `/`, and `fn` bodies of any length (persisted as one
  `begin` form). Every existing bound is unchanged and reported by `:limits`.
- **Graphical `:help` fits its status line.** The postcard had outgrown the
  256-byte line and was silently truncated; its length is now a compile-time
  assertion, and `:workspace` and `let` are listed.
- **`run-graphics.sh` validates its flags** and accepts `--native`, `--help`
  and `--` for QEMU arguments.
- **Renderer tests.** `agel-vector` gains integration tests freezing the
  kitchen-sink SVG digest and rejecting malformed or oversized frames.
- **Documentation corrected** where it disagreed with the code: the ladder in
  `architecture.md` covers every release through this one; the dependency and
  `unsafe` claims name the actual crates; `standard-library.md` documents every
  installed module; orphaned examples are linked; the two meanings of
  `:promote` are stated.

## Verification

```sh
cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
./scripts/test-bootstrap.sh
sh scripts/test-native-agents.sh
./scripts/build-boot.sh --features native-graphics
./scripts/test-native-repl.sh
python3 scripts/test-native-workbench.py target/boot/agel-v1.img
```

The full QEMU suite (boot, monitor, native REPL and persistence, graphics,
live desktop, graphical workshop and console, dock, workbench, modules,
three-architecture isolation) passes with the evaluator changes.

## What this does not claim

The workspace broker is in-memory, not host filesystem confinement. The model
adapters' policy is enforced by the trusted Rust host. Proposal files are read
with the operator's authority. Images are hash-chained, not signed. The
freestanding evaluator still has no strings, lists or maps, and compilation
still happens on the host. No model calls are used by any test.
