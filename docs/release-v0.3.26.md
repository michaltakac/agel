# v0.3.26 — Acquire quest containers on a fresh account

Agel picks up an empty pot in Lumbridge Castle, acquires a pickaxe,
mines copper and tin, smelts a bronze bar, and sells the bar for three
coins. It spends two of those coins on an empty bucket at the General Store.

The revision 241 Agel adapter adds ground-item pickup and supports
interfaces whose clickable slots are offset from their inventory slots.
The packet fixture checks both encodings, including the existing zero-based
inventory action used to sell the bar. The map coordinates, item IDs,
prices, and expected saved inventory live in the RuneScape fixture.

```sh
sh scripts/test-osrs-quest.sh --containers
```

The hosted test uses a fresh normal account at 1× XP. An independent
server check verifies one empty pot, one empty bucket, one remaining coin,
and an unstarted quest. The separate staged hand-in fixture still checks
Cook's Assistant completion and rewards. Gathering the full ingredient set
through ordinary gameplay remains open.

See the [live traces](evidence/v0.3.26-cook-containers.txt) and
[pinned world manifest](evidence/v0.3.26-world.json). The Agel standard
library suite, lint, and formatting checks passed.
