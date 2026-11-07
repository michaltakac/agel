# v0.3.215

The adamant smelting check verifies the full saved inventory before
and after the batch. The normal account must start with the exact
ore and coal, then save only the expected bars. Its Smithing XP gain
and the Agel inventory report must agree with the recipe.

Fixtures rejected starting bars, elevated rights, a different account,
incorrect XP, and a mismatched Agel report. The live batch is queued.
