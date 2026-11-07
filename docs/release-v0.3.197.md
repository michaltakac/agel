# v0.3.197

A sequential adaptive-first one-rat comparison ran in 1 GiB Agel guests
on fresh private-server accounts. Both arms completed combat and saved 8
Attack XP at the normal XP rate. The fixed arm took 88.010 seconds and
the adaptive arm took 119.842 seconds, a difference of 31.833 seconds.
The world and combat randomness were uncontrolled.

The adaptive guest committed a policy after text judgment and replay.
The candidate reduced replay service calls from 73 to 25. The guest
estimated 212 comparable replays to recover the 72.033 seconds spent
on optimization, using measured replay savings of 0.340 seconds per
run. The committed policy served zero live packets. The fixed arm made
zero model calls. The adaptive arm recorded one judge call costing
$0.000040782 and one writer call with token usage and no price receipt.

A separate native fixture exercised the v0.3.195 completion check with
the task finishing during preparation. The guest stopped the temporary
service and exited successfully on the zero-call path. The matched
comparison used the earlier v0.3.192 source.

[Agel OS score output](evidence/v0.3.197-native-one-rat-score.txt)
