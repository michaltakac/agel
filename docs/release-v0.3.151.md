# v0.3.151

The native benchmark runner accepts either execution order for the
fixed and adaptive arms. It records the planned order before starting
either account. The scorer still receives the same arm identities
regardless of which run finished first.

Runner fixtures verified both orders. Live counterbalanced repetitions
are pending.
