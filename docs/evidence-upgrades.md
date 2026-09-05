# Agel v0.0.5: evidence-carrying upgrades

Agel deliberately calls these upgrades *evidence-carrying*, not formally
verified. A model, macro, or human may author a proposal, but only the small
Rust verifier can admit it. Model agreement is evidence about intent; it is
never authority.

An immutable `Proposal` binds a live base revision and 256-bit content digest,
source and source digest, verifier policy version, declared effects,
deterministic budget, and executable tests. `Verifier::verify` rejects altered
or stale proposals, undeclared syntactic effects, and attempted definitions in
the `agel/trusted-*` namespace. It evaluates the source in a fork with a new
world identity, empty ambient authority, and a private effect journal. Every
test gets its own fork of that candidate.

The resulting `Evidence` binds the proposal, base, candidate, inferred effects,
test count, and checker version. Promotion rechecks the binding against the
current live world and submits the source as one ordinary atomic transaction.
There is no route for proposal code to replace or waive the verifier.

```sh
cargo run -q -p agel-verify --example safe_upgrade
```

## Irreversible-effect repair

v0.4 kept model request status in rollbackable state. Restoring an older
snapshot could therefore resurrect a paid request. v0.0.5 adds a shared monotonic
effect journal outside transactional `State`. A model request receives a
SHA-256 effect key bound to world identity, request id, provider, and prompt
digest. Claim is written to this journal before provider launch; rollback and
snapshot restore cannot erase it.

Completions also carry the effect key and cannot satisfy a different request.
Replay uses a private journal so deterministic reconstruction cannot poison or
consume the live irreversible-effect ledger.

## Capability epochs

Capabilities now bind their issuer world and authority epoch. Isolated canary
forks use a new world identity. Restoring a snapshot increments the live epoch,
so bearer handles minted before restore become inert and the trusted host must
explicitly reissue current authority.

## Honest limitation

The v0.0.5 content digest is cryptographic but its payload is still an explicitly
versioned representation of the Rust seed's state. v0.0.7 replaces this with the
portable canonical image encoding required for cross-version persistence and
diverse-bootstrap comparison. It is not yet a detached signature.
