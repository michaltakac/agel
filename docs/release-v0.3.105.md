# v0.3.105

The native network runner leaves a separate grace period between the
inference bridge deadline and the runner's hard timeout. If the hard
timeout is reached, the runner still inspects the serial log and judge
evidence. Timed-out runs cannot be counted as task passes.

The native judge fixture also exercises a bridge deadline while a
rate-limited judge request is in flight. The result and cost reach the
guest ledger without dispatching a retry.
