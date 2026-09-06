# Agel versioning

Agel uses Semantic Versioning, but remains explicitly pre-production. Project
releases stay below `1.0.0` until the language, native runtime, recovery model,
and supported operating-system surface are ready for a stable compatibility
promise.

- `v0.0.5` through `v0.0.9` identify the hosted bootstrap milestones.
- `v0.1.0` through `v0.1.7` identify the native workshop milestones.
- Patch releases repair a milestone without claiming a new capability rung.
- `v0.2.0` begins the agentic desktop line with an Agel-authored scene and live
  change protocol. `v0.2.1` adds the Agel-authored default shell, layout compiler,
  display-list contract, and semantic hit-testing. `v0.2.2` adds Agel-authored
  vector primitives and UI-to-vector compilation plus a bounded deterministic
  SVG output service. `v0.2.3` adds the VBE boot handoff and an unprivileged
  native compositor consuming an Agel-authored bounded vector stream, including
  retained-frame recovery. `v0.2.4` adds PS/2 and serial input, a visible native
  Agel command surface, and validated live scene commit, rejection, inspection,
  and rollback. `v0.2.5` adds the tested graphical kitchen-sink program and its
  exact 2880×1800 browser-rendered screenshot. `v0.2.6` joins the graphical
  command surface to the protected native evaluator and crash-tolerant source
  workspace, including replay-validated save and reconstruction after reboot.
  Minor releases may make
  deliberate breaking changes while Agel is still experimental; those changes
  must be documented and migration-tested.
- `v1.0.0` is reserved for the first production-ready Agel system.

Project releases and protocol versions are separate namespaces. In particular,
the frozen **Agel kernel contract v1.0** remains protocol v1.0: changing the
project release labels does not rewrite its wire format, conformance transcript,
or compatibility claim.

Historical Git commits retain their original subjects. Corrected release tags
are the canonical public names for those snapshots.
