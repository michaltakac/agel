# Threat-model evolution

Each release records newly reachable attack surfaces and the invariant that
contains them.

## v0.5

- **Staged source:** proposals may be malformed, stale, effectful, or authored
  by a compromised model. The external verifier binds source and base digests,
  denies undeclared effects, runs without authority, and atomically promotes.
- **Time travel after external effects:** rollback may restore pending language
  state. A monotonic effect journal outside that state prevents a second claim.
- **Restored bearer authority:** snapshot restore invalidates existing handles
  by advancing the capability epoch.
- **Forged completion:** completions are bound to a world/request/provider/
  prompt-derived effect key.

The verifier is a deterministic gate, not a theorem prover. Unknown semantic
behavior is constrained by zero-authority canaries and executable tests; later
releases add stronger effect interposition and model checking.
