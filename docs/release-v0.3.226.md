# v0.3.226

The private-world banking check passed on a normal account. Agel
deposited and withdrew staged ore, closed the bank, and verified the
saved inventory and XP against the starting snapshot.

A second live check deposited and withdrew five staged adamant bars,
closed the bank, and forged a platebody on the same connection.
The server saved one platebody, retained the hammer, and recorded
312.5 Smithing XP. [Live evidence](evidence/v0.3.226-bank-and-forge.txt)

Run `sh scripts/test-osrs-crafting.sh --bank-forge` to exercise the
route with staged bars.
