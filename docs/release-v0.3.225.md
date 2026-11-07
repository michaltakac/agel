# v0.3.225

Agel waits for the full Smithing XP update as well as the completed
inventory before reporting a smelting batch. On the live private
world, a normal account consumed three staged adamantite ore and
eighteen coal, saved three bars, and gained exactly 112.5 Smithing XP.
[Live evidence](evidence/v0.3.225-adamant-batch.txt)

Opening a bank now waits for both interface panels and fresh bank
stock. The previous live check tried to deposit before observing the
side panel. All seven interface tests passed, including the new bank
opening fixture. The revised live banking check is in progress.
