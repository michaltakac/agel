# v0.3.11: General Agel agents propose and activate repairs

`agel/repair` is application-neutral Agel code. It takes an existing versioned
tool, its source text, an application contract, observations, displayed probes,
held-out cases, provider names, delegated capabilities and an attempt limit.
It contains no RuneScape protocol, item, skill, NPC or task assumptions.

The running actor requests a diagnosis, revised source and predicted probe
outputs. The Agel reader parses the report; a restricted metacircular
environment constructs its data. A separate model judges the text. Only an
accepted text judgment reaches replay: predicted outputs, expected outputs
and held-out cases must all agree before the active tool changes. Refusal
preserves the active version, records the reason and includes the previous
report in the next proposal request. Provider failures also consume bounded
attempts and preserve the tool. Agent identity and history survive activation.

`repair-evaluate` measures both the retained control version and the activated
version, returning every case, result, error, correctness count and total.
The Rust additions are test harnesses only. Prompts, response parsing, state
transitions, judgment interpretation, retries, version activation and evals
are Agel. The existing CLI provides inference transport and portable images.

## Real model evidence

The [recorded reports](evidence/v0.3.11-repair.agel) show two independently
model-written repairs in one fresh hosted run:

| Application-neutral tool | Frozen version | Activated version |
| --- | --- | --- |
| Whole-batch resource planner | 4/8 correct | 8/8 correct |
| Event readiness gate | 3/6 correct | 6/6 correct |

The planner checks include signed 64-bit boundaries. The proposer wrote a
quotient/remainder implementation that avoids adding `size - 1` to the input.
The readiness checks include false, nil, zero and an unseen target. Held-out
cases are absent from proposal and judge prompts. The existing CLI has
read-only tools, so absence from prompts is not an adversarial secrecy proof.
This is a small mechanism demonstration, not an Exo task result or a claim of
general understanding. The diagnoses are model statements; predictions and
replay provide the executable evidence supporting these particular repairs.

The successful dispatch/evaluation took 13.748 seconds, excluding build and
earlier development attempts. This is not a paired runtime comparison against
the control. The final run used two Codex proposals and two Jev judgments.
Jev approved their text at 880 and 870 thousandths, above the fixed 800
threshold. Both active versions became version 2; version 1 remains callable.

## Failed runs and accounting

Earlier attempts are retained rather than counted as successes:

- A shared `(run)` exhausted its 100,000 fuel budget processing both replies.
  The portable image retained both completed model outputs. Resuming with
  `(run 1)` per agent processed them without repeating inference. The demo
  now gives each actor a separate evaluation budget.
- The initial semantic text question refused the batch proposal twice at 780,
  while accepting readiness. A later question demanding complete mathematical
  correctness gave scores of 90–390 while reporting no identified defect.
  Those actors exhausted their attempts and retained their original tools.
- The final text question assesses scope and diagnostic relevance. Executable
  replay assesses behavior. Its threshold was not lowered. All three question
  variants and their actual receipts remain in the evidence.

The [append records](evidence/v0.3.11-judge-attempts.agel) contain all nine real
development judge calls, their requests, responses, usage and Agel-calculated
costs: USD **0.00020118** in total, including failed trials. The final trial's
judge subtotal is USD **0.00004872**. There were nine Codex proposal calls
across these trials; their dollar costs are **unknown**, not zero. Initial
control installation uses explicit zero-cost fixture receipts.

The Jev adapter appends and syncs these records to the host OS's `judge-log`.
This new hosted repair path is not yet connected to Agel OS's protected
native ledger. Actor receipts retain unknown costs because the current model
completion message exposes text, not the provider usage record. Complete
all-model cost accounting and native journal integration remain open.

## Running and validation

Run `sh scripts/test-repair.sh` for deterministic tests. They cover activation,
predictions, held-out rejection, retries carrying reasons and rejected source,
malformed reports, out-of-range judgments, text refusal before replay,
provider failure, and retaining the control. Existing evolution and live-tool
contracts also pass. Clippy, formatting and Python compilation pass.

For a real run, use `python3 scripts/demo-repair.py` with authenticated Codex
and `TYPESAFEAI_API_KEY` (environment or local `.env`). It explicitly loads
the Agel modules and example, then uses the existing provider adapters. Codex's
[official automation documentation](https://learn.chatgpt.com/docs/non-interactive-mode)
describes that external CLI interface. The driver retains the image, output
and journal under a fresh `target/repair-demo-*` directory. Resume an existing
session with `--resume DIRECTORY`; completed inference replays from the image.
An exhausted actor is terminal and is not silently restarted by resume.

Application observations and contracts are supplied by callers. There is no
automatic discovery of arbitrary OS failures yet. The module is explicitly
loaded rather than installed into the bootstrap image. Native RuneScape
transport, autonomous whole-application rewriting, evaluator extension and
replacement of actual OS programs/domains/kernel slots remain unfinished.
