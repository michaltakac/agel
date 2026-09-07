# Agel v0.2.12 — Recovery and runtime audit

This release repairs the existing milestone rather than advertising a new OS.

- Native source saves validate in a candidate bank. Rejected source no longer
  destroys live agent state or its rollback point. Both graphical and serial
  workshops use the same save implementation.
- Process timeouts cover descendant-held pipes after parent exit. Output overflow
  terminates execution early, and invalid deadlines cannot spawn a process first.
- Process audit identities bind arguments and configured workspace as well as stdin.
- Whitespace around graphical preview/source commands no longer desynchronizes
  the view from the command that was executed.
- Removed blanket dead-code suppression from the kernel. Conformance-only code
  and unused entry points are feature-gated out of the desktop/serial images;
  their implementations remain available to the architecture tests.

Regression coverage includes failed saves with a live upgraded agent, source
rebuild rollback, QEMU reboot persistence, process-pipe deadlines, early overflow
termination and distinct audit keys for distinct argument vectors.

The audit found no `todo!`/`unimplemented!` Rust function bodies to fill. BIOS/trap
"stubs" are real assembly entry points and are retained. Documented research
models and future drivers, speech, rich native values and full widget tooling
are not represented as newly implemented. The hosted process wrapper is still
not a filesystem or adversarial-process sandbox; native actors still share an
evaluator domain. See the workbench and effect-boundary guides for the limits.
