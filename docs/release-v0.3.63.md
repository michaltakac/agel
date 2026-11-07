# v0.3.63 · Replay the retained workload before fresh-account transfer

The fresh guest now runs the retained encoder on the original workload
before connecting to the private world. It checks exact output bytes
and fewer service calls than the recorded fixed baseline. The combat
scorer expects the replay receipt when the installed client includes
this gate.

In a native run using a prior live donor, the fresh guest reproduced
4,614 bytes at a 320-byte cadence with 15 service calls. The donor's
fixed replay used 73 calls. This measures the transfer preflight.
Live combat and saved XP are verified separately.

Native one-rat scenarios now use 1 GiB of RAM and a 250 million step
budget. The combat scorer compares memory and fuel for one-rat pairs
as well as melee pairs.

[Native transfer preflight trace](evidence/v0.3.63-native-transfer-tape.txt)
