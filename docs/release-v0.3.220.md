# v0.3.220

The live melee optimizer reads queued game observations between the
control and candidate replay measurements. These reads update the
active session and appear in the guest log. The replay clocks still
measure the encoder workloads themselves.

A 1 GiB guest fixture verified that observation ran between each of
the three measurements and that the probe kept both control receipts.
The live melee pair uses an earlier build.
