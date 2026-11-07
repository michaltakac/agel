# v0.3.60 · Verify stance changes through combat outcomes

The melee example gives the game a bounded window to confirm a stance
change, then proceeds with a bounded encounter. The requested skill
must gain XP and the target must be removed before the encounter counts.
The report retains the stance the guest actually observed. The runner
still checks the saved XP and final Defence stance on the private server.

A regression fixture withholds the Defence stance update and verifies
that the encounter counts only when Defence XP increases. A live hosted
smoke run saved 8 XP in each melee skill and confirmed stance 3.
A fresh native paired run is in progress.

The standalone native optimization test now uses 1 GiB of guest RAM
after its 64 MiB run exhausted memory before inference.
