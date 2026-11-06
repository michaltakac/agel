# v0.3.9 — Observed-target RuneScape combat

Run `sh scripts/test-osrs-combat.sh` against the private world. A fresh normal
account discovers nearby rats from live NPC packets, selects one, attacks,
and waits for its removal plus exactly 8 Attack XP and increased Hitpoints
XP. A separate read-only server oracle confirms saved XP and normal rights
at 1× rates. This passed on three fresh accounts during development; the
[final strict run](evidence/v0.3.9-combat.txt) and
[world manifest](evidence/v0.3.9-world.json) record the release evidence.

## Changes

The bit reader, NPC information decoder, tracked-index state, target
selection, attack packet and combat assertion are Agel. The Rust carrier
only loads the two additional modules; no new host protocol logic or kernel
ABI is introduced. NPC observations are separated by active world. The
combat API selects from the observed root-world indices and refuses an
index absent from that local state.

The decoder handles retained NPC counts, movement bitcodes, removals, new
indices, variable-width type IDs and spawn clocks, signed small/large spawn
offsets and extended-info terminators. It follows the pinned revision-241
RSProt source rather than an older protocol layout. The root world is index
zero in this version.

The test uses the public pinned-cache mapping `npc.rat=2854`. It supplies
neither NPC indices nor combat rewards through an administrator. Its success
condition is target removal and the expected XP after attacking, not a
separate server kill counter. The existing skill/item setup and movement
tests remain available and passed with NPC observation enabled.

## Validation

- Live combat and saved XP, with exact 8 Attack XP on the final run.
- Live movement, skill/item setup, ordinary-account permissions and saved state.
- Independent bit-field fixtures: random offsets/widths, all type-ID and
  spawn-clock widths, retained movement, pruning/removal, signs and truncation.
- Carrier Clippy, formatting and Python syntax checks.

The NPC fixtures run through `sh scripts/test-osrs-login.sh`, including in CI.

## Limits

This is a combat smoke test, not maximum combat stats, an autonomous combat
training policy or an Exo comparison. There is no model call or learning
loop in this scenario. Crafting, quests, money-making and transfer remain
unfinished.

Extended NPC masks are retained as raw packet data, not interpreted. Health,
death animations and transformations are not decoded, and spawn offsets are
not continuously updated world coordinates. The combat API currently targets
root-world NPCs. Full semantic observations and parity checks remain open.

The client remains hosted. The live Agel OS window mirrors actual test status;
native game transport, rendering, durable OS game history and guarded agent/OS
activation remain in the [programme](exo-programme.md). The existing private
profile still disables login proof-of-work while retaining password and game
ciphers; it must be identical for both future comparison arms.
