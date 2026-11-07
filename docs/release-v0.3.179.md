# v0.3.179

`scripts/test-native-economy-pair.py --combat` runs the one-rat task
on fresh fixed and adaptive accounts. Both arms pin the same source,
runtime, and keepalive settings. If the adaptive arm exhausts its
proposals, it can finish combat with the fixed policy and retain the
cost and refusal evidence.

The scorer accepted a retained adaptive one-rat run with verified
judge receipts. A new matched run is queued.
