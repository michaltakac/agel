# v0.3.38 · Return malformed repair proposals to the writer

The Agel repair loop now analyzes a candidate's syntax before asking
the text judge. If the analyzer rejects the form, the retry prompt
includes the rejected source and the specific error. Well-formed
proposals still require text approval and successful replay before
activation.

The regression submits a malformed `let` binding and verifies that
the next request goes to the writer with the binding error and the
original proposal. Run `cargo test -p agel-stdlib --test repair`.
