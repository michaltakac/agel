# Agel v0.2.21 — Modular behaviors reach the live OS

An Agel-written static module linker now resolves explicit imports/exports,
private bindings and lexical scopes, and expands restricted `defsyntax`
expression templates before native compilation. It runs as native Agel after
bootstrap; no new evaluator primitive is added.

The optional graphical console now edits module bundles and compiles them on
the host, then previews the expanded dock behavior inside the real QEMU OS.
Promotion/discard and source staging/save remain separate explicit actions.
An Agel-written adapter connects the pure behavior to the workbench's painting
function. Saved expanded source survives reboot.

```sh
cargo run --release -q -p agel-jit --example module_workshop
./scripts/run-graphics.sh --workbench --web
```

In a fresh world enter `:workbench`, open **Compile a modular dock behavior**,
then **Compile and preview**. Enter `:promote` or `:discard`. Stage the source
and `:save` to retain the expanded behavior after reboot.

Tests cover imports/privacy, lexical scopes, lazy templates and rejection cases.
A new CI QEMU test checks visual changes, preview/discard/promotion, failure on
current guest state, persistence and reboot using a disposable disk copy.

This ships a bounded static module/template dialect and an expanded-source OS
bridge—not general procedural `defmacro`, dynamically linked modules, or an
in-guest Cranelift JIT. Original module bundles are not persisted by the guest;
keep their source separately. No model calls are used.

See `docs/native-modules.md` for exact semantics and remaining self-hosting work.
