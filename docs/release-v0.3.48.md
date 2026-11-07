# v0.3.48 · Verify combat charges against the guest disk

The native combat comparison now reads the protected append ledger from
the adaptive guest disk. It checks the extracted frames and the Agel-decoded
records against the exported evidence, then matches each judge start and
receipt to its bridge dispatch. Build the Agel journal reader with
`cargo build -p agel-stdlib --example journal` before scoring the run.

Billing uses the receipt's published-rate field. Missing prices or usage
remain unknown, even if the provider's response text contains a number
that resembles a cost. Verification accepted a previously captured native
ledger and rejected independently modified frame and text exports.
