# v0.3.36 · Verified native melee baseline

The fixed Agel policy trained Attack, Strength and Defence to level 2
on one fresh account inside Agel OS. The server saved 84 Attack XP,
88 Strength XP and 88 Defence XP after 32 confirmed encounters.
The game connection stayed open through both stance changes.

The guest measured 343.093 seconds for training after login and initial
observations, using the exact task clock recorded in the evidence.
The host measured 420.263 seconds including boot and the persistence
check. The run dispatched zero external model or judge calls.

The guest runner supports a bounded evaluation budget and logs each
confirmed encounter. The combat policy can try another observed target
after bounded retries, and the maintenance timer avoids sending
keepalives on every service pulse.

The matching adaptive command judges and replays a candidate service
policy on the live connection. This release records the fixed run.
The adaptive comparison remains pending.

[Recorded native run](evidence/v0.3.36-native-melee-fixed.txt)
