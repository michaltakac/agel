# v0.3.23 — Mine and smelt the first bronze bar

A fresh normal account can obtain a bronze pickaxe from the Lumbridge tutor,
mine one copper ore and one tin ore, and smelt the two ores into one bronze
bar. The route uses normal NPC, location and interface actions. The test
checks for zero recipe stock and zero Mining and Smithing XP, then observes
both ores before opening the furnace menu. It waits for the ores to be
consumed, the bar to appear, and Smithing XP to rise.

The revision 241 client adds the second location option. The pinned server
opens the smelting menu through this option. The existing dialogue resume
action selects quantity one on the first available recipe. The Agel packet
fixture checks the wire field order and rejects unsupported location
options. The first location option keeps its previous payload.

```sh
sh scripts/test-osrs-mining.sh --bronze
```

The runner uses the hosted Agel evaluator. The route takes the fresh account
from the mine through a waypoint near the starting area, then acts on the
Lumbridge furnace at `(3226, 3256, 0)`. It retains packet and action history
with the existing Agel record chain. The read-only server oracle checks the
saved inventory and skill XP independently of the packet observations.

The live run completed in 118.389 seconds through server verification. The
oracle saved one bronze bar, one bronze pickaxe, zero copper or tin ore,
35 Mining XP, and 6.2 Smithing XP at 1× rates. The first live attempt reached
the furnace area but targeted a decorative object. The verified run used
location 24009, the object with the Smelt action. See the
[live trace](evidence/v0.3.23-bronze.txt) and
[pinned world manifest](evidence/v0.3.23-world.json).

The standard library suite, packet fixture, lint, and formatting checks passed.

This is the first production chain from a fresh account. The higher-level
ores, Smithing training, and native adaptive comparison for the adamant
platebody benchmark remain open.
