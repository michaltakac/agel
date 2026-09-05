# Typed effects and copy-on-write sandboxes

Agel v0.0.6 gives host effects one vocabulary and one interposition point. The
language core still only creates transactional intents. A trusted host decides
whether an intent may cross into the operating system.

## The envelope

Every `EffectIntent` states five things: principal, effect kind, operation,
resource, and a digest of its payload. Length-delimited canonical encoding binds
those fields into a SHA-256 key. Policies are default-deny; an operation must be
explicitly allowed or selected for virtualization.

The deliberately small effect taxonomy is:

```text
file/read  file/write  process/run  network/access
clock/read random/read model/infer
```

This taxonomy is a library API, not new Lisp syntax. The language stays small;
libraries supply policy, persistence, orchestration, and devices.

## Real process boundary

Claude Code and Codex no longer spawn child processes inside `agel-model`.
Instead, each enabled adapter installs exactly one executable in a
`ProcessSandbox`. The boundary:

- executes an argument vector directly, never through a shell;
- requires the exact configured executable;
- uses one canonical existing workspace directory;
- clears ambient environment and restores named variables only;
- pipes stdin/stdout/stderr, enforces a deadline and byte ceiling; and
- appends allow/deny and success/failure records to a shared audit log.

The Agel outbox's non-rollback effect key prevents paid inference from being
claimed twice. The process audit key serves a different purpose: it explains
what the host actually attempted, including provider, request number, principal,
executable, and prompt digest.

## Disposable worlds

`CowWorkspace` models AgentFS-style isolation without exposing a host path. A
base file map is immutable to proposed writes. Reads combine it with an overlay;
`diff` is deterministic; `rollback` discards the overlay; `commit` is the only
operation that merges it into the base. Paths are virtual UTF-8 paths and any
parent traversal is rejected.

Run the example:

```sh
cargo run -q -p agel-effects --example cow_workspace
```

This in-memory implementation establishes semantics for the persistent,
single-file image planned for v0.0.7. It does not yet virtualize an arbitrary
native process's filesystem accesses.

## Boundary honesty

`agel-effects` is an enforceable architectural choke point only for code that
cannot bypass it. It is not equivalent to a VM, seccomp, a capability kernel,
or Chimera-style dynamic binary translation. The Rust seed is trusted; hostile
native code is out of scope until a lower syscall boundary exists. This release
turns previously scattered host execution into something small enough to audit
and later replace.
