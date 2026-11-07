# v0.3.21 — Forge an adamant platebody through the game interface

Agel can now use a world location and click a component on an observed
interface. The revision 241 client records open, moved and closed
subinterfaces and replaces its observations on a server resync. The server
still checks whether the location and button actions are allowed. The
packet test covers the outgoing field order, the interface lifecycle and
rejection of a click after close.

The crafting test stages Smithing 88, one hammer and five adamant bars on a
dedicated fixture account through the existing admin commands. It logs out,
verifies the saved inventory and removes admin rights before the crafting
session logs in. The normal account uses the anvil at `(3188, 3424, 0)`,
waits for interface 312 and the starting stock, then clicks the platebody component. The scenario
reads inventory and skill packets to check the result. An independent
server query checks the saved state.

The [hosted run](evidence/v0.3.21-hosted-crafting.txt) verified five bars consumed, one hammer retained, and one
adamant platebody produced. Smithing rose from 4,385,776 to 4,386,088 in
whole-XP packet observations. The server saved 312.5 XP at 1× rates. The
pinned cache supplied the anvil tile and item and component identifiers.

The [native run](evidence/v0.3.21-native-crafting.txt) repeated that
result inside Agel OS, with 38,449,077 evaluation steps in the 64 MiB guest.
It took 222.388 seconds from password acceptance to the guest's outcome
report and 280.112 seconds from native boot through guest exit. The outer
runner then verified the saved state. The trace shows one cache connection
and one game connection. See the [pinned world manifest](evidence/v0.3.21-world.json).

The [first native attempt](evidence/v0.3.21-native-trial.txt) delivered the
crafting action and the server saved the item, but the guest hit its
evaluation limit while reading queued packets. The revised order sends
the anvil interaction after login so the interface response can arrive
alongside the startup observations. The guest checks both the interface and
starting inventory before clicking. The evaluation budget stays at 50 million.

```sh
sh scripts/test-osrs-crafting.sh
sh scripts/test-osrs-crafting.sh --native
```

Both modes stage stock through the hosted client. The native flag runs the
normal-account crafting phase in Agel OS. The protocol fixture, standard
library suite, live movement and admin permission checks passed.

The supplied stock isolates the final manufacturing action. Gathering the
bars and training the account belong to the full crafting benchmark.
