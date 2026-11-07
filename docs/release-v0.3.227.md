# v0.3.227

The Agel crafting example now composes two smelting batches and an
anvil action on one game state. It smelts three bars from the carried
ore and coal, withdraws the materials for two more, closes the bank,
and forges the five resulting bars. The entry point rejects
pre-existing bars and retains both smelting reports.

A hosted fixture exercised the complete sequence using supplied
travel callbacks. The stricter live bank-to-anvil repeat also passed.
A live route between the furnace and the bank is still required to
run the composed staged-ore workload on the private world.
