# v0.3.213

When a melee stance change falls back to XP confirmation, the trainer
keeps watching for a fresh stance update during the encounter. A fresh
contradiction stops the encounter before another target is attempted.
Earning XP in the requested skill clears the pending check.

The OSRS interface tests covered a contradictory update arriving after
the first attack, as well as confirmation through XP with no update.
