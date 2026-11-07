# v0.3.156

The RuneScape bronze-sale example checks progress after each observed packet
while waiting for ore, smelting and shop actions. The existing tick budgets
still bound those waits.

A fresh normal account on the private server mined both ores, smelted a bar
and sold it for three coins. The hosted run verified the saved Mining and
Smithing XP, consumed ores and bar, and coin balance in 154 seconds.
