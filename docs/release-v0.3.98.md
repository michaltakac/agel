# v0.3.98

A successful evolution replay retains its inputs and expected outputs in
Agel history. `(evolution-replay-version state version)` uses those cases
with the selected version's source, including versions that are no longer
active. Older stores still load and report when a version has no retained
corpus. The native checkpoint fixture uses a 65 million step budget to
accommodate the larger history during recovery and revert.

The byte-service optimizer prompt now asks for exactly two predictions
after both live proposals in the latest crafting pair failed preflight
on prediction count. The pair still verified both platebodies. The new
prompt has yet to run against the live writer.
