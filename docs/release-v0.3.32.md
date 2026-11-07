# v0.3.32 — Train to an Attack XP target

Agel now runs a bounded training episode on one fresh normal account. It
discovers a rat, attacks, and checks both XP gains and target removal.
After a successful encounter it waits for the same observed spawn to
return. If an attack stalls, it can reposition and retry within the
same connection. The report retains each encounter's XP observations.

```sh
sh scripts/test-osrs-combat.sh --attack-xp 83
```

This fixture uses the private world's ordinary combat and 1× XP rate.
The host compares the guest's final Attack and Hitpoints observations
with the account's saved server state. The original single-encounter
policy remains available for the fixed and transferred native comparison.

The hosted run reached level 2 with 84 Attack XP across ten observed
encounters in 73.043 seconds. The server saved level 2 and 84 Attack XP
at 1× XP. Checking completion after each packet lets the fixture react
to target removal without waiting for the end of a multi-tick batch.
This is an early Attack milestone. Training all combat stats and
comparing full training policies remain open.

See the [live training trace](evidence/v0.3.32-attack-training.txt).
