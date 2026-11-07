# v0.3.214

The RuneScape crafting example can smelt one to three adamant bars
from staged ore and coal. Agel waits for a fresh furnace interface,
selects the quantity, and verifies the recipe against the resulting
inventory. The batch returns both its report and the live game state
so a caller can continue crafting on the same connection.

The hosted fixture checked a three-bar batch with an existing bar.
The private-server oracle also supports checking the exact saved
Smithing XP and inventory for a batch.

Run `sh scripts/test-osrs-crafting.sh --smelt --smelt-count 3`
to exercise the private-server batch. Ore acquisition and Smithing
training are outside this staged workload.
