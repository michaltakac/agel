# v0.3.80 · Score verified economy pairs

Pass `--economy FIXED ADAPTIVE` to `scripts/compare-native-combat.py`
to emit an Agel score for two native economy runs. The evidence loader
checks the installed sources, guest binaries, matching world, distinct
accounts, elapsed clocks, and server-saved sale proceeds. It reconciles
the adaptive judge journal and writer receipts before passing the
observed charges to Agel. Writer dollar totals remain unknown.

New runs also record time to failure and save an independent account
snapshot for failed sale attempts. The scorer rejects a failure report
if the snapshot shows coin proceeds. Fixtures cover a sale, a measured
failure, and a failure contradicted by saved proceeds.
