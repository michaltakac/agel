# v0.3.34 · Train three melee skills in one session

Agel trains Attack, Strength and Defence to a requested XP threshold on a
fresh normal account. It starts in the unarmed accurate stance, then uses
combat-tab clicks to select aggressive and defensive stances. Each switch
waits for the server's stance update. Each encounter checks XP gains and
target removal.

```sh
sh scripts/test-osrs-combat.sh --melee-xp 83
```

The hosted runner checks each phase's XP and level against the saved
server account, including the final stance, normal rights and the 1× XP
rate. Training uses the existing chunked packet recorder so the evaluator
does less copying as the session grows.

The verified level-2 run took 241.108 seconds including persistence
checks. Attack reached 84 XP in 10 encounters, Strength reached 84 XP
in 10, and Defence reached 88 XP in 11. The server saved level 2 in
all three skills and defensive stance. The 8 XP smoke and the original
one-rat regression also passed. See the [recorded runs](evidence/v0.3.34-melee-training.txt).

The full Exo combat objective also
requires ranged and magic training and much higher levels. This release
covers the fixed melee policy.
