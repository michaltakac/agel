# v0.3.14: Live language forms in an Agel-written evaluator

`agel/live-runtime` adds an evaluator whose language forms resolve through
Agel's version store. A model can change an AST transformer used by already
created closures. The caller supplies the current runtime when invoking a
closure; its stored body and lexical environment are retained. No native
image rebuild or controller restart is needed for a form edit.

The core evaluator, form dispatch, temporary candidate overlay, behavioral
replay and guarded revert are Agel code. This release adds no Rust code.
It does not replace the Rust seed evaluator, scheduler, filesystem or kernel.
See [the runtime contract and API](live-runtime.md).

The native example begins with an eager `unless` transformer. An existing
closure incorrectly returns 42 when its condition is true. The repair policy
provides that observation and an explicit expansion contract to the writer,
then requests a diagnosis, source AST and predicted expansion outputs. A
separate text judge precedes metacircular expansion replay and six runtime
canaries. Those canaries cover skipped division by zero, multiple body forms,
lexical let, quoted syntax, closure calls and an empty body.

After repair, the same closure skips its body on true and still computes 42
from its captured `saved = 9` on false. An attempted revert to eager behavior
passes its expansion fixture but fails actual runtime replay with division by
zero. The active version stays unchanged. A scripted equivalent wrapper then
activates, and a guarded revert to the retained model-written form creates
another version. The same closure is used throughout.

The example archives all four versions and their gate history in protected
OS `judge-log`, then reads back and compares the archived bytes. This does
not implement automatic canonical pointer recovery after reboot. Only the
initial repair is model-written; the equivalent wrapper and revert probes
are deterministic fixtures with zero-cost local judgments logged to the OS.

## Real run and accounting

The [native transcript](evidence/v0.3.14-native-runtime.txt) records one real
run with two attempts. The first text judgment scored 770/1000 against the
fixed 800 threshold; Agel refused it and passed that reason back to the writer.
The second proposal scored 810 and passed eight expansion/prediction cases
and all six runtime canaries. The expansion control scored 0/6 and the
repaired transformer 6/6 on the same cases. The native process exited 0 after
verifying the protected OS version archive.

The run took 135.443 seconds including VM startup and both attempts, excluding
builds and fixture development. Another fixture VM ran concurrently for part
of that interval, so this is an observed wall time, not a performance comparison.
The VM retained its 64 MiB configuration.

Both Jev calls, including the refusal, are in the [decoded protected OS ledger](evidence/v0.3.14-real-judge-records.agel).
They used 726 and 746 input tokens, with 68 output tokens each. Published-rate
costs were 30,492 and 31,332 nanodollars, totaling **$0.000061824**. Two Codex
proposals also ran; their dollar cost is unknown. Four local fixture judgments
(bootstrap, unsafe revert, equivalent edit, guarded revert) have zero-cost
receipts in protected `judge-log`. Fixture provider usage is synthetic and
excluded from paid totals. No all-in cost claim is available.

## Verification

The [unpaid native fixture](evidence/v0.3.14-runtime-fixture.txt) passes, as does
the existing native compiled-program scenario after the harness change. Focused
repair, live-tool, evolution and runtime tests and clippy checks pass.

Run `sh scripts/test-native-repair.sh --scenario runtime` for an unpaid
native fixture. Add `--live` for authenticated Codex and Jev. The harness keeps
each run's private disk, scenario, mode, serial output and decoded protected
judge ledger. `cargo test -p agel-stdlib --test live_runtime` checks retained
closures and environments, lazy core conditionals, quote, parallel let,
parameter validation, rejected reverts and guarded equivalent-version reverts.

## Scope

The experiment supplies a small, precise language-form contract and a
concrete observation. It does not establish autonomous runtime design or
performance superiority. Expansion scores compare output ASTs against that
contract; they are not Exo task scores or a cost/time comparison.

The evaluator implements a subset of Agel and delegates primitives to an
explicit environment. It does not implement its own agent scheduler, module
loader, hygienic macro expander, compiler or OS services. Current expansion
and behavioral replay share the controller's evaluator budget; a runaway
transformer is not contained in an independent process. Old runtime values
remain usable for explicit historical execution. Clients must pass the new
runtime to use its form versions.

Domain/kernel replacement, automatic recovery, full Agel self-hosting, native
game transport and Exo combat/crafting/coins/transfer/quest comparisons remain
open. RuneScape is a workload; this runtime has no game-specific policy.
