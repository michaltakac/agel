# v0.3.89 · Retain measured probe receipts in active versions

Agel repair now stores a successful external probe alongside the
activation receipt of the committed version. A separate activation
hook can still attach its own artifacts. Refused probes continue to
feed the next attempt and leave the active version unchanged.

Fresh-account transfer checks a retained probe's cadence and call
count against the recorded-byte replay when the receipt is present.
Older donor versions without an activation receipt still use the
existing exact-byte replay checks.

The repair and transfer fixtures passed. The hosted Cook hand-in also
passed with the v0.3.88 route logic. The native hand-in ended with a
broken connection, and the pinned economy pair is still running.
